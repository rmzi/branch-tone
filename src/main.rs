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

    /// Spooky mode: thin sines, dark filter, eerie resonance (halloween vibes)
    #[arg(long)]
    spooky: bool,

    /// Reverse note order (descending instead of ascending)
    #[arg(long)]
    reverse: bool,

    /// Randomize note selection (stays in key)
    #[arg(long, hide = true)]
    randomize: bool,

    /// Drum break mode (used by hook for session events)
    #[arg(long, hide = true)]
    drums: bool,

    /// Dub tape delay effect (used by hook for tonal events)
    #[arg(long, hide = true)]
    dub_delay: bool,

    /// Layer melody over drums (used by hook for hybrid events)
    #[arg(long, hide = true)]
    melody_over_drums: bool,

    /// Single drum hit mode (used by hook for short percussive events)
    #[arg(long, hide = true)]
    single_hit: bool,

    /// Event category for melodic variation (not a CLI arg)
    #[arg(skip)]
    event_category: EventCategory,

    /// Per-event seed: rotates pattern/hit so each hook sounds distinct (not a CLI arg)
    #[arg(skip)]
    event_seed: u8,

    /// Override drum break pattern (0=Amen, 1=Think, 2=Funky Drummer, 3=Apache,
    /// 4=Skull Snaps, 5=One Drop, 6=Steppers, 7=Rockers, 8=Dancehall, 9=Two-Step)
    #[arg(long, hide = true)]
    break_pattern: Option<usize>,

    /// Suppress informational output (used by hook)
    #[arg(long, hide = true)]
    quiet: bool,
}

#[derive(clap::Args, Debug)]
struct TestArgs {
    /// Git repo path to test all hook sounds for (default: current directory)
    #[arg(value_name = "PATH", default_value = ".")]
    path: String,

    /// Spooky mode: thin sines, dark filter, eerie resonance
    #[arg(long)]
    spooky: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Play a tone for a branch (default behavior)
    Play(PlayArgs),

    /// Read Claude Code hook JSON from stdin, detect branch, play tone
    Hook,

    /// Register Claude Code hooks
    Init {
        /// Installation scope: user (default), project, or local
        #[arg(long, default_value = "user")]
        scope: String,
        /// Use legacy direct settings.json patching instead of plugin system
        #[arg(long)]
        legacy: bool,
    },

    /// Alias for init — update hooks in settings.json
    Update,

    /// Remove legacy hooks from settings.json and install the plugin
    LegacyCleanup {
        /// Installation scope: user (default), project, or local
        #[arg(long, default_value = "user")]
        scope: String,
    },

    /// Test all hook sounds for a git repo
    Test(TestArgs),

    /// Interactive step sequencer — toggle drum hits in real-time
    Player {
        /// Starting break pattern (0=Amen, 1=Think, 2=Funky Drummer, 3=Apache,
        /// 4=Skull Snaps, 5=One Drop, 6=Steppers, 7=Rockers, 8=Dancehall, 9=Two-Step)
        #[arg(long, default_value_t = 0)]
        pattern: usize,
        /// Starting BPM (default: pattern's native BPM)
        #[arg(long)]
        bpm: Option<u16>,
    },
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

/// Scale intervals in semitones from root (5 notes each)
const SCALES: [[u8; 5]; 10] = [
    [0, 2, 4, 7, 9],    // Major pentatonic
    [0, 3, 5, 7, 10],   // Minor pentatonic
    [0, 2, 3, 7, 9],    // Dorian (penta subset)
    [0, 2, 4, 6, 9],    // Lydian (penta subset)
    [0, 2, 4, 7, 10],   // Mixolydian (penta subset)
    [0, 3, 5, 8, 10],   // Minor (penta subset)
    [0, 4, 7, 11, 14],  // Major 7th (spread voicing, dance-classic)
    [0, 3, 7, 10, 14],  // Minor 9th (deep house)
    [0, 5, 7, 12, 14],  // Sus4 add9 (open, airy)
    [0, 2, 7, 9, 12],   // Sus2 octave (floating)
];

/// Scale names for display
const SCALE_NAMES: [&str; 10] = [
    "Major Penta", "Minor Penta", "Dorian", "Lydian", "Mixolydian", "Minor",
    "Maj7 Spread", "Min9 Deep", "Sus4 Add9", "Sus2 Oct",
];

/// Octave multipliers — clean musical intervals only (no sub-bass rumble)
const OCTAVES: [f32; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];

/// Mode presets that repos can be assigned to
const MODE_NAMES: [&str; 5] = ["Arpeggio", "Chorus Arp", "Pad", "Bulldozer", "Tremolo Arp"];

/// All Claude Code hook events we register and handle
const HOOK_EVENTS: [&str; 18] = [
    "SessionStart", "SessionEnd", "Stop", "UserPromptSubmit",
    "PermissionRequest", "Notification",
    "SubagentStart", "SubagentStop", "PreCompact", "TeammateIdle",
    "PreToolUse", "PostToolUse", "PostToolUseFailure",
    "InstructionsLoaded", "ConfigChange", "TaskCompleted",
    "WorktreeCreate", "WorktreeRemove",
];

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
    drums: bool,     // Drum break mode (SessionStart/End)
    dub_delay: bool, // Tape delay effect (dub echo)
    melody_over_drums: bool, // Layer tonal generators over drums
    single_hit: bool, // Single drum hit (short percussive events)
}

impl Effects {
    /// Get effects + steps for a repo's hashed mode index
    fn from_mode(mode_idx: usize) -> (Self, u8) {
        match mode_idx {
            0 => (Effects { pad: false, chorus: false, tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }, 3),
            1 => (Effects { pad: false, chorus: true,  tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }, 5),
            2 => (Effects { pad: true,  chorus: true,  tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }, 5),
            3 => (Effects { pad: false, chorus: true,  tremolo: false, bulldozer: true,  drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }, 5),
            4 => (Effects { pad: false, chorus: true,  tremolo: true,  bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }, 3),
            _ => (Effects { pad: false, chorus: true,  tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }, 3),
        }
    }
}

// -----------------------------------------------------------------------------
// EVENT CATEGORIES — shape how hook events sound
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum EventCategory {
    SessionBoundary, // SessionStart/End: keys/pad — long chord blooms
    Attention,       // PermissionRequest, Notification: horn/lead — melodic alert
    DrumHit,         // Stop, UserPromptSubmit: kick/snare — short percussive
    ToolPulse,       // PreToolUse, PostToolUse, PostToolUseFailure: hi-hat — rapid micro-hits
    Bass,            // SubagentStart/Stop, WorktreeCreate/Remove: bass — agent lifecycle
    Lifecycle,       // InstructionsLoaded, ConfigChange, TaskCompleted, PreCompact, TeammateIdle: piano/comping
    #[default]
    Default,         // CLI / unknown
}

impl EventCategory {
    fn octave_offset(&self) -> f32 {
        match self {
            Self::SessionBoundary => 1.0,
            Self::Attention => 1.0,
            Self::DrumHit => 1.0,
            Self::ToolPulse => 2.0,
            Self::Bass => 0.5,
            Self::Lifecycle => 0.75,
            Self::Default => 1.0,
        }
    }

    fn transpose_semitones(&self) -> i32 {
        match self {
            Self::SessionBoundary => 0,
            Self::Attention => 5,
            Self::DrumHit => 0,
            Self::ToolPulse => 0,
            Self::Bass => -5,
            Self::Lifecycle => 3,
            Self::Default => 0,
        }
    }

    fn effective_steps(&self, base: u8) -> u8 {
        match self {
            Self::SessionBoundary => 5,
            Self::Attention => base.max(3),
            Self::DrumHit => 1,
            Self::ToolPulse => 1,
            Self::Bass => 3,
            Self::Lifecycle => 3,
            Self::Default => base,
        }
    }
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

    fn set_cutoff(&mut self, cutoff: f32, q: f32, sample_rate: f32) {
        for stage in &mut self.stages {
            stage.set_lowpass(cutoff, q, sample_rate);
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

/// First-order allpass filter for phaser stages
#[derive(Clone)]
struct PhaseAllpass {
    prev_in: f32,
    prev_out: f32,
}

impl PhaseAllpass {
    fn new() -> Self {
        Self { prev_in: 0.0, prev_out: 0.0 }
    }

    fn process(&mut self, input: f32, coeff: f32) -> f32 {
        let output = coeff * input + self.prev_in - coeff * self.prev_out;
        self.prev_in = input;
        self.prev_out = output;
        output
    }
}

/// Stereo phaser: swept allpass chain with L/R LFO offset (90°).
/// 6 stages = 3 sweeping notches for a smooth, organic sound.
struct StereoPhaser {
    stages_l: Vec<PhaseAllpass>,
    stages_r: Vec<PhaseAllpass>,
    sample_rate: f32,
}

impl StereoPhaser {
    fn new(sample_rate: f32) -> Self {
        let num_stages = 6;
        Self {
            stages_l: vec![PhaseAllpass::new(); num_stages],
            stages_r: vec![PhaseAllpass::new(); num_stages],
            sample_rate,
        }
    }

    fn process(&mut self, input: f32, time: f32, rate: f32) -> (f32, f32) {
        // LFO sweeps allpass center frequency — 90° offset for stereo width
        let lfo_l = (2.0 * PI * rate * time).sin();
        let lfo_r = (2.0 * PI * rate * time + PI * 0.5).sin();

        // Sweep range: 300–3000 Hz (warm to bright)
        let min_freq: f32 = 300.0;
        let max_freq: f32 = 3000.0;
        let freq_l = min_freq + (max_freq - min_freq) * (0.5 + 0.5 * lfo_l);
        let freq_r = min_freq + (max_freq - min_freq) * (0.5 + 0.5 * lfo_r);

        // Allpass coefficient from center frequency
        let coeff_l = {
            let w = PI * freq_l / self.sample_rate;
            (1.0 - w.tan()) / (1.0 + w.tan())
        };
        let coeff_r = {
            let w = PI * freq_r / self.sample_rate;
            (1.0 - w.tan()) / (1.0 + w.tan())
        };

        // Process through allpass chains
        let mut wet_l = input;
        for stage in &mut self.stages_l {
            wet_l = stage.process(wet_l, coeff_l);
        }
        let mut wet_r = input;
        for stage in &mut self.stages_r {
            wet_r = stage.process(wet_r, coeff_r);
        }

        // Mix dry + wet (0.45 depth — audible sweep, not overwhelming)
        let depth = 0.45;
        let gain = 0.69; // compensate for dry+wet sum
        ((input + wet_l * depth) * gain, (input + wet_r * depth) * gain)
    }
}

// -----------------------------------------------------------------------------
// DRUM SYNTHESIS
// -----------------------------------------------------------------------------

/// Deterministic noise — no state needed, suitable for audio callbacks.
/// Classic shader trick: fract(sin(x * big + seed * big) * big).
fn pseudo_noise(time: f32, seed: f32) -> f32 {
    let x = time * 12.9898 + seed * 78.233;
    let s = (x.sin() * 43758.5453).fract();
    // Rust's fract() can be negative (unlike GLSL); normalize to [0, 1) first
    let s = if s < 0.0 { s + 1.0 } else { s };
    s * 2.0 - 1.0 // map to [-1, 1)
}

/// Kick drum: sine with pitch sweep 150→50Hz, click transient.
fn synth_kick(time: f32, _sample_rate: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    // Pitch sweep: 150→50Hz exponential decay
    let freq_start = 150.0;
    let freq_end = 50.0;
    let sweep_rate = 30.0;

    // Phase via closed-form integral of exponential sweep
    let phase = 2.0 * PI * (freq_end * time
        + (freq_start - freq_end) * (1.0 - (-sweep_rate * time).exp()) / sweep_rate);
    let body = phase.sin();

    // Amplitude decay
    let amp = (-4.0 * time).exp();

    // Click transient: high-freq sine burst
    let click = (2.0 * PI * 1200.0 * time).sin() * (-200.0 * time).exp();

    (body * 0.85 + click * 0.15) * amp
}

/// Snare drum: sine body ~185Hz + noise burst.
fn synth_snare(time: f32, noise_seed: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    // Sine body
    let body = (2.0 * PI * 185.0 * time).sin() * (-12.0 * time).exp();

    // Noise burst
    let noise = pseudo_noise(time * 44100.0, noise_seed) * (-8.0 * time).exp();

    body * 0.4 + noise * 0.6
}

/// Hi-hat: noise + metallic sines at 6/8/10kHz.
fn synth_hihat(time: f32, open: bool, noise_seed: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    let decay = if open { -15.0 } else { -60.0 };
    let amp = (decay * time).exp();

    // Noise component
    let noise = pseudo_noise(time * 44100.0, noise_seed + 100.0);

    // Metallic sines
    let metal = (2.0 * PI * 6000.0 * time).sin() * 0.33
              + (2.0 * PI * 8000.0 * time).sin() * 0.33
              + (2.0 * PI * 10000.0 * time).sin() * 0.34;

    (noise * 0.5 + metal * 0.5) * amp
}

/// Rimshot: short pitched click + resonant ring.
fn synth_rimshot(time: f32, noise_seed: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    // Sharp click
    let click = (2.0 * PI * 900.0 * time).sin() * (-80.0 * time).exp();

    // Resonant ring (higher pitched than snare body)
    let ring = (2.0 * PI * 420.0 * time).sin() * (-25.0 * time).exp();

    // Light noise
    let noise = pseudo_noise(time * 44100.0, noise_seed + 200.0) * (-60.0 * time).exp();

    (click * 0.4 + ring * 0.45 + noise * 0.15) * 1.2
}

/// Drum hit types for single-hit events
#[derive(Debug, Clone, Copy, PartialEq)]
enum DrumHitType {
    Kick,
    Snare,
    Rimshot,
    ClosedHat,
    OpenHat,
}

/// Generate a single drum hit (~125ms decay) for short percussive events.
fn generate_single_hit(time: f32, sample_rate: f32, voice: &RepoVoice) -> f32 {
    let decay_env = (-8.0 * time).exp(); // ~125ms effective decay
    let noise_seed = voice.snare_tone * 100.0;
    let raw = match voice.drum_hit_type {
        DrumHitType::Kick => synth_kick(time, sample_rate) * voice.kick_decay,
        DrumHitType::Snare => synth_snare(time, noise_seed) * voice.snare_tone,
        DrumHitType::Rimshot => synth_rimshot(time, noise_seed),
        DrumHitType::ClosedHat => synth_hihat(time, false, noise_seed) * voice.hihat_brightness,
        DrumHitType::OpenHat => synth_hihat(time, true, noise_seed) * voice.hihat_brightness,
    };
    raw * decay_env
}

// -----------------------------------------------------------------------------
// DRUM PATTERN SEQUENCER — CLASSIC BREAKS
// -----------------------------------------------------------------------------

const K: u8 = 0b0001;  // Kick
const S: u8 = 0b0010;  // Snare
const H: u8 = 0b0100;  // Hi-hat closed
const O: u8 = 0b1000;  // Hi-hat open

/// A classic drum break: real pattern transcription with per-step velocity and native BPM.
#[allow(dead_code)]
struct ClassicBreak {
    name: &'static str,
    bpm: f32,
    steps: [u8; 16],
    velocity: [f32; 16],
}

/// 10 classic breaks transcribed from the originals.
/// Velocity: 1.0 = accent, 0.8 = normal, 0.5 = ghost, 0.3 = whisper.
const CLASSIC_BREAKS: [ClassicBreak; 10] = [
    // 0: Amen Break — The Winstons "Amen, Brother" (1969)
    //    THE jungle break. Ride cymbal pattern (using HH), syncopated kick/snare.
    ClassicBreak { name: "Amen Break", bpm: 170.0, steps: [
        K|H, 0,   H,   0,   S|H, 0,   H, S|H,   0,   H, K,   H,   S|H, 0, S,   H,
    ], velocity: [
        1.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.5, 0.0, 0.8, 1.0, 0.8, 1.0, 0.0, 0.4, 0.8,
    ]},
    // 1: Think Break — Lyn Collins "Think (About It)" (1972)
    //    Funky, clean. Used in jungle alongside the Amen.
    ClassicBreak { name: "Think Break", bpm: 170.0, steps: [
        K|H, 0,   H,   0,   S|H, 0,   K|H, 0,   H,   0,   H,   0,   S|H, 0,   H,   0,
    ], velocity: [
        1.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0, 0.8, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0,
    ]},
    // 2: Funky Drummer — Clyde Stubblefield / James Brown (1970)
    //    Most sampled break in history. Syncopated snare ghosts, relentless hi-hat.
    ClassicBreak { name: "Funky Drummer", bpm: 120.0, steps: [
        K|H, H,   H,   H,   S|H, H,   H,   H,   K|H, H,   K|H, H,   S|H, H, S|O, H,
    ], velocity: [
        1.0, 0.5, 0.8, 0.5, 1.0, 0.5, 0.8, 0.5, 0.8, 0.5, 0.5, 0.5, 1.0, 0.5, 0.4, 0.5,
    ]},
    // 3: Apache — Incredible Bongo Band (1973)
    //    Foundation of hip-hop. Double kick, clean snare on 2 and 4.
    ClassicBreak { name: "Apache", bpm: 110.0, steps: [
        K|H, 0,   H,   0,   S|H, 0,   K|H, 0,   K|H, 0,   H,   0,   S|H, 0,   H,   0,
    ], velocity: [
        1.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0, 0.9, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0,
    ]},
    // 4: Skull Snaps — "It's a New Day" (1973)
    //    Gritty, off-kilter. Heavily used in DnB.
    ClassicBreak { name: "Skull Snaps", bpm: 165.0, steps: [
        K|H, 0,   H,   H,   S|H, 0,   H,   K,   H,   0, K|H,  H,   S|H, 0,   H,   S,
    ], velocity: [
        1.0, 0.0, 0.8, 0.5, 1.0, 0.0, 0.8, 0.7, 0.8, 0.0, 0.9, 0.5, 1.0, 0.0, 0.8, 0.4,
    ]},
    // 5: One Drop — classic reggae (Studio One era)
    //    Kick+snare unison on beat 3 only. Maximum space.
    ClassicBreak { name: "One Drop", bpm: 100.0, steps: [
        H,   0,   H,   0,   H,   0,   H,   0, K|S|H, 0,   H,   0,   H,   0,   H,   0,
    ], velocity: [
        0.8, 0.0, 0.8, 0.0, 0.8, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0, 0.8, 0.0, 0.8, 0.0,
    ]},
    // 6: Steppers — four-on-floor roots dub (Channel One style)
    //    Driving kick, snare on 2&4. Dubwise foundation.
    ClassicBreak { name: "Steppers", bpm: 108.0, steps: [
        K|H, 0,   H,   0, K|S|H, 0,   H,   0,   K|H, 0,   H,   0, K|S|H, 0,   H,   0,
    ], velocity: [
        1.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0, 0.9, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0,
    ]},
    // 7: Rockers — half-time dub (King Tubby style)
    //    Heavy beat 1, snare on 3, open hat fills. Spacious.
    ClassicBreak { name: "Rockers", bpm: 95.0, steps: [
        K|H, 0,   H,   O,   0,   0,   H,   0,   S|H, 0,   H,   O,   0,   0,   H,   K,
    ], velocity: [
        1.0, 0.0, 0.8, 0.7, 0.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.7, 0.0, 0.0, 0.8, 0.6,
    ]},
    // 8: Dancehall Digital — Sleng Teng era (1985+)
    //    808-influenced, tight. Offbeat hi-hat triplets.
    ClassicBreak { name: "Dancehall", bpm: 105.0, steps: [
        K|H, 0,   H,   H,   S|H, 0,   H,   0,   K|H, 0,   H,   H,   S|H, 0,   O,   H,
    ], velocity: [
        1.0, 0.0, 0.8, 0.5, 1.0, 0.0, 0.8, 0.0, 0.9, 0.0, 0.8, 0.5, 1.0, 0.0, 0.7, 0.5,
    ]},
    // 9: Two-Step — liquid DnB / Calibre style
    //    Minimal kick/snare, crisp hats. Space for bass.
    ClassicBreak { name: "Two-Step", bpm: 174.0, steps: [
        K|H, 0,   H,   0,   S|H, 0,   H,   0,   H,   0,   H,   0,   K|H, 0,   S|H, 0,
    ], velocity: [
        1.0, 0.0, 0.8, 0.0, 1.0, 0.0, 0.8, 0.0, 0.8, 0.0, 0.8, 0.0, 0.9, 0.0, 1.0, 0.0,
    ]},
];

/// Break style names for display
const BREAK_STYLE_NAMES: [&str; 10] = [
    "Amen Break", "Think Break", "Funky Drummer", "Apache", "Skull Snaps",
    "One Drop", "Steppers", "Rockers", "Dancehall", "Two-Step",
];

/// Chop orders: segment rearrangement for jungle-style break manipulation.
/// Each entry reorders the 4 segments (of 4 steps each) within a bar.
const CHOP_ORDERS: [[usize; 4]; 8] = [
    [0, 1, 2, 3],  // Original (no chop)
    [0, 2, 1, 3],  // Swap middle segments
    [2, 0, 3, 1],  // Interleave
    [3, 2, 1, 0],  // Full reverse
    [0, 0, 2, 3],  // Double first segment
    [0, 1, 3, 2],  // Swap tail
    [2, 3, 0, 1],  // Halves swapped
    [1, 0, 3, 2],  // Pairs reversed
];

/// Generate drums for a single sample. BPM-based timing with looping, chopping, ghost notes.
fn generate_drums(
    current_sample: usize,
    _total_samples: usize,
    sample_rate: f32,
    voice: &RepoVoice,
    melody: &BranchMelody,
) -> f32 {
    let break_idx = voice.drum_pattern_idx % CLASSIC_BREAKS.len();
    let brk = &CLASSIC_BREAKS[break_idx];

    // BPM-based step timing (16th notes) — pattern loops naturally
    let step_samples = (60.0 / brk.bpm / 4.0 * sample_rate) as usize;
    if step_samples == 0 { return 0.0; }

    // Swing: shift odd steps (off-beats) forward
    let raw_step = current_sample / step_samples;
    let step_time = (current_sample % step_samples) as f32 / sample_rate;

    let swing_offset = if raw_step % 2 == 1 {
        (melody.drum_swing * step_samples as f32) as usize
    } else {
        0
    };
    let swung_sample = current_sample.saturating_sub(swing_offset);
    let swung_step = swung_sample / step_samples;
    let swung_time = (swung_sample % step_samples) as f32 / sample_rate;

    // Apply chop: remap step through segment reordering
    let looped_step = swung_step % 16;
    let chop_idx = melody.drum_chop_idx % CHOP_ORDERS.len();
    let effective_step = if chop_idx == 0 {
        looped_step // no chop
    } else {
        let segment = looped_step / 4;
        let within = looped_step % 4;
        let remapped = CHOP_ORDERS[chop_idx][segment];
        remapped * 4 + within
    };

    let flags = brk.steps[effective_step];
    let vel = brk.velocity[effective_step];
    if vel == 0.0 && melody.drum_ghost_level < 0.15 { return 0.0; }

    let mut out = 0.0;

    // Per-step velocity variation from branch
    let vel_var = melody.drum_velocity_var;
    let step_vel = vel * (1.0 - vel_var * ((effective_step as f32 * 3.7).sin() * 0.5 + 0.5));

    if flags & K != 0 {
        out += synth_kick(swung_time, sample_rate) * voice.kick_decay * step_vel;
    }
    if flags & S != 0 {
        out += synth_snare(swung_time, effective_step as f32) * voice.snare_tone * step_vel;
    }
    if flags & H != 0 {
        out += synth_hihat(step_time, false, effective_step as f32) * voice.hihat_brightness * step_vel * 0.6;
    }
    if flags & O != 0 {
        out += synth_hihat(step_time, true, effective_step as f32) * voice.hihat_brightness * step_vel * 0.5;
    }

    // Ghost snare on empty offbeat steps (adds ghost note shuffle)
    if flags & S == 0 && looped_step % 2 == 1 && melody.drum_ghost_level > 0.15 {
        out += synth_snare(step_time, effective_step as f32 + 0.5)
            * melody.drum_ghost_level * 0.3;
    }

    out
}

// -----------------------------------------------------------------------------
// TAPE DELAY (DUB ECHO)
// -----------------------------------------------------------------------------

/// Single-channel tape delay with filtered feedback and wow/flutter.
struct TapeDelay {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples: f32,
    feedback: f32,
    filter: Biquad,
    wow_rate: f32,
    wow_depth: f32,
    sample_count: usize,
}

impl TapeDelay {
    fn new(delay_ms: f32, feedback: f32, filter_cutoff: f32, wow_rate: f32, sample_rate: f32) -> Self {
        let max_delay = (sample_rate * 0.7) as usize; // 700ms max
        let delay_samples = delay_ms / 1000.0 * sample_rate;
        let mut filter = Biquad::new();
        filter.set_lowpass(filter_cutoff, 0.707, sample_rate);
        Self {
            buffer: vec![0.0; max_delay],
            write_pos: 0,
            delay_samples,
            feedback: feedback.min(0.60), // safety cap
            filter,
            wow_rate,
            wow_depth: 2.0, // samples of wow modulation
            sample_count: 0,
        }
    }

    fn process(&mut self, input: f32, sample_rate: f32) -> f32 {
        let buf_len = self.buffer.len();
        if buf_len == 0 { return input; }

        // Wow/flutter: subtle delay time modulation
        let wow = (2.0 * PI * self.wow_rate * self.sample_count as f32 / sample_rate).sin()
                  * self.wow_depth;
        let read_delay = self.delay_samples + wow;
        let read_pos = self.write_pos as f32 - read_delay;
        let read_pos = if read_pos < 0.0 { read_pos + buf_len as f32 } else { read_pos };

        // Linear interpolation for fractional delay
        let idx0 = read_pos.floor() as usize % buf_len;
        let idx1 = (idx0 + 1) % buf_len;
        let frac = read_pos.fract();
        let delayed = self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac;

        // Filter the feedback (tape darkening)
        let filtered = self.filter.process(delayed);

        // Write input + filtered feedback into buffer
        self.buffer[self.write_pos] = input + filtered * self.feedback;
        self.write_pos = (self.write_pos + 1) % buf_len;
        self.sample_count += 1;

        delayed // return wet signal
    }
}

/// Stereo tape delay: two channels with offset delay times for ping-pong dub echo.
struct StereoTapeDelay {
    left: TapeDelay,
    right: TapeDelay,
    mix: f32,
}

impl StereoTapeDelay {
    fn new(
        delay_ms_l: f32,
        delay_ms_r: f32,
        feedback: f32,
        filter_cutoff: f32,
        wow_rate: f32,
        mix: f32,
        sample_rate: f32,
    ) -> Self {
        Self {
            left: TapeDelay::new(delay_ms_l, feedback, filter_cutoff, wow_rate, sample_rate),
            right: TapeDelay::new(delay_ms_r, feedback, filter_cutoff, wow_rate, sample_rate),
            mix,
        }
    }

    fn process(&mut self, input: f32, sample_rate: f32) -> (f32, f32) {
        let wet_l = self.left.process(input, sample_rate);
        let wet_r = self.right.process(input, sample_rate);
        (
            input * (1.0 - self.mix) + wet_l * self.mix,
            input * (1.0 - self.mix) + wet_r * self.mix,
        )
    }
}

/// Convert days since Unix epoch to (year, month, day) — civil calendar (UTC).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's chrono-compatible date library
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Compute pad filter cutoff — shape-dependent, with LFO movement.
/// Dance-friendly: base cutoffs in 800–3000 Hz range (warm presence, not muffled/dark).
fn pad_filter_cutoff(progress: f32, time: f32, shape: PadShape) -> f32 {
    let lfo = (2.0 * PI * 0.3 * time).sin() * 120.0;

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
            900.0 + env * 2000.0
        }
        PadShape::Cascade => {
            // Starts wide open, slowly closes
            let env = (-2.0 * progress).exp();
            1000.0 + env * 2400.0
        }
        PadShape::Bloom => {
            // Starts warm, opens progressively to peak, then closes
            let env = if progress < 0.70 {
                let t = progress / 0.70;
                t * t
            } else {
                ((1.0 - progress) / 0.30 * PI / 2.0).sin()
            };
            800.0 + env * 2200.0
        }
        PadShape::Pulse => {
            // Filter pulses in sync with amplitude
            let pulse = (progress * 2.0 * PI).sin().abs();
            900.0 + pulse * 1500.0
        }
        PadShape::Drift => {
            // Filter wanders in warm range
            let lfo2 = (progress * 3.7 * PI).sin() * 500.0;
            1500.0 + lfo2
        }
        PadShape::Stab => {
            // Opens on hit, slowly closes over the tail
            if progress < 0.05 {
                1000.0 + (progress / 0.05) * 2400.0
            } else {
                let t = (progress - 0.05) / 0.95;
                3400.0 * (-2.5 * t).exp() + 800.0
            }
        }
    };

    (base + lfo).max(800.0)
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

/// Repo determines harmonic identity: key, scale, timbre, note pattern, pad shape, mode
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
    // Extended timbral dimensions
    reverb_mix: f32,      // 0.10–0.40 (spaciousness)
    filter_q: f32,        // 0.5–1.2 (resonance)
    chorus_rate: f32,     // 0.3–1.2 Hz (slow dreamy vs fast shimmer)
    sub_level: f32,       // 0.05–0.25 (bass weight)
    saw_mix: f32,         // 0.0–1.0 (sine-pure vs saw-heavy)
    // Mode/personality
    mode_idx: usize,      // 0=arp, 1=chorus_arp, 2=pad, 3=bulldozer, 4=tremolo_arp
    num_voices: usize,    // 3 or 5 detuned voices per note
    // Drum parameters (hash bytes 14–17)
    drum_pattern_idx: usize,  // 0–7
    kick_decay: f32,          // 0.7–1.0 (amplitude scaling)
    snare_tone: f32,          // 0.7–1.0
    hihat_brightness: f32,    // 0.5–1.0
    // Delay parameters (hash bytes 18–21)
    delay_time_base: f32,     // 200–500ms
    delay_feedback: f32,      // 0.30–0.60
    delay_filter_cutoff: f32, // 2000–3500Hz
    delay_wow_rate: f32,      // 0.5–2.0Hz
    // Synth preset (hash byte 22)
    synth_preset_idx: usize,  // index into SYNTH_PRESETS
    // Single-hit drum type (hash byte 23)
    drum_hit_type: DrumHitType,
    // Jazz micro-pattern for single hits (hash bytes 24–25)
    hit_count: usize,       // 1–4 hits per event (primary + ghost notes)
    hit_spacing_ms: f32,    // 15–60ms between ghost notes
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

        // Extended timbral dimensions (hash bytes 7–11)
        let reverb_mix = 0.15 + (hash[7] as f32 / 255.0) * 0.25;
        let filter_q = 0.6 + (hash[8] as f32 / 255.0) * 0.5;
        let chorus_rate = 0.3 + (hash[9] as f32 / 255.0) * 0.9;
        let sub_level = 0.08 + (hash[10] as f32 / 255.0) * 0.17;
        // Dance default: always saw-dominant (0.65–1.0), filter shapes warmth
        let saw_mix = 0.65 + (hash[11] as f32 / 255.0) * 0.35;

        // Mode: each repo gets its own personality
        let mode_idx = (hash[12] as usize) % 5;
        // Voice count: 3 or 5 detuned voices per note
        let num_voices = if hash[13] % 2 == 0 { 5 } else { 3 };

        // Drum parameters (hash bytes 14–17)
        let drum_pattern_idx = (hash[14] as usize) % CLASSIC_BREAKS.len();
        let kick_decay = 0.7 + (hash[15] as f32 / 255.0) * 0.3;
        let snare_tone = 0.7 + (hash[16] as f32 / 255.0) * 0.3;
        let hihat_brightness = 0.5 + (hash[17] as f32 / 255.0) * 0.5;

        // Delay parameters (hash bytes 18–21)
        let delay_time_base = 200.0 + (hash[18] as f32 / 255.0) * 300.0;
        let delay_feedback = 0.30 + (hash[19] as f32 / 255.0) * 0.30;
        let delay_filter_cutoff = 2000.0 + (hash[20] as f32 / 255.0) * 1500.0;
        let delay_wow_rate = 0.5 + (hash[21] as f32 / 255.0) * 1.5;

        // Synth preset: deterministic per-repo (hash byte 22)
        let synth_preset_idx = (hash[22] as usize) % SYNTH_PRESETS.len();

        // Single-hit drum type (hash byte 23)
        let drum_hit_type = match hash[23] % 5 {
            0 => DrumHitType::Kick,
            1 => DrumHitType::Snare,
            2 => DrumHitType::Rimshot,
            3 => DrumHitType::ClosedHat,
            _ => DrumHitType::OpenHat,
        };

        // Jazz micro-pattern (hash bytes 24–25): ghost notes and flams
        // hit_count 1–4: some repos get clean singles, others get drags/flams
        let hit_count = 1 + (hash[24] as usize % 4);
        // hit_spacing 15–60ms: tighter = flam, wider = drag
        let hit_spacing_ms = 15.0 + (hash[25] as f32 / 255.0) * 45.0;

        Self {
            root_name,
            scale_name,
            scale_freqs,
            octave,
            harmonic_blend,
            third_harmonic,
            pattern_idx,
            pad_shape,
            reverb_mix,
            filter_q,
            chorus_rate,
            sub_level,
            saw_mix,
            mode_idx,
            num_voices,
            drum_pattern_idx,
            kick_decay,
            snare_tone,
            hihat_brightness,
            delay_time_base,
            delay_feedback,
            delay_filter_cutoff,
            delay_wow_rate,
            synth_preset_idx,
            drum_hit_type,
            hit_count,
            hit_spacing_ms,
        }
    }

    /// Blend repo's synth preset (85%) with hash-derived values (15%)
    fn effective_timbral(&self) -> EffectiveTimbral {
        let preset = &SYNTH_PRESETS[self.synth_preset_idx.min(SYNTH_PRESETS.len() - 1)];
        EffectiveTimbral {
            saw_mix: preset.saw_mix * 0.85 + self.saw_mix * 0.15,
            num_voices: preset.num_voices as usize,
            sub_level: preset.sub_level * 0.85 + self.sub_level * 0.15,
            harmonic_blend: preset.harmonic_2nd * 0.7 + self.harmonic_blend * 0.3,
            third_harmonic: preset.harmonic_3rd * 0.7 + self.third_harmonic * 0.3,
            filter_base: preset.filter_base,
            filter_env_amount: preset.filter_env_amount,
            detune_cents: preset.detune_cents,
            chorus_depth: preset.chorus_depth,
            chorus_rate: preset.chorus_rate,
            decay_rate: preset.decay_rate,
        }
    }

    /// Apply spooky overrides: thin sines, dark filter, eerie resonance
    fn make_spooky(&mut self) {
        self.saw_mix = self.saw_mix * 0.15;  // mostly sine
        self.filter_q = 1.5 + (self.filter_q - 0.6) * 2.0; // high resonance
        self.sub_level *= 0.5;
        self.reverb_mix = (self.reverb_mix + 0.15).min(0.50);
    }
}

/// Blended timbral parameters: synth preset (85%/70%) + hash-derived (15%/30%)
#[derive(Debug, Clone)]
struct EffectiveTimbral {
    saw_mix: f32,
    num_voices: usize,
    sub_level: f32,
    harmonic_blend: f32,
    third_harmonic: f32,
    filter_base: f32,
    filter_env_amount: f32,
    detune_cents: f32,
    chorus_depth: f32,
    chorus_rate: f32,
    decay_rate: f32,
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
    // Dub delay parameters (hash bytes 12–15)
    delay_send_level: f32,   // 0.15–0.45
    delay_time_offset: f32,  // -50 to +50ms
    drum_swing: f32,         // 0.0–0.15
    drum_velocity_var: f32,  // 0.0–0.3
    // Drum modulation (hash bytes 16–17)
    drum_chop_idx: usize,    // 0–7 (CHOP_ORDERS index)
    drum_ghost_level: f32,   // 0.0–0.4
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

        // Dub delay parameters (hash bytes 12–15)
        let delay_send_level = 0.15 + (hash[12] as f32 / 255.0) * 0.30;
        let delay_time_offset = -50.0 + (hash[13] as f32 / 255.0) * 100.0;
        let drum_swing = (hash[14] as f32 / 255.0) * 0.15;
        let drum_velocity_var = (hash[15] as f32 / 255.0) * 0.3;

        // Drum modulation (hash bytes 16–17)
        let drum_chop_idx = (hash[16] as usize) % 8;
        let drum_ghost_level = (hash[17] as f32 / 255.0) * 0.4;

        Self {
            swing,
            envelope_shape,
            chorus_detune,
            tremolo_rate,
            tremolo_depth,
            interval_spread,
            stagger_offsets,
            attack_variation,
            delay_send_level,
            delay_time_offset,
            drum_swing,
            drum_velocity_var,
            drum_chop_idx,
            drum_ghost_level,
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
    spooky: bool,
    #[allow(dead_code)]
    event_category: EventCategory,
}

impl PhraseParams {
    fn from_identity(repo: &str, branch: &str, total_duration: u64, volume: f32, effects: Effects, steps: u8, spooky: bool, event_category: EventCategory, event_seed: u8) -> Self {
        let mut voice = RepoVoice::from_repo(repo);
        if spooky { voice.make_spooky(); }

        // Apply event category transformations
        let effective_steps = event_category.effective_steps(steps);
        let octave_mult = event_category.octave_offset();
        let transpose = event_category.transpose_semitones();

        // Transpose scale frequencies
        let transpose_ratio = 2.0_f32.powf(transpose as f32 / 12.0) * octave_mult;
        for freq in &mut voice.scale_freqs {
            *freq *= transpose_ratio;
        }

        // Rotate pattern index by event_seed — each event gets a different melody
        // while staying in the repo's key/scale
        if event_seed > 0 {
            voice.pattern_idx = (voice.pattern_idx + event_seed as usize) % 8;
        }

        // Rotate pad shape by event_seed — each pad event gets a different envelope/filter
        if event_seed > 0 {
            let shape_idx = PAD_SHAPES.iter().position(|s| *s == voice.pad_shape).unwrap_or(0);
            voice.pad_shape = PAD_SHAPES[(shape_idx + event_seed as usize) % PAD_SHAPES.len()];
        }

        // Rotate drum hit type by event_seed — each single-hit event gets a different sound
        if event_seed > 0 {
            let hit_idx = match voice.drum_hit_type {
                DrumHitType::Kick => 0,
                DrumHitType::Snare => 1,
                DrumHitType::Rimshot => 2,
                DrumHitType::ClosedHat => 3,
                DrumHitType::OpenHat => 4,
            };
            voice.drum_hit_type = match (hit_idx + event_seed as usize) % 5 {
                0 => DrumHitType::Kick,
                1 => DrumHitType::Snare,
                2 => DrumHitType::Rimshot,
                3 => DrumHitType::ClosedHat,
                _ => DrumHitType::OpenHat,
            };
        }

        // Rotate micro-pattern by event_seed — each event gets different jazz feel
        if event_seed > 0 {
            voice.hit_count = 1 + (voice.hit_count - 1 + event_seed as usize) % 4;
        }

        let melody = BranchMelody::from_branch(branch, effective_steps);

        // Notes from repo's pattern + scale (repo = identity),
        // interval_spread from branch adds subtle reharmonization
        let notes: Vec<f32> = if effective_steps >= 5 {
            let pattern = PATTERNS_5[voice.pattern_idx];
            pattern.iter().map(|&offset| {
                let spread_offset = (offset as f32 * melody.interval_spread).round() as i32;
                let idx = spread_offset.rem_euclid(5) as usize;
                voice.scale_freqs[idx]
            }).collect()
        } else {
            let pattern = PATTERNS_3[voice.pattern_idx];
            pattern.iter().take(effective_steps as usize).map(|&offset| {
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
            spooky,
            event_category,
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
        Some(Command::Init { scope, legacy }) => run_init(&scope, legacy),
        Some(Command::Update) => run_init("user", false),
        Some(Command::LegacyCleanup { scope }) => run_legacy_cleanup(&scope),
        Some(Command::Play(args)) => run_play(args),
        Some(Command::Test(args)) => run_test(args),
        Some(Command::Player { pattern, bpm }) => run_player(pattern, bpm),
        None => run_play(cli.play_args),
    }
}

fn run_play(args: PlayArgs) -> Result<()> {
    let PlayArgs { branch, repo, duration, volume, pad, chorus, tremolo, bulldozer, steps, spooky, reverse, randomize, drums, dub_delay, melody_over_drums, single_hit, event_category, event_seed, break_pattern, dry_run, quiet } = args;

    // BRANCH_TONE_VOLUME scales all volumes (default 1.0, e.g. 3.0 = triple)
    let master_vol = std::env::var("BRANCH_TONE_VOLUME")
        .ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0).max(0.0);
    let volume = (volume * master_vol).min(1.0);

    // BRANCH_TONE_TEMPO scales all durations (default 1.0, e.g. 1.5 = 50% longer)
    let master_tempo = std::env::var("BRANCH_TONE_TEMPO")
        .ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(1.0).max(0.1);
    let duration = (duration as f64 * master_tempo) as u64;

    let branch = match branch {
        Some(b) => b,
        None => get_current_branch()
            .context("No branch specified and couldn't detect current git branch")?,
    };

    let repo = match repo {
        Some(r) => r,
        None => get_repo_name().unwrap_or_else(|_| "unknown".to_string()),
    };

    // If no explicit mode flags set, use repo's hashed mode
    let explicit_mode = pad || chorus || tremolo || bulldozer || drums || single_hit;
    let (effects, steps) = if explicit_mode {
        (Effects {
            pad: pad || bulldozer || melody_over_drums,
            chorus: chorus || bulldozer,
            tremolo,
            bulldozer,
            drums,
            dub_delay,
            melody_over_drums,
            single_hit,
        }, steps)
    } else {
        let voice = RepoVoice::from_repo(&repo);
        let (mut eff, steps) = Effects::from_mode(voice.mode_idx);
        eff.dub_delay = dub_delay;
        eff.melody_over_drums = melody_over_drums;
        (eff, steps)
    };

    // Pad/bulldozer mode benefits from longer duration
    let duration = if (effects.pad || effects.bulldozer) && duration == 600 {
        1000  // Default to 1000ms for pad
    } else {
        duration
    };

    let mut params = PhraseParams::from_identity(&repo, &branch, duration, volume, effects, steps, spooky, event_category, event_seed);
    if let Some(bp) = break_pattern {
        params.voice.drum_pattern_idx = bp % CLASSIC_BREAKS.len();
    }
    if reverse { params.notes.reverse(); }
    if randomize {
        // Pick random notes from the repo's scale (always in key)
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().subsec_nanos();
        let scale = &params.voice.scale_freqs;
        for (i, note) in params.notes.iter_mut().enumerate() {
            let idx = ((nanos as usize).wrapping_add(i * 7)) % scale.len();
            *note = scale[idx];
        }
        // Also randomize volume slightly (±15%)
        let vol_jitter = 0.85 + ((nanos % 300) as f32 / 1000.0);
        params.volume *= vol_jitter;
    }

    // Print info
    if !quiet {
        let mode_label = if single_hit {
            "single hit"
        } else if explicit_mode {
            if bulldozer { "bulldozer" }
            else if pad { "pad" }
            else if chorus { "chorus" }
            else if tremolo { "tremolo" }
            else { "arpeggio" }
        } else {
            MODE_NAMES[params.voice.mode_idx]
        };
        let spooky_tag = if spooky { " 🎃" } else { "" };
        let preset_name = SYNTH_PRESETS[params.voice.synth_preset_idx.min(SYNTH_PRESETS.len() - 1)].name;
        let hit_tag = if single_hit {
            let hit_name = match params.voice.drum_hit_type {
                DrumHitType::Kick => "Kick",
                DrumHitType::Snare => "Snare",
                DrumHitType::Rimshot => "Rimshot",
                DrumHitType::ClosedHat => "ClosedHat",
                DrumHitType::OpenHat => "OpenHat",
            };
            format!(" [{}]", hit_name)
        } else { String::new() };
        let hybrid_tag = if melody_over_drums { " +hybrid" } else { "" };
        let envelope_names = ["Punchy", "Soft", "Pluck", "Swell"];
        println!("🎵 Repo: {} | Branch: {} [{}] ({}){}{}{}", repo, branch, mode_label, preset_name, hit_tag, hybrid_tag, spooky_tag);
        if !single_hit {
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
    }

    if dry_run {
        return Ok(());
    }

    play_phrase(&params)?;

    Ok(())
}

// -----------------------------------------------------------------------------
// TEST SUBCOMMAND — scan repos/worktrees, play each
// -----------------------------------------------------------------------------

/// A git repo/worktree identified by path
#[derive(Debug)]
struct RepoEntry {
    repo_name: String,
    branch: String,
}

fn repo_entry_from_path(path: &std::path::Path) -> Option<RepoEntry> {
    let output = std::process::Command::new("git")
        .args(["-C", &path.to_string_lossy(), "branch", "--show-current"])
        .output().ok()?;
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() { return None; }

    let repo_name = std::process::Command::new("git")
        .args(["-C", &path.to_string_lossy(), "remote", "get-url", "origin"])
        .output().ok()
        .and_then(|o| {
            if o.status.success() {
                let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
                url.rsplit('/').next().map(|n| n.trim_end_matches(".git").to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            path.file_name().unwrap_or_default().to_string_lossy().to_string()
        });

    Some(RepoEntry {
        repo_name,
        branch,
    })
}

fn run_test(args: TestArgs) -> Result<()> {
    let root = std::path::Path::new(&args.path).canonicalize()
        .with_context(|| format!("Invalid path: {}", args.path))?;

    // Must be a git repo (or worktree)
    if !root.join(".git").exists() {
        anyhow::bail!("{} is not a git repository", root.display());
    }

    let entry = repo_entry_from_path(&root)
        .with_context(|| format!("Could not read git info from {}", root.display()))?;

    let voice = RepoVoice::from_repo(&entry.repo_name);
    let shape_name = PAD_SHAPE_NAMES[PAD_SHAPES.iter().position(|s| *s == voice.pad_shape).unwrap_or(0)];
    let mode_name = MODE_NAMES[voice.mode_idx];
    let preset_name = SYNTH_PRESETS[voice.synth_preset_idx.min(SYNTH_PRESETS.len() - 1)].name;

    println!("Repo: {} @ {}", entry.repo_name, entry.branch);
    println!("  Key: {} {} | Mode: {} | Preset: {} | Shape: {} | Voices: {}",
        voice.root_name, voice.scale_name, mode_name, preset_name, shape_name, voice.num_voices);
    println!("  Q={:.2} | Reverb={:.2} | Saw={:.2} | Chorus={:.1}Hz | Sub={:.2}",
        voice.filter_q, voice.reverb_mix, voice.saw_mix, voice.chorus_rate, voice.sub_level);
    println!();

    // Group labels for display — jazz ensemble voices
    let event_groups: &[(&str, &[&str])] = &[
        ("Drums (rhythm)",     &["UserPromptSubmit", "Stop"]),
        ("Hi-Hat (tools)",     &["PreToolUse", "PostToolUse", "PostToolUseFailure"]),
        ("Bass (agents)",      &["SubagentStart", "SubagentStop", "WorktreeCreate", "WorktreeRemove"]),
        ("Keys/Pad (session)", &["SessionStart", "SessionEnd"]),
        ("Horn (attention)",   &["PermissionRequest", "Notification"]),
        ("Piano (lifecycle)",  &["InstructionsLoaded", "ConfigChange", "TaskCompleted",
                                 "PreCompact", "TeammateIdle"]),
    ];

    for (group_name, events) in event_groups {
        println!("── {} ──", group_name);
        for event in *events {
            let play_args = hook_play_args(event, entry.repo_name.clone(), entry.branch.clone(), args.spooky);
            let dur = play_args.duration;
            let vol = play_args.volume;
            let hit_name = match voice.drum_hit_type {
                DrumHitType::Kick => "Kick",
                DrumHitType::Snare => "Snare",
                DrumHitType::Rimshot => "Rimshot",
                DrumHitType::ClosedHat => "ClosedHat",
                DrumHitType::OpenHat => "OpenHat",
            };
            let fx: String = if play_args.single_hit {
                format!("hit:{}", hit_name)
            } else if play_args.melody_over_drums {
                let break_name = BREAK_STYLE_NAMES[voice.drum_pattern_idx % CLASSIC_BREAKS.len()];
                let base = format!("{} @ {}bpm", break_name, CLASSIC_BREAKS[voice.drum_pattern_idx % CLASSIC_BREAKS.len()].bpm as u32);
                if play_args.dub_delay { format!("{}+melody+dub", base) }
                else { format!("{}+melody", base) }
            } else if play_args.drums {
                let break_name = BREAK_STYLE_NAMES[voice.drum_pattern_idx % CLASSIC_BREAKS.len()];
                format!("{} @ {}bpm", break_name, CLASSIC_BREAKS[voice.drum_pattern_idx % CLASSIC_BREAKS.len()].bpm as u32)
            } else if play_args.pad && play_args.dub_delay && play_args.chorus { format!("pad+chorus+dub ({})", preset_name) }
                else if play_args.pad && play_args.tremolo && play_args.dub_delay { format!("pad+tremolo+dub ({})", preset_name) }
                else if play_args.pad && play_args.chorus && play_args.tremolo { format!("pad+chorus+tremolo ({})", preset_name) }
                else if play_args.dub_delay && play_args.bulldozer { "bulldozer+dub".into() }
                else if play_args.pad && play_args.dub_delay { format!("pad+dub ({})", preset_name) }
                else if play_args.pad && play_args.chorus { format!("pad+chorus ({})", preset_name) }
                else if play_args.dub_delay { "dub delay".into() }
                else if play_args.bulldozer { "bulldozer".into() }
                else if play_args.tremolo { "tremolo".into() }
                else if play_args.chorus { "chorus".into() }
                else if play_args.pad { format!("pad ({})", preset_name) }
                else { "clean".into() };

            print!("  {:20} {:>5}ms  vol={:.2}  [{}]", event, dur, vol, fx);
            // Flush so the label appears before audio plays
            use std::io::Write;
            std::io::stdout().flush().ok();

            let _ = run_play(play_args);
            println!();

            std::thread::sleep(Duration::from_millis(300));
        }
        println!();
    }

    println!("Done. [player available: branch-tone player --pattern {}]",
        voice.drum_pattern_idx % CLASSIC_BREAKS.len());
    Ok(())
}

// -----------------------------------------------------------------------------
// HOOK SUBCOMMAND
// -----------------------------------------------------------------------------

/// Sonic language: map hook event name → PlayArgs for a given repo/branch.
/// Shared by run_hook (production) and run_test (demo).
fn hook_play_args(event: &str, repo: String, branch: String, spooky: bool) -> PlayArgs {
    // Each event gets a unique seed (1–10) so pattern/hit rotates per event.
    // seed=0 means "no rotation" (CLI default).
    match event {
        // ── Keys/Pad (session boundaries — the band starts/stops) ──
        "SessionStart" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 3500, volume: 0.30,
            pad: true, chorus: true, tremolo: false, bulldozer: false,
            steps: 5, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: true, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::SessionBoundary,
            event_seed: 1,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "SessionEnd" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 3500, volume: 0.25,
            pad: true, chorus: false, tremolo: true, bulldozer: false,
            steps: 5, spooky, reverse: true, randomize: false,
            drums: false, dub_delay: true, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::SessionBoundary,
            event_seed: 4,  // seed=4 (vs start=1) → different pad shape + pattern rotation
            break_pattern: None, dry_run: false, quiet: true,
        },
        // ── Drums — kick/snare (frequent rhythm) ───────────────────
        "Stop" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 400, volume: 0.12,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::DrumHit,
            event_seed: 3,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "UserPromptSubmit" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 350, volume: 0.08,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::DrumHit,
            event_seed: 5,  // +2 offset from Stop → always a different hit type
            break_pattern: None, dry_run: false, quiet: true,
        },
        // ── Hi-Hat — tool pulse (very frequent, very quiet) ────────
        "PreToolUse" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 120, volume: 0.05,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::ToolPulse,
            event_seed: 11,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "PostToolUse" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 150, volume: 0.05,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::ToolPulse,
            event_seed: 12,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "PostToolUseFailure" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 250, volume: 0.08,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::ToolPulse,
            event_seed: 13,
            break_pattern: None, dry_run: false, quiet: true,
        },
        // ── Horn/Lead — attention required (melodic alert) ─────────
        "PermissionRequest" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 2500, volume: 0.18,
            pad: true, chorus: false, tremolo: true, bulldozer: false,
            steps: 5, spooky, reverse: true, randomize: false,
            drums: false, dub_delay: true, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Attention,
            event_seed: 2,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "Notification" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 2000, volume: 0.15,
            pad: true, chorus: true, tremolo: true, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Attention,
            event_seed: 6,
            break_pattern: None, dry_run: false, quiet: true,
        },
        // ── Bass — agent lifecycle (voices entering/leaving) ───────
        "SubagentStart" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1000, volume: 0.10,
            pad: true, chorus: false, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: true,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Bass,
            event_seed: 7,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "SubagentStop" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1000, volume: 0.10,
            pad: true, chorus: true, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: true, randomize: true,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Bass,
            event_seed: 8,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "WorktreeCreate" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1200, volume: 0.10,
            pad: true, chorus: false, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Bass,
            event_seed: 14,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "WorktreeRemove" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1200, volume: 0.10,
            pad: true, chorus: false, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: true, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Bass,
            event_seed: 15,
            break_pattern: None, dry_run: false, quiet: true,
        },
        // ── Piano/Comping — lifecycle (structural events) ──────────
        "InstructionsLoaded" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1500, volume: 0.10,
            pad: false, chorus: true, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Lifecycle,
            event_seed: 16,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "ConfigChange" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1800, volume: 0.10,
            pad: true, chorus: false, tremolo: true, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Lifecycle,
            event_seed: 17,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "TaskCompleted" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 2500, volume: 0.15,
            pad: true, chorus: true, tremolo: false, bulldozer: false,
            steps: 5, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: true, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Lifecycle,
            event_seed: 18,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "PreCompact" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 2000, volume: 0.10,
            pad: true, chorus: true, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: true, randomize: false,
            drums: false, dub_delay: true, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Lifecycle,
            event_seed: 9,
            break_pattern: None, dry_run: false, quiet: true,
        },
        "TeammateIdle" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 1500, volume: 0.08,
            pad: true, chorus: false, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Lifecycle,
            event_seed: 10,
            break_pattern: None, dry_run: false, quiet: true,
        },
        // ── Unknown events ─────────────────────────────────────────
        _ => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 300, volume: 0.12,
            pad: true, chorus: false, tremolo: false, bulldozer: false,
            steps: 3, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: false, event_category: EventCategory::Default,
            event_seed: 0,
            break_pattern: None, dry_run: false, quiet: true,
        },
    }
}

fn run_hook() -> Result<()> {
    // Read stdin JSON from Claude Code hook, extract cwd, detect branch/repo, play tone.
    // Never fails — every fallible op is silently absorbed so we never block Claude Code.

    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();

    let mut hook_type = "unknown".to_string();

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&input) {
        let cwd = json.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
        let _ = std::env::set_current_dir(cwd);
        // Plugin system sends "hook_event_name"; legacy sends "hook_type"
        if let Some(ht) = json.get("hook_event_name").and_then(|v| v.as_str())
            .or_else(|| json.get("hook_type").and_then(|v| v.as_str()))
        {
            hook_type = ht.to_string();
        }
    }

    let branch = get_current_branch().unwrap_or_else(|_| "claude".to_string());
    let repo = get_repo_name().unwrap_or_else(|_| "unknown".to_string());

    // Append to event log (~/.branch-tone/events.log) — never fail
    if let Some(home) = dirs::home_dir() {
        let log_dir = home.join(".branch-tone");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("events.log");
        let now = {
            use std::time::SystemTime;
            let dur = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
            let secs = dur.as_secs();
            // Format as ISO-ish timestamp (UTC)
            let s = secs % 60;
            let m = (secs / 60) % 60;
            let h = (secs / 3600) % 24;
            let days = secs / 86400;
            // Simple date from days since epoch
            let (y, mo, d) = days_to_ymd(days);
            format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, m, s)
        };
        let line = format!("{} {} {} {}\n", now, hook_type, repo, branch);
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    let args = hook_play_args(&hook_type, repo, branch, false);
    let _ = run_play(args);
    Ok(())
}

// -----------------------------------------------------------------------------
// INIT SUBCOMMAND
// -----------------------------------------------------------------------------

fn run_init(scope: &str, legacy: bool) -> Result<()> {
    if legacy {
        return run_init_legacy();
    }

    // Validate scope
    match scope {
        "user" | "project" | "local" => {}
        _ => anyhow::bail!("Invalid scope '{}'. Must be one of: user, project, local", scope),
    }

    // Check for claude CLI
    let claude_path = which_in_path("claude");
    if claude_path.is_none() {
        println!("Claude CLI not found in PATH.");
        println!("Install it from: https://docs.anthropic.com/en/docs/claude-code");
        println!();
        println!("Or use legacy mode to patch settings.json directly:");
        println!("  branch-tone init --legacy");
        return Ok(());
    }

    // Add marketplace
    println!("Adding branch-tone marketplace...");
    let marketplace_status = std::process::Command::new("claude")
        .args(["plugin", "marketplace", "add", "rmzi/branch-tone"])
        .status()
        .context("Failed to run 'claude plugin marketplace add'")?;

    if !marketplace_status.success() {
        println!("Warning: marketplace add returned non-zero exit code.");
        println!("The plugin system may not be available in your version of Claude Code.");
        println!();
        println!("Falling back to legacy mode...");
        return run_init_legacy();
    }

    // Install plugin
    println!("Installing branch-tone plugin ({} scope)...", scope);
    let install_status = std::process::Command::new("claude")
        .args(["plugin", "install", "branch-tone@branch-tone", "--scope", scope])
        .status()
        .context("Failed to run 'claude plugin install'")?;

    if !install_status.success() {
        println!("Warning: plugin install returned non-zero exit code.");
        println!();
        println!("Falling back to legacy mode...");
        return run_init_legacy();
    }

    println!();
    println!("✓ branch-tone plugin installed ({} scope)", scope);
    print_init_summary();
    Ok(())
}

/// Check if a command exists in PATH
fn which_in_path(cmd: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(cmd);
            if full.is_file() { Some(full) } else { None }
        })
    })
}

/// Legacy init: directly patch ~/.claude/settings.json
fn run_init_legacy() -> Result<()> {
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
        "hooks": [{"type": "command", "command": hook_command, "async": true}]
    });

    for event in HOOK_EVENTS {
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

        if already_present {
            // Ensure existing entry has "async": true
            for entry in event_hooks.iter_mut() {
                if let Some(inner) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                    for h in inner.iter_mut() {
                        if h.get("command").and_then(|c| c.as_str()) == Some(hook_command) {
                            if h.get("async") != Some(&serde_json::json!(true)) {
                                h.as_object_mut().unwrap().insert("async".into(), serde_json::json!(true));
                                hooks_added += 1; // count as a change
                            }
                        }
                    }
                }
            }
        } else {
            event_hooks.push(new_hook_entry.clone());
            hooks_added += 1;
        }
    }

    // Add Bash(branch-tone*) to permissions.allow so commands auto-approve
    let mut permission_added = false;
    let permission_entry = "Bash(branch-tone*)";
    let permissions = settings
        .as_object_mut()
        .context("settings.json is not an object")?
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("permissions is not an object")?;
    let allow_list = permissions
        .entry("allow")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("permissions.allow is not an array")?;
    if !allow_list.iter().any(|v| v.as_str() == Some(permission_entry)) {
        allow_list.push(serde_json::json!(permission_entry));
        permission_added = true;
    }

    // Add branch-tone to sandbox.excludedCommands so it can access CoreAudio
    let mut sandbox_added = false;
    let sandbox_entry = "branch-tone";
    let sandbox = settings
        .as_object_mut()
        .context("settings.json is not an object")?
        .entry("sandbox")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .context("sandbox is not an object")?;
    let excluded = sandbox
        .entry("excludedCommands")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .context("sandbox.excludedCommands is not an array")?;
    if !excluded.iter().any(|v| v.as_str() == Some(sandbox_entry)) {
        excluded.push(serde_json::json!(sandbox_entry));
        sandbox_added = true;
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
    if permission_added {
        println!("✓ Added Bash(branch-tone*) to permissions.allow");
    }
    if sandbox_added {
        println!("✓ Added branch-tone to sandbox.excludedCommands");
    }

    print_init_summary();
    Ok(())
}

fn print_init_summary() {
    println!("\nbranch-tone is ready! Claude Code will play tones on:");
    println!("  Session:  SessionStart (2s pad+chorus+dub) · SessionEnd (2s pad+tremolo+dub)");
    println!("  Rhythm:   Stop (300ms hit) · UserPromptSubmit (250ms hit)");
    println!("  Alerts:   PermissionRequest (1.5s pad+tremolo+dub) · Notification (1.2s pad+chorus+tremolo)");
    println!("  Ambient:  SubagentStart (500ms) · SubagentStop (500ms) · PreCompact (1.1s+echo)");
    println!("  Waiting:  TeammateIdle (600ms)");
    println!();
    println!("Interactive sequencer: branch-tone player");
}

// -----------------------------------------------------------------------------
// LEGACY CLEANUP SUBCOMMAND
// -----------------------------------------------------------------------------

fn run_legacy_cleanup(scope: &str) -> Result<()> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    let settings_path = home.join(".claude").join("settings.json");

    if !settings_path.exists() {
        println!("No settings.json found at {}", settings_path.display());
        println!("Nothing to clean up. Installing plugin...");
        println!();
        return run_init(scope, false);
    }

    let content = std::fs::read_to_string(&settings_path)
        .with_context(|| format!("Failed to read {}", settings_path.display()))?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", settings_path.display()))?;

    let mut hooks_removed = 0;

    // Remove branch-tone hooks from each event
    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for event in HOOK_EVENTS {
            if let Some(event_hooks) = hooks.get_mut(event).and_then(|e| e.as_array_mut()) {
                let before = event_hooks.len();
                event_hooks.retain(|entry| {
                    // Old format: flat {type, command}
                    if let Some(cmd) = entry.get("command").and_then(|c| c.as_str()) {
                        if cmd.contains("branch-tone") { return false; }
                    }
                    // New format: {hooks: [{type, command}]}
                    if let Some(inner) = entry.get("hooks").and_then(|h| h.as_array()) {
                        let all_bt = inner.iter().all(|h| {
                            h.get("command").and_then(|c| c.as_str())
                                .map_or(false, |c| c.contains("branch-tone"))
                        });
                        if all_bt && !inner.is_empty() { return false; }
                    }
                    true
                });
                hooks_removed += before - event_hooks.len();
            }
        }
        // Remove empty event arrays
        let empty_events: Vec<String> = hooks.iter()
            .filter(|(_, v)| v.as_array().map_or(false, |a| a.is_empty()))
            .map(|(k, _)| k.clone())
            .collect();
        for key in empty_events {
            hooks.remove(&key);
        }
    }
    // Remove hooks key if empty
    if settings.get("hooks").and_then(|h| h.as_object()).map_or(false, |h| h.is_empty()) {
        settings.as_object_mut().unwrap().remove("hooks");
    }

    // Remove Bash(branch-tone*) from permissions.allow
    let mut permission_removed = false;
    if let Some(allow) = settings.pointer_mut("/permissions/allow").and_then(|a| a.as_array_mut()) {
        let before = allow.len();
        allow.retain(|v| v.as_str() != Some("Bash(branch-tone*)"));
        permission_removed = allow.len() < before;
        // Clean up empty allow array → remove permissions object if empty
        if allow.is_empty() {
            if let Some(perms) = settings.get_mut("permissions").and_then(|p| p.as_object_mut()) {
                perms.remove("allow");
                if perms.is_empty() {
                    settings.as_object_mut().unwrap().remove("permissions");
                }
            }
        }
    }

    // Remove branch-tone from sandbox.excludedCommands
    let mut sandbox_removed = false;
    if let Some(excluded) = settings.pointer_mut("/sandbox/excludedCommands").and_then(|a| a.as_array_mut()) {
        let before = excluded.len();
        excluded.retain(|v| v.as_str() != Some("branch-tone"));
        sandbox_removed = excluded.len() < before;
        if excluded.is_empty() {
            if let Some(sb) = settings.get_mut("sandbox").and_then(|s| s.as_object_mut()) {
                sb.remove("excludedCommands");
                if sb.is_empty() {
                    settings.as_object_mut().unwrap().remove("sandbox");
                }
            }
        }
    }

    // Write back
    let json_str = serde_json::to_string_pretty(&settings)
        .context("Failed to serialize settings.json")?;
    std::fs::write(&settings_path, format!("{}\n", json_str))
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;

    if hooks_removed > 0 {
        println!("✓ Removed {} legacy hook(s) from {}", hooks_removed, settings_path.display());
    } else {
        println!("✓ No legacy hooks found in {}", settings_path.display());
    }
    if permission_removed {
        println!("✓ Removed Bash(branch-tone*) from permissions.allow");
    }
    if sandbox_removed {
        println!("✓ Removed branch-tone from sandbox.excludedCommands");
    }
    println!();

    // Install via plugin system
    println!("Installing plugin...");
    run_init(scope, false)
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
    let mut phaser = StereoPhaser::new(sample_rate);

    // Stereo tape delay (dub echo) — allocated before closure, captured by move
    let delay_l_ms = voice.delay_time_base + melody.delay_time_offset;
    let delay_r_ms = voice.delay_time_base * 1.33 + melody.delay_time_offset; // offset for ping-pong
    let mut tape_delay = StereoTapeDelay::new(
        delay_l_ms.max(100.0),
        delay_r_ms.max(100.0),
        voice.delay_feedback,
        voice.delay_filter_cutoff,
        voice.delay_wow_rate,
        melody.delay_send_level,
        sample_rate,
    );

    // Compute EffectiveTimbral from repo voice + synth preset
    let timbral = voice.effective_timbral();
    let eff_reverb = voice.reverb_mix;
    let eff_filter_q = voice.filter_q;
    let eff_chorus_rate = timbral.chorus_rate;
    let eff_filter_env = timbral.filter_env_amount;
    let is_spooky = params.spooky;

    // For bulldozer: arp uses same notes but doesn't drop an octave
    let arp_effects = Effects { pad: false, chorus: true, tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false };

    // Single-hit reverb state (light 8% reverb for percussive events)
    let mut hit_reverb = SimpleReverb::new(sample_rate);

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

                // Global fade-out (last 10% of duration) — prevents abrupt ending
                let fade_start = 0.90;
                let global_fade = if progress > fade_start {
                    ((1.0 - progress) / (1.0 - fade_start)).sqrt()
                } else {
                    1.0
                };

                // ── SINGLE HIT PATH (jazz micro-pattern) ────────
                // Plays 1–4 time-offset hits: primary at full velocity,
                // ghost notes at 30–60% for flams, drags, and jazz feel.
                if effects.single_hit {
                    let spacing_secs = voice.hit_spacing_ms / 1000.0;
                    let mut sum = 0.0f32;
                    for h in 0..voice.hit_count {
                        let hit_time = time - (h as f32 * spacing_secs);
                        if hit_time >= 0.0 {
                            let vel = if h == 0 { 1.0 } else {
                                // Ghost notes: 30–60% velocity, decreasing with distance
                                0.6 - (h as f32 * 0.1)
                            };
                            sum += generate_single_hit(hit_time, sample_rate, &voice) * vel;
                        }
                    }
                    let raw = sum * volume * global_fade;
                    let hit_reverb_mix = 0.08;
                    let wet = hit_reverb.process(raw);
                    let with_reverb = raw * (1.0 - hit_reverb_mix) + wet * hit_reverb_mix;
                    let sample = T::from_sample(with_reverb);
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = sample;
                    }
                    continue;
                }

                // ── TWO-BUS ARCHITECTURE ──────────────────────────
                // DRUM BUS: if drums enabled
                let drum_bus = if effects.drums {
                    generate_drums(current_sample, total_samples, sample_rate, &voice, &melody) * volume
                } else {
                    0.0
                };

                // TONAL BUS: if melody_over_drums, or non-drum mode
                let tonal_bus = if effects.melody_over_drums || !effects.drums {
                    if effects.bulldozer {
                        let pad_out = generate_pad(&notes, time, progress, 1.0, effects, &voice, &melody, &timbral);
                        let arp_out = generate_arpeggio(&notes, time, current_sample, total_samples, 1.0, arp_effects, &voice, &melody, &timbral);
                        (pad_out * 0.7 + arp_out * 0.3) * volume
                    } else if effects.pad {
                        generate_pad(&notes, time, progress, volume, effects, &voice, &melody, &timbral)
                    } else {
                        generate_arpeggio(&notes, time, current_sample, total_samples, volume, effects, &voice, &melody, &timbral)
                    }
                } else {
                    0.0
                };

                // MIX buses
                let raw = if effects.melody_over_drums {
                    drum_bus * 0.65 + tonal_bus * 0.35
                } else if effects.drums {
                    drum_bus
                } else {
                    tonal_bus
                };

                let raw = raw * global_fade;

                if effects.drums && !effects.melody_over_drums {
                    // PURE DRUMS PATH: light reverb → mono output (crisp hits)
                    let drum_reverb_mix = 0.12;
                    let wet = reverb.process(raw);
                    let with_reverb = raw * (1.0 - drum_reverb_mix) + wet * drum_reverb_mix;
                    let sample = T::from_sample(with_reverb);
                    for channel_sample in frame.iter_mut() {
                        *channel_sample = sample;
                    }
                } else {
                    // TONAL PATH: LPF → tape delay (optional) → reverb → phaser

                    // Low-pass filter with envelope (pad/bulldozer modes)
                    let filtered = if effects.pad {
                        let base_cutoff = pad_filter_cutoff(progress, time, voice.pad_shape);
                        // Blend filter cutoff with preset's filter_env_amount
                        let cutoff = if timbral.filter_base > 0.0 {
                            timbral.filter_base + (base_cutoff - timbral.filter_base) * eff_filter_env
                        } else {
                            base_cutoff
                        };
                        let cutoff = if is_spooky { cutoff.max(200.0) } else { cutoff };
                        pad_lpf.set_cutoff(cutoff, eff_filter_q, sample_rate);
                        pad_lpf.process(raw)
                    } else {
                        raw
                    };

                    // Tape delay BEFORE reverb (dub mixing convention)
                    let (post_delay_l, post_delay_r) = if effects.dub_delay {
                        tape_delay.process(filtered, sample_rate)
                    } else {
                        (filtered, filtered)
                    };

                    // Reverb (all modes) — reduce reverb for hybrid to preserve drum transients
                    let reverb_amount = if effects.melody_over_drums { eff_reverb * 0.7 } else { eff_reverb };
                    let wet_l = reverb.process(post_delay_l);
                    let with_reverb_l = post_delay_l * (1.0 - reverb_amount) + wet_l * reverb_amount;
                    // For stereo delay, process right channel through same reverb
                    // (reverb is mono→mono, but L/R differ due to delay offset)
                    let with_reverb_r = if effects.dub_delay {
                        post_delay_r * (1.0 - reverb_amount) + wet_l * reverb_amount
                    } else {
                        with_reverb_l
                    };

                    // Stereo phaser (pad/bulldozer modes get swept allpass stereo)
                    if channels >= 2 && (effects.pad || effects.dub_delay) {
                        let (left, right) = if effects.pad {
                            let (pl, pr) = phaser.process(with_reverb_l, time, eff_chorus_rate);
                            if effects.dub_delay {
                                // Blend phaser with stereo delay
                                ((pl + with_reverb_l) * 0.5, (pr + with_reverb_r) * 0.5)
                            } else {
                                (pl, pr)
                            }
                        } else {
                            // Dub delay without pad: stereo from delay, no phaser
                            (with_reverb_l, with_reverb_r)
                        };
                        for (ch, channel_sample) in frame.iter_mut().enumerate() {
                            let s = if ch % 2 == 0 { left } else { right };
                            *channel_sample = T::from_sample(s);
                        }
                    } else {
                        let sample = T::from_sample(with_reverb_l);
                        for channel_sample in frame.iter_mut() {
                            *channel_sample = sample;
                        }
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
                // Exponential decay with release to zero in final 15%
                let decay_progress = (progress - attack) / (1.0 - attack);
                let env = 0.15 + 0.85 * (-3.0 * decay_progress).exp();
                let release = if progress > 0.85 {
                    ((1.0 - progress) / 0.15 * PI / 2.0).sin()
                } else {
                    1.0
                };
                env * release
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

fn generate_pad(notes: &[f32], time: f32, progress: f32, volume: f32, _effects: Effects, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral) -> f32 {
    let sub_level = timbral.sub_level;
    let saw_mix = timbral.saw_mix;
    let num_voices = timbral.num_voices;
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

        let base_freq = freq;

        // Supersaw-style detuning: preset detune_cents defines signature spread
        let total_spread = if timbral.detune_cents > 0.0 { timbral.detune_cents } else { melody.chorus_detune * 0.25 };
        let nv = num_voices.max(2);

        for j in 0..nv {
            // Spread voices evenly from -total_spread/2 to +total_spread/2
            let cents = if nv == 1 { 0.0 }
                else { -total_spread / 2.0 + (j as f32 / (nv - 1) as f32) * total_spread };
            let f = base_freq * 2.0_f32.powf(cents / 1200.0);
            let phase_offset = j as f32 * 2.0 * PI / nv as f32 + i as f32 * 0.5;

            let saw_phase = f * time + phase_offset;
            let saw = 2.0 * (saw_phase - saw_phase.floor()) - 1.0;
            let sine = (2.0 * PI * f * time + phase_offset).sin();
            let wave = sine * (1.0 - saw_mix) + saw * saw_mix;

            // Center voice slightly louder (supersaw weighting)
            let center_dist = ((j as f32 / (nv - 1).max(1) as f32) - 0.5).abs() * 2.0;
            let voice_gain = 1.0 - center_dist * 0.3;
            sample += wave * note_env * voice_gain / nv as f32;
        }

        // Sub layer — only on the first note for clean low end
        if i == 0 {
            let sub = (2.0 * PI * base_freq * 0.5 * time).sin() * sub_level * note_env;
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

fn generate_arpeggio(notes: &[f32], time: f32, current_sample: usize, total_samples: usize, volume: f32, effects: Effects, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral) -> f32 {
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
        let ring_env = (-timbral.decay_rate * decay_time).exp();

        let env = attack_env * ring_env;

        // Skip notes that have decayed to inaudible
        if env < 0.005 {
            continue;
        }

        let osc = generate_oscillator(frequency, time, effects.chorus, i, voice, melody, timbral);
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

fn generate_oscillator(freq: f32, time: f32, chorus: bool, voice_idx: usize, _voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral) -> f32 {
    // Sub-octave layer for warmth and depth (always present)
    let sub = (2.0 * PI * freq * 0.5 * time).sin() * timbral.sub_level;

    // Slow shimmer: gentle pitch wobble via preset chorus depth
    let shimmer_rate = 2.5 + voice_idx as f32 * 0.3;
    let depth = if timbral.chorus_depth > 0.0 { timbral.chorus_depth } else { 0.003 };
    let shimmer = 1.0 + depth * (2.0 * PI * shimmer_rate * time).sin();
    let freq = freq * shimmer;

    // Use blended timbral harmonics
    let h2_level = timbral.harmonic_blend;
    let h3_level = timbral.third_harmonic;

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
            let h2 = (2.0 * PI * f * 2.0 * time + phase_offset).sin() * h2_level;
            let h3 = (2.0 * PI * f * 3.0 * time + phase_offset).sin() * h3_level;
            sample += (fundamental + h2 + h3) / num_voices;
        }
        sample + sub
    } else {
        // Even without chorus flag, use a light 2-voice detune for spaciousness
        let light_detune = melody.chorus_detune * 0.3;
        let f1 = freq * 2.0_f32.powf(-light_detune / 1200.0);
        let f2 = freq * 2.0_f32.powf(light_detune / 1200.0);

        let s1 = (2.0 * PI * f1 * time).sin()
            + (2.0 * PI * f1 * 2.0 * time).sin() * h2_level
            + (2.0 * PI * f1 * 3.0 * time).sin() * h3_level;
        let s2 = (2.0 * PI * f2 * time).sin()
            + (2.0 * PI * f2 * 2.0 * time).sin() * h2_level
            + (2.0 * PI * f2 * 3.0 * time).sin() * h3_level;

        (s1 + s2) * 0.5 + sub
    }
}

fn apply_tremolo(sample: f32, time: f32, melody: &BranchMelody) -> f32 {
    let tremolo = 1.0 - melody.tremolo_depth * (0.5 + 0.5 * (2.0 * PI * melody.tremolo_rate * time).sin());
    sample * tremolo
}

// -----------------------------------------------------------------------------
// INTERACTIVE STEP SEQUENCER (branch-tone player)
// -----------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, AtomicI8, AtomicU8, Ordering::Relaxed};
use std::sync::Arc;

/// Synth preset: defines multi-voice detuned oscillator + filter parameters.
struct SynthPreset {
    name: &'static str,
    num_voices: u8,       // 1-7 oscillator voices
    detune_cents: f32,    // total spread in cents
    saw_mix: f32,         // 0.0=pure sine, 1.0=pure saw
    harmonic_2nd: f32,    // 2nd harmonic level
    harmonic_3rd: f32,    // 3rd harmonic level
    sub_level: f32,       // sub-oscillator level (0.5x freq)
    filter_base: f32,     // base LPF cutoff Hz (0.0 = bypass)
    filter_env_amount: f32, // how much pad-shape modulates filter
    decay_rate: f32,      // envelope decay speed (higher = faster)
    chorus_depth: f32,    // BBD chorus pitch mod depth
    chorus_rate: f32,     // BBD chorus LFO rate Hz
}

const SYNTH_PRESETS: [SynthPreset; 7] = [
    SynthPreset { name: "Juno",       num_voices: 3, detune_cents: 12.0, saw_mix: 0.85, harmonic_2nd: 0.15, harmonic_3rd: 0.05, sub_level: 0.20, filter_base: 2200.0, filter_env_amount: 0.6, decay_rate: 3.0, chorus_depth: 0.003, chorus_rate: 0.5 },
    SynthPreset { name: "Supersaw",   num_voices: 7, detune_cents: 40.0, saw_mix: 1.00, harmonic_2nd: 0.10, harmonic_3rd: 0.08, sub_level: 0.10, filter_base: 3500.0, filter_env_amount: 0.3, decay_rate: 4.0, chorus_depth: 0.001, chorus_rate: 0.3 },
    SynthPreset { name: "Iceman",     num_voices: 5, detune_cents:  8.0, saw_mix: 0.70, harmonic_2nd: 0.20, harmonic_3rd: 0.10, sub_level: 0.25, filter_base: 1800.0, filter_env_amount: 0.8, decay_rate: 2.5, chorus_depth: 0.002, chorus_rate: 0.4 },
    SynthPreset { name: "M1",         num_voices: 3, detune_cents:  5.0, saw_mix: 0.40, harmonic_2nd: 0.08, harmonic_3rd: 0.03, sub_level: 0.15, filter_base: 4000.0, filter_env_amount: 0.2, decay_rate: 3.5, chorus_depth: 0.001, chorus_rate: 0.2 },
    SynthPreset { name: "WaveStation", num_voices: 5, detune_cents: 18.0, saw_mix: 0.60, harmonic_2nd: 0.18, harmonic_3rd: 0.12, sub_level: 0.18, filter_base: 2000.0, filter_env_amount: 0.7, decay_rate: 2.0, chorus_depth: 0.004, chorus_rate: 0.6 },
    SynthPreset { name: "Bulldozer",  num_voices: 5, detune_cents: 22.0, saw_mix: 0.95, harmonic_2nd: 0.22, harmonic_3rd: 0.10, sub_level: 0.30, filter_base: 1600.0, filter_env_amount: 0.9, decay_rate: 3.0, chorus_depth: 0.003, chorus_rate: 0.5 },
    SynthPreset { name: "Raw",        num_voices: 1, detune_cents:  0.0, saw_mix: 1.00, harmonic_2nd: 0.00, harmonic_3rd: 0.00, sub_level: 0.00, filter_base:    0.0, filter_env_amount: 0.0, decay_rate: 5.0, chorus_depth: 0.000, chorus_rate: 0.0 },
];

struct PlayerState {
    steps: [AtomicU8; 16],
    velocities: [AtomicU8; 16],
    bpm: AtomicU8,
    pattern_idx: AtomicU8,
    playhead: AtomicU8,
    playing: AtomicBool,
    quit: AtomicBool,
    note_triggers: [AtomicU8; 16],  // chromatic piano: 16 semitones from root
    recording: AtomicBool,           // record mode: z/x/c/v writes drums at playhead
    octave_shift: AtomicI8,          // -3 to +3 (octave transpose for piano keys)
    synth_preset: AtomicU8,          // 0-6 (index into SYNTH_PRESETS)
    pad_shape_idx: AtomicU8,         // 0-5 (index into PAD_SHAPES)
    sustain: AtomicBool,             // hold notes without decay
}

impl PlayerState {
    fn new(pattern_idx: usize) -> Self {
        let idx = pattern_idx.min(CLASSIC_BREAKS.len() - 1);
        let brk = &CLASSIC_BREAKS[idx];

        // Initialize atomic arrays from the classic break pattern
        let steps: [AtomicU8; 16] = std::array::from_fn(|i| AtomicU8::new(brk.steps[i]));
        let velocities: [AtomicU8; 16] = std::array::from_fn(|i| {
            AtomicU8::new((brk.velocity[i] * 255.0) as u8)
        });

        Self {
            steps,
            velocities,
            bpm: AtomicU8::new(brk.bpm.min(255.0) as u8),
            pattern_idx: AtomicU8::new(idx as u8),
            playhead: AtomicU8::new(0),
            playing: AtomicBool::new(true),
            quit: AtomicBool::new(false),
            note_triggers: std::array::from_fn(|_| AtomicU8::new(0)),
            recording: AtomicBool::new(false),
            octave_shift: AtomicI8::new(0),
            synth_preset: AtomicU8::new(2), // Iceman (default)
            pad_shape_idx: AtomicU8::new(0),
            sustain: AtomicBool::new(false),
        }
    }

    fn load_pattern(&self, idx: usize) {
        let idx = idx.min(CLASSIC_BREAKS.len() - 1);
        let brk = &CLASSIC_BREAKS[idx];
        for i in 0..16 {
            self.steps[i].store(brk.steps[i], Relaxed);
            self.velocities[i].store((brk.velocity[i] * 255.0) as u8, Relaxed);
        }
        self.bpm.store(brk.bpm.min(255.0) as u8, Relaxed);
        self.pattern_idx.store(idx as u8, Relaxed);
    }

    fn toggle_step(&self, step: usize, drum_flag: u8) {
        if step < 16 {
            let old = self.steps[step].load(Relaxed);
            self.steps[step].store(old ^ drum_flag, Relaxed);
            // If toggling on and velocity is 0, set to full
            if old & drum_flag == 0 && self.velocities[step].load(Relaxed) == 0 {
                self.velocities[step].store(200, Relaxed);
            }
        }
    }

    fn cycle_velocity(&self, step: usize) {
        if step < 16 {
            let v = self.velocities[step].load(Relaxed);
            // empty(0) → full(200) → ghost(80) → empty(0)
            let next = if v == 0 { 200 } else if v > 100 { 80 } else { 0 };
            self.velocities[step].store(next, Relaxed);
        }
    }
}

/// Map cursor row (0=K, 1=S, 2=H, 3=O) to drum flag bitmask.
fn drum_for_row(row: usize) -> u8 {
    match row {
        0 => K,
        1 => S,
        2 => H,
        3 => O,
        _ => 0,
    }
}

/// RAII guard for terminal raw mode + alternate screen.
struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        use crossterm::{execute, terminal::EnterAlternateScreen, cursor::Hide};
        execute!(std::io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        use crossterm::{execute, terminal::LeaveAlternateScreen, cursor::Show};
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, Show);
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Detect repo voice from current working directory's git repo name.
fn detect_repo_voice() -> RepoVoice {
    let repo = get_repo_name().unwrap_or_else(|_| "unknown".to_string());
    RepoVoice::from_repo(&repo)
}

/// Start the continuous audio stream that reads from PlayerState.
fn start_player_audio(state: Arc<PlayerState>, voice: &RepoVoice) -> Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host.default_output_device()
        .context("No audio output device found")?;
    let config = device.default_output_config()
        .context("Failed to get default audio config")?;
    let config: cpal::StreamConfig = config.into();
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    let kick_decay = voice.kick_decay;
    let snare_tone = voice.snare_tone;
    let hihat_brightness = voice.hihat_brightness;

    // Base frequency for keyboard (root of the scale)
    let base_freq = voice.scale_freqs[0];

    let mut sample_counter: u64 = 0;
    let mut prev_step: u8 = 255; // sentinel: no previous step yet
    let mut step_time: f32 = 0.0;
    let mut reverb = SimpleReverb::new(sample_rate);

    // Per-note, per-voice keyboard synthesis state (lives in closure)
    let mut key_phases: [[f32; 7]; 16] = [[0.0; 7]; 16];
    let mut key_amps: [f32; 16] = [0.0; 16];
    let mut key_times: [f32; 16] = [0.0; 16];
    let mut key_filter = LowPass24::new();
    let mut chorus_lfo_phase: f32 = 0.0;

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let oct_shift = state.octave_shift.load(Relaxed) as f32;
            let oct_mult = 2.0_f32.powf(oct_shift);
            let preset_idx = (state.synth_preset.load(Relaxed) as usize).min(SYNTH_PRESETS.len() - 1);
            let preset = &SYNTH_PRESETS[preset_idx];
            let pad_shape_idx = (state.pad_shape_idx.load(Relaxed) as usize).min(PAD_SHAPES.len() - 1);
            let pad_shape = PAD_SHAPES[pad_shape_idx];
            let sustain_on = state.sustain.load(Relaxed);

            for frame in data.chunks_mut(channels) {
                // Advance chorus LFO
                chorus_lfo_phase += preset.chorus_rate / sample_rate;
                if chorus_lfo_phase >= 1.0 { chorus_lfo_phase -= 1.0; }
                let chorus_mod = (chorus_lfo_phase * 2.0 * PI).sin() * preset.chorus_depth;

                // Keyboard notes always ring (even when drums paused)
                let mut keys_out = 0.0;
                for i in 0..16 {
                    let trigger = state.note_triggers[i].swap(0, Relaxed);
                    if trigger > 0 {
                        key_amps[i] = trigger as f32 / 255.0;
                        key_times[i] = 0.0;
                    }
                    if key_amps[i] > 0.001 {
                        let base_key_freq = base_freq * 2.0_f32.powf(i as f32 / 12.0) * oct_mult;
                        let nv = preset.num_voices as usize;
                        let mut note_out = 0.0;

                        for v in 0..nv {
                            // Spread voices evenly across detune range
                            let detune_offset = if nv > 1 {
                                let t = v as f32 / (nv - 1) as f32 - 0.5; // -0.5 to +0.5
                                t * preset.detune_cents
                            } else {
                                0.0
                            };
                            let detune_ratio = 2.0_f32.powf(detune_offset / 1200.0);
                            // BBD chorus: slow pitch modulation
                            let freq = base_key_freq * detune_ratio * (1.0 + chorus_mod);

                            key_phases[i][v] += freq / sample_rate;
                            if key_phases[i][v] >= 1.0 { key_phases[i][v] -= 1.0; }
                            let phase = key_phases[i][v];

                            // Waveform: sine/saw blend per saw_mix
                            let sine = (phase * 2.0 * PI).sin();
                            let saw = phase * 2.0 - 1.0;
                            let mut osc = sine * (1.0 - preset.saw_mix) + saw * preset.saw_mix;

                            // Add harmonics
                            let phase2 = (phase * 2.0) % 1.0;
                            let phase3 = (phase * 3.0) % 1.0;
                            osc += (phase2 * 2.0 * PI).sin() * preset.harmonic_2nd;
                            osc += (phase3 * 2.0 * PI).sin() * preset.harmonic_3rd;

                            // Center voice louder (supersaw-style weighting)
                            let weight = if nv > 1 {
                                let center = (nv - 1) as f32 / 2.0;
                                let dist = ((v as f32 - center) / center).abs();
                                1.0 - dist * 0.4
                            } else {
                                1.0
                            };

                            note_out += osc * weight;
                        }

                        // Normalize by voice count
                        if nv > 1 { note_out /= nv as f32 * 0.6; }

                        // Sub layer: sine at 0.5x freq
                        if preset.sub_level > 0.0 {
                            let sub_freq = base_key_freq * 0.5;
                            // Reuse voice 0 phase scaled for sub
                            let sub_phase = (key_phases[i][0] * 0.5) % 1.0;
                            let _ = sub_freq; // freq used implicitly via phase relationship
                            note_out += (sub_phase * 2.0 * PI).sin() * preset.sub_level;
                        }

                        // Apply envelope
                        let decay = if sustain_on {
                            (-0.1 / sample_rate).exp() // near-zero decay in sustain
                        } else {
                            (-preset.decay_rate / sample_rate).exp()
                        };

                        keys_out += note_out * key_amps[i];
                        key_amps[i] *= decay;
                        key_times[i] += 1.0 / sample_rate;
                    }
                }

                // Global filter on keys output (skip for Raw preset with filter_base == 0)
                if preset.filter_base > 0.0 && keys_out.abs() > 0.0001 {
                    // Shape-driven filter modulation
                    let filter_mod = pad_filter_cutoff(0.5, chorus_lfo_phase * 3.0, pad_shape);
                    let cutoff = preset.filter_base + (filter_mod - 1500.0) * preset.filter_env_amount;
                    let cutoff = cutoff.clamp(200.0, 18000.0);
                    key_filter.set_cutoff(cutoff, 0.707, sample_rate);
                    keys_out = key_filter.process(keys_out);
                }

                if state.quit.load(Relaxed) {
                    for s in frame.iter_mut() { *s = 0.0; }
                    continue;
                }

                // Drums (only when playing)
                let mut drum_out = 0.0;
                if state.playing.load(Relaxed) {
                    let bpm = state.bpm.load(Relaxed) as f32;
                    let step_samples = (60.0 / bpm / 4.0 * sample_rate) as u64;
                    if step_samples > 0 {
                        let current_step = ((sample_counter / step_samples) % 16) as u8;
                        state.playhead.store(current_step, Relaxed);

                        if current_step != prev_step {
                            step_time = 0.0;
                            prev_step = current_step;
                        }

                        let flags = state.steps[current_step as usize].load(Relaxed);
                        let vel_raw = state.velocities[current_step as usize].load(Relaxed);
                        let vel = vel_raw as f32 / 255.0;

                        if vel > 0.0 {
                            if flags & K != 0 {
                                drum_out += synth_kick(step_time, sample_rate) * kick_decay * vel;
                            }
                            if flags & S != 0 {
                                drum_out += synth_snare(step_time, current_step as f32) * snare_tone * vel;
                            }
                            if flags & H != 0 {
                                drum_out += synth_hihat(step_time, false, current_step as f32) * hihat_brightness * vel * 0.6;
                            }
                            if flags & O != 0 {
                                drum_out += synth_hihat(step_time, true, current_step as f32) * hihat_brightness * vel * 0.5;
                            }
                        }
                    }
                }

                // Mix drums + keys, apply reverb
                let combined = drum_out + keys_out * 0.35;
                let wet = reverb.process(combined);
                let mixed = combined * 0.88 + wet * 0.12;

                for s in frame.iter_mut() { *s = mixed; }

                step_time += 1.0 / sample_rate;
                sample_counter += 1;
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
    ).context("Failed to build player audio stream")?;

    stream.play().context("Failed to start player audio")?;
    Ok(stream)
}

/// Render the step sequencer grid to the terminal.
fn render_grid(
    state: &PlayerState,
    cursor_step: usize,
    cursor_row: usize,
    scale_name: &str,
    stdout: &mut impl std::io::Write,
) -> Result<()> {
    use crossterm::{cursor::MoveTo, terminal::{Clear, ClearType}};

    crossterm::queue!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

    let pattern_idx = state.pattern_idx.load(Relaxed) as usize;
    let break_name = BREAK_STYLE_NAMES[pattern_idx.min(BREAK_STYLE_NAMES.len() - 1)];
    let bpm = state.bpm.load(Relaxed);
    let playing = state.playing.load(Relaxed);
    let playhead = state.playhead.load(Relaxed) as usize;
    let recording = state.recording.load(Relaxed);
    let status = if recording { "\x1b[31mREC\x1b[0m" }
        else if playing { "PLAYING" }
        else { "PAUSED" };

    // Header
    write!(stdout, " branch-tone player --- {} @ {} BPM --- [{}]\r\n\r\n", break_name, bpm, status)?;

    // Column headers
    write!(stdout, "   ")?;
    for i in 0..16 {
        if i == playhead && playing {
            write!(stdout, "{}{:>2} {}", "\x1b[1m", i + 1, "\x1b[0m")?;
        } else {
            write!(stdout, "{:>2} ", i + 1)?;
        }
    }
    write!(stdout, "\r\n")?;

    // Drum rows: K, S, H, O
    let row_labels = ['K', 'S', 'H', 'O'];
    let row_flags = [K, S, H, O];

    for (row, (&label, &flag)) in row_labels.iter().zip(row_flags.iter()).enumerate() {
        write!(stdout, "{}  ", label)?;
        for step in 0..16 {
            let flags = state.steps[step].load(Relaxed);
            let vel = state.velocities[step].load(Relaxed);
            let active = flags & flag != 0;
            let is_cursor = step == cursor_step && row == cursor_row;
            let is_playhead = step == playhead && playing;

            let symbol = if active && vel > 100 {
                "●"
            } else if active && vel > 0 {
                "○" // ghost hit
            } else {
                "·"     // empty
            };

            if is_cursor {
                // Inverted for cursor
                write!(stdout, "\x1b[7m")?;
            }
            if is_playhead && !is_cursor {
                // Bold for playhead column
                write!(stdout, "\x1b[1m")?;
            }

            write!(stdout, " {} ", symbol)?;

            if is_cursor || (is_playhead && !is_cursor) {
                write!(stdout, "\x1b[0m")?;
            }
        }
        write!(stdout, "\r\n")?;
    }

    // Playhead indicator
    write!(stdout, "\r\n   ")?;
    for i in 0..16 {
        if i == playhead && playing {
            write!(stdout, " \u{25b2} ")?; // ▲
        } else {
            write!(stdout, "   ")?;
        }
    }
    write!(stdout, "\r\n")?;

    // Piano settings display
    let oct = state.octave_shift.load(Relaxed);
    let oct_str = if oct > 0 { format!("+{}", oct) } else { format!("{}", oct) };
    let preset_idx = state.synth_preset.load(Relaxed) as usize;
    let preset_name = SYNTH_PRESETS[preset_idx.min(SYNTH_PRESETS.len() - 1)].name;
    let pad_idx = state.pad_shape_idx.load(Relaxed) as usize;
    let pad_name = PAD_SHAPE_NAMES[pad_idx.min(PAD_SHAPE_NAMES.len() - 1)];
    let sustain = state.sustain.load(Relaxed);
    let sustain_str = if sustain { " | \x1b[33mSUSTAIN\x1b[0m" } else { "" };

    write!(stdout, "\r\n Piano: Oct {} | Synth: {} | Shape: {}{}\r\n", oct_str, preset_name, pad_name, sustain_str)?;

    // Piano keyboard display
    write!(stdout, "\r\n    W E   T Y U   O P\r\n")?;
    write!(stdout, "   A S D F G H J K L       [{}]\r\n", scale_name)?;

    // Help text
    write!(stdout, "\r\n [enter] toggle  [i] ghost  [</>] move  [^/v] row\r\n")?;
    write!(stdout, " [1-0] pattern   [+/-] BPM  [space] play/pause  [q] quit\r\n")?;
    write!(stdout, " [A-L] piano     [r] record  [z/x/c/v] rec K/S/H/O\r\n")?;
    write!(stdout, " [\\[/\\]] octave  [,/.] synth  [;/'] pad shape  [tab] sustain\r\n")?;

    stdout.flush()?;
    Ok(())
}

/// Main entry point for the interactive step sequencer.
fn run_player(initial_pattern: usize, initial_bpm: Option<u16>) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};

    let voice = detect_repo_voice();
    let scale_name = format!("{} {}", voice.root_name, voice.scale_name);
    let state = Arc::new(PlayerState::new(initial_pattern));
    if let Some(bpm) = initial_bpm {
        state.bpm.store(bpm.min(255) as u8, Relaxed);
    }
    // Initialize pad shape from repo voice
    let voice_pad_idx = PAD_SHAPES.iter().position(|s| *s == voice.pad_shape).unwrap_or(0);
    state.pad_shape_idx.store(voice_pad_idx as u8, Relaxed);

    let _guard = RawModeGuard::enter()?;
    let _stream = start_player_audio(Arc::clone(&state), &voice)?;

    let mut stdout = std::io::stdout();
    let mut cursor_step: usize = 0;
    let mut cursor_row: usize = 0; // 0=K, 1=S, 2=H, 3=O

    loop {
        render_grid(&state, cursor_step, cursor_row, &scale_name, &mut stdout)?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                // Only handle Press events (avoid double-firing on release)
                if key.kind != KeyEventKind::Press { continue; }

                let recording = state.recording.load(Relaxed);

                match key.code {
                    KeyCode::Char('q') => {
                        state.quit.store(true, Relaxed);
                        break;
                    }
                    // Grid editing
                    KeyCode::Enter => {
                        state.toggle_step(cursor_step, drum_for_row(cursor_row));
                    }
                    KeyCode::Char('i') => {
                        state.cycle_velocity(cursor_step);
                    }
                    // Transport
                    KeyCode::Char(' ') => {
                        let was = state.playing.load(Relaxed);
                        state.playing.store(!was, Relaxed);
                    }
                    KeyCode::Char('r') => {
                        let was = state.recording.load(Relaxed);
                        state.recording.store(!was, Relaxed);
                        if !was { state.playing.store(true, Relaxed); }
                    }
                    // Navigation
                    KeyCode::Left => {
                        cursor_step = cursor_step.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        cursor_step = (cursor_step + 1).min(15);
                    }
                    KeyCode::Up => {
                        cursor_row = cursor_row.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        cursor_row = (cursor_row + 1).min(3);
                    }
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        let cur = state.bpm.load(Relaxed);
                        if cur < 250 { state.bpm.store(cur + 5, Relaxed); }
                    }
                    KeyCode::Char('-') => {
                        let cur = state.bpm.load(Relaxed);
                        if cur > 60 { state.bpm.store(cur - 5, Relaxed); }
                    }
                    KeyCode::Char(c @ '0'..='9') => {
                        let idx = if c == '0' { 9 } else { (c as usize) - ('1' as usize) };
                        if idx < CLASSIC_BREAKS.len() {
                            state.load_pattern(idx);
                        }
                    }
                    // Piano: white keys A-L (chromatic semitones 0,2,4,5,7,9,11,12,14)
                    KeyCode::Char('a') => state.note_triggers[0].store(200, Relaxed),  // root
                    KeyCode::Char('s') => state.note_triggers[2].store(200, Relaxed),  // +2
                    KeyCode::Char('d') => state.note_triggers[4].store(200, Relaxed),  // +4
                    KeyCode::Char('f') => state.note_triggers[5].store(200, Relaxed),  // +5
                    KeyCode::Char('g') => state.note_triggers[7].store(200, Relaxed),  // +7
                    KeyCode::Char('h') => state.note_triggers[9].store(200, Relaxed),  // +9
                    KeyCode::Char('j') => state.note_triggers[11].store(200, Relaxed), // +11
                    KeyCode::Char('k') => state.note_triggers[12].store(200, Relaxed), // +12 (octave)
                    KeyCode::Char('l') => state.note_triggers[14].store(200, Relaxed), // +14
                    // Piano: black keys W,E,T,Y,U,O,P (sharps/flats)
                    KeyCode::Char('w') => state.note_triggers[1].store(200, Relaxed),  // +1
                    KeyCode::Char('e') => state.note_triggers[3].store(200, Relaxed),  // +3
                    KeyCode::Char('t') => state.note_triggers[6].store(200, Relaxed),  // +6
                    KeyCode::Char('y') => state.note_triggers[8].store(200, Relaxed),  // +8
                    KeyCode::Char('u') => state.note_triggers[10].store(200, Relaxed), // +10
                    KeyCode::Char('o') => state.note_triggers[13].store(200, Relaxed), // +13
                    KeyCode::Char('p') => state.note_triggers[15].store(200, Relaxed), // +15
                    // Record-mode drum triggers: z/x/c/v write at playhead
                    KeyCode::Char('z') if recording => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(K, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    KeyCode::Char('x') if recording => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(S, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    KeyCode::Char('c') if recording => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(H, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    KeyCode::Char('v') if recording => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(O, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    // Piano controls: octave, wave shape, pad shape
                    KeyCode::Char('[') => {
                        let cur = state.octave_shift.load(Relaxed);
                        if cur > -3 { state.octave_shift.store(cur - 1, Relaxed); }
                    }
                    KeyCode::Char(']') => {
                        let cur = state.octave_shift.load(Relaxed);
                        if cur < 3 { state.octave_shift.store(cur + 1, Relaxed); }
                    }
                    KeyCode::Char(',') => {
                        let cur = state.synth_preset.load(Relaxed);
                        if cur > 0 { state.synth_preset.store(cur - 1, Relaxed); }
                        else { state.synth_preset.store(SYNTH_PRESETS.len() as u8 - 1, Relaxed); }
                    }
                    KeyCode::Char('.') => {
                        let cur = state.synth_preset.load(Relaxed);
                        let next = cur + 1;
                        if (next as usize) < SYNTH_PRESETS.len() { state.synth_preset.store(next, Relaxed); }
                        else { state.synth_preset.store(0, Relaxed); }
                    }
                    KeyCode::Tab => {
                        let was = state.sustain.load(Relaxed);
                        state.sustain.store(!was, Relaxed);
                    }
                    KeyCode::Char(';') => {
                        let cur = state.pad_shape_idx.load(Relaxed);
                        if cur > 0 { state.pad_shape_idx.store(cur - 1, Relaxed); }
                        else { state.pad_shape_idx.store(PAD_SHAPES.len() as u8 - 1, Relaxed); }
                    }
                    KeyCode::Char('\'') => {
                        let cur = state.pad_shape_idx.load(Relaxed);
                        let next = cur + 1;
                        if (next as usize) < PAD_SHAPES.len() { state.pad_shape_idx.store(next, Relaxed); }
                        else { state.pad_shape_idx.store(0, Relaxed); }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_effects() -> Effects {
        Effects { pad: false, chorus: false, tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false }
    }

    // -- Determinism: same input always produces same output --

    #[test]
    fn same_identity_same_notes() {
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        let b = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        assert_eq!(a.notes, b.notes);
    }

    #[test]
    fn different_branch_different_notes() {
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        let b = PhraseParams::from_identity("myrepo", "feature/auth", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        assert_ne!(a.notes, b.notes);
    }

    #[test]
    fn different_repo_different_notes() {
        let a = PhraseParams::from_identity("repo-a", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        let b = PhraseParams::from_identity("repo-b", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        assert_ne!(a.notes, b.notes);
    }

    // -- Two-layer hashing: repo controls voice, branch controls melody --

    #[test]
    fn same_repo_shares_voice() {
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        let b = PhraseParams::from_identity("myrepo", "feature/x", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        assert_eq!(a.voice.scale_freqs, b.voice.scale_freqs);
        assert_eq!(a.voice.octave, b.voice.octave);
        assert_eq!(a.voice.harmonic_blend, b.voice.harmonic_blend);
    }

    #[test]
    fn same_branch_shares_melody() {
        let a = PhraseParams::from_identity("repo-a", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        let b = PhraseParams::from_identity("repo-b", "main", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        assert_eq!(a.melody.swing, b.melody.swing);
        assert_eq!(a.melody.envelope_shape, b.melody.envelope_shape);
        assert_eq!(a.melody.stagger_offsets, b.melody.stagger_offsets);
    }

    // -- Note count matches step parameter --

    #[test]
    fn three_steps_produces_three_notes() {
        let p = PhraseParams::from_identity("r", "b", 400, 0.25, default_effects(), 3, false, EventCategory::Default, 0);
        assert_eq!(p.notes.len(), 3);
    }

    #[test]
    fn five_steps_produces_five_notes() {
        let p = PhraseParams::from_identity("r", "b", 400, 0.25, default_effects(), 5, false, EventCategory::Default, 0);
        assert_eq!(p.notes.len(), 5);
    }

    // -- All notes land on valid scale frequencies --

    #[test]
    fn notes_are_valid_scale_frequencies() {
        for branch in ["main", "develop", "feature/x", "fix/bug-123", "release/v2"] {
            let p = PhraseParams::from_identity("repo", branch, 400, 0.25, default_effects(), 5, false, EventCategory::Default, 0);
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
        let timbral = voice.effective_timbral();
        for t in 0..1000 {
            let time = t as f32 / 44100.0;
            let sample = generate_oscillator(440.0, time, false, 0, &voice, &melody, &timbral);
            assert!(sample >= -1.5 && sample <= 1.5, "sample out of range: {}", sample);
        }
    }

    #[test]
    fn chorus_oscillator_output_in_range() {
        let voice = RepoVoice::from_repo("test");
        let melody = BranchMelody::from_branch("test", 3);
        let timbral = voice.effective_timbral();
        for t in 0..1000 {
            let time = t as f32 / 44100.0;
            let sample = generate_oscillator(440.0, time, true, 0, &voice, &melody, &timbral);
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
        let effects = Effects { pad: true, chorus: false, tremolo: false, bulldozer: false, drums: false, dub_delay: false, melody_over_drums: false, single_hit: false };
        let voice = RepoVoice::from_repo("test");
        let melody = BranchMelody::from_branch("test", 3);
        let timbral = EffectiveTimbral {
            saw_mix: 1.0, num_voices: 3, sub_level: 0.15,
            harmonic_blend: 0.15, third_harmonic: 0.05,
            filter_base: 2200.0, filter_env_amount: 0.6,
            detune_cents: 12.0, chorus_depth: 0.003, chorus_rate: 0.5, decay_rate: 3.0,
        };

        let start = generate_pad(&notes, 0.0, 0.01, 1.0, effects, &voice, &melody, &timbral).abs();
        let mid = generate_pad(&notes, 0.5, 0.5, 1.0, effects, &voice, &melody, &timbral).abs();
        let end = generate_pad(&notes, 1.0, 0.99, 1.0, effects, &voice, &melody, &timbral).abs();

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
            "hooks": [{"type": "command", "command": hook_command, "async": true}]
        });

        for event in HOOK_EVENTS {
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

            if already_present {
                // Ensure existing entry has "async": true
                for entry in event_hooks.iter_mut() {
                    if let Some(inner) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                        for h in inner.iter_mut() {
                            if h.get("command").and_then(|c| c.as_str()) == Some(hook_command) {
                                if h.get("async") != Some(&serde_json::json!(true)) {
                                    h.as_object_mut().unwrap().insert("async".into(), serde_json::json!(true));
                                }
                            }
                        }
                    }
                }
            } else {
                event_hooks.push(new_hook_entry.clone());
            }
        }
    }

    #[test]
    fn hook_init_writes_new_format() {
        let mut settings = serde_json::json!({});
        apply_hook_init(&mut settings);

        for event in HOOK_EVENTS {
            let event_hooks = settings["hooks"][event].as_array()
                .unwrap_or_else(|| panic!("hooks.{} should be an array", event));
            assert_eq!(event_hooks.len(), 1, "{} should have exactly one matcher group", event);

            let group = &event_hooks[0];
            let inner = group["hooks"].as_array()
                .unwrap_or_else(|| panic!("{} matcher group should have hooks array", event));
            assert_eq!(inner.len(), 1);
            assert_eq!(inner[0]["type"], "command");
            assert_eq!(inner[0]["command"], "branch-tone hook");
            assert_eq!(inner[0]["async"], true);
        }
    }

    #[test]
    fn hook_init_is_idempotent() {
        let mut settings = serde_json::json!({});
        apply_hook_init(&mut settings);
        apply_hook_init(&mut settings); // run twice

        for event in HOOK_EVENTS {
            let event_hooks = settings["hooks"][event].as_array().unwrap();
            assert_eq!(event_hooks.len(), 1, "{} should not duplicate on re-init", event);
        }
    }

    #[test]
    fn hook_init_patches_missing_async() {
        // Simulate hooks written by an older version (missing "async": true)
        let mut settings = serde_json::json!({
            "hooks": {
                "Stop": [
                    {"hooks": [{"type": "command", "command": "branch-tone hook"}]}
                ],
                "SessionStart": [
                    {"hooks": [{"type": "command", "command": "branch-tone hook"}]}
                ]
            }
        });
        // Verify async is missing before init
        assert!(settings["hooks"]["Stop"][0]["hooks"][0].get("async").is_none());

        apply_hook_init(&mut settings);

        // All events should now have async: true
        for event in HOOK_EVENTS {
            let event_hooks = settings["hooks"][event].as_array().unwrap();
            assert_eq!(event_hooks.len(), 1, "{} should have one entry", event);
            let inner = event_hooks[0]["hooks"].as_array().unwrap();
            assert_eq!(inner[0]["async"], true, "{} should have async: true after init", event);
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

        // Migrated events should be new format
        for event in ["Stop", "PermissionRequest"] {
            let event_hooks = settings["hooks"][event].as_array().unwrap();
            assert_eq!(event_hooks.len(), 1, "{} should have one entry after migration", event);
            assert!(event_hooks[0].get("hooks").is_some(),
                "{} entry should be new format with 'hooks' key", event);
            assert!(event_hooks[0].get("type").is_none(),
                "{} entry should not have top-level 'type' (old format)", event);
        }
        // All events should be present
        for event in HOOK_EVENTS {
            assert!(settings["hooks"][event].as_array().is_some(),
                "{} should be registered", event);
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

    // -- Drum synthesis tests --

    #[test]
    fn kick_output_in_range() {
        for i in 0..2000 {
            let t = i as f32 / 44100.0;
            let s = synth_kick(t, 44100.0);
            assert!(!s.is_nan(), "kick NaN at t={}", t);
            assert!(s.abs() <= 1.5, "kick out of range at t={}: {}", t, s);
        }
    }

    #[test]
    fn snare_output_in_range() {
        for i in 0..2000 {
            let t = i as f32 / 44100.0;
            let s = synth_snare(t, 0.0);
            assert!(!s.is_nan(), "snare NaN at t={}", t);
            assert!(s.abs() <= 1.5, "snare out of range at t={}: {}", t, s);
        }
    }

    #[test]
    fn hihat_output_in_range() {
        for open in [false, true] {
            for i in 0..2000 {
                let t = i as f32 / 44100.0;
                let s = synth_hihat(t, open, 0.0);
                assert!(!s.is_nan(), "hihat NaN at t={} open={}", t, open);
                assert!(s.abs() <= 1.5, "hihat out of range at t={}: {}", t, s);
            }
        }
    }

    #[test]
    fn all_breaks_are_16_steps() {
        for (i, brk) in CLASSIC_BREAKS.iter().enumerate() {
            assert_eq!(brk.steps.len(), 16, "break {} should have 16 steps", i);
            assert_eq!(brk.velocity.len(), 16, "break {} should have 16 velocities", i);
        }
    }

    #[test]
    fn all_breaks_have_kick_and_snare() {
        for (i, brk) in CLASSIC_BREAKS.iter().enumerate() {
            let has_kick = brk.steps.iter().any(|&f| f & K != 0);
            let has_snare = brk.steps.iter().any(|&f| f & S != 0);
            assert!(has_kick, "break {} ({}) should have at least one kick", i, brk.name);
            assert!(has_snare, "break {} ({}) should have at least one snare", i, brk.name);
        }
    }

    #[test]
    fn all_breaks_have_valid_bpm() {
        for (i, brk) in CLASSIC_BREAKS.iter().enumerate() {
            assert!(brk.bpm >= 80.0 && brk.bpm <= 200.0,
                "break {} ({}) BPM {} out of range", i, brk.name, brk.bpm);
        }
    }

    #[test]
    fn chop_orders_are_valid_permutations() {
        for (i, order) in CHOP_ORDERS.iter().enumerate() {
            for &seg in order {
                assert!(seg < 4, "chop order {} has invalid segment index {}", i, seg);
            }
        }
    }

    // -- Tape delay tests --

    #[test]
    fn tape_delay_bounded_output() {
        let mut delay = TapeDelay::new(300.0, 0.50, 3000.0, 1.0, 44100.0);
        // Feed an impulse then silence
        let first = delay.process(1.0, 44100.0);
        assert!(!first.is_nan(), "tape delay NaN on impulse");
        for i in 0..44100 {
            let s = delay.process(0.0, 44100.0);
            assert!(!s.is_nan(), "tape delay NaN at sample {}", i);
            assert!(s.abs() <= 2.0, "tape delay exploded at sample {}: {}", i, s);
        }
    }

    #[test]
    fn tape_delay_decays_to_silence() {
        let mut delay = TapeDelay::new(200.0, 0.45, 3000.0, 1.0, 44100.0);
        // Impulse
        delay.process(1.0, 44100.0);
        // Process 2 seconds of silence
        let mut last_sample = 0.0f32;
        for _ in 0..88200 {
            last_sample = delay.process(0.0, 44100.0);
        }
        assert!(last_sample.abs() < 0.001, "tape delay should decay to near-silence, got {}", last_sample);
    }

    // -- Hook mapping tests --

    #[test]
    fn session_events_use_pads() {
        for event in ["SessionStart", "SessionEnd"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(args.pad, "{} should use pad", event);
            assert!(args.dub_delay, "{} should use dub_delay", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert!(!args.single_hit, "{} should not use single_hit", event);
            assert_eq!(args.event_category, EventCategory::SessionBoundary, "{} should be SessionBoundary", event);
        }
        // Start uses chorus, End uses tremolo — different sonic character
        let start = hook_play_args("SessionStart", "repo".into(), "main".into(), false);
        let end = hook_play_args("SessionEnd", "repo".into(), "main".into(), false);
        assert!(start.chorus, "SessionStart should use chorus");
        assert!(!start.tremolo, "SessionStart should not use tremolo");
        assert!(end.tremolo, "SessionEnd should use tremolo");
        assert!(!end.chorus, "SessionEnd should not use chorus");
    }

    #[test]
    fn attention_events_use_pad() {
        for event in ["PermissionRequest", "Notification"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(args.pad, "{} should use pad", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert!(!args.single_hit, "{} should not use single_hit", event);
            assert_eq!(args.event_category, EventCategory::Attention, "{} should be Attention", event);
        }
        // PreCompact still uses dub_delay but no drums
        let precompact = hook_play_args("PreCompact", "repo".into(), "main".into(), false);
        assert!(precompact.dub_delay, "PreCompact should use dub_delay");
        assert!(!precompact.drums, "PreCompact should not use drums");
        assert!(!precompact.single_hit, "PreCompact should not use single_hit");
    }

    #[test]
    fn frequent_events_use_single_hit() {
        for event in ["Stop", "UserPromptSubmit"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(args.single_hit, "{} should use single_hit", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert!(!args.pad, "{} should not use pad", event);
            assert_eq!(args.event_category, EventCategory::DrumHit, "{} should be DrumHit", event);
        }
    }

    #[test]
    fn bass_events_no_delay() {
        for event in ["SubagentStart", "SubagentStop", "WorktreeCreate", "WorktreeRemove"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(!args.dub_delay, "{} should not use dub_delay", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert!(!args.melody_over_drums, "{} should not use melody_over_drums", event);
            assert!(!args.single_hit, "{} should not use single_hit", event);
        }
    }

    #[test]
    fn tool_pulse_events_use_single_hit() {
        for event in ["PreToolUse", "PostToolUse", "PostToolUseFailure"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(args.single_hit, "{} should use single_hit", event);
            assert!(!args.pad, "{} should not use pad", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert_eq!(args.event_category, EventCategory::ToolPulse, "{} should be ToolPulse", event);
        }
    }

    #[test]
    fn tool_pulse_events_are_quiet() {
        for event in ["PreToolUse", "PostToolUse"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(args.volume <= 0.06, "{} volume {} should be <= 0.06", event, args.volume);
            assert!(args.duration <= 200, "{} duration {} should be <= 200ms", event, args.duration);
        }
    }

    #[test]
    fn bass_events_use_low_octave() {
        for event in ["SubagentStart", "SubagentStop", "WorktreeCreate", "WorktreeRemove"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert_eq!(args.event_category, EventCategory::Bass, "{} should be Bass", event);
        }
        assert!(EventCategory::Bass.octave_offset() < 1.0, "Bass octave should be below center");
    }

    #[test]
    fn worktree_events_are_directional() {
        let create = hook_play_args("WorktreeCreate", "repo".into(), "main".into(), false);
        let remove = hook_play_args("WorktreeRemove", "repo".into(), "main".into(), false);
        assert!(!create.reverse, "WorktreeCreate should ascend (not reversed)");
        assert!(remove.reverse, "WorktreeRemove should descend (reversed)");
    }

    #[test]
    fn lifecycle_events_medium_duration() {
        for event in ["InstructionsLoaded", "ConfigChange", "TaskCompleted", "PreCompact", "TeammateIdle"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert_eq!(args.event_category, EventCategory::Lifecycle, "{} should be Lifecycle", event);
            assert!(args.duration >= 1500, "{} duration {} should be >= 1500ms", event, args.duration);
        }
    }

    #[test]
    fn task_completed_uses_resolved_cadence() {
        let args = hook_play_args("TaskCompleted", "repo".into(), "main".into(), false);
        assert!(args.pad, "TaskCompleted should use pad");
        assert!(args.chorus, "TaskCompleted should use chorus");
        assert!(args.dub_delay, "TaskCompleted should use dub_delay");
        assert_eq!(args.steps, 5, "TaskCompleted should have 5 steps for full phrase");
        assert!(args.duration >= 2000, "TaskCompleted should be long enough for resolution");
    }

    #[test]
    fn open_hat_single_hit_output_in_range() {
        let mut voice = RepoVoice::from_repo("test");
        voice.drum_hit_type = DrumHitType::OpenHat;
        for i in 0..4000 {
            let t = i as f32 / 44100.0;
            let s = generate_single_hit(t, 44100.0, &voice);
            assert!(!s.is_nan(), "OpenHat NaN at t={}", t);
            assert!(s.abs() <= 2.0, "OpenHat out of range at t={}: {}", t, s);
        }
    }

    #[test]
    fn jazz_micro_pattern_deterministic() {
        let v1 = RepoVoice::from_repo("my-project");
        let v2 = RepoVoice::from_repo("my-project");
        assert_eq!(v1.hit_count, v2.hit_count, "same repo should get same hit_count");
        assert_eq!(v1.hit_spacing_ms, v2.hit_spacing_ms, "same repo should get same spacing");
        assert!(v1.hit_count >= 1 && v1.hit_count <= 4, "hit_count should be 1–4, got {}", v1.hit_count);
        assert!(v1.hit_spacing_ms >= 15.0 && v1.hit_spacing_ms <= 60.0,
            "spacing should be 15–60ms, got {}", v1.hit_spacing_ms);
    }

    #[test]
    fn event_seed_rotates_micro_pattern() {
        // Different seeds should produce different hit counts (rotating through 1–4)
        let a = PhraseParams::from_identity("repo", "main", 400, 0.12,
            Effects { pad: false, chorus: false, tremolo: false, bulldozer: false,
                      drums: false, dub_delay: false, melody_over_drums: false, single_hit: true },
            1, false, EventCategory::DrumHit, 1);
        let b = PhraseParams::from_identity("repo", "main", 400, 0.12,
            Effects { pad: false, chorus: false, tremolo: false, bulldozer: false,
                      drums: false, dub_delay: false, melody_over_drums: false, single_hit: true },
            1, false, EventCategory::DrumHit, 3);
        // Seeds 1 and 3 differ by 2, so hit_count should rotate differently
        assert_ne!(a.voice.hit_count, b.voice.hit_count,
            "different seeds should rotate hit_count: seed1={} seed3={}", a.voice.hit_count, b.voice.hit_count);
    }

    #[test]
    fn new_hash_fields_dont_break_existing() {
        // Regression: ensure that adding drum/delay hash fields doesn't change existing voice/melody
        let voice = RepoVoice::from_repo("myrepo");
        assert_eq!(voice.root_name, RepoVoice::from_repo("myrepo").root_name);
        assert_eq!(voice.scale_name, RepoVoice::from_repo("myrepo").scale_name);
        assert_eq!(voice.mode_idx, RepoVoice::from_repo("myrepo").mode_idx);
        // New fields should be populated
        assert!(voice.drum_pattern_idx < CLASSIC_BREAKS.len());
        assert!(voice.kick_decay >= 0.7 && voice.kick_decay <= 1.0);
        assert!(voice.delay_time_base >= 200.0 && voice.delay_time_base <= 500.0);
        assert!(voice.delay_feedback >= 0.30 && voice.delay_feedback <= 0.60);

        let melody = BranchMelody::from_branch("main", 3);
        assert!(melody.delay_send_level >= 0.15 && melody.delay_send_level <= 0.45);
        assert!(melody.drum_swing >= 0.0 && melody.drum_swing <= 0.15);
        assert!(melody.drum_chop_idx < CHOP_ORDERS.len());
        assert!(melody.drum_ghost_level >= 0.0 && melody.drum_ghost_level <= 0.4);
    }

    // -- PlayerState tests --

    #[test]
    fn player_state_toggle_step() {
        let state = PlayerState::new(0);
        let original = state.steps[0].load(Relaxed);

        // Toggle kick off (Amen step 0 has K|H)
        state.toggle_step(0, K);
        assert_eq!(state.steps[0].load(Relaxed), original ^ K);

        // Toggle kick back on
        state.toggle_step(0, K);
        assert_eq!(state.steps[0].load(Relaxed), original);
    }

    #[test]
    fn player_state_load_pattern() {
        let state = PlayerState::new(0); // Amen
        state.load_pattern(3); // Apache

        let apache = &CLASSIC_BREAKS[3];
        for i in 0..16 {
            assert_eq!(state.steps[i].load(Relaxed), apache.steps[i], "step {} mismatch", i);
        }
        assert_eq!(state.pattern_idx.load(Relaxed), 3);
        assert_eq!(state.bpm.load(Relaxed), apache.bpm.min(255.0) as u8);
    }

    #[test]
    fn player_state_cycle_velocity() {
        let state = PlayerState::new(0);
        // Set to 0 first
        state.velocities[5].store(0, Relaxed);

        // 0 → 200 (full)
        state.cycle_velocity(5);
        assert_eq!(state.velocities[5].load(Relaxed), 200);

        // 200 → 80 (ghost)
        state.cycle_velocity(5);
        assert_eq!(state.velocities[5].load(Relaxed), 80);

        // 80 → 0 (empty)
        state.cycle_velocity(5);
        assert_eq!(state.velocities[5].load(Relaxed), 0);
    }

    #[test]
    fn player_state_bpm_range() {
        // All classic breaks should have BPM that fits in u8 (≤255)
        for brk in &CLASSIC_BREAKS {
            assert!(brk.bpm <= 255.0, "{} has BPM {} > 255", brk.name, brk.bpm);
            assert!(brk.bpm >= 60.0, "{} has BPM {} < 60", brk.name, brk.bpm);
        }

        // PlayerState new should clamp
        let state = PlayerState::new(0);
        let bpm = state.bpm.load(Relaxed);
        assert!(bpm >= 60);
    }

    // -- Synth preset tests --

    #[test]
    fn synth_presets_sane_ranges() {
        for preset in &SYNTH_PRESETS {
            assert!(preset.num_voices >= 1 && preset.num_voices <= 7,
                "{}: voices {} out of 1-7", preset.name, preset.num_voices);
            assert!(preset.detune_cents >= 0.0 && preset.detune_cents <= 60.0,
                "{}: detune {} out of 0-60", preset.name, preset.detune_cents);
            // filter_base 0.0 means bypass (Raw), otherwise >= 200
            assert!(preset.filter_base == 0.0 || preset.filter_base >= 200.0,
                "{}: filter_base {} invalid", preset.name, preset.filter_base);
            assert!(preset.saw_mix >= 0.0 && preset.saw_mix <= 1.0,
                "{}: saw_mix {} out of 0-1", preset.name, preset.saw_mix);
        }
    }

    #[test]
    fn raw_preset_is_single_voice_no_effects() {
        let raw = &SYNTH_PRESETS[6];
        assert_eq!(raw.name, "Raw");
        assert_eq!(raw.num_voices, 1);
        assert_eq!(raw.detune_cents, 0.0);
        assert_eq!(raw.harmonic_2nd, 0.0);
        assert_eq!(raw.harmonic_3rd, 0.0);
        assert_eq!(raw.sub_level, 0.0);
        assert_eq!(raw.filter_base, 0.0); // bypass
        assert_eq!(raw.chorus_depth, 0.0);
    }

    #[test]
    fn preset_count_matches() {
        assert_eq!(SYNTH_PRESETS.len(), 7);
    }

    #[test]
    fn default_preset_is_iceman() {
        let state = PlayerState::new(0);
        let idx = state.synth_preset.load(Relaxed) as usize;
        assert_eq!(idx, 2);
        assert_eq!(SYNTH_PRESETS[idx].name, "Iceman");
    }

    #[test]
    fn sustain_toggle() {
        let state = PlayerState::new(0);
        assert!(!state.sustain.load(Relaxed), "sustain should start off");
        state.sustain.store(true, Relaxed);
        assert!(state.sustain.load(Relaxed));
        state.sustain.store(false, Relaxed);
        assert!(!state.sustain.load(Relaxed));
    }

    // -- Synth preset per repo tests --

    #[test]
    fn synth_preset_idx_deterministic() {
        let v1 = RepoVoice::from_repo("my-project");
        let v2 = RepoVoice::from_repo("my-project");
        assert_eq!(v1.synth_preset_idx, v2.synth_preset_idx);
        assert!(v1.synth_preset_idx < SYNTH_PRESETS.len());

        // Different repos should (usually) get different presets
        let v3 = RepoVoice::from_repo("other-project");
        assert!(v3.synth_preset_idx < SYNTH_PRESETS.len());
    }

    #[test]
    fn effective_timbral_blends_correctly() {
        let voice = RepoVoice::from_repo("test-blend");
        let timbral = voice.effective_timbral();
        let preset = &SYNTH_PRESETS[voice.synth_preset_idx];

        // 85% preset + 15% hash (saw_mix, sub_level)
        let expected_saw = preset.saw_mix * 0.85 + voice.saw_mix * 0.15;
        assert!((timbral.saw_mix - expected_saw).abs() < 0.001,
            "saw_mix blend: expected {}, got {}", expected_saw, timbral.saw_mix);

        let expected_sub = preset.sub_level * 0.85 + voice.sub_level * 0.15;
        assert!((timbral.sub_level - expected_sub).abs() < 0.001,
            "sub_level blend: expected {}, got {}", expected_sub, timbral.sub_level);

        // Preset wins for these fields
        assert_eq!(timbral.num_voices, preset.num_voices as usize);
        assert_eq!(timbral.filter_base, preset.filter_base);
        assert_eq!(timbral.detune_cents, preset.detune_cents);
        assert_eq!(timbral.chorus_depth, preset.chorus_depth);
        assert_eq!(timbral.decay_rate, preset.decay_rate);
    }

    #[test]
    fn event_categories_are_set_correctly() {
        // Keys/Pad (session)
        assert_eq!(hook_play_args("SessionStart", "r".into(), "b".into(), false).event_category, EventCategory::SessionBoundary);
        assert_eq!(hook_play_args("SessionEnd", "r".into(), "b".into(), false).event_category, EventCategory::SessionBoundary);
        // Drums (kick/snare)
        assert_eq!(hook_play_args("Stop", "r".into(), "b".into(), false).event_category, EventCategory::DrumHit);
        assert_eq!(hook_play_args("UserPromptSubmit", "r".into(), "b".into(), false).event_category, EventCategory::DrumHit);
        // Hi-Hat (tool pulse)
        assert_eq!(hook_play_args("PreToolUse", "r".into(), "b".into(), false).event_category, EventCategory::ToolPulse);
        assert_eq!(hook_play_args("PostToolUse", "r".into(), "b".into(), false).event_category, EventCategory::ToolPulse);
        assert_eq!(hook_play_args("PostToolUseFailure", "r".into(), "b".into(), false).event_category, EventCategory::ToolPulse);
        // Horn (attention)
        assert_eq!(hook_play_args("PermissionRequest", "r".into(), "b".into(), false).event_category, EventCategory::Attention);
        assert_eq!(hook_play_args("Notification", "r".into(), "b".into(), false).event_category, EventCategory::Attention);
        // Bass (agent lifecycle)
        assert_eq!(hook_play_args("SubagentStart", "r".into(), "b".into(), false).event_category, EventCategory::Bass);
        assert_eq!(hook_play_args("SubagentStop", "r".into(), "b".into(), false).event_category, EventCategory::Bass);
        assert_eq!(hook_play_args("WorktreeCreate", "r".into(), "b".into(), false).event_category, EventCategory::Bass);
        assert_eq!(hook_play_args("WorktreeRemove", "r".into(), "b".into(), false).event_category, EventCategory::Bass);
        // Piano (lifecycle)
        assert_eq!(hook_play_args("InstructionsLoaded", "r".into(), "b".into(), false).event_category, EventCategory::Lifecycle);
        assert_eq!(hook_play_args("ConfigChange", "r".into(), "b".into(), false).event_category, EventCategory::Lifecycle);
        assert_eq!(hook_play_args("TaskCompleted", "r".into(), "b".into(), false).event_category, EventCategory::Lifecycle);
        assert_eq!(hook_play_args("PreCompact", "r".into(), "b".into(), false).event_category, EventCategory::Lifecycle);
        assert_eq!(hook_play_args("TeammateIdle", "r".into(), "b".into(), false).event_category, EventCategory::Lifecycle);
        // Unknown
        assert_eq!(hook_play_args("Unknown", "r".into(), "b".into(), false).event_category, EventCategory::Default);
    }

    #[test]
    fn background_events_no_melody_over_drums() {
        let bg_events = [
            "SubagentStart", "SubagentStop", "WorktreeCreate", "WorktreeRemove",
            "PreCompact", "TeammateIdle", "InstructionsLoaded", "ConfigChange",
        ];
        for event in bg_events {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(!args.melody_over_drums, "{} should not use melody_over_drums", event);
            assert!(!args.single_hit, "{} should not use single_hit", event);
        }
    }

    // -- Single hit tests --

    #[test]
    fn single_hit_output_in_range() {
        let voice = RepoVoice::from_repo("test");
        for i in 0..4000 {
            let t = i as f32 / 44100.0;
            let s = generate_single_hit(t, 44100.0, &voice);
            assert!(!s.is_nan(), "single_hit NaN at t={}", t);
            assert!(s.abs() <= 2.0, "single_hit out of range at t={}: {}", t, s);
        }
    }

    #[test]
    fn single_hit_decays_quickly() {
        let voice = RepoVoice::from_repo("test");
        // After 200ms the signal should be very quiet
        let late = generate_single_hit(0.2, 44100.0, &voice);
        assert!(late.abs() < 0.05, "single_hit should decay by 200ms, got {}", late);
    }

    #[test]
    fn rimshot_output_in_range() {
        for i in 0..2000 {
            let t = i as f32 / 44100.0;
            let s = synth_rimshot(t, 0.0);
            assert!(!s.is_nan(), "rimshot NaN at t={}", t);
            assert!(s.abs() <= 2.0, "rimshot out of range at t={}: {}", t, s);
        }
    }

    #[test]
    fn drum_hit_type_deterministic() {
        let v1 = RepoVoice::from_repo("my-project");
        let v2 = RepoVoice::from_repo("my-project");
        assert_eq!(v1.drum_hit_type, v2.drum_hit_type);

        // Different repos can get different hit types
        let v3 = RepoVoice::from_repo("other-project");
        // Just verify it's a valid variant (always true for an enum, but ensures no panic)
        let _ = match v3.drum_hit_type {
            DrumHitType::Kick | DrumHitType::Snare | DrumHitType::Rimshot | DrumHitType::ClosedHat | DrumHitType::OpenHat => true,
        };
    }

    // -- EventCategory tests --

    #[test]
    fn event_category_octave_offsets() {
        assert_eq!(EventCategory::SessionBoundary.octave_offset(), 1.0);
        assert_eq!(EventCategory::Attention.octave_offset(), 1.0);
        assert_eq!(EventCategory::DrumHit.octave_offset(), 1.0);
        assert_eq!(EventCategory::ToolPulse.octave_offset(), 2.0);
        assert_eq!(EventCategory::Bass.octave_offset(), 0.5);
        assert_eq!(EventCategory::Lifecycle.octave_offset(), 0.75);
        assert_eq!(EventCategory::Default.octave_offset(), 1.0);
    }

    #[test]
    fn event_category_transpose() {
        assert_eq!(EventCategory::SessionBoundary.transpose_semitones(), 0);
        assert_eq!(EventCategory::Attention.transpose_semitones(), 5);
        assert_eq!(EventCategory::DrumHit.transpose_semitones(), 0);
        assert_eq!(EventCategory::ToolPulse.transpose_semitones(), 0);
        assert_eq!(EventCategory::Bass.transpose_semitones(), -5);
        assert_eq!(EventCategory::Lifecycle.transpose_semitones(), 3);
        assert_eq!(EventCategory::Default.transpose_semitones(), 0);
    }

    #[test]
    fn event_category_effective_steps() {
        assert_eq!(EventCategory::SessionBoundary.effective_steps(3), 5);
        assert_eq!(EventCategory::Attention.effective_steps(1), 3);
        assert_eq!(EventCategory::Attention.effective_steps(5), 5);
        assert_eq!(EventCategory::DrumHit.effective_steps(5), 1);
        assert_eq!(EventCategory::ToolPulse.effective_steps(5), 1);
        assert_eq!(EventCategory::Bass.effective_steps(5), 3);
        assert_eq!(EventCategory::Bass.effective_steps(1), 3);
        assert_eq!(EventCategory::Lifecycle.effective_steps(5), 3);
        assert_eq!(EventCategory::Lifecycle.effective_steps(1), 3);
        assert_eq!(EventCategory::Default.effective_steps(3), 3);
    }

    // -- Event seed tests --

    #[test]
    fn event_seed_produces_different_patterns() {
        // Same repo+branch but different seeds → different note patterns
        let a = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 5, false, EventCategory::Default, 1);
        let b = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 5, false, EventCategory::Default, 2);
        // Pattern indices are rotated, so notes will differ
        assert_ne!(a.voice.pattern_idx, b.voice.pattern_idx,
            "different seeds should rotate pattern_idx");
    }

    #[test]
    fn event_seed_zero_preserves_original() {
        // seed=0 means no rotation — should match base pattern
        let base = RepoVoice::from_repo("myrepo");
        let params = PhraseParams::from_identity("myrepo", "main", 400, 0.25, default_effects(), 5, false, EventCategory::Default, 0);
        assert_eq!(params.voice.pattern_idx, base.pattern_idx,
            "seed=0 should not rotate pattern_idx");
        assert_eq!(params.voice.drum_hit_type, base.drum_hit_type,
            "seed=0 should not rotate drum_hit_type");
    }

    #[test]
    fn stop_and_submit_get_different_hit_types() {
        // Stop seed=3, UserPromptSubmit seed=5 — offset of 2 guarantees different hit type
        let stop = hook_play_args("Stop", "repo".into(), "main".into(), false);
        let submit = hook_play_args("UserPromptSubmit", "repo".into(), "main".into(), false);
        assert_ne!(stop.event_seed, submit.event_seed);
        // The actual hit type depends on the repo, but the seed difference of 2
        // means they'll always rotate to a different position in the 4-type cycle
        let stop_params = PhraseParams::from_identity("repo", "main", 200, 0.12,
            Effects { pad: false, chorus: false, tremolo: false, bulldozer: false,
                      drums: false, dub_delay: false, melody_over_drums: false, single_hit: true },
            1, false, EventCategory::DrumHit, stop.event_seed);
        let submit_params = PhraseParams::from_identity("repo", "main", 150, 0.08,
            Effects { pad: false, chorus: false, tremolo: false, bulldozer: false,
                      drums: false, dub_delay: false, melody_over_drums: false, single_hit: true },
            1, false, EventCategory::DrumHit, submit.event_seed);
        assert_ne!(stop_params.voice.drum_hit_type, submit_params.voice.drum_hit_type,
            "Stop and UserPromptSubmit should have different hit types for same repo");
    }

    #[test]
    fn session_start_and_end_get_different_melodies() {
        let start = hook_play_args("SessionStart", "repo".into(), "main".into(), false);
        let end = hook_play_args("SessionEnd", "repo".into(), "main".into(), false);
        assert_ne!(start.event_seed, end.event_seed);
        let start_params = PhraseParams::from_identity("repo", "main", 1500, 0.30, default_effects(), 5, false, EventCategory::SessionBoundary, start.event_seed);
        let end_params = PhraseParams::from_identity("repo", "main", 1500, 0.25, default_effects(), 5, false, EventCategory::SessionBoundary, end.event_seed);
        assert_ne!(start_params.notes, end_params.notes,
            "SessionStart and SessionEnd should have different melodies");
    }

    #[test]
    fn all_hook_events_have_unique_seeds() {
        let seeds: Vec<u8> = HOOK_EVENTS.iter()
            .map(|e| hook_play_args(e, "r".into(), "b".into(), false).event_seed)
            .collect();
        for i in 0..seeds.len() {
            for j in (i+1)..seeds.len() {
                assert_ne!(seeds[i], seeds[j],
                    "events {} and {} should have different seeds, both got {}",
                    HOOK_EVENTS[i], HOOK_EVENTS[j], seeds[i]);
            }
        }
    }

    // -- Init CLI arg tests --

    #[test]
    fn init_scope_default_is_user() {
        let cli = Cli::try_parse_from(["branch-tone", "init"]).unwrap();
        match cli.command {
            Some(Command::Init { scope, legacy }) => {
                assert_eq!(scope, "user", "default scope should be 'user'");
                assert!(!legacy, "legacy should default to false");
            }
            other => panic!("expected Init, got {:?}", other),
        }
    }

    #[test]
    fn init_legacy_flag_exists() {
        let cli = Cli::try_parse_from(["branch-tone", "init", "--legacy"]).unwrap();
        match cli.command {
            Some(Command::Init { scope, legacy }) => {
                assert!(legacy, "--legacy flag should be true");
                assert_eq!(scope, "user", "scope should still default to 'user'");
            }
            other => panic!("expected Init, got {:?}", other),
        }
    }

    #[test]
    fn init_scope_accepts_project() {
        let cli = Cli::try_parse_from(["branch-tone", "init", "--scope", "project"]).unwrap();
        match cli.command {
            Some(Command::Init { scope, .. }) => {
                assert_eq!(scope, "project");
            }
            other => panic!("expected Init, got {:?}", other),
        }
    }

    #[test]
    fn init_scope_accepts_local() {
        let cli = Cli::try_parse_from(["branch-tone", "init", "--scope", "local"]).unwrap();
        match cli.command {
            Some(Command::Init { scope, .. }) => {
                assert_eq!(scope, "local");
            }
            other => panic!("expected Init, got {:?}", other),
        }
    }

    // -- Legacy cleanup tests --

    #[test]
    fn legacy_cleanup_cli_parses() {
        let cli = Cli::try_parse_from(["branch-tone", "legacy-cleanup"]).unwrap();
        match cli.command {
            Some(Command::LegacyCleanup { scope }) => {
                assert_eq!(scope, "user", "default scope should be 'user'");
            }
            other => panic!("expected LegacyCleanup, got {:?}", other),
        }
    }

    #[test]
    fn legacy_cleanup_cli_accepts_scope() {
        let cli = Cli::try_parse_from(["branch-tone", "legacy-cleanup", "--scope", "project"]).unwrap();
        match cli.command {
            Some(Command::LegacyCleanup { scope }) => {
                assert_eq!(scope, "project");
            }
            other => panic!("expected LegacyCleanup, got {:?}", other),
        }
    }

    /// Helper: simulate the cleanup logic on an in-memory settings value
    fn apply_legacy_cleanup(settings: &mut serde_json::Value) {
        if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
            for event in HOOK_EVENTS {
                if let Some(event_hooks) = hooks.get_mut(event).and_then(|e| e.as_array_mut()) {
                    event_hooks.retain(|entry| {
                        if let Some(cmd) = entry.get("command").and_then(|c| c.as_str()) {
                            if cmd.contains("branch-tone") { return false; }
                        }
                        if let Some(inner) = entry.get("hooks").and_then(|h| h.as_array()) {
                            let all_bt = inner.iter().all(|h| {
                                h.get("command").and_then(|c| c.as_str())
                                    .map_or(false, |c| c.contains("branch-tone"))
                            });
                            if all_bt && !inner.is_empty() { return false; }
                        }
                        true
                    });
                }
            }
            let empty_events: Vec<String> = hooks.iter()
                .filter(|(_, v)| v.as_array().map_or(false, |a| a.is_empty()))
                .map(|(k, _)| k.clone())
                .collect();
            for key in empty_events {
                hooks.remove(&key);
            }
        }
        if settings.get("hooks").and_then(|h| h.as_object()).map_or(false, |h| h.is_empty()) {
            settings.as_object_mut().unwrap().remove("hooks");
        }

        if let Some(allow) = settings.pointer_mut("/permissions/allow").and_then(|a| a.as_array_mut()) {
            allow.retain(|v| v.as_str() != Some("Bash(branch-tone*)"));
        }
        if let Some(excluded) = settings.pointer_mut("/sandbox/excludedCommands").and_then(|a| a.as_array_mut()) {
            excluded.retain(|v| v.as_str() != Some("branch-tone"));
        }
    }

    #[test]
    fn legacy_cleanup_removes_branch_tone_hooks() {
        let mut settings = serde_json::json!({});
        apply_hook_init(&mut settings);

        // Add an unrelated hook to Stop
        settings["hooks"]["Stop"].as_array_mut().unwrap()
            .push(serde_json::json!({"hooks": [{"type": "command", "command": "other-tool"}]}));

        // Verify branch-tone hooks are present
        assert_eq!(settings["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);

        apply_legacy_cleanup(&mut settings);

        // branch-tone hooks should be gone
        // Stop should still have the other-tool hook
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1, "Stop should only have the unrelated hook");
        assert_eq!(stop[0]["hooks"][0]["command"], "other-tool");

        // Events that only had branch-tone hooks should be cleaned up
        assert!(settings["hooks"].get("SessionStart").is_none()
            || settings["hooks"]["SessionStart"].as_array().unwrap().is_empty()
            || settings.get("hooks").is_none(),
            "SessionStart should be empty or removed");
    }

    #[test]
    fn legacy_cleanup_removes_permissions_and_sandbox() {
        let mut settings = serde_json::json!({
            "permissions": { "allow": ["Bash(branch-tone*)", "Bash(other-tool*)"] },
            "sandbox": { "excludedCommands": ["branch-tone", "other-tool"] }
        });

        apply_legacy_cleanup(&mut settings);

        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0], "Bash(other-tool*)");

        let excluded = settings["sandbox"]["excludedCommands"].as_array().unwrap();
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0], "other-tool");
    }

    #[test]
    fn legacy_cleanup_preserves_unrelated_settings() {
        let mut settings = serde_json::json!({
            "hooks": {
                "Stop": [
                    {"hooks": [{"type": "command", "command": "other-tool"}]}
                ]
            },
            "someOtherKey": "preserved"
        });

        apply_legacy_cleanup(&mut settings);

        assert_eq!(settings["someOtherKey"], "preserved");
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(stop[0]["hooks"][0]["command"], "other-tool");
    }
}
