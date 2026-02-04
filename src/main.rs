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
#[command(version = "0.4.0")]
#[command(about = "Generate unique musical phrases from git branch names")]
struct Args {
    /// The branch name to generate a tone for
    #[arg(value_name = "BRANCH")]
    branch: Option<String>,

    /// Repository name (auto-detected if not provided)
    #[arg(short, long)]
    repo: Option<String>,

    /// Duration of the phrase in milliseconds
    #[arg(short, long, default_value = "400")]
    duration: u64,

    /// Volume level (0.0 to 1.0)
    #[arg(short, long, default_value = "0.25")]
    volume: f32,

    /// Pad mode: play notes as a warm chord with long attack/release
    #[arg(long)]
    pad: bool,

    /// Add chorus effect (detuned layers for richness)
    #[arg(long)]
    chorus: bool,

    /// Add tremolo effect (volume wobble)
    #[arg(long)]
    tremolo: bool,

    /// Number of notes in sequence (3 or 5)
    #[arg(long, default_value = "3")]
    steps: u8,

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

/// Arpeggio patterns - 3 note (intervals from root in scale degrees)
const PATTERNS_3: [[i32; 3]; 4] = [
    [0, 2, 4],   // Rising third (hopeful)
    [0, 1, 2],   // Rising step (gentle)
    [2, 1, 0],   // Falling (calming)
    [0, 2, 0],   // Up and back (playful)
];

/// Arpeggio patterns - 5 note (more melodic)
const PATTERNS_5: [[i32; 5]; 4] = [
    [0, 2, 4, 2, 0],   // Up and down (resolved)
    [0, 1, 2, 3, 4],   // Rising scale (ascending)
    [4, 3, 2, 1, 0],   // Falling scale (descending)
    [0, 2, 1, 3, 2],   // Winding (playful)
];

// -----------------------------------------------------------------------------
// SOUND EFFECTS
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Effects {
    pad: bool,      // Chord mode with long envelope
    chorus: bool,   // Detuned layers
    tremolo: bool,  // Volume modulation
}

// -----------------------------------------------------------------------------
// PHRASE PARAMETERS
// -----------------------------------------------------------------------------

#[derive(Debug)]
struct PhraseParams {
    notes: Vec<f32>,      // Frequencies to play
    total_duration: u64,  // Total duration in ms
    volume: f32,
    effects: Effects,
}

impl PhraseParams {
    fn from_identity(repo: &str, branch: &str, total_duration: u64, volume: f32, effects: Effects, steps: u8) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", repo, branch).as_bytes());
        let hash = hasher.finalize();

        // Pick root note from pentatonic scale
        let root_idx = (hash[0] as usize) % PENTATONIC.len();

        // Pick octave
        let octave_idx = (hash[1] as usize) % OCTAVES.len();
        let octave = OCTAVES[octave_idx];

        // Build the notes based on step count
        let notes: Vec<f32> = if steps >= 5 {
            let pattern_idx = (hash[2] as usize) % PATTERNS_5.len();
            let pattern = PATTERNS_5[pattern_idx];
            pattern.iter().map(|&offset| {
                let idx = ((root_idx as i32 + offset).rem_euclid(PENTATONIC.len() as i32)) as usize;
                PENTATONIC[idx] * octave
            }).collect()
        } else {
            let pattern_idx = (hash[2] as usize) % PATTERNS_3.len();
            let pattern = PATTERNS_3[pattern_idx];
            pattern.iter().map(|&offset| {
                let idx = ((root_idx as i32 + offset).rem_euclid(PENTATONIC.len() as i32)) as usize;
                PENTATONIC[idx] * octave
            }).collect()
        };

        Self {
            notes,
            total_duration,
            volume,
            effects,
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

    let repo = match args.repo {
        Some(r) => r,
        None => get_repo_name().unwrap_or_else(|_| "unknown".to_string()),
    };

    let effects = Effects {
        pad: args.pad,
        chorus: args.chorus,
        tremolo: args.tremolo,
    };

    // Pad mode benefits from longer duration
    let duration = if args.pad && args.duration == 400 {
        800  // Default to 800ms for pad
    } else {
        args.duration
    };

    let params = PhraseParams::from_identity(&repo, &branch, duration, args.volume, effects, args.steps);

    // Print info
    let mode = match (args.pad, args.chorus, args.tremolo) {
        (true, _, _) => " [pad]",
        (_, true, _) => " [chorus]",
        (_, _, true) => " [tremolo]",
        _ => " [arpeggio]",
    };
    println!("🎵 Repo: {} | Branch: {}{}", repo, branch, mode);
    println!("   Notes: {:?}", params.notes.iter().map(|f| format!("{:.0}Hz", f)).collect::<Vec<_>>());
    println!("   Duration: {}ms", params.total_duration);
    if args.chorus { println!("   + Chorus (detuned layers)"); }
    if args.tremolo { println!("   + Tremolo (6Hz wobble)"); }

    if args.dry_run {
        return Ok(());
    }

    play_phrase(&params)?;

    Ok(())
}

// -----------------------------------------------------------------------------
// GIT DETECTION
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

fn get_repo_name() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(name) = url.rsplit('/').next() {
                return Ok(name.trim_end_matches(".git").to_string());
            }
        }
    }

    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to get git root")?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some(name) = path.rsplit('/').next() {
            return Ok(name.to_string());
        }
    }

    Ok("unknown".to_string())
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

    let notes = params.notes.clone();
    let total_duration = params.total_duration;
    let volume = params.volume;
    let effects = params.effects;

    let total_samples = (sample_rate * total_duration as f32 / 1000.0) as usize;

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

                let time = current_sample as f32 / sample_rate;
                let progress = current_sample as f32 / total_samples as f32;

                let sample_value = if effects.pad {
                    // PAD MODE: All notes as chord with long envelope
                    generate_pad(&notes, time, progress, volume, effects)
                } else {
                    // ARPEGGIO MODE: Sequential notes
                    generate_arpeggio(&notes, time, current_sample, total_samples, volume, effects)
                };

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

// -----------------------------------------------------------------------------
// SOUND GENERATORS
// -----------------------------------------------------------------------------

fn generate_pad(notes: &[f32], time: f32, progress: f32, volume: f32, effects: Effects) -> f32 {
    // Long attack and release for pad sound
    let attack = 0.3;  // 30% of duration
    let release = 0.3; // 30% of duration

    let envelope = if progress < attack {
        // Smooth ease-in (sine curve)
        (progress / attack * PI / 2.0).sin()
    } else if progress > (1.0 - release) {
        // Smooth ease-out
        ((1.0 - progress) / release * PI / 2.0).sin()
    } else {
        1.0
    };

    // Generate chord (all notes together)
    let mut sample = 0.0;
    for (i, &freq) in notes.iter().enumerate() {
        let osc = generate_oscillator(freq, time, effects.chorus, i);
        sample += osc;
    }

    // Normalize by number of notes
    sample /= notes.len() as f32;

    // Apply tremolo if enabled
    let sample = if effects.tremolo {
        apply_tremolo(sample, time)
    } else {
        sample
    };

    sample * envelope * volume
}

fn generate_arpeggio(notes: &[f32], time: f32, current_sample: usize, total_samples: usize, volume: f32, effects: Effects) -> f32 {
    let samples_per_note = total_samples / notes.len();
    let note_idx = (current_sample / samples_per_note).min(notes.len() - 1);
    let sample_in_note = current_sample % samples_per_note;
    let frequency = notes[note_idx];

    // Quick attack/decay for arpeggio
    let attack_samples = (samples_per_note as f32 * 0.1) as usize;
    let decay_samples = (samples_per_note as f32 * 0.2) as usize;

    let envelope = if sample_in_note < attack_samples {
        sample_in_note as f32 / attack_samples as f32
    } else if sample_in_note > samples_per_note - decay_samples {
        (samples_per_note - sample_in_note) as f32 / decay_samples as f32
    } else {
        1.0
    };

    let osc = generate_oscillator(frequency, time, effects.chorus, 0);

    let sample = if effects.tremolo {
        apply_tremolo(osc, time)
    } else {
        osc
    };

    sample * envelope * volume
}

fn generate_oscillator(freq: f32, time: f32, chorus: bool, voice_idx: usize) -> f32 {
    if chorus {
        // Chorus: 3 detuned oscillators
        let detune_cents = [0.0, -8.0, 8.0]; // cents
        let mut sample = 0.0;
        for (i, &cents) in detune_cents.iter().enumerate() {
            let detune_factor = 2.0_f32.powf(cents / 1200.0);
            let f = freq * detune_factor;
            // Slight phase offset per voice for richness
            let phase_offset = (voice_idx as f32 + i as f32) * 0.1;
            let fundamental = (2.0 * PI * f * time + phase_offset).sin();
            let harmonic = (2.0 * PI * f * 2.0 * time + phase_offset).sin() * 0.15;
            sample += (fundamental + harmonic) / 3.0;
        }
        sample
    } else {
        // Simple oscillator with slight warmth
        let fundamental = (2.0 * PI * freq * time).sin();
        let harmonic = (2.0 * PI * freq * 2.0 * time).sin() * 0.1;
        fundamental + harmonic
    }
}

fn apply_tremolo(sample: f32, time: f32) -> f32 {
    // 6Hz tremolo with 30% depth
    let tremolo_freq = 6.0;
    let tremolo_depth = 0.3;
    let tremolo = 1.0 - tremolo_depth * (0.5 + 0.5 * (2.0 * PI * tremolo_freq * time).sin());
    sample * tremolo
}
