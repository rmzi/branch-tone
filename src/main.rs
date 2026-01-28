// =============================================================================
// branch-tone: Generate unique musical phrases from git branch names
// =============================================================================

use std::f32::consts::PI;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use sha2::{Sha256, Digest};

// -----------------------------------------------------------------------------
// CLI ARGUMENTS
// -----------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "branch-tone")]
#[command(version = "0.2.0")]
#[command(about = "Generate unique musical phrases from git branch names")]
struct Args {
    /// The branch name to generate a tone for
    #[arg(value_name = "BRANCH")]
    branch: Option<String>,

    /// Duration of the phrase in milliseconds
    #[arg(short, long, default_value = "400")]
    duration: u64,

    /// Volume level (0.0 to 1.0)
    #[arg(short, long, default_value = "0.25")]
    volume: f32,

    /// Just print the parameters without playing
    #[arg(long)]
    dry_run: bool,
}

// -----------------------------------------------------------------------------
// MUSICAL CONSTANTS
// -----------------------------------------------------------------------------

/// Pentatonic scale frequencies (C, D, E, G, A) - always sounds good together
const PENTATONIC: [f32; 5] = [261.63, 293.66, 329.63, 392.00, 440.00];

/// Octave options
const OCTAVES: [f32; 3] = [0.5, 1.0, 2.0];

/// Arpeggio patterns (intervals from root in scale degrees)
/// Each pattern creates a different "feel"
const PATTERNS: [[i32; 3]; 4] = [
    [0, 2, 4],   // Rising third (hopeful)
    [0, 1, 2],   // Rising step (gentle)
    [2, 1, 0],   // Falling (calming)
    [0, 2, 0],   // Up and back (playful)
];

// -----------------------------------------------------------------------------
// PHRASE PARAMETERS
// -----------------------------------------------------------------------------

#[derive(Debug)]
struct PhraseParams {
    notes: Vec<f32>,      // Frequencies to play
    note_duration: u64,   // Duration per note in ms
    volume: f32,
    attack_ms: u64,
    decay_ms: u64,
}

impl PhraseParams {
    fn from_branch(branch: &str, total_duration: u64, volume: f32) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(branch.as_bytes());
        let hash = hasher.finalize();

        // Pick root note from pentatonic scale
        let root_idx = (hash[0] as usize) % PENTATONIC.len();

        // Pick octave
        let octave_idx = (hash[1] as usize) % OCTAVES.len();
        let octave = OCTAVES[octave_idx];

        // Pick arpeggio pattern
        let pattern_idx = (hash[2] as usize) % PATTERNS.len();
        let pattern = PATTERNS[pattern_idx];

        // Build the notes
        let notes: Vec<f32> = pattern.iter().map(|&offset| {
            let idx = ((root_idx as i32 + offset).rem_euclid(PENTATONIC.len() as i32)) as usize;
            PENTATONIC[idx] * octave
        }).collect();

        // Timing
        let note_duration = total_duration / 3;
        let attack_ms = 15 + ((hash[3] as u64) % 20);  // 15-35ms
        let decay_ms = 30 + ((hash[4] as u64) % 40);   // 30-70ms

        Self {
            notes,
            note_duration,
            volume,
            attack_ms,
            decay_ms,
        }
    }
}

// -----------------------------------------------------------------------------
// MAIN
// -----------------------------------------------------------------------------

fn main() -> Result<()> {
    let args = Args::parse();

    let branch = match args.branch {
        Some(b) => b,
        None => get_current_branch()
            .context("No branch specified and couldn't detect current git branch")?,
    };

    let params = PhraseParams::from_branch(&branch, args.duration, args.volume);

    // Print info
    println!("🎵 Branch: {}", branch);
    println!("   Notes: {:?}", params.notes.iter().map(|f| format!("{:.0}Hz", f)).collect::<Vec<_>>());
    println!("   Duration: {}ms per note", params.note_duration);

    if args.dry_run {
        return Ok(());
    }

    play_phrase(&params)?;

    Ok(())
}

// -----------------------------------------------------------------------------
// GIT BRANCH DETECTION
// -----------------------------------------------------------------------------

fn get_current_branch() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .context("Failed to run git command")?;

    if !output.status.success() {
        anyhow::bail!("git command failed - are you in a git repository?");
    }

    let branch = String::from_utf8(output.stdout)
        .context("Git output was not valid UTF-8")?
        .trim()
        .to_string();

    if branch.is_empty() {
        anyhow::bail!("No current branch (detached HEAD?)");
    }

    Ok(branch)
}

// -----------------------------------------------------------------------------
// AUDIO PLAYBACK
// -----------------------------------------------------------------------------

fn play_phrase(params: &PhraseParams) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No audio output device found")?;
    let config = device
        .default_output_config()
        .context("Failed to get default audio config")?;

    match config.sample_format() {
        cpal::SampleFormat::F32 => run_audio::<f32>(&device, &config.into(), params),
        cpal::SampleFormat::I16 => run_audio::<i16>(&device, &config.into(), params),
        cpal::SampleFormat::U16 => run_audio::<u16>(&device, &config.into(), params),
        format => anyhow::bail!("Unsupported sample format: {:?}", format),
    }
}

fn run_audio<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    params: &PhraseParams,
) -> Result<()>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // Copy params for the closure
    let notes = params.notes.clone();
    let note_duration = params.note_duration;
    let volume = params.volume;
    let attack_ms = params.attack_ms;
    let decay_ms = params.decay_ms;

    let samples_per_note = (sample_rate * note_duration as f32 / 1000.0) as usize;
    let total_samples = samples_per_note * notes.len();
    let attack_samples = (sample_rate * attack_ms as f32 / 1000.0) as usize;
    let decay_samples = (sample_rate * decay_ms as f32 / 1000.0) as usize;

    let sample_clock = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sample_clock_clone = sample_clock.clone();

    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_clone = finished.clone();

    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                let current_sample = sample_clock_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                if current_sample >= total_samples {
                    finished_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = T::from_sample(0.0f32);
                    }
                    continue;
                }

                // Which note are we on?
                let note_idx = current_sample / samples_per_note;
                let sample_in_note = current_sample % samples_per_note;
                let frequency = notes[note_idx];

                // Envelope for this note
                let envelope = if sample_in_note < attack_samples {
                    sample_in_note as f32 / attack_samples as f32
                } else if sample_in_note > samples_per_note - decay_samples {
                    (samples_per_note - sample_in_note) as f32 / decay_samples as f32
                } else {
                    1.0
                };

                // Soft sine wave with a touch of warmth (slight 2nd harmonic)
                let time = current_sample as f32 / sample_rate;
                let fundamental = (2.0 * PI * frequency * time).sin();
                let harmonic = (2.0 * PI * frequency * 2.0 * time).sin() * 0.1; // Subtle warmth

                let sample_value = (fundamental + harmonic) * envelope * volume;

                let sample = T::from_sample(sample_value);
                for channel_sample in frame.iter_mut() {
                    *channel_sample = sample;
                }
            }
        },
        err_fn,
        None,
    ).context("Failed to build audio stream")?;

    stream.play().context("Failed to start audio playback")?;

    while !finished.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }

    std::thread::sleep(Duration::from_millis(50));

    Ok(())
}
