// =============================================================================
// branch-tone: Generate unique musical phrases from git branch names
// =============================================================================

use std::f32::consts::PI;
use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use sha2::{Sha256, Digest};

// -----------------------------------------------------------------------------
// CLI ARGUMENTS
// -----------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "branch-tone")]
#[command(version)]
#[command(about = "Generate unique musical phrases from git branch names")]
#[command(args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    // Top-level flags for backwards compatibility (bare `branch-tone [BRANCH] [flags]`)
    #[command(flatten)]
    play_args: PlayArgs,
}

#[derive(clap::Args, Debug)]
struct PlayArgs {
    /// The branch name to generate a tone for
    #[arg(value_name = "BRANCH")]
    branch: Option<String>,

    /// Repository name (auto-detected if not provided)
    #[arg(short, long)]
    repo: Option<String>,

    /// Duration of the phrase in milliseconds
    #[arg(short, long, default_value = "600")]
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

    /// Bulldozer mode: pad + arpeggiated shimmer layer
    #[arg(long)]
    bulldozer: bool,

    /// Number of notes in sequence (3 or 5)
    #[arg(long, default_value = "3")]
    steps: u8,

    /// Just print the parameters without playing
    #[arg(long)]
    dry_run: bool,

    /// Suppress informational output (used by hook)
    #[arg(long, hide = true)]
    quiet: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Play a tone for a branch (default behavior)
    Play(PlayArgs),

    /// Read Claude Code hook JSON from stdin, detect branch, play tone
    Hook,

    /// Wire up Claude Code hooks (updates settings.json)
    Init,
}

// -----------------------------------------------------------------------------
// MUSICAL CONSTANTS
// -----------------------------------------------------------------------------

/// Chromatic root frequencies (C4 through B4)
const CHROMATIC_ROOTS: [f32; 12] = [
    261.63, 277.18, 293.66, 311.13, 329.63, 349.23,
    369.99, 392.00, 415.30, 440.00, 466.16, 493.88,
];

/// Note names for display
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Scale intervals in semitones from root (5 notes each, pentatonic-safe)
const SCALES: [[u8; 5]; 6] = [
    [0, 2, 4, 7, 9],    // Major pentatonic
    [0, 3, 5, 7, 10],   // Minor pentatonic
    [0, 2, 3, 7, 9],    // Dorian (penta subset)
    [0, 2, 4, 6, 9],    // Lydian (penta subset)
    [0, 2, 4, 7, 10],   // Mixolydian (penta subset)
    [0, 3, 5, 8, 10],   // Minor (penta subset)
];

/// Scale names for display
const SCALE_NAMES: [&str; 6] = [
    "Major Pentatonic", "Minor Pentatonic", "Dorian", "Lydian", "Mixolydian", "Minor",
];

/// Octave multipliers (more spread than before)
const OCTAVES: [f32; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];

/// Arpeggio patterns - 3 note (intervals from root in scale degrees)
const PATTERNS_3: [[i32; 3]; 8] = [
    [0, 2, 4],   // Rising third (hopeful)
    [0, 1, 2],   // Rising step (gentle)
    [2, 1, 0],   // Falling (calming)
    [0, 2, 0],   // Up and back (playful)
    [0, 4, 2],   // Leap then settle
    [4, 0, 2],   // Drop then rise
    [1, 3, 0],   // Offset start
    [0, 3, 1],   // Wide then narrow
];

/// Arpeggio patterns - 5 note (more melodic)
const PATTERNS_5: [[i32; 5]; 8] = [
    [0, 2, 4, 2, 0],   // Up and down (resolved)
    [0, 1, 2, 3, 4],   // Rising scale (ascending)
    [4, 3, 2, 1, 0],   // Falling scale (descending)
    [0, 2, 1, 3, 2],   // Winding (playful)
    [0, 4, 1, 3, 2],   // Leap and weave
    [2, 0, 4, 1, 3],   // Scattered
    [0, 3, 1, 4, 0],   // Wide arc
    [4, 2, 0, 3, 1],   // Descending weave
];

/// Envelope shapes: (attack_fraction, decay_fraction)
const ENVELOPE_SHAPES: [(f32, f32); 4] = [
    (0.05, 0.15),  // Punchy: fast attack, short decay
    (0.25, 0.30),  // Soft: slow attack, long decay
    (0.02, 0.20),  // Pluck: instant attack, medium decay
    (0.40, 0.10),  // Swell: very slow attack, quick decay
];

// -----------------------------------------------------------------------------
// SOUND EFFECTS
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Effects {
    pad: bool,       // Chord mode with long envelope
    chorus: bool,    // Detuned layers
    tremolo: bool,   // Volume modulation
    bulldozer: bool, // Pad + arp shimmer layer
}

// -----------------------------------------------------------------------------
// DSP PRIMITIVES
// -----------------------------------------------------------------------------

/// Biquad IIR filter (12 dB/oct per stage)
#[derive(Clone)]
struct Biquad {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl Biquad {
    fn new() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0,
               x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    fn set_lowpass(&mut self, cutoff: f32, q: f32, sample_rate: f32) {
        let w0 = 2.0 * PI * (cutoff / sample_rate).min(0.49);
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 - cos_w0) / 2.0) / a0;
        self.b1 = (1.0 - cos_w0) / a0;
        self.b2 = self.b0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
              - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// 24 dB/oct low-pass filter (two cascaded biquads, Butterworth alignment)
struct LowPass24 {
    stages: [Biquad; 2],
}

impl LowPass24 {
    fn new() -> Self {
        Self { stages: [Biquad::new(), Biquad::new()] }
    }

    fn set_cutoff(&mut self, cutoff: f32, sample_rate: f32) {
        for stage in &mut self.stages {
            stage.set_lowpass(cutoff, 0.707, sample_rate);
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.stages[0].process(x);
        self.stages[1].process(y)
    }
}

/// Comb filter for Schroeder reverb
struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    damp1: f32,
    damp2: f32,
    prev: f32,
}

impl CombFilter {
    fn new(size: usize, feedback: f32, damping: f32) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback,
            damp1: damping,
            damp2: 1.0 - damping,
            prev: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.prev = output * self.damp2 + self.prev * self.damp1;
        self.buffer[self.index] = input + self.prev * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// Allpass filter for Schroeder reverb
struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
}

impl AllpassFilter {
    fn new(size: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = buffered - input;
        self.buffer[self.index] = input + buffered * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

/// Schroeder reverb: 4 parallel comb filters → 2 series allpass filters
struct SimpleReverb {
    combs: Vec<CombFilter>,
    allpasses: Vec<AllpassFilter>,
}

impl SimpleReverb {
    fn new(sample_rate: f32) -> Self {
        let scale = sample_rate / 44100.0;
        // ~35ms comb delays (prime-ish for reduced coloration)
        let comb_delays = [1557, 1617, 1491, 1422];
        let allpass_delays = [225, 556];

        let combs = comb_delays.iter()
            .map(|&d| CombFilter::new((d as f32 * scale) as usize, 0.84, 0.4))
            .collect();
        let allpasses = allpass_delays.iter()
            .map(|&d| AllpassFilter::new((d as f32 * scale) as usize, 0.5))
            .collect();

        Self { combs, allpasses }
    }

    fn process(&mut self, input: f32) -> f32 {
        let mut output = 0.0;
        for comb in &mut self.combs {
            output += comb.process(input);
        }
        output /= self.combs.len() as f32;
        for ap in &mut self.allpasses {
            output = ap.process(output);
        }
        output
    }
}

/// Stereo chorus: two modulated delay lines with inverted LFO phase
struct StereoChorus {
    buffer_l: Vec<f32>,
    buffer_r: Vec<f32>,
    write_idx: usize,
    base_delay: usize,
    sample_rate: f32,
}

impl StereoChorus {
    fn new(sample_rate: f32) -> Self {
        let base_delay = (sample_rate * 0.003) as usize; // 3ms base delay
        let buf_size = (sample_rate * 0.020) as usize;   // 20ms max
        Self {
            buffer_l: vec![0.0; buf_size.max(1)],
            buffer_r: vec![0.0; buf_size.max(1)],
            write_idx: 0,
            base_delay,
            sample_rate,
        }
    }

    fn process(&mut self, input: f32, time: f32, rate: f32) -> (f32, f32) {
        let buf_len = self.buffer_l.len();
        self.buffer_l[self.write_idx] = input;
        self.buffer_r[self.write_idx] = input;

        // LFO: triangle wave, inverted phase for L/R
        let lfo = (2.0 * PI * rate * time).sin();
        let mod_samples = (self.sample_rate * 0.0015) as f32; // ±1.5ms depth

        let delay_l = self.base_delay as f32 + lfo * mod_samples;
        let delay_r = self.base_delay as f32 - lfo * mod_samples;

        let read_l = (self.write_idx as f32 - delay_l + buf_len as f32) % buf_len as f32;
        let read_r = (self.write_idx as f32 - delay_r + buf_len as f32) % buf_len as f32;

        // Linear interpolation for fractional delay
        let l = lerp_buffer(&self.buffer_l, read_l);
        let r = lerp_buffer(&self.buffer_r, read_r);

        self.write_idx = (self.write_idx + 1) % buf_len;

        // 50/50 wet/dry
        ((input + l) * 0.5, (input + r) * 0.5)
    }
}

fn lerp_buffer(buf: &[f32], pos: f32) -> f32 {
    let len = buf.len();
    let idx0 = pos.floor() as usize % len;
    let idx1 = (idx0 + 1) % len;
    let frac = pos - pos.floor();
    buf[idx0] * (1.0 - frac) + buf[idx1] * frac
}

/// Compute pad filter cutoff — shape-dependent, with LFO movement.
fn pad_filter_cutoff(progress: f32, time: f32, shape: PadShape) -> f32 {
    let lfo = (2.0 * PI * 0.3 * time).sin() * 200.0;

    let base = match shape {
        PadShape::Swell => {
            // Opens with swell, closes with release
            let env = if progress < 0.40 {
                (progress / 0.40 * PI / 2.0).sin()
            } else if progress > 0.65 {
                ((1.0 - progress) / 0.35 * PI / 2.0).sin()
            } else {
                1.0
            };
            400.0 + env * 2000.0
        }
        PadShape::Cascade => {
            // Starts wide open, slowly closes
            let env = (-2.0 * progress).exp();
            600.0 + env * 2400.0
        }
        PadShape::Bloom => {
            // Starts dark, opens progressively to peak, then closes
            let env = if progress < 0.70 {
                let t = progress / 0.70;
                t * t // quadratic opening
            } else {
                ((1.0 - progress) / 0.30 * PI / 2.0).sin()
            };
            300.0 + env * 2200.0
        }
        PadShape::Pulse => {
            // Filter pulses in sync with amplitude
            let pulse = (progress * 2.0 * PI).sin().abs();
            500.0 + pulse * 1500.0
        }
        PadShape::Drift => {
            // Filter wanders
            let lfo2 = (progress * 3.7 * PI).sin() * 600.0;
            1200.0 + lfo2
        }
        PadShape::Stab => {
            // Opens on hit, slowly closes over the tail
            if progress < 0.05 {
                600.0 + (progress / 0.05) * 2400.0
            } else {
                let t = (progress - 0.05) / 0.95;
                3000.0 * (-2.5 * t).exp() + 400.0
            }
        }
    };

    (base + lfo).max(200.0)
}

// -----------------------------------------------------------------------------
// VOICE & MELODY (two-layer hashing)
// -----------------------------------------------------------------------------

/// Pad envelope shapes — each gives a fundamentally different character
#[derive(Debug, Clone, Copy, PartialEq)]
enum PadShape {
    Swell,    // Gentle fade in → sustain → fade out (classic ambient)
    Cascade,  // Full chord hit → long exponential decay (striking a chord)
    Bloom,    // Quiet start → accelerating build → peak → quick release
    Pulse,    // 2–3 rhythmic volume swells (breathing)
    Drift,    // Starts mid, wanders via LFO, gentle ending
    Stab,     // Fast attack → drop to sustain → long filtered tail
}

const PAD_SHAPES: [PadShape; 6] = [
    PadShape::Swell, PadShape::Cascade, PadShape::Bloom,
    PadShape::Pulse, PadShape::Drift, PadShape::Stab,
];

const PAD_SHAPE_NAMES: [&str; 6] = [
    "Swell", "Cascade", "Bloom", "Pulse", "Drift", "Stab",
];

/// Repo determines harmonic identity: key, scale, timbre, note pattern, pad shape
#[derive(Debug, Clone)]
struct RepoVoice {
    root_name: String,
    scale_name: String,
    scale_freqs: [f32; 5],
    octave: f32,
    harmonic_blend: f32,  // 0.05–0.35 (warmth of 2nd harmonic)
    third_harmonic: f32,  // 0.0–0.15 (brightness from 3rd harmonic)
    pattern_idx: usize,   // which arp pattern (repo's melodic signature)
    pad_shape: PadShape,  // envelope character
}

impl RepoVoice {
    fn from_repo(repo: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(repo.as_bytes());
        let hash = hasher.finalize();

        let root_idx = (hash[0] as usize) % 12;
        let root_freq = CHROMATIC_ROOTS[root_idx];
        let root_name = NOTE_NAMES[root_idx].to_string();

        let scale_idx = (hash[1] as usize) % SCALES.len();
        let scale_name = SCALE_NAMES[scale_idx].to_string();
        let intervals = SCALES[scale_idx];

        let octave_idx = (hash[2] as usize) % OCTAVES.len();
        let octave = OCTAVES[octave_idx];

        // Build 5 frequencies from root + scale intervals
        let mut scale_freqs = [0.0f32; 5];
        for (i, &semitones) in intervals.iter().enumerate() {
            scale_freqs[i] = root_freq * 2.0_f32.powf(semitones as f32 / 12.0) * octave;
        }

        // Timbre: harmonic blend 0.05–0.35
        let harmonic_blend = 0.05 + (hash[3] as f32 / 255.0) * 0.30;

        // Third harmonic 0.0–0.15
        let third_harmonic = (hash[4] as f32 / 255.0) * 0.15;

        // Pattern: repo's melodic signature (same notes every time)
        let pattern_idx = (hash[5] as usize) % 8;

        // Pad shape: repo's envelope character
        let pad_shape = PAD_SHAPES[(hash[6] as usize) % PAD_SHAPES.len()];

        Self {
            root_name,
            scale_name,
            scale_freqs,
            octave,
            harmonic_blend,
            third_harmonic,
            pattern_idx,
            pad_shape,
        }
    }
}

/// Branch determines timing/rhythm: how notes enter, swing, modulation
#[derive(Debug, Clone)]
struct BranchMelody {
    swing: f32,              // 0.0–0.3
    envelope_shape: usize,   // index into ENVELOPE_SHAPES
    chorus_detune: f32,      // 4.0–16.0 cents
    tremolo_rate: f32,       // 3.0–9.0 Hz
    tremolo_depth: f32,      // 0.15–0.45
    interval_spread: f32,    // 0.8–1.4 multiplier on scale degree offsets
    stagger_offsets: [f32; 5], // per-note entry times (0.0–1.0, normalized)
    attack_variation: f32,   // 0.0–0.15 per-note attack time spread
}

impl BranchMelody {
    fn from_branch(branch: &str, _steps: u8) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(branch.as_bytes());
        let hash = hasher.finalize();

        // Swing: 0.0–0.3
        let swing = (hash[0] as f32 / 255.0) * 0.3;

        let envelope_shape = (hash[1] as usize) % ENVELOPE_SHAPES.len();

        // Chorus detune: 4.0–16.0 cents
        let chorus_detune = 4.0 + (hash[2] as f32 / 255.0) * 12.0;

        // Tremolo rate: 3.0–9.0 Hz
        let tremolo_rate = 3.0 + (hash[3] as f32 / 255.0) * 6.0;

        // Tremolo depth: 0.15–0.45
        let tremolo_depth = 0.15 + (hash[3] as f32 / 255.0) * 0.30;

        // Interval spread: 0.8–1.4
        let interval_spread = 0.8 + (hash[4] as f32 / 255.0) * 0.6;

        // Per-note stagger: hash bytes 5–9 give non-uniform entry offsets.
        // Sorted so notes still enter in order, but gaps between them vary.
        let mut raw_offsets: Vec<f32> = (0..5)
            .map(|i| hash[5 + i] as f32 / 255.0)
            .collect();
        raw_offsets.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // First note always at 0, rest spread across 0.0–0.35 of phrase
        let stagger_span = 0.12 + (hash[10] as f32 / 255.0) * 0.23; // 0.12–0.35
        let mut stagger_offsets = [0.0f32; 5];
        for i in 0..5 {
            stagger_offsets[i] = raw_offsets[i] * stagger_span;
        }
        // Normalize: first note at 0
        let min = stagger_offsets[0];
        for offset in &mut stagger_offsets {
            *offset -= min;
        }

        // Attack variation: 0.0–0.15 (how much attack time differs per note)
        let attack_variation = (hash[11] as f32 / 255.0) * 0.15;

        Self {
            swing,
            envelope_shape,
            chorus_detune,
            tremolo_rate,
            tremolo_depth,
            interval_spread,
            stagger_offsets,
            attack_variation,
        }
    }
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
    voice: RepoVoice,
    melody: BranchMelody,
}

impl PhraseParams {
    fn from_identity(repo: &str, branch: &str, total_duration: u64, volume: f32, effects: Effects, steps: u8) -> Self {
        let voice = RepoVoice::from_repo(repo);
        let melody = BranchMelody::from_branch(branch, steps);

        // Notes from repo's pattern + scale (repo = identity),
        // interval_spread from branch adds subtle reharmonization
        let notes: Vec<f32> = if steps >= 5 {
            let pattern = PATTERNS_5[voice.pattern_idx];
            pattern.iter().map(|&offset| {
                let spread_offset = (offset as f32 * melody.interval_spread).round() as i32;
                let idx = spread_offset.rem_euclid(5) as usize;
                voice.scale_freqs[idx]
            }).collect()
        } else {
            let pattern = PATTERNS_3[voice.pattern_idx];
            pattern.iter().map(|&offset| {
                let spread_offset = (offset as f32 * melody.interval_spread).round() as i32;
                let idx = spread_offset.rem_euclid(5) as usize;
                voice.scale_freqs[idx]
            }).collect()
        };

        Self {
            notes,
            total_duration,
            volume,
            effects,
            voice,
            melody,
        }
    }
}

// -----------------------------------------------------------------------------
// MAIN
// -----------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Hook) => run_hook(),
        Some(Command::Init) => run_init(),
        Some(Command::Play(args)) => run_play(args),
        None => run_play(cli.play_args),
    }
}

fn run_play(args: PlayArgs) -> Result<()> {
    let PlayArgs { branch, repo, duration, volume, pad, chorus, tremolo, bulldozer, steps, dry_run, quiet } = args;
    let branch = match branch {
        Some(b) => b,
        None => get_current_branch()
            .context("No branch specified and couldn't detect current git branch")?,
    };

    let repo = match repo {
        Some(r) => r,
        None => get_repo_name().unwrap_or_else(|_| "unknown".to_string()),
    };

    let effects = Effects {
        pad: pad || bulldozer,
        chorus: chorus || bulldozer,
        tremolo,
        bulldozer,
    };

    // Pad/bulldozer mode benefits from longer duration
    let duration = if (pad || bulldozer) && duration == 600 {
        1000  // Default to 1000ms for pad
    } else {
        duration
    };

    let params = PhraseParams::from_identity(&repo, &branch, duration, volume, effects, steps);

    // Print info
    if !quiet {
        let mode = if bulldozer { " [bulldozer]" }
            else if pad { " [pad]" }
            else if chorus { " [chorus]" }
            else if tremolo { " [tremolo]" }
            else { " [arpeggio]" };
        let envelope_names = ["Punchy", "Soft", "Pluck", "Swell"];
        println!("🎵 Repo: {} | Branch: {}{}", repo, branch, mode);
        let shape_name = PAD_SHAPE_NAMES[PAD_SHAPES.iter().position(|s| *s == params.voice.pad_shape).unwrap_or(0)];
        println!("   Key: {} {} | Octave: {}x | Shape: {}", params.voice.root_name, params.voice.scale_name, params.voice.octave, shape_name);
        println!("   Timbre: harmonic={:.2}, 3rd={:.2}", params.voice.harmonic_blend, params.voice.third_harmonic);
        println!("   Notes: {:?}", params.notes.iter().map(|f| format!("{:.0}Hz", f)).collect::<Vec<_>>());
        println!("   Pattern: #{} | Envelope: {} | Swing: {:.0}%",
            params.voice.pattern_idx, envelope_names[params.melody.envelope_shape], params.melody.swing * 100.0);
        println!("   Stagger: [{:.2}, {:.2}, {:.2}, {:.2}, {:.2}]",
            params.melody.stagger_offsets[0], params.melody.stagger_offsets[1],
            params.melody.stagger_offsets[2], params.melody.stagger_offsets[3],
            params.melody.stagger_offsets[4]);
        println!("   Spread: {:.2} | Duration: {}ms", params.melody.interval_spread, params.total_duration);
        if chorus { println!("   + Chorus (detune: {:.1} cents)", params.melody.chorus_detune); }
        if tremolo { println!("   + Tremolo ({:.1}Hz, {:.0}% depth)", params.melody.tremolo_rate, params.melody.tremolo_depth * 100.0); }
    }

    if dry_run {
        return Ok(());
    }

    play_phrase(&params)?;

    Ok(())
}

// -----------------------------------------------------------------------------
// HOOK SUBCOMMAND
// -----------------------------------------------------------------------------

fn run_hook() -> Result<()> {
    // Read stdin JSON from Claude Code hook, extract cwd, detect branch/repo, play tone.
    // Never fails — every fallible op is silently absorbed so we never block Claude Code.

    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&input) {
        let cwd = json.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
        let _ = std::env::set_current_dir(cwd);
    }

    let branch = get_current_branch().unwrap_or_else(|_| "claude".to_string());
    let repo = get_repo_name().unwrap_or_else(|_| "unknown".to_string());

    let args = PlayArgs {
        branch: Some(branch),
        repo: Some(repo),
        duration: 3000,
        volume: 0.35,
        pad: false,
        chorus: false,
        tremolo: false,
        bulldozer: true,
        steps: 5,
        dry_run: false,
        quiet: true,
    };

    let _ = run_play(args);
    Ok(())
}

// -----------------------------------------------------------------------------
// INIT SUBCOMMAND
// -----------------------------------------------------------------------------

fn run_init() -> Result<()> {
    let home = dirs::home_dir().context("Could not determine home directory")?;

    // Clean up old hook.sh if it exists
    let old_hook = home.join(".config").join("branch-tone").join("hook.sh");
    if old_hook.exists() {
        let _ = std::fs::remove_file(&old_hook);
    }

    // Read/create ~/.claude/settings.json and merge hooks
    let claude_dir = home.join(".claude");
    std::fs::create_dir_all(&claude_dir)
        .with_context(|| format!("Failed to create {}", claude_dir.display()))?;

    let settings_path = claude_dir.join("settings.json");
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read {}", settings_path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", settings_path.display()))?
    } else {
        serde_json::json!({})
    };

    let hooks = settings
        .as_object_mut()
        .context("settings.json is not an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("hooks is not an object")?;

    let hook_command = "branch-tone hook";
    let mut hooks_added = 0;
    let mut hooks_migrated = 0;

    // New format: each event is an array of matcher groups, each with a "hooks" array
    // e.g. {"Stop": [{"hooks": [{"type": "command", "command": "branch-tone hook"}]}]}
    let new_hook_entry = serde_json::json!({
        "hooks": [{"type": "command", "command": hook_command}]
    });

    for event in ["SessionStart", "Stop", "PermissionRequest"] {
        let event_hooks = hooks
            .entry(event)
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .with_context(|| format!("hooks.{} is not an array", event))?;

        // Migrate old-format entries (flat {type, command}) to new format ({hooks: [{type, command}]})
        let mut had_old_format = false;
        event_hooks.retain(|entry| {
            // Old format: {"type": "command", "command": "..."} at top level
            let is_old = entry.get("type").is_some() && entry.get("hooks").is_none();
            if is_old {
                let cmd = entry.get("command").and_then(|c| c.as_str()).unwrap_or("");
                if cmd.contains("branch-tone") || cmd.contains("hook.sh") {
                    had_old_format = true;
                    return false; // remove old format, we'll re-add in new format
                }
            }
            // Also remove old hook.sh references in new format
            if let Some(inner_hooks) = entry.get("hooks").and_then(|h| h.as_array()) {
                let has_old_ref = inner_hooks.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()).unwrap_or("").contains("hook.sh")
                });
                if has_old_ref { return false; }
            }
            true
        });
        if had_old_format {
            hooks_migrated += 1;
        }

        // Check if already present in new format
        let already_present = event_hooks.iter().any(|entry| {
            entry.get("hooks").and_then(|h| h.as_array()).map_or(false, |arr| {
                arr.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()) == Some(hook_command)
                })
            })
        });

        if !already_present {
            event_hooks.push(new_hook_entry.clone());
            hooks_added += 1;
        }
    }

    let json_str = serde_json::to_string_pretty(&settings)
        .context("Failed to serialize settings.json")?;
    std::fs::write(&settings_path, format!("{}\n", json_str))
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;

    if hooks_migrated > 0 {
        println!("✓ Migrated {} hook(s) to new format", hooks_migrated);
    }
    if hooks_added > 0 {
        println!("✓ Added {} hook(s) to {}", hooks_added, settings_path.display());
    } else if hooks_migrated == 0 {
        println!("✓ Hooks already present in {}", settings_path.display());
    }

    println!("\nbranch-tone is ready! Claude Code will play tones on:");
    println!("  • SessionStart    — when you open a session");
    println!("  • Stop            — when Claude finishes responding");
    println!("  • PermissionRequest — when a permission dialog appears");

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
    let voice = params.voice.clone();
    let melody = params.melody.clone();

    let total_samples = (sample_rate * total_duration as f32 / 1000.0) as usize;

    let sample_clock = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sample_clock_clone = sample_clock.clone();

    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_clone = finished.clone();

    // DSP processing state
    let mut pad_lpf = LowPass24::new();
    let mut reverb = SimpleReverb::new(sample_rate);
    let mut chorus = StereoChorus::new(sample_rate);
    let chorus_rate = 0.6; // Hz — slow Juno-style modulation

    // For bulldozer: arp uses same notes but doesn't drop an octave
    // (the pad generator drops internally), so arp sits an octave above
    let arp_effects = Effects { pad: false, chorus: true, tremolo: false, bulldozer: false };

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

                // Generate raw audio
                let raw = if effects.bulldozer {
                    let pad_out = generate_pad(&notes, time, progress, 1.0, effects, &voice, &melody);
                    let arp_out = generate_arpeggio(&notes, time, current_sample, total_samples, 1.0, arp_effects, &voice, &melody);
                    (pad_out * 0.7 + arp_out * 0.3) * volume
                } else if effects.pad {
                    generate_pad(&notes, time, progress, volume, effects, &voice, &melody)
                } else {
                    generate_arpeggio(&notes, time, current_sample, total_samples, volume, effects, &voice, &melody)
                };

                // Low-pass filter with envelope (pad/bulldozer modes)
                let filtered = if effects.pad {
                    let cutoff = pad_filter_cutoff(progress, time, voice.pad_shape);
                    pad_lpf.set_cutoff(cutoff, sample_rate);
                    pad_lpf.process(raw)
                } else {
                    raw
                };

                // Reverb (all modes — lighter for arpeggio)
                let wet = reverb.process(filtered);
                let reverb_mix = if effects.pad { 0.30 } else { 0.15 };
                let with_reverb = filtered * (1.0 - reverb_mix) + wet * reverb_mix;

                // Stereo chorus (pad/bulldozer modes get BBD-style stereo)
                if channels >= 2 && effects.pad {
                    let (left, right) = chorus.process(with_reverb, time, chorus_rate);
                    for (ch, channel_sample) in frame.iter_mut().enumerate() {
                        let s = if ch % 2 == 0 { left } else { right };
                        *channel_sample = T::from_sample(s);
                    }
                } else {
                    let sample = T::from_sample(with_reverb);
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = sample;
                    }
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

/// Per-note amplitude envelope based on the repo's pad shape
fn pad_note_envelope(shape: PadShape, progress: f32, note_idx: usize, melody: &BranchMelody) -> f32 {
    match shape {
        PadShape::Swell => {
            // Gentle fade in → sustain → fade out
            let attack = 0.35;
            let release = 0.40;
            if progress < attack {
                (progress / attack * PI / 2.0).sin()
            } else if progress > (1.0 - release) {
                ((1.0 - progress) / release * PI / 2.0).sin()
            } else {
                1.0
            }
        }
        PadShape::Cascade => {
            // Full hit → long exponential decay (like striking a chord)
            let attack = 0.03 + note_idx as f32 * 0.02; // notes hit in quick succession
            if progress < attack {
                (progress / attack * PI / 2.0).sin()
            } else {
                // Exponential decay from 1.0 toward 0.15 over remaining time
                let decay_progress = (progress - attack) / (1.0 - attack);
                0.15 + 0.85 * (-3.0 * decay_progress).exp()
            }
        }
        PadShape::Bloom => {
            // Quiet start → accelerating build → peak at 70% → quick release
            let peak = 0.70;
            let release = 0.25;
            if progress < peak {
                // Quadratic curve — slow at first, accelerates
                let t = progress / peak;
                t * t
            } else if progress > (1.0 - release) {
                ((1.0 - progress) / release * PI / 2.0).sin()
            } else {
                1.0
            }
        }
        PadShape::Pulse => {
            // 2–3 rhythmic swells — like the pad is breathing
            let num_pulses = 2.0 + (melody.swing * 3.3).floor(); // 2 or 3 based on swing
            let pulse = (progress * num_pulses * PI).sin().abs();
            // Gentle overall fade at the very end
            let fade = if progress > 0.85 {
                ((1.0 - progress) / 0.15 * PI / 2.0).sin()
            } else {
                1.0
            };
            pulse * fade
        }
        PadShape::Drift => {
            // Starts at ~60%, wanders up/down via LFO, gentle ending
            let base = 0.6;
            // Two LFOs at different rates for complex wandering
            let lfo1 = (progress * 2.5 * PI).sin() * 0.3;
            let lfo2 = (progress * 4.1 * PI + note_idx as f32).sin() * 0.15;
            let drift = (base + lfo1 + lfo2).clamp(0.1, 1.0);
            // Fade at start and end
            let fade_in = (progress * 8.0).min(1.0);
            let fade_out = if progress > 0.80 {
                ((1.0 - progress) / 0.20 * PI / 2.0).sin()
            } else {
                1.0
            };
            drift * fade_in * fade_out
        }
        PadShape::Stab => {
            // Fast attack → quick drop to 35% → long sustain tail
            let attack = 0.04;
            let drop_end = 0.15;
            let sustain = 0.35;
            let release = 0.30;
            if progress < attack {
                progress / attack
            } else if progress < drop_end {
                let t = (progress - attack) / (drop_end - attack);
                1.0 - t * (1.0 - sustain)
            } else if progress > (1.0 - release) {
                sustain * ((1.0 - progress) / release * PI / 2.0).sin()
            } else {
                sustain
            }
        }
    }
}

fn generate_pad(notes: &[f32], time: f32, progress: f32, volume: f32, _effects: Effects, voice: &RepoVoice, melody: &BranchMelody) -> f32 {
    let mut sample = 0.0;

    for (i, &freq) in notes.iter().enumerate() {
        // Branch-derived stagger: each note enters at its own time
        let entry_point = if i < melody.stagger_offsets.len() {
            melody.stagger_offsets[i]
        } else {
            0.0
        };

        if progress < entry_point {
            continue;
        }

        // Per-note progress relative to its entry
        let note_progress = (progress - entry_point) / (1.0 - entry_point);

        // Shape-driven envelope with branch attack variation
        let base_env = pad_note_envelope(voice.pad_shape, note_progress, i, melody);
        // Branch adds subtle per-note attack variation
        let variation = melody.attack_variation * (i as f32 - 2.0).abs() * 0.5;
        let note_env = if note_progress < variation {
            base_env * (note_progress / variation).min(1.0)
        } else {
            base_env
        };

        // Drop less aggressively — warmth without muddiness
        let base_freq = freq * 0.65;

        // Tight detuning: 3 saw voices per note
        let detune_cents = melody.chorus_detune * 0.15;
        let detune_offsets = [-detune_cents, 0.0, detune_cents];

        for (j, &cents) in detune_offsets.iter().enumerate() {
            let f = base_freq * 2.0_f32.powf(cents / 1200.0);
            let phase_offset = (i as f32 + j as f32) * 0.7;

            let saw_phase = f * time + phase_offset;
            let saw = 2.0 * (saw_phase - saw_phase.floor()) - 1.0;

            sample += saw * note_env / 3.0;
        }

        // Sub layer — only on the first note for clean low end
        if i == 0 {
            let sub = (2.0 * PI * base_freq * 0.5 * time).sin() * 0.15 * note_env;
            sample += sub;
        }
    }

    sample /= notes.len() as f32;

    // Gentle breathing
    let breath_rate = 0.03 + (melody.tremolo_rate - 3.0) * 0.003;
    let breath = 1.0 - 0.08 * (0.5 + 0.5 * (2.0 * PI * breath_rate * time).sin());
    sample *= breath;

    sample * volume
}

fn generate_arpeggio(notes: &[f32], time: f32, current_sample: usize, total_samples: usize, volume: f32, effects: Effects, voice: &RepoVoice, melody: &BranchMelody) -> f32 {
    let num_notes = notes.len();

    // Calculate swing-adjusted note boundaries
    let base_per_note = total_samples as f32 / num_notes as f32;
    let mut boundaries = Vec::with_capacity(num_notes + 1);
    boundaries.push(0usize);
    let mut accum = 0.0f32;
    for i in 0..num_notes {
        let factor = if i % 2 == 0 { 1.0 + melody.swing } else { 1.0 - melody.swing };
        accum += base_per_note * factor;
        boundaries.push(accum.round() as usize);
    }
    *boundaries.last_mut().unwrap() = total_samples;

    // Ethereal arpeggio: notes ring out and overlap rather than cutting off.
    // Each note triggers at its boundary and decays exponentially over remaining time.
    let mut sample = 0.0f32;
    let (attack_frac, _) = ENVELOPE_SHAPES[melody.envelope_shape];

    for i in 0..num_notes {
        let note_start = boundaries[i];
        if current_sample < note_start {
            continue; // Note hasn't triggered yet
        }

        let samples_since_trigger = current_sample - note_start;
        let note_slot_len = boundaries[i + 1] - boundaries[i];
        let frequency = notes[i];

        // Attack: ramp up at the start of each note
        let attack_samples = (note_slot_len as f32 * attack_frac) as usize;
        let attack_env = if attack_samples > 0 && samples_since_trigger < attack_samples {
            samples_since_trigger as f32 / attack_samples as f32
        } else {
            1.0
        };

        // Exponential decay — notes ring out well past their slot boundary
        // Decay rate: reaches ~5% amplitude after ~4x the note slot length
        let decay_time = samples_since_trigger as f32 / note_slot_len as f32;
        let ring_env = (-1.0 * decay_time).exp();

        let env = attack_env * ring_env;

        // Skip notes that have decayed to inaudible
        if env < 0.005 {
            continue;
        }

        let osc = generate_oscillator(frequency, time, effects.chorus, i, voice, melody);
        sample += osc * env;
    }

    // Normalize to prevent clipping from overlapping notes
    sample *= 0.7;

    let sample = if effects.tremolo {
        apply_tremolo(sample, time, melody)
    } else {
        sample
    };

    // Global fade-out over last 15% for smooth ending
    let fade_start = 0.85;
    let progress = current_sample as f32 / total_samples as f32;
    let global_fade = if progress > fade_start {
        ((1.0 - progress) / (1.0 - fade_start)).sqrt()
    } else {
        1.0
    };

    sample * global_fade * volume
}

fn generate_oscillator(freq: f32, time: f32, chorus: bool, voice_idx: usize, voice: &RepoVoice, melody: &BranchMelody) -> f32 {
    // Sub-octave layer for warmth and depth (always present)
    let sub = (2.0 * PI * freq * 0.5 * time).sin() * 0.15;

    // Slow shimmer: gentle pitch wobble for ethereal quality
    let shimmer_rate = 2.5 + voice_idx as f32 * 0.3;
    let shimmer = 1.0 + 0.003 * (2.0 * PI * shimmer_rate * time).sin();
    let freq = freq * shimmer;

    if chorus {
        let detune = melody.chorus_detune;
        let detune_cents = [0.0, -detune, detune, -detune * 0.5, detune * 0.5];
        let num_voices = detune_cents.len() as f32;
        let mut sample = 0.0;
        for (i, &cents) in detune_cents.iter().enumerate() {
            let detune_factor = 2.0_f32.powf(cents / 1200.0);
            let f = freq * detune_factor;
            let phase_offset = (voice_idx as f32 + i as f32) * 0.1;
            let fundamental = (2.0 * PI * f * time + phase_offset).sin();
            let h2 = (2.0 * PI * f * 2.0 * time + phase_offset).sin() * voice.harmonic_blend;
            let h3 = (2.0 * PI * f * 3.0 * time + phase_offset).sin() * voice.third_harmonic;
            sample += (fundamental + h2 + h3) / num_voices;
        }
        sample + sub
    } else {
        // Even without chorus flag, use a light 2-voice detune for spaciousness
        let light_detune = melody.chorus_detune * 0.3;
        let f1 = freq * 2.0_f32.powf(-light_detune / 1200.0);
        let f2 = freq * 2.0_f32.powf(light_detune / 1200.0);

        let s1 = (2.0 * PI * f1 * time).sin()
            + (2.0 * PI * f1 * 2.0 * time).sin() * voice.harmonic_blend
            + (2.0 * PI * f1 * 3.0 * time).sin() * voice.third_harmonic;
        let s2 = (2.0 * PI * f2 * time).sin()
            + (2.0 * PI * f2 * 2.0 * time).sin() * voice.harmonic_blend
            + (2.0 * PI * f2 * 3.0 * time).sin() * voice.third_harmonic;

        (s1 + s2) * 0.5 + sub
    }
}

fn apply_tremolo(sample: f32, time: f32, melody: &BranchMelody) -> f32 {
    let tremolo = 1.0 - melody.tremolo_depth * (0.5 + 0.5 * (2.0 * PI * melody.tremolo_rate * time).sin());
    sample * tremolo
}

// -----------------------------------------------------------------------------
// TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_effects() -> Effects {
        Effects { pad: false, chorus: false, tremolo: false, bulldozer: false }
    }

    // -- Determinism: same input always produces same output --

    #[test]
    fn same_identity_same_notes() {
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3);
        let b = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3);
        assert_eq!(a.notes, b.notes);
    }

    #[test]
    fn different_branch_different_notes() {
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3);
        let b = PhraseParams::from_identity("myrepo", "feature/auth", 400, 0.25, default_effects(), 3);
        assert_ne!(a.notes, b.notes);
    }

    #[test]
    fn different_repo_different_notes() {
        let a = PhraseParams::from_identity("repo-a", "main", 400, 0.25, default_effects(), 3);
        let b = PhraseParams::from_identity("repo-b", "main", 400, 0.25, default_effects(), 3);
        assert_ne!(a.notes, b.notes);
    }

    // -- Two-layer hashing: repo controls voice, branch controls melody --

    #[test]
    fn same_repo_shares_voice() {
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3);
        let b = PhraseParams::from_identity("myrepo", "feature/x", 400, 0.25, default_effects(), 3);
        assert_eq!(a.voice.scale_freqs, b.voice.scale_freqs);
        assert_eq!(a.voice.octave, b.voice.octave);
        assert_eq!(a.voice.harmonic_blend, b.voice.harmonic_blend);
    }

    #[test]
    fn same_branch_shares_melody() {
        let a = PhraseParams::from_identity("repo-a", "main", 400, 0.25, default_effects(), 3);
        let b = PhraseParams::from_identity("repo-b", "main", 400, 0.25, default_effects(), 3);
        assert_eq!(a.melody.swing, b.melody.swing);
        assert_eq!(a.melody.envelope_shape, b.melody.envelope_shape);
        assert_eq!(a.melody.stagger_offsets, b.melody.stagger_offsets);
    }

    // -- Note count matches step parameter --

    #[test]
    fn three_steps_produces_three_notes() {
        let p = PhraseParams::from_identity("r", "b", 400, 0.25, default_effects(), 3);
        assert_eq!(p.notes.len(), 3);
    }

    #[test]
    fn five_steps_produces_five_notes() {
        let p = PhraseParams::from_identity("r", "b", 400, 0.25, default_effects(), 5);
        assert_eq!(p.notes.len(), 5);
    }

    // -- All notes land on valid scale frequencies --

    #[test]
    fn notes_are_valid_scale_frequencies() {
        for branch in ["main", "develop", "feature/x", "fix/bug-123", "release/v2"] {
            let p = PhraseParams::from_identity("repo", branch, 400, 0.25, default_effects(), 5);
            for note in &p.notes {
                assert!(
                    p.voice.scale_freqs.iter().any(|v| (v - note).abs() < 0.01),
                    "Note {:.2}Hz is not in the repo's scale {:?}",
                    note, p.voice.scale_freqs
                );
            }
        }
    }

    // -- Oscillator produces signal in reasonable range --

    #[test]
    fn oscillator_output_in_range() {
        let voice = RepoVoice::from_repo("test");
        let melody = BranchMelody::from_branch("test", 3);
        for t in 0..1000 {
            let time = t as f32 / 44100.0;
            let sample = generate_oscillator(440.0, time, false, 0, &voice, &melody);
            assert!(sample >= -1.5 && sample <= 1.5, "sample out of range: {}", sample);
        }
    }

    #[test]
    fn chorus_oscillator_output_in_range() {
        let voice = RepoVoice::from_repo("test");
        let melody = BranchMelody::from_branch("test", 3);
        for t in 0..1000 {
            let time = t as f32 / 44100.0;
            let sample = generate_oscillator(440.0, time, true, 0, &voice, &melody);
            assert!(sample >= -1.5 && sample <= 1.5, "chorus sample out of range: {}", sample);
        }
    }

    // -- Tremolo modulates but doesn't invert --

    #[test]
    fn tremolo_stays_positive_for_positive_input() {
        let melody = BranchMelody::from_branch("test", 3);
        for t in 0..44100 {
            let time = t as f32 / 44100.0;
            let result = apply_tremolo(1.0, time, &melody);
            assert!(result > 0.0, "tremolo went negative at t={}: {}", time, result);
            assert!(result <= 1.0, "tremolo exceeded input at t={}: {}", time, result);
        }
    }

    // -- Pad envelope shape --

    #[test]
    fn pad_envelope_rises_and_falls() {
        let notes = vec![440.0];
        let effects = Effects { pad: true, chorus: false, tremolo: false, bulldozer: false };
        let voice = RepoVoice::from_repo("test");
        let melody = BranchMelody::from_branch("test", 3);

        let start = generate_pad(&notes, 0.0, 0.01, 1.0, effects, &voice, &melody).abs();
        let mid = generate_pad(&notes, 0.5, 0.5, 1.0, effects, &voice, &melody).abs();
        let end = generate_pad(&notes, 1.0, 0.99, 1.0, effects, &voice, &melody).abs();

        assert!(mid > start, "pad should be louder in middle than at start");
        assert!(mid > end, "pad should be louder in middle than at end");
    }

    // -- Hook format validation (Claude Code settings.json) --

    /// Helper: simulate run_init logic on an in-memory settings value
    fn apply_hook_init(settings: &mut serde_json::Value) {
        let hooks = settings
            .as_object_mut().unwrap()
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut().unwrap();

        let hook_command = "branch-tone hook";
        let new_hook_entry = serde_json::json!({
            "hooks": [{"type": "command", "command": hook_command}]
        });

        for event in ["SessionStart", "Stop", "PermissionRequest"] {
            let event_hooks = hooks
                .entry(event)
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut().unwrap();

            // Migrate old format
            event_hooks.retain(|entry| {
                let is_old = entry.get("type").is_some() && entry.get("hooks").is_none();
                if is_old {
                    let cmd = entry.get("command").and_then(|c| c.as_str()).unwrap_or("");
                    if cmd.contains("branch-tone") || cmd.contains("hook.sh") {
                        return false;
                    }
                }
                if let Some(inner_hooks) = entry.get("hooks").and_then(|h| h.as_array()) {
                    let has_old_ref = inner_hooks.iter().any(|h| {
                        h.get("command").and_then(|c| c.as_str()).unwrap_or("").contains("hook.sh")
                    });
                    if has_old_ref { return false; }
                }
                true
            });

            let already_present = event_hooks.iter().any(|entry| {
                entry.get("hooks").and_then(|h| h.as_array()).map_or(false, |arr| {
                    arr.iter().any(|h| {
                        h.get("command").and_then(|c| c.as_str()) == Some(hook_command)
                    })
                })
            });

            if !already_present {
                event_hooks.push(new_hook_entry.clone());
            }
        }
    }

    #[test]
    fn hook_init_writes_new_format() {
        let mut settings = serde_json::json!({});
        apply_hook_init(&mut settings);

        for event in ["SessionStart", "Stop", "PermissionRequest"] {
            let event_hooks = settings["hooks"][event].as_array()
                .unwrap_or_else(|| panic!("hooks.{} should be an array", event));
            assert_eq!(event_hooks.len(), 1, "{} should have exactly one matcher group", event);

            let group = &event_hooks[0];
            let inner = group["hooks"].as_array()
                .unwrap_or_else(|| panic!("{} matcher group should have hooks array", event));
            assert_eq!(inner.len(), 1);
            assert_eq!(inner[0]["type"], "command");
            assert_eq!(inner[0]["command"], "branch-tone hook");
        }
    }

    #[test]
    fn hook_init_is_idempotent() {
        let mut settings = serde_json::json!({});
        apply_hook_init(&mut settings);
        apply_hook_init(&mut settings); // run twice

        for event in ["SessionStart", "Stop", "PermissionRequest"] {
            let event_hooks = settings["hooks"][event].as_array().unwrap();
            assert_eq!(event_hooks.len(), 1, "{} should not duplicate on re-init", event);
        }
    }

    #[test]
    fn hook_init_migrates_old_format() {
        // Old format: flat entries with type+command at top level
        let mut settings = serde_json::json!({
            "hooks": {
                "Stop": [
                    {"type": "command", "command": "branch-tone hook"}
                ],
                "PermissionRequest": [
                    {"type": "command", "command": "branch-tone hook"}
                ]
            }
        });
        apply_hook_init(&mut settings);

        for event in ["SessionStart", "Stop", "PermissionRequest"] {
            let event_hooks = settings["hooks"][event].as_array().unwrap();
            assert_eq!(event_hooks.len(), 1, "{} should have one entry after migration", event);
            // Verify it's new format (has "hooks" key, not flat "type")
            assert!(event_hooks[0].get("hooks").is_some(),
                "{} entry should be new format with 'hooks' key", event);
            assert!(event_hooks[0].get("type").is_none(),
                "{} entry should not have top-level 'type' (old format)", event);
        }
    }

    #[test]
    fn hook_init_removes_old_hook_sh_refs() {
        let mut settings = serde_json::json!({
            "hooks": {
                "Stop": [
                    {"hooks": [{"type": "command", "command": "/Users/me/.config/branch-tone/hook.sh"}]}
                ]
            }
        });
        apply_hook_init(&mut settings);

        let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 1);
        assert_eq!(stop_hooks[0]["hooks"][0]["command"], "branch-tone hook");
    }

    #[test]
    fn hook_init_preserves_unrelated_hooks() {
        let mut settings = serde_json::json!({
            "hooks": {
                "Stop": [
                    {"hooks": [{"type": "command", "command": "some-other-tool"}]}
                ]
            }
        });
        apply_hook_init(&mut settings);

        let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_hooks.len(), 2, "should preserve existing hook and add ours");
        // Verify the other tool is still there
        let commands: Vec<&str> = stop_hooks.iter().flat_map(|entry| {
            entry["hooks"].as_array().unwrap().iter().filter_map(|h| {
                h["command"].as_str()
            })
        }).collect();
        assert!(commands.contains(&"some-other-tool"));
        assert!(commands.contains(&"branch-tone hook"));
    }
}
