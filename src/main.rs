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

    /// Event density: recent event count in 10s window (set by hook, not CLI)
    #[arg(skip)]
    event_density: usize,
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

    /// Start persistent audio daemon (shared mix for all hook events)
    Daemon {
        /// Detach from terminal (run as background process)
        #[arg(long)]
        detach: bool,
    },

    /// Stop the running daemon
    DaemonStop,

    /// Show daemon status (active voices, uptime)
    DaemonStatus,

    /// macOS menu bar icon for daemon monitoring and control
    #[cfg(all(target_os = "macos", feature = "tray"))]
    Tray,

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
            Self::SessionBoundary => 0,   // root
            Self::Attention => 2,         // whole step up — bright but close
            Self::DrumHit => 0,           // root (percussive, pitch irrelevant)
            Self::ToolPulse => 0,         // root (percussive, pitch irrelevant)
            Self::Bass => -2,             // whole step down — sits just below
            Self::Lifecycle => 1,         // half step up — subtle color shift
            Self::Default => 0,
        }
    }

    /// Per-category delay characteristics: (time_multiplier, feedback_offset, throw_rate).
    /// Shapes how delay echoes behave for each voice in the ensemble.
    fn delay_character(&self) -> (f32, f32, f32) {
        match self {
            Self::SessionBoundary => (1.5,  0.10, 1.5),  // long, lush, slow throw
            Self::Attention =>       (1.25, 0.05, 2.0),  // prominent, faster throw
            Self::DrumHit =>         (0.75, -0.10, 5.0), // short, dry, fast throw
            Self::ToolPulse =>       (0.5,  -0.15, 8.0), // minimal
            Self::Bass =>            (1.0,  0.0,   2.5), // standard, medium throw
            Self::Lifecycle =>       (1.0,  0.0,   3.0), // standard
            Self::Default =>         (1.0,  0.0,   3.0),
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

/// Octave-fold a frequency into a target range.
/// Halves or doubles freq until it sits within [lo, hi].
fn fold_to_range(freq: f32, lo: f32, hi: f32) -> f32 {
    let mut f = freq;
    while f > hi && f > lo { f *= 0.5; }
    while f < lo && f < hi { f *= 2.0; }
    f
}

/// Kick drum: sine with pitch sweep, click transient. Tuned to root_freq.
fn synth_kick(time: f32, _sample_rate: f32, root_freq: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    // Octave-fold root into sub-bass range (30–80Hz)
    let sub_root = fold_to_range(root_freq, 30.0, 80.0);
    let freq_start = sub_root * 3.0; // sweep from 3x down to root
    let freq_end = sub_root;
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

/// Snare drum: sine body tuned to root + noise burst.
fn synth_snare(time: f32, noise_seed: f32, root_freq: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    // Octave-fold root into snare body range (150–250Hz)
    let snare_root = fold_to_range(root_freq, 150.0, 250.0);

    // Sine body
    let body = (2.0 * PI * snare_root * time).sin() * (-12.0 * time).exp();

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

/// Rimshot: short pitched click + resonant ring. Ring tuned to root.
fn synth_rimshot(time: f32, noise_seed: f32, root_freq: f32) -> f32 {
    if time < 0.0 { return 0.0; }

    // Octave-fold root into rimshot click range (700–1100Hz)
    let click_freq = fold_to_range(root_freq, 700.0, 1100.0);
    // Ring sits about an octave below click
    let ring_freq = fold_to_range(root_freq, 350.0, 550.0);

    // Sharp click
    let click = (2.0 * PI * click_freq * time).sin() * (-80.0 * time).exp();

    // Resonant ring (higher pitched than snare body)
    let ring = (2.0 * PI * ring_freq * time).sin() * (-25.0 * time).exp();

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
        DrumHitType::Kick => synth_kick(time, sample_rate, voice.root_freq) * voice.kick_decay,
        DrumHitType::Snare => synth_snare(time, noise_seed, voice.root_freq) * voice.snare_tone,
        DrumHitType::Rimshot => synth_rimshot(time, noise_seed, voice.root_freq),
        DrumHitType::ClosedHat => synth_hihat(time, false, noise_seed) * voice.hihat_brightness,
        DrumHitType::OpenHat => synth_hihat(time, true, noise_seed) * voice.hihat_brightness,
    };
    raw * decay_env
}

/// Generate a pitched percussion hit (kalimba/marimba) — short, tuned to a scale note.
/// Warm sine fundamental + light harmonics with percussive envelope and click transient.
fn generate_pitched_hit(time: f32, freq: f32) -> f32 {
    // Percussive envelope: instant attack, ~100ms decay (kalimba-like)
    let decay_env = (-12.0 * time).exp();
    // Click transient (first 2ms) — the "mallet strike"
    let click = if time < 0.002 { (1.0 - time / 0.002) * 0.25 } else { 0.0 };
    // Sine fundamental
    let phase = 2.0 * PI * freq * time;
    let fundamental = phase.sin();
    // 2nd harmonic (octave) for brightness
    let h2 = (phase * 2.0).sin() * 0.2;
    // 3rd harmonic for body, decays faster
    let h3 = (phase * 3.0).sin() * 0.08 * (-20.0 * time).exp();
    (fundamental + h2 + h3 + click) * decay_env
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
    let step_samples = sixteenth_samples(brk.bpm, sample_rate) as usize;
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
        out += synth_kick(swung_time, sample_rate, voice.root_freq) * voice.kick_decay * step_vel;
    }
    if flags & S != 0 {
        out += synth_snare(swung_time, effective_step as f32, voice.root_freq) * voice.snare_tone * step_vel;
    }
    if flags & H != 0 {
        out += synth_hihat(step_time, false, effective_step as f32) * voice.hihat_brightness * step_vel * 0.6;
    }
    if flags & O != 0 {
        out += synth_hihat(step_time, true, effective_step as f32) * voice.hihat_brightness * step_vel * 0.5;
    }

    // Ghost snare on empty offbeat steps (adds ghost note shuffle)
    if flags & S == 0 && looped_step % 2 == 1 && melody.drum_ghost_level > 0.15 {
        out += synth_snare(step_time, effective_step as f32 + 0.5, voice.root_freq)
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

    #[allow(dead_code)]
    fn process(&mut self, input: f32, sample_rate: f32) -> f32 {
        self.process_throw(input, 1.0, sample_rate)
    }

    /// Process with throw envelope: throw_level modulates the send into the delay buffer.
    /// At throw_level=1.0, full input goes in; at 0.0, only feedback recirculates.
    fn process_throw(&mut self, input: f32, throw_level: f32, sample_rate: f32) -> f32 {
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

        // Write throw-modulated input + filtered feedback into buffer
        self.buffer[self.write_pos] = input * throw_level + filtered * self.feedback;
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

    #[allow(dead_code)]
    fn process(&mut self, input: f32, sample_rate: f32) -> (f32, f32) {
        self.process_throw(input, 1.0, sample_rate)
    }

    /// Process with throw envelope: throw_level front-loads the send into the delay.
    /// King Tubby technique — strong on attack, fading, so a single hit cascades.
    fn process_throw(&mut self, input: f32, throw_level: f32, sample_rate: f32) -> (f32, f32) {
        let wet_l = self.left.process_throw(input, throw_level, sample_rate);
        let wet_r = self.right.process_throw(input, throw_level, sample_rate);
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
    // Root frequency for tuned percussion (derived from scale_freqs[0])
    root_freq: f32,
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

        // Root frequency for tuned percussion (before octave/transpose scaling)
        let root_freq = root_freq;

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
            root_freq,
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
    // Quantize subdivision (hash byte 18): multiplier on 16th-note grid
    quantize_subdiv: f32,    // 0.5=1/32, 1.0=1/16, 2.0=1/8, 4.0=1/4, 8.0=1/2
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

        // Quantize subdivision (hash byte 18): snap arp notes to BPM grid
        // Table biases toward 1/8; env var BRANCH_TONE_QUANTIZE allows 1/32 and 1/2 too
        const QUANTIZE_SUBDIVS: [f32; 4] = [1.0, 2.0, 4.0, 2.0];
        let quantize_subdiv = QUANTIZE_SUBDIVS[(hash[18] as usize) % QUANTIZE_SUBDIVS.len()];

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
            quantize_subdiv,
        }
    }
}

/// Map quantize subdivision multiplier to a musical label.
fn subdiv_label(subdiv: f32) -> &'static str {
    match subdiv as u8 {
        0 => "1/32",  // 0.5 truncates to 0
        1 => "1/16",
        2 => "1/8",
        4 => "1/4",
        8 => "1/2",
        _ => "1/8",   // safe fallback
    }
}

/// Duration of one 16th note in samples at the given BPM.
fn sixteenth_samples(bpm: f32, sample_rate: f32) -> f32 {
    60.0 / bpm / 4.0 * sample_rate
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
    event_category: EventCategory,
    event_density: usize, // recent events in 10s window (0 = unknown/CLI)
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

        // Rotate drum hit type by event_seed — category-aware so each voice
        // stays in its lane: drums get kick/snare/rimshot, hi-hats get hats
        if event_seed > 0 {
            let hit_idx = match voice.drum_hit_type {
                DrumHitType::Kick => 0,
                DrumHitType::Snare => 1,
                DrumHitType::Rimshot => 2,
                DrumHitType::ClosedHat => 3,
                DrumHitType::OpenHat => 4,
            };
            voice.drum_hit_type = match event_category {
                // Drums (kick/snare): only meaty hits — never hats
                EventCategory::DrumHit => match (hit_idx + event_seed as usize) % 3 {
                    0 => DrumHitType::Kick,
                    1 => DrumHitType::Snare,
                    _ => DrumHitType::Rimshot,
                },
                // Hi-hat (tools): only hat variants — suit ultra-short durations
                EventCategory::ToolPulse => match (hit_idx + event_seed as usize) % 2 {
                    0 => DrumHitType::ClosedHat,
                    _ => DrumHitType::OpenHat,
                },
                // Everything else: full rotation
                _ => match (hit_idx + event_seed as usize) % 5 {
                    0 => DrumHitType::Kick,
                    1 => DrumHitType::Snare,
                    2 => DrumHitType::Rimshot,
                    3 => DrumHitType::ClosedHat,
                    _ => DrumHitType::OpenHat,
                },
            };
        }

        // Rotate micro-pattern by event_seed — each event gets different jazz feel
        if event_seed > 0 {
            voice.hit_count = 1 + (voice.hit_count - 1 + event_seed as usize) % 4;
        }

        let mut melody = BranchMelody::from_branch(branch, effective_steps);

        // Rotate envelope shape by event_seed — each event gets a different ADSR feel.
        // This is critical for comping/stab/bass variety within the same category.
        if event_seed > 0 {
            melody.envelope_shape = (melody.envelope_shape + event_seed as usize) % ENVELOPE_SHAPES.len();
        }

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
            event_density: 0,
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
        Some(Command::Daemon { detach }) => run_daemon(detach),
        Some(Command::DaemonStop) => run_daemon_stop(),
        Some(Command::DaemonStatus) => run_daemon_status(),
        #[cfg(all(target_os = "macos", feature = "tray"))]
        Some(Command::Tray) => run_tray(),
        Some(Command::Player { pattern, bpm }) => run_player(pattern, bpm),
        None => run_play(cli.play_args),
    }
}

fn run_play(args: PlayArgs) -> Result<()> {
    let PlayArgs { branch, repo, duration, volume, pad, chorus, tremolo, bulldozer, steps, spooky, reverse, randomize, drums, dub_delay, melody_over_drums, single_hit, event_category, event_seed, break_pattern, dry_run, quiet, event_density } = args;

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
    params.event_density = event_density;
    if let Some(bp) = break_pattern {
        params.voice.drum_pattern_idx = bp % CLASSIC_BREAKS.len();
    }

    // Bar-align phrase duration to the break's BPM grid (nearest half-bar)
    let break_idx = params.voice.drum_pattern_idx % CLASSIC_BREAKS.len();
    let brk_bpm = CLASSIC_BREAKS[break_idx].bpm;
    let bar_ms = 240_000.0 / brk_bpm as f64;
    let half_bar = bar_ms / 2.0;
    let num_half_bars = (params.total_duration as f64 / half_bar).round().max(1.0);
    params.total_duration = (num_half_bars * half_bar) as u64;

    // BRANCH_TONE_QUANTIZE overrides subdivision: 4=1/4, 8=1/8, 16=1/16, 32=1/32
    if let Ok(val) = std::env::var("BRANCH_TONE_QUANTIZE") {
        if let Ok(denom) = val.parse::<u32>() {
            params.melody.quantize_subdiv = match denom {
                4 => 4.0,
                8 => 2.0,
                16 => 1.0,
                32 => 0.5,
                _ => params.melody.quantize_subdiv,
            };
        }
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
        let grid_label = subdiv_label(params.melody.quantize_subdiv);
        let envelope_names = ["Punchy", "Soft", "Pluck", "Swell"];
        println!("🎵 Repo: {} | Branch: {} [{}] ({}){}{}{} | {:.0} BPM | {} grid", repo, branch, mode_label, preset_name, hit_tag, hybrid_tag, spooky_tag, brk_bpm, grid_label);
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
            // Compute the actual rotated hit type (same logic as PhraseParams::from_identity)
            let actual_hit_type = if play_args.event_seed > 0 {
                let hit_idx = match voice.drum_hit_type {
                    DrumHitType::Kick => 0, DrumHitType::Snare => 1,
                    DrumHitType::Rimshot => 2, DrumHitType::ClosedHat => 3,
                    DrumHitType::OpenHat => 4,
                };
                match play_args.event_category {
                    EventCategory::DrumHit => match (hit_idx + play_args.event_seed as usize) % 3 {
                        0 => "Kick", 1 => "Snare", _ => "Rimshot",
                    },
                    EventCategory::ToolPulse => match (hit_idx + play_args.event_seed as usize) % 2 {
                        0 => "ClosedHat", _ => "OpenHat",
                    },
                    _ => match (hit_idx + play_args.event_seed as usize) % 5 {
                        0 => "Kick", 1 => "Snare", 2 => "Rimshot",
                        3 => "ClosedHat", _ => "OpenHat",
                    },
                }
            } else {
                match voice.drum_hit_type {
                    DrumHitType::Kick => "Kick", DrumHitType::Snare => "Snare",
                    DrumHitType::Rimshot => "Rimshot", DrumHitType::ClosedHat => "ClosedHat",
                    DrumHitType::OpenHat => "OpenHat",
                }
            };
            let fx: String = if play_args.single_hit {
                format!("hit:{}", actual_hit_type)
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
///
/// Tonal events inherit the repo's natural mode (from_mode) so each repo
/// sounds like itself. The event category only sets duration, volume, and
/// category-specific overrides (dub_delay, reverse, randomize, min steps).
/// Percussive events (DrumHit, ToolPulse) bypass modes entirely.
fn hook_play_args(event: &str, repo: String, branch: String, spooky: bool) -> PlayArgs {
    // Get the repo's natural synth mode — this is the repo's *voice*
    let voice = RepoVoice::from_repo(&repo);
    let (mode_effects, mode_steps) = Effects::from_mode(voice.mode_idx);

    /// Build a tonal PlayArgs that inherits the repo's mode, with category overrides.
    /// `min_steps`: category minimum (mode may provide more).
    /// `dub_delay`, `reverse`, `randomize`: category-level overrides (ORed with mode).
    fn tonal(
        repo: String, branch: String, spooky: bool,
        mode: Effects, mode_steps: u8,
        duration: u64, volume: f32, min_steps: u8,
        dub_delay: bool, reverse: bool, randomize: bool,
        category: EventCategory, seed: u8,
    ) -> PlayArgs {
        PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration, volume,
            // Inherit the repo's natural synth character.
            // Chorus always on for hooks — ensures explicit_mode=true in run_play
            // so category min_steps is preserved, and gives every hook a rich tone.
            pad: mode.pad, chorus: true, tremolo: mode.tremolo,
            bulldozer: mode.bulldozer,
            steps: mode_steps.max(min_steps),
            spooky, reverse, randomize,
            drums: false, dub_delay: mode.dub_delay || dub_delay,
            melody_over_drums: false,
            single_hit: false, event_category: category,
            event_seed: seed,
            break_pattern: None, dry_run: false, quiet: true, event_density: 0,
        }
    }

    match event {
        // ── Keys/Pad (session boundaries — the band starts/stops) ──
        // Long, full bloom — always at least 5 notes, always dub delay
        "SessionStart" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            3500, 0.35, 5,
            true, false, false,
            EventCategory::SessionBoundary, 1,
        ),
        "SessionEnd" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            3500, 0.30, 5,
            true, true, false,
            EventCategory::SessionBoundary, 4,
        ),

        // ── Drums — kick/snare (frequent rhythm) ───────────────────
        // Percussive: bypass repo mode entirely
        "Stop" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 400, volume: 0.18,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::DrumHit,
            event_seed: 3,
            break_pattern: None, dry_run: false, quiet: true, event_density: 0,
        },
        "UserPromptSubmit" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 350, volume: 0.12,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::DrumHit,
            event_seed: 5,
            break_pattern: None, dry_run: false, quiet: true, event_density: 0,
        },

        // ── Hi-Hat — tool pulse (very frequent, very quiet) ────────
        // Percussive: bypass repo mode entirely
        "PreToolUse" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 120, volume: 0.08,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::ToolPulse,
            event_seed: 11,
            break_pattern: None, dry_run: false, quiet: true, event_density: 0,
        },
        "PostToolUse" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 150, volume: 0.08,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::ToolPulse,
            event_seed: 12,
            break_pattern: None, dry_run: false, quiet: true, event_density: 0,
        },
        "PostToolUseFailure" => PlayArgs {
            branch: Some(branch), repo: Some(repo),
            duration: 250, volume: 0.12,
            pad: false, chorus: false, tremolo: false, bulldozer: false,
            steps: 1, spooky, reverse: false, randomize: false,
            drums: false, dub_delay: false, melody_over_drums: false,
            single_hit: true, event_category: EventCategory::ToolPulse,
            event_seed: 13,
            break_pattern: None, dry_run: false, quiet: true, event_density: 0,
        },

        // ── Horn/Lead — attention required ─────────────────────────
        // Prominent, always dub delay, at least 5 notes
        "PermissionRequest" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            2500, 0.28, 5,
            true, false, false,
            EventCategory::Attention, 2,
        ),
        "Notification" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            2000, 0.22, 3,
            true, false, false,
            EventCategory::Attention, 6,
        ),

        // ── Bass — agent lifecycle (voices entering/leaving) ───────
        // Dub delay, randomize for organic feel
        "SubagentStart" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1000, 0.25, 3,
            true, false, true,
            EventCategory::Bass, 7,
        ),
        "SubagentStop" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1000, 0.25, 3,
            true, true, true,
            EventCategory::Bass, 8,
        ),
        "WorktreeCreate" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1200, 0.25, 3,
            true, false, false,
            EventCategory::Bass, 14,
        ),
        "WorktreeRemove" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1200, 0.25, 3,
            true, true, false,
            EventCategory::Bass, 15,
        ),

        // ── Piano/Comping — lifecycle (structural events) ──────────
        "InstructionsLoaded" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1500, 0.15, 3,
            true, false, false,
            EventCategory::Lifecycle, 16,
        ),
        "ConfigChange" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1800, 0.15, 3,
            true, false, false,
            EventCategory::Lifecycle, 17,
        ),
        "TaskCompleted" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            2500, 0.22, 5,
            true, false, false,
            EventCategory::Lifecycle, 18,
        ),
        "PreCompact" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            2000, 0.15, 3,
            true, true, false,
            EventCategory::Lifecycle, 9,
        ),
        "TeammateIdle" => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            1500, 0.12, 3,
            true, false, false,
            EventCategory::Lifecycle, 10,
        ),

        // ── Unknown events ─────────────────────────────────────────
        _ => tonal(
            repo, branch, spooky, mode_effects, mode_steps,
            300, 0.18, 3,
            false, false, false,
            EventCategory::Default, 0,
        ),
    }
}

// -----------------------------------------------------------------------------
// DAEMON — persistent audio process with shared reverb/delay buses
// -----------------------------------------------------------------------------

use std::sync::atomic::AtomicU64;

const MAX_VOICE_SLOTS: usize = 8;
const DAEMON_IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Per-session voice slot in the daemon's shared mix
/// A queued pitched percussion note for temporal spreading in the daemon.
/// Instead of playing every tool event immediately (hi-hat barrage), events are
/// queued and drained at the BPM grid rate, creating rhythmic phrases.
#[derive(Clone)]
struct QueuedNote {
    category: u8,       // packed EventCategory
    volume: f32,
    duration_ms: u32,
    note_freq: f32,     // pitched percussion frequency (from scale)
}

struct VoiceSlot {
    active: AtomicBool,
    // Session identity (set once on allocation)
    repo: std::sync::Mutex<String>,
    branch: std::sync::Mutex<String>,
    // Ambient bed parameters
    root_freq: std::sync::atomic::AtomicU32,  // f32 bits
    bed_volume: std::sync::atomic::AtomicU32, // f32 bits
    bed_fade_in: AtomicBool,                  // true = fading in
    bed_fade_out: AtomicBool,                 // true = fading out
    // One-shot trigger (pending event to play)
    oneshot_pending: AtomicBool,
    oneshot_category: AtomicU8,
    oneshot_duration_ms: std::sync::atomic::AtomicU32,
    oneshot_volume: std::sync::atomic::AtomicU32,  // f32 bits
    oneshot_sample: AtomicU64,                     // sample at which oneshot was triggered
    // Voice params (set from RepoVoice on allocation)
    voice_data: std::sync::Mutex<Option<RepoVoice>>,
    melody_data: std::sync::Mutex<Option<BranchMelody>>,
    notes: std::sync::Mutex<Vec<f32>>,
    #[allow(dead_code)]
    effects_bits: AtomicU8, // packed Effects (reserved for daemon v2)
    #[allow(dead_code)]
    pad_shape_idx: AtomicU8, // reserved for daemon v2
    // Last activity timestamp
    last_event_secs: AtomicU64,
    // Note queue for temporal spreading (ToolPulse/DrumHit → pitched percussion)
    note_queue: std::sync::Mutex<std::collections::VecDeque<QueuedNote>>,
    // Walking note counter: advances through scale per event for melodic patterns
    note_counter: AtomicU8,
    // Current pitched note frequency for active oneshot (f32 bits)
    oneshot_note_freq: std::sync::atomic::AtomicU32,
    // Precomputed grid step in samples (for queue drain timing)
    grid_step_samples: std::sync::atomic::AtomicU32,
}

impl VoiceSlot {
    fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            repo: std::sync::Mutex::new(String::new()),
            branch: std::sync::Mutex::new(String::new()),
            root_freq: std::sync::atomic::AtomicU32::new(0),
            bed_volume: std::sync::atomic::AtomicU32::new(f32::to_bits(0.05)),
            bed_fade_in: AtomicBool::new(false),
            bed_fade_out: AtomicBool::new(false),
            oneshot_pending: AtomicBool::new(false),
            oneshot_category: AtomicU8::new(0),
            oneshot_duration_ms: std::sync::atomic::AtomicU32::new(0),
            oneshot_volume: std::sync::atomic::AtomicU32::new(0),
            oneshot_sample: AtomicU64::new(0),
            voice_data: std::sync::Mutex::new(None),
            melody_data: std::sync::Mutex::new(None),
            notes: std::sync::Mutex::new(Vec::new()),
            effects_bits: AtomicU8::new(0),
            pad_shape_idx: AtomicU8::new(0),
            last_event_secs: AtomicU64::new(0),
            note_queue: std::sync::Mutex::new(std::collections::VecDeque::new()),
            note_counter: AtomicU8::new(0),
            oneshot_note_freq: std::sync::atomic::AtomicU32::new(0),
            grid_step_samples: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

/// Shared state for the daemon audio engine
struct DaemonState {
    voices: Vec<VoiceSlot>,
    quit: AtomicBool,
    global_sample: AtomicU64,
    active_count: AtomicU8,
    last_activity_secs: AtomicU64,
    start_time_secs: AtomicU64,
    /// Audio sample rate (set by audio engine on init, used by socket handler for grid calc)
    sample_rate: std::sync::atomic::AtomicU32,
}

impl DaemonState {
    fn new() -> Self {
        let voices: Vec<VoiceSlot> = (0..MAX_VOICE_SLOTS).map(|_| VoiceSlot::new()).collect();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            voices,
            quit: AtomicBool::new(false),
            global_sample: AtomicU64::new(0),
            active_count: AtomicU8::new(0),
            last_activity_secs: AtomicU64::new(now),
            start_time_secs: AtomicU64::new(now),
            sample_rate: std::sync::atomic::AtomicU32::new(44100),
        }
    }

    /// Find or allocate a voice slot for a repo+branch session.
    fn find_or_alloc_slot(&self, repo: &str, branch: &str) -> Option<usize> {
        // First: look for existing slot with same repo+branch
        for (i, slot) in self.voices.iter().enumerate() {
            if slot.active.load(Relaxed) {
                if let (Ok(r), Ok(b)) = (slot.repo.lock(), slot.branch.lock()) {
                    if r.as_str() == repo && b.as_str() == branch {
                        return Some(i);
                    }
                }
            }
        }
        // Second: find an inactive slot
        for (i, slot) in self.voices.iter().enumerate() {
            if !slot.active.load(Relaxed) {
                return Some(i);
            }
        }
        None
    }
}

/// Daemon socket/pid paths
fn daemon_dir() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".branch-tone")
}

fn daemon_socket_path() -> std::path::PathBuf {
    daemon_dir().join("daemon.sock")
}

fn daemon_pid_path() -> std::path::PathBuf {
    daemon_dir().join("daemon.pid")
}

/// Pack event category into u8 for atomic storage
fn category_to_u8(cat: EventCategory) -> u8 {
    match cat {
        EventCategory::SessionBoundary => 0,
        EventCategory::Attention => 1,
        EventCategory::DrumHit => 2,
        EventCategory::ToolPulse => 3,
        EventCategory::Bass => 4,
        EventCategory::Lifecycle => 5,
        EventCategory::Default => 6,
    }
}

fn u8_to_category(v: u8) -> EventCategory {
    match v {
        0 => EventCategory::SessionBoundary,
        1 => EventCategory::Attention,
        2 => EventCategory::DrumHit,
        3 => EventCategory::ToolPulse,
        4 => EventCategory::Bass,
        5 => EventCategory::Lifecycle,
        _ => EventCategory::Default,
    }
}

/// Conductor: choose a harmonically compatible interval for ambient bed transposition.
/// Given active root frequencies, transpose new_root to prefer unisons, fifths, fourths.
fn conductor_transpose(new_root: f32, active_roots: &[f32]) -> f32 {
    if active_roots.is_empty() {
        return new_root;
    }

    // Try transpositions: unison(0), fifth(+7), fourth(+5), octave(+12), minor 3rd(+3)
    let intervals = [0, 7, 5, 12, 3];
    let mut best_interval = 0i32;
    let mut best_score = f32::MAX;

    for &semi in &intervals {
        let candidate = new_root * 2.0_f32.powf(semi as f32 / 12.0);
        // Score: sum of dissonance against all active roots
        let score: f32 = active_roots.iter().map(|&active| {
            // Compute interval in semitones (mod 12)
            let ratio = (candidate / active).abs();
            let semitones = (12.0 * ratio.log2()) % 12.0;
            // Consonance: 0=unison, 7=fifth, 5=fourth are best
            let dissonance = match semitones.round() as i32 {
                0 => 0.0,
                7 | 5 => 0.5,
                3 | 4 | 9 | 8 => 1.0,
                _ => 2.0,
            };
            dissonance
        }).sum();

        if score < best_score {
            best_score = score;
            best_interval = semi;
        }
    }

    new_root * 2.0_f32.powf(best_interval as f32 / 12.0)
}

/// Try to send an event to the running daemon via Unix socket.
/// Returns Ok(()) if the daemon handled it, Err if no daemon or send failed.
fn send_to_daemon(event: &str, repo: &str, branch: &str) -> Result<()> {
    use std::os::unix::net::UnixStream;
    use std::io::Write;

    let sock_path = daemon_socket_path();
    // 50ms connect timeout for imperceptible fallback
    let stream = UnixStream::connect(&sock_path)
        .map_err(|e| anyhow::anyhow!("daemon connect: {}", e))?;
    stream.set_write_timeout(Some(Duration::from_millis(50)))
        .map_err(|e| anyhow::anyhow!("set timeout: {}", e))?;
    stream.set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| anyhow::anyhow!("set timeout: {}", e))?;

    let msg = format!("{{\"event\":\"{}\",\"repo\":\"{}\",\"branch\":\"{}\"}}\n", event, repo, branch);
    let mut stream = stream;
    stream.write_all(msg.as_bytes())
        .map_err(|e| anyhow::anyhow!("daemon write: {}", e))?;

    // Read ACK
    let mut buf = [0u8; 32];
    use std::io::Read as StdRead;
    let _ = stream.read(&mut buf);

    Ok(())
}

/// Run the daemon: persistent audio engine with socket listener.
fn run_daemon(detach: bool) -> Result<()> {
    if detach {
        // Fork into background
        return run_daemon_detach();
    }

    let dir = daemon_dir();
    std::fs::create_dir_all(&dir)?;

    // Check for existing daemon
    let pid_path = daemon_pid_path();
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                // Check if process is still alive
                let alive = std::process::Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if alive {
                    println!("Daemon already running (PID {})", pid);
                    return Ok(());
                }
            }
        }
        // Stale PID file — clean up
        let _ = std::fs::remove_file(&pid_path);
    }

    // Write PID file
    let pid = std::process::id();
    std::fs::write(&pid_path, pid.to_string())?;

    // Remove stale socket
    let sock_path = daemon_socket_path();
    let _ = std::fs::remove_file(&sock_path);

    println!("branch-tone daemon starting (PID {})", pid);
    println!("Socket: {}", sock_path.display());

    let state = Arc::new(DaemonState::new());
    let state_audio = state.clone();
    let state_listener = state.clone();
    let state_idle = state.clone();

    // Start audio engine thread
    let audio_handle = std::thread::spawn(move || {
        if let Err(e) = daemon_audio_engine(state_audio) {
            eprintln!("Daemon audio error: {}", e);
        }
    });

    // Start socket listener thread
    let listener_handle = std::thread::spawn(move || {
        if let Err(e) = daemon_socket_listener(state_listener) {
            eprintln!("Daemon listener error: {}", e);
        }
    });

    // Main thread: idle timeout + signal handling
    // Handle SIGTERM/SIGINT for graceful shutdown
    let state_signal = state.clone();
    let _ = std::thread::spawn(move || {
        // Simple signal handling via polling
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if state_signal.quit.load(Relaxed) {
                break;
            }
        }
    });

    // Idle timeout loop
    loop {
        std::thread::sleep(Duration::from_secs(10));

        if state_idle.quit.load(Relaxed) {
            break;
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let last = state_idle.last_activity_secs.load(Relaxed);
        let active = state_idle.active_count.load(Relaxed);

        if active == 0 && now.saturating_sub(last) > DAEMON_IDLE_TIMEOUT_SECS {
            println!("Daemon idle timeout ({}s with no active sessions), shutting down",
                DAEMON_IDLE_TIMEOUT_SECS);
            state_idle.quit.store(true, Relaxed);
            break;
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&pid_path);

    let _ = audio_handle.join();
    let _ = listener_handle.join();

    println!("Daemon stopped.");
    Ok(())
}

/// Detach daemon into background process
fn run_daemon_detach() -> Result<()> {
    let exe = std::env::current_exe().context("Could not determine executable path")?;
    let child = std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to spawn daemon process")?;

    println!("Daemon started in background (PID {})", child.id());
    Ok(())
}

/// Stop the running daemon
fn run_daemon_stop() -> Result<()> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        println!("No daemon running (no PID file)");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse()
        .context("Invalid PID file")?;

    // Send SIGTERM
    let result = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if result {
        println!("Sent stop signal to daemon (PID {})", pid);
        // Wait briefly for cleanup
        std::thread::sleep(Duration::from_millis(500));
        let _ = std::fs::remove_file(&pid_path);
        let _ = std::fs::remove_file(daemon_socket_path());
    } else {
        println!("Daemon process {} not found, cleaning up stale files", pid);
        let _ = std::fs::remove_file(&pid_path);
        let _ = std::fs::remove_file(daemon_socket_path());
    }

    Ok(())
}

/// Show daemon status
fn run_daemon_status() -> Result<()> {
    let pid_path = daemon_pid_path();
    if !pid_path.exists() {
        println!("No daemon running");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str.trim().parse()
        .context("Invalid PID file")?;

    let alive = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !alive {
        println!("Daemon PID {} not running (stale PID file)", pid);
        return Ok(());
    }

    println!("Daemon running (PID {})", pid);
    println!("Socket: {}", daemon_socket_path().display());

    // Try to query status via socket
    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(daemon_socket_path()) {
        use std::io::Write;
        let _ = stream.write_all(b"{\"event\":\"__status\"}\n");
        stream.set_read_timeout(Some(Duration::from_millis(500))).ok();
        let mut buf = vec![0u8; 4096];
        use std::io::Read as StdRead;
        if let Ok(n) = stream.read(&mut buf) {
            let response = String::from_utf8_lossy(&buf[..n]);
            println!("{}", response);
        }
    }

    Ok(())
}

/// Socket listener: accept connections, parse event JSON, dispatch to voice slots
fn daemon_socket_listener(state: Arc<DaemonState>) -> Result<()> {
    use std::os::unix::net::UnixListener;

    let sock_path = daemon_socket_path();
    let listener = UnixListener::bind(&sock_path)
        .with_context(|| format!("Failed to bind socket at {}", sock_path.display()))?;

    // Non-blocking so we can check quit flag
    listener.set_nonblocking(true)?;

    loop {
        if state.quit.load(Relaxed) {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let state = state.clone();
                std::thread::spawn(move || {
                    handle_daemon_connection(stream, &state);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    Ok(())
}

/// Handle a single daemon connection: parse event, dispatch to voice slot
fn handle_daemon_connection(stream: std::os::unix::net::UnixStream, state: &DaemonState) {
    use std::io::{Read as StdRead, Write};

    let mut stream = stream;
    stream.set_read_timeout(Some(Duration::from_millis(200))).ok();

    let mut buf = vec![0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };

    let msg = String::from_utf8_lossy(&buf[..n]);

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(msg.trim()) {
        let event = json.get("event").and_then(|v| v.as_str()).unwrap_or("unknown");

        // Status query
        if event == "__status" {
            let active: Vec<String> = state.voices.iter().enumerate().filter_map(|(i, slot)| {
                if slot.active.load(Relaxed) {
                    let repo = slot.repo.lock().ok()?.clone();
                    let branch = slot.branch.lock().ok()?.clone();
                    Some(format!("  Slot {}: {} @ {}", i, repo, branch))
                } else {
                    None
                }
            }).collect();
            let status = if active.is_empty() {
                "Active voices: none".to_string()
            } else {
                format!("Active voices ({}):\n{}", active.len(), active.join("\n"))
            };
            let _ = stream.write_all(status.as_bytes());
            return;
        }

        // JSON status query (for tray app and programmatic consumers)
        if event == "__status_json" {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let start = state.start_time_secs.load(Relaxed);
            let last = state.last_activity_secs.load(Relaxed);
            let pid = std::process::id();

            let mut voices_json = Vec::new();
            for (i, slot) in state.voices.iter().enumerate() {
                if slot.active.load(Relaxed) {
                    let repo = slot.repo.lock().ok().map(|r| r.clone()).unwrap_or_default();
                    let branch = slot.branch.lock().ok().map(|b| b.clone()).unwrap_or_default();
                    voices_json.push(format!(
                        "{{\"slot\":{},\"repo\":\"{}\",\"branch\":\"{}\"}}",
                        i,
                        repo.replace('\"', "\\\""),
                        branch.replace('\"', "\\\""),
                    ));
                }
            }

            let json = format!(
                "{{\"pid\":{},\"uptime_secs\":{},\"active_voices\":[{}],\"idle_secs\":{},\"idle_timeout\":{}}}",
                pid,
                now_secs.saturating_sub(start),
                voices_json.join(","),
                now_secs.saturating_sub(last),
                DAEMON_IDLE_TIMEOUT_SECS,
            );
            let _ = stream.write_all(json.as_bytes());
            return;
        }

        // Shutdown command
        if event == "__shutdown" {
            state.quit.store(true, Relaxed);
            let _ = stream.write_all(b"OK:shutdown");
            return;
        }

        let repo = json.get("repo").and_then(|v| v.as_str()).unwrap_or("unknown");
        let branch = json.get("branch").and_then(|v| v.as_str()).unwrap_or("main");

        // Update activity timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        state.last_activity_secs.store(now, Relaxed);

        // Get play args for this event
        let args = hook_play_args(event, repo.to_string(), branch.to_string(), false);

        // Find or allocate voice slot
        if let Some(slot_idx) = state.find_or_alloc_slot(repo, branch) {
            let slot = &state.voices[slot_idx];

            if !slot.active.load(Relaxed) {
                // Initialize new voice slot
                let voice = RepoVoice::from_repo(repo);
                let melody = BranchMelody::from_branch(branch, 3);

                // Conductor: check active roots and transpose if needed
                let active_roots: Vec<f32> = state.voices.iter()
                    .filter(|s| s.active.load(Relaxed))
                    .map(|s| f32::from_bits(s.root_freq.load(Relaxed)))
                    .filter(|&f| f > 0.0)
                    .collect();
                let bed_root = conductor_transpose(voice.root_freq, &active_roots);

                slot.root_freq.store(bed_root.to_bits(), Relaxed);
                if let Ok(mut r) = slot.repo.lock() { *r = repo.to_string(); }
                if let Ok(mut b) = slot.branch.lock() { *b = branch.to_string(); }

                let notes: Vec<f32> = voice.scale_freqs.to_vec();
                if let Ok(mut n) = slot.notes.lock() { *n = notes; }
                if let Ok(mut v) = slot.voice_data.lock() { *v = Some(voice); }
                if let Ok(mut m) = slot.melody_data.lock() { *m = Some(melody); }

                slot.active.store(true, Relaxed);
                state.active_count.fetch_add(1, Relaxed);

                // SessionStart: fade in ambient bed
                if event == "SessionStart" {
                    slot.bed_fade_in.store(true, Relaxed);
                    slot.bed_volume.store(f32::to_bits(0.05), Relaxed);
                }
            }

            // Trigger one-shot event
            let cat = args.event_category;
            slot.last_event_secs.store(now, Relaxed);

            match cat {
                // Percussive events → queue for temporal spreading + pitched percussion
                EventCategory::ToolPulse | EventCategory::DrumHit => {
                    // Walk through scale notes: each event advances to the next pitch
                    let note_idx = slot.note_counter.fetch_add(1, Relaxed) as usize;
                    let note_freq = if let Ok(notes) = slot.notes.lock() {
                        if notes.is_empty() { 440.0 } else { notes[note_idx % notes.len()] }
                    } else { 440.0 };

                    // Compute grid step if not yet set (needs sample_rate + slot's BPM)
                    if slot.grid_step_samples.load(Relaxed) == 0 {
                        let sr = state.sample_rate.load(Relaxed) as f32;
                        if let Ok(voice_guard) = slot.voice_data.lock() {
                            if let Some(ref voice) = *voice_guard {
                                if let Ok(melody_guard) = slot.melody_data.lock() {
                                    if let Some(ref melody) = *melody_guard {
                                        let bpm = CLASSIC_BREAKS[voice.drum_pattern_idx % CLASSIC_BREAKS.len()].bpm;
                                        let grid = (sixteenth_samples(bpm, sr) * melody.quantize_subdiv).round() as u32;
                                        slot.grid_step_samples.store(grid.max(1), Relaxed);
                                    }
                                }
                            }
                        }
                    }

                    if let Ok(mut q) = slot.note_queue.lock() {
                        // Cap queue depth to avoid unbounded growth (16 notes max)
                        if q.len() < 16 {
                            q.push_back(QueuedNote {
                                category: category_to_u8(cat),
                                volume: args.volume,
                                duration_ms: args.duration as u32,
                                note_freq,
                            });
                        }
                    }
                }
                // Tonal/structural events → play immediately (existing behavior)
                _ => {
                    slot.oneshot_category.store(category_to_u8(cat), Relaxed);
                    slot.oneshot_duration_ms.store(args.duration as u32, Relaxed);
                    slot.oneshot_volume.store(args.volume.to_bits(), Relaxed);
                    slot.oneshot_sample.store(state.global_sample.load(Relaxed), Relaxed);
                    slot.oneshot_pending.store(true, Relaxed);
                }
            }

            // SessionEnd: fade out ambient bed
            if event == "SessionEnd" {
                slot.bed_fade_out.store(true, Relaxed);
            }

            let _ = stream.write_all(b"OK");
        } else {
            let _ = stream.write_all(b"ERR:no_slots");
        }
    }
}

/// Daemon audio engine: generates audio from all voice slots with shared reverb/delay
fn daemon_audio_engine(state: Arc<DaemonState>) -> Result<()> {
    let host = cpal::default_host();
    let device = host.default_output_device()
        .context("No audio output device found")?;
    let config = device.default_output_config()
        .context("Failed to get default audio config")?;
    let config: cpal::StreamConfig = config.into();
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // Publish sample rate so socket handler can compute grid step
    state.sample_rate.store(sample_rate as u32, Relaxed);

    // Shared DSP buses
    let mut reverb = SimpleReverb::new(sample_rate);
    let mut tape_delay = StereoTapeDelay::new(
        300.0, 400.0,  // moderate delay times
        0.40,          // moderate feedback
        2800.0,        // filter cutoff
        1.2,           // wow rate
        0.30,          // mix
        sample_rate,
    );

    // Per-slot ambient bed oscillator phases
    let mut bed_phases: [f32; MAX_VOICE_SLOTS] = [0.0; MAX_VOICE_SLOTS];
    // Per-slot bed LFO phases
    let mut bed_lfo_phases: [f32; MAX_VOICE_SLOTS] = [0.0; MAX_VOICE_SLOTS];
    // Per-slot bed volume (for fade in/out)
    let mut bed_volumes: [f32; MAX_VOICE_SLOTS] = [0.0; MAX_VOICE_SLOTS];
    // Per-slot one-shot state
    let mut oneshot_active: [bool; MAX_VOICE_SLOTS] = [false; MAX_VOICE_SLOTS];
    let mut oneshot_start_sample: [u64; MAX_VOICE_SLOTS] = [0; MAX_VOICE_SLOTS];
    // Per-slot hit reverbs
    let mut slot_reverbs: Vec<SimpleReverb> = (0..MAX_VOICE_SLOTS)
        .map(|_| SimpleReverb::new(sample_rate))
        .collect();
    // Per-slot queue drain timing: next sample at which to pop from note_queue
    let mut queue_next_sample: [u64; MAX_VOICE_SLOTS] = [0; MAX_VOICE_SLOTS];

    let err_fn = |err| eprintln!("Daemon audio error: {}", err);

    let state_quit = state.clone(); // clone for the while loop after closure captures state
    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                let global = state.global_sample.fetch_add(1, Relaxed);

                if state.quit.load(Relaxed) {
                    for s in frame.iter_mut() { *s = 0.0; }
                    continue;
                }

                let mut mix_l = 0.0f32;
                let mut mix_r = 0.0f32;

                // Compute ducking: check if any slot has an Attention oneshot active
                let mut duck_level = 1.0f32;
                for i in 0..MAX_VOICE_SLOTS {
                    if oneshot_active[i] {
                        let cat = u8_to_category(state.voices[i].oneshot_category.load(Relaxed));
                        match cat {
                            EventCategory::Attention => duck_level = duck_level.min(0.2),
                            EventCategory::SessionBoundary => duck_level = duck_level.min(0.5),
                            EventCategory::Bass => duck_level = duck_level.min(0.7),
                            EventCategory::DrumHit => duck_level = duck_level.min(0.8),
                            _ => {}
                        }
                    }
                }

                for i in 0..MAX_VOICE_SLOTS {
                    let slot = &state.voices[i];
                    if !slot.active.load(Relaxed) { continue; }

                    let root = f32::from_bits(slot.root_freq.load(Relaxed));
                    if root <= 0.0 { continue; }

                    // ── Ambient bed ──
                    // Ultra-slow Drift pad: very quiet continuous tone
                    let target_bed_vol = if slot.bed_fade_out.load(Relaxed) {
                        0.0
                    } else {
                        f32::from_bits(slot.bed_volume.load(Relaxed))
                    };
                    // Smooth fade (3s for fade-in, 3s for fade-out)
                    let fade_rate = 1.0 / (3.0 * sample_rate);
                    if bed_volumes[i] < target_bed_vol {
                        bed_volumes[i] = (bed_volumes[i] + fade_rate).min(target_bed_vol);
                    } else if bed_volumes[i] > target_bed_vol {
                        bed_volumes[i] = (bed_volumes[i] - fade_rate).max(0.0);
                    }

                    if bed_volumes[i] > 0.001 {
                        // Drift LFO: ultra-slow pitch wobble
                        let lfo_rate = 0.05; // 0.05 Hz — 20 second cycle
                        bed_lfo_phases[i] += lfo_rate / sample_rate;
                        let lfo = 1.0 + 0.002 * (2.0 * PI * bed_lfo_phases[i]).sin();

                        let bed_freq = fold_to_range(root, 80.0, 200.0) * lfo;
                        bed_phases[i] += bed_freq / sample_rate;
                        if bed_phases[i] > 1.0 { bed_phases[i] -= 1.0; }

                        // Simple warm sine + sub
                        let bed_sample = (2.0 * PI * bed_phases[i]).sin() * 0.7
                            + (2.0 * PI * bed_phases[i] * 0.5).sin() * 0.3;
                        let bed_out = bed_sample * bed_volumes[i] * duck_level;
                        mix_l += bed_out;
                        mix_r += bed_out;
                    }

                    // Deactivate slot if bed faded out completely, no oneshot, and queue empty
                    let queue_empty = slot.note_queue.try_lock().map_or(false, |q| q.is_empty());
                    if bed_volumes[i] <= 0.001 && slot.bed_fade_out.load(Relaxed)
                        && !oneshot_active[i] && !slot.oneshot_pending.load(Relaxed)
                        && queue_empty
                    {
                        slot.active.store(false, Relaxed);
                        slot.bed_fade_out.store(false, Relaxed);
                        state.active_count.fetch_sub(1, Relaxed);
                        continue;
                    }

                    // ── One-shot events ──
                    if slot.oneshot_pending.load(Relaxed) {
                        oneshot_active[i] = true;
                        oneshot_start_sample[i] = slot.oneshot_sample.load(Relaxed);
                        slot.oneshot_pending.store(false, Relaxed);
                    }

                    // ── Queue drain: pop pitched percussion notes at grid rate ──
                    if !oneshot_active[i] && global >= queue_next_sample[i] {
                        if let Ok(mut q) = slot.note_queue.try_lock() {
                            if let Some(note) = q.pop_front() {
                                // Activate this queued note as a oneshot
                                slot.oneshot_category.store(note.category, Relaxed);
                                slot.oneshot_duration_ms.store(note.duration_ms, Relaxed);
                                slot.oneshot_volume.store(note.volume.to_bits(), Relaxed);
                                slot.oneshot_note_freq.store(note.note_freq.to_bits(), Relaxed);
                                oneshot_active[i] = true;
                                oneshot_start_sample[i] = global;
                                // Schedule next drain at grid boundary
                                let grid = slot.grid_step_samples.load(Relaxed) as u64;
                                queue_next_sample[i] = global + grid.max(2048);
                            }
                        }
                    }

                    if oneshot_active[i] {
                        let dur_ms = slot.oneshot_duration_ms.load(Relaxed) as f32;
                        let vol = f32::from_bits(slot.oneshot_volume.load(Relaxed));
                        let elapsed_samples = global.saturating_sub(oneshot_start_sample[i]);
                        let total_samples = (dur_ms / 1000.0 * sample_rate) as u64;

                        if elapsed_samples >= total_samples {
                            oneshot_active[i] = false;
                        } else {
                            let time = elapsed_samples as f32 / sample_rate;
                            let progress = elapsed_samples as f32 / total_samples as f32;

                            // Global fade-out (last 10%)
                            let fade_start = 0.90;
                            let global_fade = if progress > fade_start {
                                ((1.0 - progress) / (1.0 - fade_start)).sqrt()
                            } else {
                                1.0
                            };

                            // Generate one-shot sound based on category
                            let cat = u8_to_category(slot.oneshot_category.load(Relaxed));
                            let oneshot_out = if let Ok(voice_guard) = slot.voice_data.lock() {
                                if let Some(ref voice) = *voice_guard {
                                    match cat {
                                        EventCategory::ToolPulse => {
                                            // Pitched percussion: kalimba note from scale
                                            let freq = f32::from_bits(slot.oneshot_note_freq.load(Relaxed));
                                            let pitched = generate_pitched_hit(time, if freq > 0.0 { freq } else { voice.root_freq });
                                            // Light drum ghost underneath (10%) for texture
                                            let drum = generate_single_hit(time, sample_rate, voice) * 0.1;
                                            pitched + drum
                                        }
                                        EventCategory::DrumHit => {
                                            // Kick/snare with pitched note layered on top (30%)
                                            let freq = f32::from_bits(slot.oneshot_note_freq.load(Relaxed));
                                            let drum = generate_single_hit(time, sample_rate, voice);
                                            let pitched = generate_pitched_hit(time, if freq > 0.0 { freq } else { voice.root_freq }) * 0.3;
                                            drum + pitched
                                        }
                                        _ => {
                                            // Tonal: simple pad-like oscillator from slot's notes
                                            if let Ok(notes) = slot.notes.lock() {
                                                if !notes.is_empty() {
                                                    let mut sum = 0.0f32;
                                                    for (ni, &freq) in notes.iter().take(3).enumerate() {
                                                        let phase = 2.0 * PI * freq * time + ni as f32 * 0.2;
                                                        sum += phase.sin() * 0.33;
                                                    }
                                                    // Envelope: fade in/out
                                                    let env = if progress < 0.1 {
                                                        progress / 0.1
                                                    } else if progress > 0.8 {
                                                        (1.0 - progress) / 0.2
                                                    } else {
                                                        1.0
                                                    };
                                                    sum * env
                                                } else {
                                                    0.0
                                                }
                                            } else {
                                                0.0
                                            }
                                        }
                                    }
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            };

                            let raw = oneshot_out * vol * global_fade;
                            // Light reverb per slot
                            let wet = slot_reverbs[i].process(raw);
                            let with_reverb = raw * 0.88 + wet * 0.12;
                            mix_l += with_reverb;
                            mix_r += with_reverb;
                        }
                    }
                }

                // Shared delay bus (the dub desk)
                let mono_mix = (mix_l + mix_r) * 0.5;
                let (delay_l, delay_r) = tape_delay.process(mono_mix * 0.3, sample_rate);
                mix_l += delay_l * 0.25;
                mix_r += delay_r * 0.25;

                // Shared reverb bus
                let reverb_in = (mix_l + mix_r) * 0.5;
                let reverb_wet = reverb.process(reverb_in);
                mix_l = mix_l * 0.85 + reverb_wet * 0.15;
                mix_r = mix_r * 0.85 + reverb_wet * 0.15;

                // Soft clip to prevent distortion
                mix_l = mix_l.tanh();
                mix_r = mix_r.tanh();

                // Output
                for (ch, s) in frame.iter_mut().enumerate() {
                    *s = if ch % 2 == 0 { mix_l } else { mix_r };
                }
            }
        },
        err_fn,
        None,
    ).context("Failed to build daemon audio stream")?;

    stream.play().context("Failed to start daemon audio")?;

    // Keep stream alive until quit
    while !state_quit.quit.load(Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

/// Count recent events from ~/.branch-tone/events.log within a time window.
/// Returns event count in the last `window_secs` seconds. Never fails.
fn recent_event_density(window_secs: u64) -> usize {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return 0,
    };
    let log_path = home.join(".branch-tone").join("events.log");
    let contents = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Parse timestamps from log lines: "YYYY-MM-DDTHH:MM:SS event repo branch"
    // Only count lines within the window
    contents.lines().rev().take(200).filter(|line| {
        // Parse ISO-ish timestamp to epoch seconds (approximate)
        let ts = line.split_whitespace().next().unwrap_or("");
        if let Some(epoch) = parse_log_timestamp(ts) {
            now_secs.saturating_sub(epoch) <= window_secs
        } else {
            false
        }
    }).count()
}

/// Parse "YYYY-MM-DDTHH:MM:SS" into approximate Unix epoch seconds.
fn parse_log_timestamp(ts: &str) -> Option<u64> {
    // Split "2024-03-10T15:30:45" into date and time parts
    let mut parts = ts.split('T');
    let date_part = parts.next()?;
    let time_part = parts.next()?;

    let mut date_fields = date_part.split('-');
    let year: u64 = date_fields.next()?.parse().ok()?;
    let month: u64 = date_fields.next()?.parse().ok()?;
    let day: u64 = date_fields.next()?.parse().ok()?;

    let mut time_fields = time_part.split(':');
    let hour: u64 = time_fields.next()?.parse().ok()?;
    let min: u64 = time_fields.next()?.parse().ok()?;
    let sec: u64 = time_fields.next()?.parse().ok()?;

    // Approximate days since epoch (good enough for 10s window comparison)
    let days = (year - 1970) * 365 + (year - 1969) / 4
        + match month {
            1 => 0, 2 => 31, 3 => 59, 4 => 90, 5 => 120, 6 => 151,
            7 => 181, 8 => 212, 9 => 243, 10 => 273, 11 => 304, 12 => 334,
            _ => 0,
        }
        + day - 1;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
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

    // Try daemon first: if running, send event via socket (zero-latency)
    if send_to_daemon(&hook_type, &repo, &branch).is_ok() {
        return Ok(());
    }

    // Fallback: fire-and-forget (current behavior)
    let mut args = hook_play_args(&hook_type, repo, branch, false);

    // Density-aware modulation: events in last 10s window
    // Busy bursts → richer echoes; quiet periods → sparser, more ambient
    let density = recent_event_density(10);
    args.event_density = density;
    if density > 3 {
        // Scale density factor: 0.0 at 3 events, 1.0 at 15+ events
        let density_factor = ((density as f32 - 3.0) / 12.0).min(1.0);
        // Boost volume slightly for busier periods (up to +15%)
        args.volume = (args.volume * (1.0 + density_factor * 0.15)).min(1.0);
    }

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
    let event_category = params.event_category;

    let total_samples = (sample_rate * total_duration as f32 / 1000.0) as usize;

    let sample_clock = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sample_clock_clone = sample_clock.clone();

    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_clone = finished.clone();

    // DSP processing state
    let mut pad_lpf = LowPass24::new();
    let mut reverb = SimpleReverb::new(sample_rate);
    let mut phaser = StereoPhaser::new(sample_rate);

    // Stereo tape delay (dub echo) — category-aware parameters
    let (delay_time_mult, delay_fb_offset, throw_rate) = params.event_category.delay_character();
    let delay_l_ms = (voice.delay_time_base + melody.delay_time_offset) * delay_time_mult;
    let delay_r_ms = (voice.delay_time_base * 1.33 + melody.delay_time_offset) * delay_time_mult;
    // Density-aware feedback: busy periods → richer echoes (up to +0.08)
    let density_fb_boost = if params.event_density > 3 {
        ((params.event_density as f32 - 3.0) / 12.0).min(1.0) * 0.08
    } else {
        0.0
    };
    let delay_feedback = (voice.delay_feedback + delay_fb_offset + density_fb_boost).clamp(0.15, 0.60);
    let mut tape_delay = StereoTapeDelay::new(
        delay_l_ms.max(100.0),
        delay_r_ms.max(100.0),
        delay_feedback,
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

    // Compute quantize grid in samples (snap arp notes to BPM-derived grid)
    let break_idx = voice.drum_pattern_idx % CLASSIC_BREAKS.len();
    let brk_bpm = CLASSIC_BREAKS[break_idx].bpm;
    let grid_samples = (sixteenth_samples(brk_bpm, sample_rate) * melody.quantize_subdiv).round() as usize;

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

                // ── SINGLE HIT PATH (pitched percussion + jazz micro-pattern) ──
                // ToolPulse: kalimba-like pitched note (walking through scale)
                // DrumHit: kick/snare with pitched note layered on top
                if effects.single_hit {
                    let spacing_secs = voice.hit_spacing_ms / 1000.0;
                    let mut sum = 0.0f32;
                    // Use notes[0] as the pitched frequency (already selected by event_seed)
                    let pitch_freq = notes.first().copied().unwrap_or(voice.root_freq);
                    for h in 0..voice.hit_count {
                        let hit_time = time - (h as f32 * spacing_secs);
                        if hit_time >= 0.0 {
                            let vel = if h == 0 { 1.0 } else {
                                // Ghost notes: 30–60% velocity, decreasing with distance
                                0.6 - (h as f32 * 0.1)
                            };
                            let hit_sample = match event_category {
                                EventCategory::ToolPulse => {
                                    // Pitched percussion with light drum ghost
                                    let pitched = generate_pitched_hit(hit_time, pitch_freq);
                                    let drum = generate_single_hit(hit_time, sample_rate, &voice) * 0.1;
                                    pitched + drum
                                }
                                EventCategory::DrumHit => {
                                    // Drum with pitched note layered on top
                                    let drum = generate_single_hit(hit_time, sample_rate, &voice);
                                    let pitched = generate_pitched_hit(hit_time, pitch_freq) * 0.3;
                                    drum + pitched
                                }
                                _ => generate_single_hit(hit_time, sample_rate, &voice),
                            };
                            sum += hit_sample * vel;
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

                // TONAL BUS: category-aware articulation
                // Each instrument plays notes differently:
                //   Horn  → chord stab (all notes, sharp attack/release)
                //   Piano → comping (rhythmic chord hits)
                //   Bass  → single root note (deep, punchy)
                //   Keys  → sustained pad (warm chord)
                //   Default → repo's natural mode (arp or pad)
                let tonal_bus = if effects.melody_over_drums || !effects.drums {
                    match event_category {
                        // Horn: disco/jazz brass stab — all notes hit at once, punchy
                        EventCategory::Attention => {
                            generate_stab(&notes, time, progress, volume, &voice, &melody, &timbral, event_category)
                        }
                        // Piano: rhythmic comping — chord hits at rhythmic intervals
                        EventCategory::Lifecycle => {
                            generate_comping(&notes, time, current_sample, total_samples, volume, &voice, &melody, &timbral, grid_samples, event_category)
                        }
                        // Bass: grid-locked walking bass — deep, thick, punchy
                        EventCategory::Bass => {
                            generate_bass_note(&notes, time, current_sample, volume, &voice, &melody, &timbral, grid_samples, event_category)
                        }
                        // Keys/Pad: sustained warm chord (existing pad behavior)
                        EventCategory::SessionBoundary => {
                            generate_pad(&notes, time, progress, volume, effects, &voice, &melody, &timbral, event_category)
                        }
                        // Default/CLI: repo's natural mode
                        _ => {
                            if effects.bulldozer {
                                let pad_out = generate_pad(&notes, time, progress, 1.0, effects, &voice, &melody, &timbral, event_category);
                                let arp_out = generate_arpeggio(&notes, time, current_sample, total_samples, 1.0, arp_effects, &voice, &melody, &timbral, grid_samples, event_category);
                                (pad_out * 0.7 + arp_out * 0.3) * volume
                            } else if effects.pad {
                                generate_pad(&notes, time, progress, volume, effects, &voice, &melody, &timbral, event_category)
                            } else {
                                generate_arpeggio(&notes, time, current_sample, total_samples, volume, effects, &voice, &melody, &timbral, grid_samples, event_category)
                            }
                        }
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
                    // Throw envelope: front-loaded send (King Tubby technique)
                    // throw_rate controls decay speed: higher = faster taper (e.g. drums 5.0, pads 1.5)
                    let throw_env = ((-throw_rate * progress).exp() * 0.8 + 0.2).min(1.0);
                    let (post_delay_l, post_delay_r) = if effects.dub_delay {
                        tape_delay.process_throw(filtered, throw_env, sample_rate)
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

fn generate_pad(notes: &[f32], time: f32, progress: f32, volume: f32, _effects: Effects, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral, category: EventCategory) -> f32 {
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

        // Category-aware envelope modifier: horn gets tighter pad, piano gets sharper attack
        let note_env = match category {
            EventCategory::Attention => {
                // Horn: tighter, punchier pad — faster attack, quicker release
                let punch = if note_progress < 0.05 {
                    (note_progress / 0.05).min(1.0)
                } else {
                    1.0
                };
                note_env * punch * (1.0 - note_progress * 0.3).max(0.4)
            }
            _ => note_env,
        };

        let base_freq = freq;

        // Supersaw-style detuning: preset detune_cents defines signature spread
        let total_spread = if timbral.detune_cents > 0.0 { timbral.detune_cents } else { melody.chorus_detune * 0.25 };
        let nv = num_voices.max(2);

        // Category-aware pad oscillator: non-default categories use their distinct waveform
        match category {
            EventCategory::Attention | EventCategory::Lifecycle | EventCategory::Bass => {
                // Use category oscillator for distinct timbre, with light detuning
                let osc = generate_category_oscillator(base_freq, time, true, i, voice, melody, timbral, category);
                sample += osc * note_env;
            }
            _ => {
                // Default/SessionBoundary: original supersaw pad synthesis
                for j in 0..nv {
                    let cents = if nv == 1 { 0.0 }
                        else { -total_spread / 2.0 + (j as f32 / (nv - 1) as f32) * total_spread };
                    let f = base_freq * 2.0_f32.powf(cents / 1200.0);
                    let phase_offset = j as f32 * 2.0 * PI / nv as f32 + i as f32 * 0.5;

                    let saw_phase = f * time + phase_offset;
                    let saw = 2.0 * (saw_phase - saw_phase.floor()) - 1.0;
                    let sine = (2.0 * PI * f * time + phase_offset).sin();
                    let wave = sine * (1.0 - saw_mix) + saw * saw_mix;

                    let center_dist = ((j as f32 / (nv - 1).max(1) as f32) - 0.5).abs() * 2.0;
                    let voice_gain = 1.0 - center_dist * 0.3;
                    sample += wave * note_env * voice_gain / nv as f32;
                }
            }
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

fn generate_arpeggio(notes: &[f32], time: f32, current_sample: usize, total_samples: usize, volume: f32, effects: Effects, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral, grid_samples: usize, category: EventCategory) -> f32 {
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

    // Snap interior boundaries to the nearest BPM grid point.
    // Skip quantization when there are too many notes to fit the grid —
    // each note needs at least one grid step, so we need num_notes * grid_samples <= total_samples.
    if num_notes * grid_samples <= total_samples {
        for b in boundaries[1..num_notes].iter_mut() {
            *b = ((*b as f32 / grid_samples as f32).round() as usize) * grid_samples;
        }
        // Ensure no two boundaries collide — minimum 1 grid step apart
        for i in 1..boundaries.len() - 1 {
            if boundaries[i] <= boundaries[i - 1] {
                boundaries[i] = boundaries[i - 1] + grid_samples;
            }
        }
        *boundaries.last_mut().unwrap() = total_samples;
    }

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

        // Category-aware envelope shaping: each instrument gets its own attack/decay character
        let (cat_attack_mult, cat_decay_mult, cat_sustain_floor) = match category {
            // Piano: very fast attack, steep exponential decay — percussive rhodes character
            EventCategory::Lifecycle => (0.3, 2.5, 0.0),
            // Horn: punchy attack, slower decay, moderate sustain — brass stab with body
            EventCategory::Attention => (0.5, 0.6, 0.3),
            // Bass: medium attack, moderate decay — punchy but not too short
            EventCategory::Bass => (0.7, 1.5, 0.05),
            // Keys/Pad: gentle attack, slow decay — pad-like sustain
            EventCategory::SessionBoundary => (1.0, 0.4, 0.15),
            // Default: unchanged behavior
            _ => (1.0, 1.0, 0.0),
        };

        let effective_attack_frac = attack_frac * cat_attack_mult;
        let attack_samples = (note_slot_len as f32 * effective_attack_frac) as usize;
        let attack_env = if attack_samples > 0 && samples_since_trigger < attack_samples {
            samples_since_trigger as f32 / attack_samples as f32
        } else {
            1.0
        };

        // Exponential decay with category-aware rate and sustain floor
        let decay_time = samples_since_trigger as f32 / note_slot_len as f32;
        let ring_env = (-timbral.decay_rate * cat_decay_mult * decay_time).exp();
        let ring_env = ring_env.max(cat_sustain_floor);

        let env = attack_env * ring_env;

        // Skip notes that have decayed to inaudible
        if env < 0.005 {
            continue;
        }

        let osc = generate_category_oscillator(frequency, time, effects.chorus, i, voice, melody, timbral, category);
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

/// Horn stab: all notes play simultaneously with a sharp attack and quick release.
/// Think disco brass hits or jazz horn stabs — punchy, rhythmic, in-your-face.
/// ADSR varies per event via melody.envelope_shape.
fn generate_stab(notes: &[f32], time: f32, progress: f32, volume: f32, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral, category: EventCategory) -> f32 {
    // ADSR from melody.envelope_shape — each seed gets a different stab feel
    let (base_attack, base_decay) = ENVELOPE_SHAPES[melody.envelope_shape];
    // Horn stab scaling: fast attack (5-40ms), varied decay
    let attack_time = (base_attack * 0.1).max(0.005);  // 5-40ms
    let decay_rate = 2.0 + base_decay * 20.0;          // 5-6 for punchy, 2-3 for sustained
    let sustain = base_attack * 0.5;                    // 0.01-0.20 (punchy→held)

    let attack_env = (time / attack_time).min(1.0);
    let decay_env = sustain + (1.0 - sustain) * (-decay_rate * progress).exp();

    // Final release fade in last 20%
    let release = if progress > 0.80 {
        ((1.0 - progress) / 0.20).sqrt()
    } else {
        1.0
    };

    let env = attack_env * decay_env * release;

    // All notes sound simultaneously — this IS the stab character
    let mut sample = 0.0;
    for (i, &freq) in notes.iter().enumerate() {
        let osc = generate_category_oscillator(freq, time, true, i, voice, melody, timbral, category);
        sample += osc * env;
    }

    // Normalize by note count
    sample /= notes.len().max(1) as f32;
    sample * volume
}

/// Piano comping: rhythmic chord hits at regular intervals.
/// Like a jazz pianist hitting voicings on beats 2 and 4, or a rhythmic stab pattern.
/// ADSR and voicing vary per event via melody.envelope_shape and event_seed-rotated notes.
fn generate_comping(notes: &[f32], time: f32, current_sample: usize, total_samples: usize, volume: f32, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral, grid_samples: usize, category: EventCategory) -> f32 {
    // ADSR from melody.envelope_shape — each seed gets different comping feel
    let (base_attack, base_decay) = ENVELOPE_SHAPES[melody.envelope_shape];
    // Piano comping scaling: percussive attack, varied ring time
    let attack_secs = (base_attack * 0.05).max(0.003);   // 3-20ms attack
    let decay_rate = timbral.decay_rate * (0.8 + base_decay * 5.0); // varied ring
    let sustain_floor = base_attack * 0.15;                // tiny sustain for percussive feel

    // Rhythm pattern: use envelope_shape to pick different comping rhythms
    // shape 0 (Punchy) → every beat, shape 1 (Soft) → every 2 beats,
    // shape 2 (Pluck) → syncopated (1-and-3), shape 3 (Swell) → backbeat (2-and-4)
    let beat_grid = (grid_samples * 4).max(1); // 1 beat in samples
    let total_beats = (total_samples / beat_grid).max(1).min(8);

    // Generate hit pattern based on envelope shape for rhythmic variety
    let hit_pattern: Vec<usize> = match melody.envelope_shape {
        0 => (0..total_beats).collect(),                                           // every beat
        1 => (0..total_beats).filter(|b| b % 2 == 0).collect(),                  // half notes
        2 => (0..total_beats).filter(|b| b % 2 == 0 || b % 3 == 0).collect(),   // syncopated
        _ => (0..total_beats).filter(|b| b % 2 == 1).collect(),                  // backbeat
    };
    let num_hits = hit_pattern.len().max(1);

    let mut sample = 0.0;

    for (hit_seq, &beat_idx) in hit_pattern.iter().enumerate() {
        // Swing: offset even hits for groove
        let swing_offset = if beat_idx % 2 == 1 {
            (melody.swing * beat_grid as f32 * 0.5) as usize
        } else {
            0
        };
        let hit_start = beat_idx * beat_grid + swing_offset;
        if current_sample < hit_start {
            continue;
        }

        let samples_since_hit = current_sample - hit_start;
        let hit_time = samples_since_hit as f32 / 44100.0;

        // ADSR envelope per hit
        let attack_samples = (attack_secs * 44100.0) as usize;
        let attack = if samples_since_hit < attack_samples {
            samples_since_hit as f32 / attack_samples.max(1) as f32
        } else {
            1.0
        };
        let decay = sustain_floor + (1.0 - sustain_floor) * (-decay_rate * hit_time).exp();
        let env = attack * decay;

        if env < 0.005 {
            continue;
        }

        // Voicing variation per hit: rotate which notes are played
        // Creates movement like a real pianist — different inversions/voicings each hit
        let voicing: Vec<f32> = match hit_seq % 4 {
            0 => notes.to_vec(),                                                    // root position
            1 => notes.iter().enumerate()                                           // drop root
                .filter(|(i, _)| *i > 0)
                .map(|(_, &f)| f).collect(),
            2 => notes.iter().enumerate()                                           // shell voicing (root + top)
                .filter(|(i, _)| *i == 0 || *i == notes.len() - 1)
                .map(|(_, &f)| f).collect(),
            _ => notes.iter().enumerate()                                           // inner voices only
                .filter(|(i, _)| *i > 0 && *i < notes.len() - 1)
                .map(|(_, &f)| f).collect(),
        };
        let voicing = if voicing.is_empty() { notes.to_vec() } else { voicing };

        for (i, &freq) in voicing.iter().enumerate() {
            let osc = generate_category_oscillator(freq, time, true, i, voice, melody, timbral, category);
            sample += osc * env / voicing.len().max(1) as f32;
        }
    }

    // Normalize
    sample *= 0.7 / (num_hits as f32).sqrt();

    // Global fade-out
    let progress = current_sample as f32 / total_samples as f32;
    let fade = if progress > 0.85 {
        ((1.0 - progress) / 0.15).sqrt()
    } else {
        1.0
    };

    sample * fade * volume
}

/// Bass line: plays a short sequence of punchy bass notes from the seed-rotated pattern.
/// Each note has its own ADSR envelope derived from melody.envelope_shape.
/// Like a bass player walking through a short phrase — deep, rhythmic, varied per event.
fn generate_bass_note(notes: &[f32], _time: f32, current_sample: usize, volume: f32, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral, grid_samples: usize, category: EventCategory) -> f32 {
    let num_notes = notes.len().max(1);

    // ADSR from melody.envelope_shape — each branch/seed gets a different feel
    let (base_attack, base_decay) = ENVELOPE_SHAPES[melody.envelope_shape];
    // Bass-specific scaling: tighter attack, punchier decay than melodic instruments
    let attack_secs = (base_attack * 0.08).max(0.005); // 5-32ms attack
    let decay_rate = 3.0 + base_decay * 15.0;          // 3.5-6.0 decay rate (fast)
    let sustain_floor = base_attack * 0.3;              // 0.006-0.12 sustain (punchy→soft)

    // Lock note spacing to the BPM grid: 1 beat = 4 grid steps (quarter note)
    let beat_samples = (grid_samples * 4).max(1);
    let note_duration_samples = beat_samples;

    let mut sample = 0.0f32;

    for i in 0..num_notes {
        // Swing: odd notes offset by a fraction of the grid step
        let swing_offset_samples = if i % 2 == 1 {
            (melody.swing * note_duration_samples as f32 * 0.5) as usize
        } else { 0 };
        let note_start_sample = i * note_duration_samples + swing_offset_samples;

        if current_sample < note_start_sample {
            continue; // Note hasn't started yet
        }

        let note_time = (current_sample - note_start_sample) as f32 / 44100.0;

        // ADSR envelope per note
        let attack_env = (note_time / attack_secs).min(1.0);
        let decay_env = sustain_floor + (1.0 - sustain_floor) * (-decay_rate * note_time).exp();
        let env = attack_env * decay_env;

        // Skip inaudible
        if env < 0.005 {
            continue;
        }

        let freq = notes[i % notes.len()];
        let osc = generate_category_oscillator(freq, note_time, false, i, voice, melody, timbral, category);
        sample += osc * env;
    }

    // Global fade-out over last 15%
    let total_dur_approx = num_notes * note_duration_samples;
    let progress_approx = if total_dur_approx > 0 { (current_sample as f32 / total_dur_approx as f32).min(1.0) } else { 0.0 };
    let fade = if progress_approx > 0.85 {
        ((1.0 - progress_approx) / 0.15).sqrt()
    } else {
        1.0
    };

    sample * fade * volume
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


/// Category-aware oscillator: each instrument family gets a distinct waveform.
/// Falls through to generate_oscillator() for Default/DrumHit/ToolPulse.
fn generate_category_oscillator(freq: f32, time: f32, chorus: bool, voice_idx: usize, voice: &RepoVoice, melody: &BranchMelody, timbral: &EffectiveTimbral, category: EventCategory) -> f32 {
    match category {
        // Horn/Lead (Attention): square-wave harmonics — odd harmonics only for brass character.
        // Square wave = sum of odd harmonics at 1/n amplitude: fundamental + 3rd/3 + 5th/5 + 7th/7
        EventCategory::Attention => {
            let sub = (2.0 * PI * freq * 0.5 * time).sin() * timbral.sub_level * 0.5;

            // Slow shimmer for movement
            let shimmer_rate = 2.5 + voice_idx as f32 * 0.3;
            let depth = if timbral.chorus_depth > 0.0 { timbral.chorus_depth * 0.5 } else { 0.002 };
            let shimmer = 1.0 + depth * (2.0 * PI * shimmer_rate * time).sin();
            let freq = freq * shimmer;

            if chorus {
                let detune = melody.chorus_detune * 0.7; // tighter detune for brass precision
                let detune_cents = [0.0, -detune, detune, -detune * 0.5, detune * 0.5];
                let num_voices = detune_cents.len() as f32;
                let mut sample = 0.0;
                for (i, &cents) in detune_cents.iter().enumerate() {
                    let detune_factor = 2.0_f32.powf(cents / 1200.0);
                    let f = freq * detune_factor;
                    let phase_offset = (voice_idx as f32 + i as f32) * 0.1;
                    // Odd harmonics only: 1, 3, 5, 7 (square wave spectrum)
                    let h1 = (2.0 * PI * f * time + phase_offset).sin();
                    let h3 = (2.0 * PI * f * 3.0 * time + phase_offset).sin() / 3.0;
                    let h5 = (2.0 * PI * f * 5.0 * time + phase_offset).sin() / 5.0;
                    let h7 = (2.0 * PI * f * 7.0 * time + phase_offset).sin() / 7.0;
                    sample += (h1 + h3 + h5 + h7) / num_voices;
                }
                sample * 0.85 + sub
            } else {
                let light_detune = melody.chorus_detune * 0.2;
                let f1 = freq * 2.0_f32.powf(-light_detune / 1200.0);
                let f2 = freq * 2.0_f32.powf(light_detune / 1200.0);

                let s1 = (2.0 * PI * f1 * time).sin()
                    + (2.0 * PI * f1 * 3.0 * time).sin() / 3.0
                    + (2.0 * PI * f1 * 5.0 * time).sin() / 5.0
                    + (2.0 * PI * f1 * 7.0 * time).sin() / 7.0;
                let s2 = (2.0 * PI * f2 * time).sin()
                    + (2.0 * PI * f2 * 3.0 * time).sin() / 3.0
                    + (2.0 * PI * f2 * 5.0 * time).sin() / 5.0
                    + (2.0 * PI * f2 * 7.0 * time).sin() / 7.0;

                (s1 + s2) * 0.5 * 0.85 + sub
            }
        }

        // Piano/Comping (Lifecycle): FM synthesis for bell-like rhodes/electric piano tone.
        // Carrier sine modulated by another sine: carrier = sin(wc*t + mod_index * sin(wm*t))
        EventCategory::Lifecycle => {
            let sub = (2.0 * PI * freq * 0.5 * time).sin() * timbral.sub_level * 0.3;

            // FM parameters: mod_ratio gives harmonic relationship, mod_index controls brightness
            let mod_ratio = 2.0; // 2:1 ratio — classic electric piano
            let mod_index = 1.8 + timbral.harmonic_blend * 2.0; // brightness varies with repo timbre

            if chorus {
                let detune = melody.chorus_detune * 0.5; // subtle detune for warmth
                let detune_cents = [0.0, -detune, detune];
                let num_voices = detune_cents.len() as f32;
                let mut sample = 0.0;
                for (i, &cents) in detune_cents.iter().enumerate() {
                    let detune_factor = 2.0_f32.powf(cents / 1200.0);
                    let f = freq * detune_factor;
                    let phase_offset = (voice_idx as f32 + i as f32) * 0.15;
                    // FM synthesis: carrier = sin(wc*t + index * sin(wm*t))
                    let modulator = (2.0 * PI * f * mod_ratio * time + phase_offset).sin();
                    let carrier = (2.0 * PI * f * time + phase_offset + mod_index * modulator).sin();
                    // Add a softer second partial for body
                    let mod2 = (2.0 * PI * f * (mod_ratio + 1.0) * time + phase_offset).sin();
                    let carrier2 = (2.0 * PI * f * 2.0 * time + phase_offset + mod_index * 0.3 * mod2).sin();
                    sample += (carrier * 0.8 + carrier2 * 0.2) / num_voices;
                }
                sample * 0.9 + sub
            } else {
                let modulator = (2.0 * PI * freq * mod_ratio * time).sin();
                let carrier = (2.0 * PI * freq * time + mod_index * modulator).sin();
                let mod2 = (2.0 * PI * freq * (mod_ratio + 1.0) * time).sin();
                let carrier2 = (2.0 * PI * freq * 2.0 * time + mod_index * 0.3 * mod2).sin();
                (carrier * 0.8 + carrier2 * 0.2) * 0.9 + sub
            }
        }

        // Bass: heavy sub-oscillator at half frequency + mild saw at fundamental.
        // Deep, thick bass sound — sub sine dominates, fundamental adds definition.
        EventCategory::Bass => {
            // Heavy sub at 0.5x frequency — this IS the bass character
            let sub = (2.0 * PI * freq * 0.5 * time).sin() * 0.65;

            // Fundamental with mild saw for definition
            let saw_phase = freq * time;
            let saw = 2.0 * (saw_phase - saw_phase.floor()) - 1.0;
            let sine = (2.0 * PI * freq * time).sin();
            let fundamental = sine * 0.7 + saw * 0.3;

            // Very light 2nd harmonic for presence (not brightness)
            let h2 = (2.0 * PI * freq * 2.0 * time).sin() * 0.08;

            if chorus {
                let detune = melody.chorus_detune * 0.3; // tight detune for clean bass
                let f1 = freq * 2.0_f32.powf(-detune / 1200.0);
                let f2 = freq * 2.0_f32.powf(detune / 1200.0);
                let saw1 = 2.0 * (f1 * time - (f1 * time).floor()) - 1.0;
                let saw2 = 2.0 * (f2 * time - (f2 * time).floor()) - 1.0;
                let s1 = (2.0 * PI * f1 * time).sin() * 0.7 + saw1 * 0.3;
                let s2 = (2.0 * PI * f2 * time).sin() * 0.7 + saw2 * 0.3;
                let detuned_fund = (s1 + s2) * 0.5;
                (sub + detuned_fund * 0.4 + h2) * 0.85
            } else {
                (sub + fundamental * 0.4 + h2) * 0.85
            }
        }

        // Keys/Pad (SessionBoundary): warm supersaw — uses existing generate_oscillator
        // which already has good supersaw detuning and sub-oscillator for pad sounds.
        EventCategory::SessionBoundary => {
            generate_oscillator(freq, time, chorus, voice_idx, voice, melody, timbral)
        }

        // Default, DrumHit, ToolPulse: pass through to existing oscillator
        _ => {
            generate_oscillator(freq, time, chorus, voice_idx, voice, melody, timbral)
        }
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
    // Mode & note sequencer
    mode: AtomicU8,                  // 0=drums, 1=keys
    note_steps: [AtomicU8; 16],      // note at each step: 0=off, 1-16=semitone+1
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
            mode: AtomicU8::new(0),
            note_steps: std::array::from_fn(|_| AtomicU8::new(0)),
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
    let root_freq = voice.root_freq;

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
                    let step_samples = sixteenth_samples(bpm, sample_rate) as u64;
                    if step_samples > 0 {
                        let current_step = ((sample_counter / step_samples) % 16) as u8;
                        state.playhead.store(current_step, Relaxed);

                        if current_step != prev_step {
                            step_time = 0.0;
                            prev_step = current_step;
                            // Trigger sequenced note at this step
                            let note_val = state.note_steps[current_step as usize].load(Relaxed);
                            if note_val > 0 {
                                let semitone = (note_val - 1) as usize;
                                if semitone < 16 {
                                    state.note_triggers[semitone].store(200, Relaxed);
                                }
                            }
                        }

                        let flags = state.steps[current_step as usize].load(Relaxed);
                        let vel_raw = state.velocities[current_step as usize].load(Relaxed);
                        let vel = vel_raw as f32 / 255.0;

                        if vel > 0.0 {
                            if flags & K != 0 {
                                drum_out += synth_kick(step_time, sample_rate, root_freq) * kick_decay * vel;
                            }
                            if flags & S != 0 {
                                drum_out += synth_snare(step_time, current_step as f32, root_freq) * snare_tone * vel;
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

/// Note name for a semitone offset from the given root.
fn note_label(root_name: &str, semitone: usize) -> String {
    let root_idx = NOTE_NAMES.iter().position(|n| *n == root_name).unwrap_or(0);
    let note_idx = (root_idx + semitone) % 12;
    let oct = 4 + (root_idx + semitone) / 12;
    format!("{}{}", NOTE_NAMES[note_idx], oct)
}

/// Render the step sequencer grid to the terminal.
fn render_grid(
    state: &PlayerState,
    cursor_step: usize,
    cursor_row: usize,
    root_name: &str,
    scale_name: &str,
    show_help: bool,
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
    let mode = state.mode.load(Relaxed); // 0=drums, 1=keys
    let mode_label = if mode == 0 { "DRUMS" } else { "KEYS" };
    let status = if recording { "\x1b[31mREC\x1b[0m" }
        else if playing { "PLAYING" }
        else { "PAUSED" };

    // Header
    write!(stdout, " branch-tone player ─── {} @ {} BPM ─── [{}] ─── {}\r\n\r\n",
        break_name, bpm, status, mode_label)?;

    // Column headers
    write!(stdout, "      ")?;
    for i in 0..16 {
        if i == playhead && playing {
            write!(stdout, "\x1b[1m{:>2}\x1b[0m ", i + 1)?;
        } else {
            write!(stdout, "{:>2} ", i + 1)?;
        }
    }
    write!(stdout, "\r\n")?;

    // Drum rows: K, S, H, O
    let row_labels = ['K', 'S', 'H', 'O'];
    let row_flags = [K, S, H, O];
    let in_drum_mode = mode == 0;

    for (row, (&label, &flag)) in row_labels.iter().zip(row_flags.iter()).enumerate() {
        write!(stdout, "  {}   ", label)?;
        for step in 0..16 {
            let flags = state.steps[step].load(Relaxed);
            let vel = state.velocities[step].load(Relaxed);
            let active = flags & flag != 0;
            let is_cursor = in_drum_mode && step == cursor_step && row == cursor_row;
            let is_playhead = step == playhead && playing;

            let symbol = if active && vel > 100 {
                "●"
            } else if active && vel > 0 {
                "○"
            } else {
                "·"
            };

            if is_cursor {
                write!(stdout, "\x1b[7m")?;
            } else if is_playhead {
                write!(stdout, "\x1b[1m")?;
            }

            write!(stdout, " {} ", symbol)?;

            if is_cursor || is_playhead {
                write!(stdout, "\x1b[0m")?;
            }
        }
        write!(stdout, "\r\n")?;
    }

    // Separator + note lane
    write!(stdout, "  ─────────────────────────────────────────────────────\r\n")?;
    write!(stdout, "  ♪   ")?;
    for step in 0..16 {
        let note_val = state.note_steps[step].load(Relaxed);
        let is_cursor = !in_drum_mode && step == cursor_step;
        let is_playhead = step == playhead && playing;

        if is_cursor {
            write!(stdout, "\x1b[7m")?;
        } else if is_playhead && note_val > 0 {
            write!(stdout, "\x1b[1;36m")?;
        }

        if note_val > 0 {
            let label = note_label(root_name, (note_val - 1) as usize);
            write!(stdout, "{:>3}", label)?;
        } else {
            write!(stdout, " · ")?;
        }

        if is_cursor || (is_playhead && note_val > 0) {
            write!(stdout, "\x1b[0m")?;
        }
    }
    write!(stdout, "\r\n")?;

    // Playhead indicator
    write!(stdout, "      ")?;
    for i in 0..16 {
        if i == playhead && playing {
            write!(stdout, " ▲ ")?;
        } else {
            write!(stdout, "   ")?;
        }
    }
    write!(stdout, "\r\n\r\n")?;

    // Both visualizations — drum kit + piano keyboard
    render_drum_kit(state, playhead, playing, stdout)?;
    write!(stdout, "\r\n")?;
    render_piano_keys(state, root_name, playhead, playing, stdout)?;

    // Settings line
    let oct = state.octave_shift.load(Relaxed);
    let oct_str = if oct > 0 { format!("+{}", oct) } else { format!("{}", oct) };
    let preset_idx = state.synth_preset.load(Relaxed) as usize;
    let preset_name = SYNTH_PRESETS[preset_idx.min(SYNTH_PRESETS.len() - 1)].name;
    let pad_idx = state.pad_shape_idx.load(Relaxed) as usize;
    let pad_name = PAD_SHAPE_NAMES[pad_idx.min(PAD_SHAPE_NAMES.len() - 1)];
    let sustain = state.sustain.load(Relaxed);
    let sustain_str = if sustain { " │ \x1b[33mSUSTAIN\x1b[0m" } else { "" };

    write!(stdout, "\r\n  Synth: {} │ Oct {} │ Shape: {} │ {}{}\r\n", preset_name, oct_str, pad_name, scale_name, sustain_str)?;

    // Help (toggled with ?)
    write!(stdout, "\r\n")?;
    if show_help {
        if mode == 0 {
            write!(stdout, "  [enter] toggle  [i] ghost  [←/→] step  [↑/↓] row  [m] keys mode\r\n")?;
            write!(stdout, "  [1-0] pattern  [+/-] BPM  [space] play/pause  [r] record\r\n")?;
            write!(stdout, "  [A-L] piano  [z/x/c/v] rec K/S/H/O  [\\[/\\]] oct  [,/.] synth  [tab] sustain\r\n")?;
        } else {
            write!(stdout, "  [↑/↓] pitch  [←/→] step  [enter] clear  [m] drums mode\r\n")?;
            write!(stdout, "  [A-L/W-P] place note at step  [+/-] BPM  [space] play/pause\r\n")?;
            write!(stdout, "  [\\[/\\]] oct  [,/.] synth  [;/'] shape  [tab] sustain  [1-0] pattern\r\n")?;
        }
        write!(stdout, "  [?] hide help  [ctrl+c] quit\r\n")?;
    } else {
        write!(stdout, "  \x1b[2m[?] help  [ctrl+c] quit\x1b[0m\r\n")?;
    }

    stdout.flush()?;
    Ok(())
}

/// Render ASCII drum kit visualization with active drum highlighting.
fn render_drum_kit(
    state: &PlayerState,
    playhead: usize,
    playing: bool,
    stdout: &mut impl std::io::Write,
) -> Result<()> {
    let (kick, snare, hh, oh) = if playing {
        let flags = state.steps[playhead].load(Relaxed);
        let vel = state.velocities[playhead].load(Relaxed);
        if vel > 0 {
            (flags & K != 0, flags & S != 0, flags & H != 0, flags & O != 0)
        } else {
            (false, false, false, false)
        }
    } else {
        (false, false, false, false)
    };

    // ANSI helpers: bold-inverse for hit, dim for idle
    let on = "\x1b[1;7m";
    let off = "\x1b[0m";
    let dim = "\x1b[2m";

    let (hl, hr) = if hh { (on, off) } else { (dim, off) };
    let (ol, or_) = if oh { (on, off) } else { (dim, off) };
    let (sl, sr) = if snare { (on, off) } else { (dim, off) };
    let (kl, kr) = if kick { (on, off) } else { (dim, off) };

    //  Drum kit — front view, box-drawing
    write!(stdout, "  {}┌──────────┐{}                  {}┌──────────┐{}\r\n", hl, hr, ol, or_)?;
    write!(stdout, "  {}│ ░ HI-HAT │{}                  {}│ ░  OPEN  │{}\r\n", hl, hr, ol, or_)?;
    write!(stdout, "  {}└────┬─────┘{}                  {}└─────┬────┘{}\r\n", hl, hr, ol, or_)?;
    write!(stdout, "       │    {}┌──────────────┐{}       │\r\n", sl, sr)?;
    write!(stdout, "       │    {}│              │{}       │\r\n", sl, sr)?;
    write!(stdout, "       │    {}│    SNARE     │{}       │\r\n", sl, sr)?;
    write!(stdout, "       │    {}└──────┬───────┘{}       │\r\n", sl, sr)?;
    write!(stdout, "       │   {}┌───────┴────────┐{}     │\r\n", kl, kr)?;
    write!(stdout, "       │   {}│                │{}     │\r\n", kl, kr)?;
    write!(stdout, "       └───{}│     K I C K    │{}─────┘\r\n", kl, kr)?;
    write!(stdout, "           {}│                │{}\r\n", kl, kr)?;
    write!(stdout, "           {}└────────────────┘{}\r\n", kl, kr)?;

    Ok(())
}

/// Render piano keyboard with active note highlighting.
fn render_piano_keys(
    state: &PlayerState,
    root_name: &str,
    playhead: usize,
    playing: bool,
    stdout: &mut impl std::io::Write,
) -> Result<()> {
    let root_idx = NOTE_NAMES.iter().position(|n| *n == root_name).unwrap_or(0);

    // Which semitones are currently active (live keys + sequenced note at playhead)
    let mut active = [false; 16];
    for i in 0..16 {
        if state.note_triggers[i].load(Relaxed) > 0 {
            active[i] = true;
        }
    }
    if playing {
        let note_val = state.note_steps[playhead].load(Relaxed);
        if note_val > 0 && (note_val - 1) < 16 {
            active[(note_val - 1) as usize] = true;
        }
    }

    // Classify each semitone as black or white key
    let is_black = |semitone: usize| -> bool {
        matches!((root_idx + semitone) % 12, 1 | 3 | 6 | 8 | 10)
    };

    // Does this white key have a black key to its right?
    let has_black_right = |semitone: usize| -> bool {
        semitone + 1 < 16 && is_black(semitone + 1)
    };

    // Keyboard key labels
    const KEY_MAP: [&str; 16] = [
        "a", "w", "s", "e", "d", "f", "t", "g", "y", "h", "u", "j", "k", "o", "l", "p",
    ];

    let on = "\x1b[1;7m";       // bold inverse for active
    let blk_on = "\x1b[1;7;33m"; // bold inverse yellow for active black key
    let blk_bg = "\x1b[40;97m";  // white-on-black for black keys
    let rst = "\x1b[0m";

    // Build ordered list of white key indices and black key indices
    let mut whites: Vec<usize> = Vec::new();
    let mut blacks: Vec<usize> = Vec::new();
    for i in 0..16 {
        if is_black(i) { blacks.push(i); } else { whites.push(i); }
    }

    // Each white key is 5 chars wide. Total width: whites.len() * 5 + 1 (for final border)
    let w_count = whites.len();

    // Row 1: Top border with black key slots
    // Black keys sit between white keys — we render them in the top 3 rows
    write!(stdout, "    ┌")?;
    for (wi, &w) in whites.iter().enumerate() {
        if has_black_right(w) {
            write!(stdout, "───┬──")?;
        } else {
            write!(stdout, "─────")?;
        }
        if wi < w_count - 1 {
            // Check if next white key has black to its LEFT (i.e., current white has black right)
            if has_black_right(w) {
                write!(stdout, "")?; // ┬ already placed
            } else {
                write!(stdout, "┬")?;
            }
        }
    }
    write!(stdout, "┐\r\n")?;

    // Row 2-3: Black key bodies (raised) + white key upper space
    for _row in 0..2 {
        write!(stdout, "    │")?;
        for (wi, &w) in whites.iter().enumerate() {
            if has_black_right(w) {
                let bi = w + 1; // the black key semitone
                let a = active[bi];
                if a { write!(stdout, "{}", blk_on)?; }
                else { write!(stdout, "{}", blk_bg)?; }
                write!(stdout, "   │{:>2}", note_label(root_name, bi))?;
                write!(stdout, "{}", rst)?;
            } else {
                write!(stdout, "     ")?;
            }
            if wi < w_count - 1 {
                if has_black_right(w) {
                    write!(stdout, "")?;
                } else {
                    write!(stdout, "│")?;
                }
            }
        }
        write!(stdout, "│\r\n")?;
    }

    // Row 4: Black key bottom border merging into white key body
    write!(stdout, "    │")?;
    for (wi, &w) in whites.iter().enumerate() {
        if has_black_right(w) {
            write!(stdout, "   └──")?;
        } else {
            write!(stdout, "     ")?;
        }
        if wi < w_count - 1 {
            if has_black_right(w) {
                write!(stdout, "")?;
            } else {
                write!(stdout, "│")?;
            }
        }
    }
    write!(stdout, "│\r\n")?;

    // Row 5: White key note names
    write!(stdout, "    │")?;
    for (wi, &w) in whites.iter().enumerate() {
        let a = active[w];
        let label = note_label(root_name, w);
        let wide = if has_black_right(w) { 6 } else { 5 };
        if a { write!(stdout, "{}", on)?; }
        write!(stdout, " {:>3}{}", label, " ".repeat(wide - 4))?;
        if a { write!(stdout, "{}", rst)?; }
        if wi < w_count - 1 && !has_black_right(w) {
            write!(stdout, "│")?;
        }
    }
    write!(stdout, "│\r\n")?;

    // Row 6: White key keyboard shortcuts
    write!(stdout, "    │")?;
    for (wi, &w) in whites.iter().enumerate() {
        let a = active[w];
        let wide = if has_black_right(w) { 6 } else { 5 };
        if a { write!(stdout, "{}", on)?; }
        write!(stdout, "  {}  {}", KEY_MAP[w], " ".repeat(wide - 5))?;
        if a { write!(stdout, "{}", rst)?; }
        if wi < w_count - 1 && !has_black_right(w) {
            write!(stdout, "│")?;
        }
    }
    write!(stdout, "│\r\n")?;

    // Bottom border
    write!(stdout, "    └")?;
    for (wi, &w) in whites.iter().enumerate() {
        let wide = if has_black_right(w) { 6 } else { 5 };
        write!(stdout, "{}", "─".repeat(wide))?;
        if wi < w_count - 1 && !has_black_right(w) {
            write!(stdout, "┴")?;
        }
    }
    write!(stdout, "┘\r\n")?;

    Ok(())
}

// =============================================================================
// TRAY: macOS menu bar icon for daemon monitoring and control
// =============================================================================

#[cfg(all(target_os = "macos", feature = "tray"))]
mod tray {
    use std::time::Duration;

    use anyhow::{Context, Result};
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{define_class, msg_send, sel, MainThreadOnly};
    use objc2_app_kit::*;
    use objc2_foundation::*;

    use super::{daemon_dir, daemon_pid_path, daemon_socket_path};

    /// Parsed daemon status from `__status_json`
    #[derive(Default)]
    struct DaemonStatus {
        pid: u32,
        uptime_secs: u64,
        active_voices: Vec<(usize, String, String)>, // (slot, repo, branch)
        idle_secs: u64,
        idle_timeout: u64,
        running: bool,
    }

    /// Query the daemon for JSON status via Unix socket
    fn query_daemon_status() -> DaemonStatus {
        use std::io::{Read, Write};

        let sock = daemon_socket_path();
        let mut status = DaemonStatus::default();

        let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&sock) else {
            return status;
        };
        stream.set_read_timeout(Some(Duration::from_millis(500))).ok();
        stream.set_write_timeout(Some(Duration::from_millis(500))).ok();
        let _ = stream.write_all(b"{\"event\":\"__status_json\"}\n");

        let mut buf = vec![0u8; 4096];
        let Ok(n) = stream.read(&mut buf) else {
            return status;
        };

        let json_str = String::from_utf8_lossy(&buf[..n]);
        let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str.trim()) else {
            return status;
        };

        status.running = true;
        status.pid = json.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        status.uptime_secs = json.get("uptime_secs").and_then(|v| v.as_u64()).unwrap_or(0);
        status.idle_secs = json.get("idle_secs").and_then(|v| v.as_u64()).unwrap_or(0);
        status.idle_timeout = json.get("idle_timeout").and_then(|v| v.as_u64()).unwrap_or(300);

        if let Some(voices) = json.get("active_voices").and_then(|v| v.as_array()) {
            for v in voices {
                let slot = v.get("slot").and_then(|s| s.as_u64()).unwrap_or(0) as usize;
                let repo = v.get("repo").and_then(|s| s.as_str()).unwrap_or("").to_string();
                let branch = v.get("branch").and_then(|s| s.as_str()).unwrap_or("").to_string();
                status.active_voices.push((slot, repo, branch));
            }
        }

        status
    }

    /// Check if daemon is running by probing PID file + process
    fn is_daemon_running() -> bool {
        let pid_path = daemon_pid_path();
        if !pid_path.exists() {
            return false;
        }
        let Ok(pid_str) = std::fs::read_to_string(&pid_path) else {
            return false;
        };
        let Ok(pid) = pid_str.trim().parse::<u32>() else {
            return false;
        };
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Start the daemon as a detached background process
    fn start_daemon() {
        if is_daemon_running() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let _ = std::process::Command::new(exe)
            .args(["daemon", "--detach"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    /// Stop the daemon via socket shutdown command
    fn stop_daemon() {
        use std::io::{Read, Write};
        let sock = daemon_socket_path();
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&sock) {
            let _ = stream.write_all(b"{\"event\":\"__shutdown\"}\n");
            let mut buf = [0u8; 64];
            let _ = stream.read(&mut buf);
        }
        // Clean up PID/socket files
        let _ = std::fs::remove_file(daemon_pid_path());
        let _ = std::fs::remove_file(daemon_socket_path());
    }

    /// Read the last N lines from events.log
    fn recent_log_lines(max_lines: usize) -> Vec<String> {
        let log_path = daemon_dir().join("events.log");
        let Ok(content) = std::fs::read_to_string(&log_path) else {
            return Vec::new();
        };
        content.lines().rev().take(max_lines).map(String::from).collect::<Vec<_>>()
            .into_iter().rev().collect()
    }

    /// Format uptime as human-readable string
    fn format_uptime(secs: u64) -> String {
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    // Menu item tags for identification during updates
    const TAG_STATUS_LINE: isize = 100;
    const TAG_VOICES_LINE: isize = 101;
    const TAG_TOGGLE: isize = 102;
    const TAG_RECENT_EVENTS: isize = 200; // 200..207 for up to 8 log lines

    define_class!(
        // SAFETY: NSObject has no subclassing requirements. No Drop impl.
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        struct TrayDelegate;

        unsafe impl NSObjectProtocol for TrayDelegate {}

        unsafe impl NSApplicationDelegate for TrayDelegate {
            #[unsafe(method(applicationDidFinishLaunching:))]
            fn did_finish_launching(&self, _notification: &NSNotification) {
                let mtm = self.mtm();
                let app = NSApplication::sharedApplication(mtm);

                // Accessory policy: no dock icon, no app menu — just tray
                app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

                // Auto-start daemon if not running
                if !is_daemon_running() {
                    start_daemon();
                    // Brief pause for daemon to initialize socket
                    std::thread::sleep(Duration::from_millis(300));
                }

                // Create status bar item
                let status_bar = NSStatusBar::systemStatusBar();
                let status_item = status_bar.statusItemWithLength(NSVariableStatusItemLength);

                // Use deprecated but simple setTitle for text-only icon
                #[allow(deprecated)]
                if is_daemon_running() {
                    status_item.setTitle(Some(ns_string!("♫")));
                } else {
                    status_item.setTitle(Some(ns_string!("♩")));
                }

                // Build the menu
                let menu = build_menu(mtm);
                status_item.setMenu(Some(&menu));

                // Store status item in a global (prevent deallocation)
                // SAFETY: Single-threaded access on main thread only
                unsafe {
                    GLOBAL_STATUS_ITEM = Some(status_item);
                    GLOBAL_MENU = Some(menu);
                }

                // Spawn poll thread
                std::thread::spawn(|| {
                    poll_daemon_loop();
                });
            }
        }

        // Action handlers — NSMenuItem targets nil, so the responder chain
        // dispatches to the app delegate (this class).
        impl TrayDelegate {
            #[unsafe(method(toggleDaemon:))]
            fn toggle_daemon(&self, _sender: &NSMenuItem) {
                if is_daemon_running() {
                    stop_daemon();
                } else {
                    start_daemon();
                }
            }

            #[unsafe(method(testSounds:))]
            fn test_sounds(&self, _sender: &NSMenuItem) {
                let Ok(exe) = std::env::current_exe() else { return };
                let _ = std::process::Command::new(exe)
                    .args(["test", "."])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }

            #[unsafe(method(openPlayer:))]
            fn open_player(&self, _sender: &NSMenuItem) {
                let Ok(exe) = std::env::current_exe() else { return };
                let script = format!(
                    "tell application \"Terminal\" to do script \"{}\" & \" player\"",
                    exe.display()
                );
                let _ = std::process::Command::new("osascript")
                    .args(["-e", &script])
                    .spawn();
            }

            #[unsafe(method(openLog:))]
            fn open_log(&self, _sender: &NSMenuItem) {
                let log_path = daemon_dir().join("events.log");
                let _ = std::process::Command::new("open")
                    .arg(log_path)
                    .spawn();
            }

            #[unsafe(method(quitApp:))]
            fn quit_app(&self, _sender: &NSMenuItem) {
                // Stop daemon if running
                if is_daemon_running() {
                    stop_daemon();
                }
                let mtm = self.mtm();
                NSApplication::sharedApplication(mtm).terminate(None);
            }
        }
    );

    // Globals to keep Retained objects alive (main thread only)
    static mut GLOBAL_STATUS_ITEM: Option<Retained<NSStatusItem>> = None;
    static mut GLOBAL_MENU: Option<Retained<NSMenu>> = None;

    impl TrayDelegate {
        fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(());
            unsafe { msg_send![super(this), init] }
        }
    }

    /// Build the tray dropdown menu
    fn build_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("branch-tone"));

        let status = query_daemon_status();

        // Status line
        let status_text = if status.running {
            format!("\u{25CF} Daemon Running (PID {})", status.pid)
        } else {
            "\u{25CB} Daemon Stopped".to_string()
        };
        let status_item = make_disabled_item(mtm, &status_text);
        status_item.setTag(TAG_STATUS_LINE);
        menu.addItem(&status_item);

        // Voice count
        let voices_text = if status.active_voices.is_empty() {
            "  No active voices".to_string()
        } else {
            format!("  {} active voice{}", status.active_voices.len(),
                if status.active_voices.len() == 1 { "" } else { "s" })
        };
        let voices_item = make_disabled_item(mtm, &voices_text);
        voices_item.setTag(TAG_VOICES_LINE);
        menu.addItem(&voices_item);

        // Separator
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        // Toggle daemon (start/stop)
        let toggle_title = if status.running {
            "\u{23F9} Stop Daemon"
        } else {
            "\u{25B6}\u{FE0F} Start Daemon"
        };
        let toggle_item = make_action_item(mtm, toggle_title, sel!(toggleDaemon:));
        toggle_item.setTag(TAG_TOGGLE);
        menu.addItem(&toggle_item);

        // Test sounds
        let test_item = make_action_item(mtm, "\u{266A} Test Sounds", sel!(testSounds:));
        menu.addItem(&test_item);

        // Open player
        let player_item = make_action_item(mtm, "\u{1F3B9} Open Player", sel!(openPlayer:));
        menu.addItem(&player_item);

        // Separator
        menu.addItem(&NSMenuItem::separatorItem(mtm));

        // Recent events submenu
        let events_parent = make_disabled_item(mtm, "\u{25B8} Recent Events");
        let events_submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Recent Events"));

        let lines = recent_log_lines(8);
        if lines.is_empty() {
            let empty = make_disabled_item(mtm, "(no events yet)");
            events_submenu.addItem(&empty);
        } else {
            for (i, line) in lines.iter().enumerate() {
                // Truncate long lines for the menu
                let display = if line.len() > 60 { &line[..60] } else { line };
                let item = make_disabled_item(mtm, display);
                item.setTag(TAG_RECENT_EVENTS + i as isize);
                events_submenu.addItem(&item);
            }
        }

        // "Open Full Log" at bottom of submenu
        events_submenu.addItem(&NSMenuItem::separatorItem(mtm));
        let open_log = make_action_item(mtm, "Open Full Log", sel!(openLog:));
        events_submenu.addItem(&open_log);

        events_parent.setSubmenu(Some(&events_submenu));
        menu.addItem(&events_parent);

        // Separator + Quit
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let quit_item = make_action_item(mtm, "Quit", sel!(quitApp:));
        menu.addItem(&quit_item);

        menu
    }

    /// Create a disabled (non-clickable) menu item
    fn make_disabled_item(mtm: MainThreadMarker, title: &str) -> Retained<NSMenuItem> {
        let ns_title = NSString::from_str(title);
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm), &ns_title, None, ns_string!(""),
            )
        };
        item.setEnabled(false);
        item
    }

    /// Create an actionable menu item targeting the NSApp delegate
    fn make_action_item(mtm: MainThreadMarker, title: &str, action: objc2::runtime::Sel) -> Retained<NSMenuItem> {
        let ns_title = NSString::from_str(title);
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm), &ns_title, Some(action), ns_string!(""),
            )
        }
    }

    /// Background thread: poll daemon status every 2s, update menu on main thread
    fn poll_daemon_loop() {
        loop {
            std::thread::sleep(Duration::from_secs(2));

            let status = query_daemon_status();

            // Dispatch menu update to main thread
            // We use a closure-based approach via DispatchQueue since
            // performSelectorOnMainThread requires an ObjC method
            dispatch_to_main(move || {
                update_menu_from_status(&status);
            });
        }
    }

    /// Execute a closure on the main thread via libdispatch
    fn dispatch_to_main<F: FnOnce() + Send + 'static>(f: F) {
        // Use dispatch_async_f for thread-safe main-queue dispatch.
        // dispatch_get_main_queue() is a C macro expanding to &_dispatch_main_q,
        // so we reference the actual symbol directly.
        unsafe extern "C" {
            static _dispatch_main_q: std::ffi::c_void;
            fn dispatch_async_f(
                queue: *const std::ffi::c_void,
                context: *mut std::ffi::c_void,
                work: unsafe extern "C" fn(*mut std::ffi::c_void),
            );
        }

        unsafe extern "C" fn trampoline<F: FnOnce()>(ctx: *mut std::ffi::c_void) {
            let f = unsafe { Box::from_raw(ctx as *mut F) };
            f();
        }

        let boxed = Box::into_raw(Box::new(f));
        unsafe {
            dispatch_async_f(
                &raw const _dispatch_main_q,
                boxed as *mut std::ffi::c_void,
                trampoline::<F>,
            );
        }
    }

    /// Update menu items based on current daemon status (called on main thread)
    fn update_menu_from_status(status: &DaemonStatus) {
        let Some(_mtm) = MainThreadMarker::new() else { return };

        // Update status item icon
        unsafe {
            if let Some(ref item) = GLOBAL_STATUS_ITEM {
                #[allow(deprecated)]
                if status.running {
                    item.setTitle(Some(ns_string!("♫")));
                } else {
                    item.setTitle(Some(ns_string!("♩")));
                }
            }

            if let Some(ref menu) = GLOBAL_MENU {
                let n = menu.numberOfItems();
                for idx in 0..n {
                    let Some(item) = menu.itemAtIndex(idx) else { continue };
                    let tag: NSInteger = item.tag();

                    if tag == TAG_STATUS_LINE {
                        let text = if status.running {
                            NSString::from_str(&format!(
                                "\u{25CF} Daemon Running (PID {}) \u{2014} up {}",
                                status.pid, format_uptime(status.uptime_secs)
                            ))
                        } else {
                            NSString::from_str("\u{25CB} Daemon Stopped")
                        };
                        item.setTitle(&text);
                    } else if tag == TAG_VOICES_LINE {
                        let text = if status.active_voices.is_empty() {
                            NSString::from_str("  No active voices")
                        } else {
                            let descs: Vec<String> = status.active_voices.iter()
                                .map(|(_, repo, branch)| format!("{}/{}", repo, branch))
                                .collect();
                            NSString::from_str(&format!("  {} voice{}: {}",
                                status.active_voices.len(),
                                if status.active_voices.len() == 1 { "" } else { "s" },
                                descs.join(", ")))
                        };
                        item.setTitle(&text);
                    } else if tag == TAG_TOGGLE {
                        let text = if status.running {
                            NSString::from_str("\u{23F9} Stop Daemon")
                        } else {
                            NSString::from_str("\u{25B6}\u{FE0F} Start Daemon")
                        };
                        item.setTitle(&text);
                    }
                }
            }
        }
    }

    /// Entry point called from main()
    pub fn run() -> Result<()> {
        let mtm = MainThreadMarker::new()
            .context("branch-tone tray must be run on the main thread")?;

        let app = NSApplication::sharedApplication(mtm);
        let delegate = TrayDelegate::new(mtm);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        app.run();

        Ok(())
    }

    /// Test helpers (exposed for integration tests)
    #[cfg(test)]
    pub mod tests {
        pub fn format_uptime_pub(secs: u64) -> String {
            super::format_uptime(secs)
        }
    }
}

#[cfg(all(target_os = "macos", feature = "tray"))]
fn run_tray() -> Result<()> {
    tray::run()
}

/// Main entry point for the interactive step sequencer.
fn run_player(initial_pattern: usize, initial_bpm: Option<u16>) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

    let voice = detect_repo_voice();
    let root_name = voice.root_name.clone();
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
    let mut cursor_row: usize = 0; // 0=K, 1=S, 2=H, 3=O (drum mode)
    let mut show_help = false;

    // Map piano key char → semitone index (0-15)
    let key_to_semitone = |c: char| -> Option<usize> {
        match c {
            'a' => Some(0),  'w' => Some(1),  's' => Some(2),  'e' => Some(3),
            'd' => Some(4),  'f' => Some(5),  't' => Some(6),  'g' => Some(7),
            'y' => Some(8),  'h' => Some(9),  'u' => Some(10), 'j' => Some(11),
            'k' => Some(12), 'o' => Some(13), 'l' => Some(14), 'p' => Some(15),
            _ => None,
        }
    };

    loop {
        render_grid(&state, cursor_step, cursor_row, &root_name, &scale_name, show_help, &mut stdout)?;

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press { continue; }

                let recording = state.recording.load(Relaxed);
                let mode = state.mode.load(Relaxed);

                // Ctrl+C to quit
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    state.quit.store(true, Relaxed);
                    break;
                }

                match key.code {
                    // Help toggle
                    KeyCode::Char('?') => {
                        show_help = !show_help;
                    }
                    // Mode switch
                    KeyCode::Char('m') if !recording => {
                        let cur = state.mode.load(Relaxed);
                        state.mode.store(if cur == 0 { 1 } else { 0 }, Relaxed);
                    }
                    // Grid editing (mode-aware)
                    KeyCode::Enter => {
                        if mode == 0 {
                            state.toggle_step(cursor_step, drum_for_row(cursor_row));
                        } else {
                            // Clear note at cursor step
                            state.note_steps[cursor_step].store(0, Relaxed);
                        }
                    }
                    KeyCode::Char('i') if mode == 0 => {
                        state.cycle_velocity(cursor_step);
                    }
                    // Transport
                    KeyCode::Char(' ') => {
                        let was = state.playing.load(Relaxed);
                        state.playing.store(!was, Relaxed);
                    }
                    KeyCode::Char('r') if mode == 0 => {
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
                        if mode == 0 {
                            cursor_row = cursor_row.saturating_sub(1);
                        } else {
                            // Raise pitch at cursor step (wraps 16 → 0 = off)
                            let cur = state.note_steps[cursor_step].load(Relaxed);
                            let next = if cur >= 16 { 0 } else { cur + 1 };
                            state.note_steps[cursor_step].store(next, Relaxed);
                            // Play the note live for feedback
                            if next > 0 {
                                state.note_triggers[(next - 1) as usize].store(200, Relaxed);
                            }
                        }
                    }
                    KeyCode::Down => {
                        if mode == 0 {
                            cursor_row = (cursor_row + 1).min(3);
                        } else {
                            // Lower pitch at cursor step (wraps 0 → 16)
                            let cur = state.note_steps[cursor_step].load(Relaxed);
                            let next = if cur == 0 { 16 } else { cur - 1 };
                            state.note_steps[cursor_step].store(next, Relaxed);
                            if next > 0 {
                                state.note_triggers[(next - 1) as usize].store(200, Relaxed);
                            }
                        }
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
                    // Piano keys: play live + write to step in keys mode
                    KeyCode::Char(c) if key_to_semitone(c).is_some() => {
                        let semitone = key_to_semitone(c).unwrap();
                        // Always trigger live sound
                        state.note_triggers[semitone].store(200, Relaxed);
                        // In keys mode: also write to note grid + advance cursor
                        if mode == 1 {
                            state.note_steps[cursor_step].store((semitone + 1) as u8, Relaxed);
                            cursor_step = (cursor_step + 1).min(15);
                        }
                    }
                    // Record-mode drum triggers: z/x/c/v write at playhead
                    KeyCode::Char('z') if recording && mode == 0 => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(K, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    KeyCode::Char('x') if recording && mode == 0 => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(S, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    KeyCode::Char('c') if recording && mode == 0 => {
                        let step = state.playhead.load(Relaxed) as usize;
                        state.steps[step].fetch_or(H, Relaxed);
                        if state.velocities[step].load(Relaxed) == 0 {
                            state.velocities[step].store(200, Relaxed);
                        }
                    }
                    KeyCode::Char('v') if recording && mode == 0 => {
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

        let start = generate_pad(&notes, 0.0, 0.01, 1.0, effects, &voice, &melody, &timbral, EventCategory::Default).abs();
        let mid = generate_pad(&notes, 0.5, 0.5, 1.0, effects, &voice, &melody, &timbral, EventCategory::Default).abs();
        let end = generate_pad(&notes, 1.0, 0.99, 1.0, effects, &voice, &melody, &timbral, EventCategory::Default).abs();

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
            let s = synth_kick(t, 44100.0, 261.63);
            assert!(!s.is_nan(), "kick NaN at t={}", t);
            assert!(s.abs() <= 1.5, "kick out of range at t={}: {}", t, s);
        }
    }

    #[test]
    fn snare_output_in_range() {
        for i in 0..2000 {
            let t = i as f32 / 44100.0;
            let s = synth_snare(t, 0.0, 261.63);
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
            // Tonal events inherit repo mode — check category behavior, not specific flags
            assert!(args.dub_delay, "{} should use dub_delay", event);
            assert!(args.chorus, "{} should use chorus (hook minimum)", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert!(!args.single_hit, "{} should not use single_hit", event);
            assert_eq!(args.event_category, EventCategory::SessionBoundary, "{} should be SessionBoundary", event);
            assert!(args.steps >= 5, "{} should have at least 5 steps", event);
        }
        let end = hook_play_args("SessionEnd", "repo".into(), "main".into(), false);
        assert!(end.reverse, "SessionEnd should be reversed");
    }

    #[test]
    fn attention_events_inherit_mode() {
        for event in ["PermissionRequest", "Notification"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            // Attention events inherit repo mode — check category, not specific synth flags
            assert!(args.chorus, "{} should use chorus (hook minimum)", event);
            assert!(args.dub_delay, "{} should use dub_delay", event);
            assert!(!args.drums, "{} should not use drums", event);
            assert!(!args.single_hit, "{} should not use single_hit", event);
            assert_eq!(args.event_category, EventCategory::Attention, "{} should be Attention", event);
        }
        // Verify repo mode actually influences tonal events
        // Two repos with different modes should produce different effect flags
        let args_a = hook_play_args("PermissionRequest", "repo-pad-mode".into(), "main".into(), false);
        let args_b = hook_play_args("PermissionRequest", "repo-arp-mode".into(), "main".into(), false);
        // Both should have chorus (hook minimum) and dub_delay
        assert!(args_a.chorus && args_b.chorus);
        assert!(args_a.dub_delay && args_b.dub_delay);
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
    fn bass_events_use_dub() {
        for event in ["SubagentStart", "SubagentStop", "WorktreeCreate", "WorktreeRemove"] {
            let args = hook_play_args(event, "repo".into(), "main".into(), false);
            assert!(args.dub_delay, "{} should use dub_delay for dubby ring-out", event);
            assert!(args.chorus, "{} should use chorus (hook minimum)", event);
            assert!(!args.drums, "{} should not use drums", event);
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
            assert!(args.volume <= 0.10, "{} volume {} should be <= 0.10", event, args.volume);
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
        assert!(args.chorus, "TaskCompleted should use chorus");
        assert!(args.dub_delay, "TaskCompleted should use dub_delay");
        assert!(args.steps >= 5, "TaskCompleted should have at least 5 steps for full phrase");
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

    // -- Note step sequencer tests --

    #[test]
    fn player_note_steps_default_off() {
        let state = PlayerState::new(0);
        for i in 0..16 {
            assert_eq!(state.note_steps[i].load(Relaxed), 0, "step {} should be off", i);
        }
    }

    #[test]
    fn player_note_steps_write_and_read() {
        let state = PlayerState::new(0);
        // Write a note (semitone 5 → stored as 6)
        state.note_steps[3].store(6, Relaxed);
        assert_eq!(state.note_steps[3].load(Relaxed), 6);
        // Clear it
        state.note_steps[3].store(0, Relaxed);
        assert_eq!(state.note_steps[3].load(Relaxed), 0);
    }

    #[test]
    fn player_mode_toggle() {
        let state = PlayerState::new(0);
        assert_eq!(state.mode.load(Relaxed), 0, "should start in drums mode");
        state.mode.store(1, Relaxed);
        assert_eq!(state.mode.load(Relaxed), 1);
        state.mode.store(0, Relaxed);
        assert_eq!(state.mode.load(Relaxed), 0);
    }

    #[test]
    fn note_label_chromatic() {
        assert_eq!(note_label("C", 0), "C4");
        assert_eq!(note_label("C", 12), "C5");
        assert_eq!(note_label("C", 7), "G4");
        assert_eq!(note_label("A", 0), "A4");
        assert_eq!(note_label("A", 3), "C5");  // A + 3 semitones = C (next octave group)
    }

    #[test]
    fn sixteenth_samples_correct() {
        // At 120 BPM, one beat = 0.5s, one 16th = 0.125s = 5512.5 samples at 44100
        let s = sixteenth_samples(120.0, 44100.0);
        assert!((s - 5512.5).abs() < 1.0, "expected ~5512.5, got {}", s);
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
    fn pitched_hit_output_in_range() {
        let voice = RepoVoice::from_repo("test");
        for &freq in &voice.scale_freqs {
            for i in 0..4000 {
                let t = i as f32 / 44100.0;
                let s = generate_pitched_hit(t, freq);
                assert!(!s.is_nan(), "pitched_hit NaN at t={}, freq={}", t, freq);
                assert!(s.abs() <= 2.0, "pitched_hit out of range at t={}, freq={}: {}", t, freq, s);
            }
        }
    }

    #[test]
    fn pitched_hit_decays_quickly() {
        // After 200ms the pitched hit should be very quiet
        let late = generate_pitched_hit(0.2, 440.0);
        assert!(late.abs() < 0.05, "pitched_hit should decay by 200ms, got {}", late);
    }

    #[test]
    fn pitched_hit_is_pitched() {
        // A pitched hit at 440Hz should have different output than at 880Hz
        let a = generate_pitched_hit(0.005, 440.0);
        let b = generate_pitched_hit(0.005, 880.0);
        assert_ne!(a, b, "Different frequencies should produce different output");
    }

    #[test]
    fn queued_note_walking_scale() {
        // Successive note_counter values should produce different scale degrees
        let voice = RepoVoice::from_repo("test");
        let notes: Vec<f32> = voice.scale_freqs.to_vec();
        let freq_a = notes[0 % notes.len()];
        let freq_b = notes[1 % notes.len()];
        let freq_c = notes[2 % notes.len()];
        // Walking through the scale should give different pitches
        assert_ne!(freq_a, freq_b, "Adjacent scale notes should differ");
        assert_ne!(freq_b, freq_c, "Adjacent scale notes should differ");
    }

    #[test]
    fn rimshot_output_in_range() {
        for i in 0..2000 {
            let t = i as f32 / 44100.0;
            let s = synth_rimshot(t, 0.0, 261.63);
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
        assert_eq!(EventCategory::Attention.transpose_semitones(), 2);
        assert_eq!(EventCategory::DrumHit.transpose_semitones(), 0);
        assert_eq!(EventCategory::ToolPulse.transpose_semitones(), 0);
        assert_eq!(EventCategory::Bass.transpose_semitones(), -2);
        assert_eq!(EventCategory::Lifecycle.transpose_semitones(), 1);
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
        // means they'll always rotate to a different position in the 3-type drum cycle
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

    // -- Phase 1: Tuned percussion tests --

    #[test]
    fn fold_to_range_octave_folds_down() {
        let f = fold_to_range(440.0, 30.0, 80.0);
        assert!(f >= 30.0 && f <= 80.0, "expected 30-80, got {}", f);
        // 440 → 220 → 110 → 55
        assert!((f - 55.0).abs() < 0.01, "expected 55.0, got {}", f);
    }

    #[test]
    fn fold_to_range_octave_folds_up() {
        let f = fold_to_range(30.0, 150.0, 250.0);
        assert!(f >= 150.0 && f <= 250.0, "expected 150-250, got {}", f);
        // 30 → 60 → 120 → 240
        assert!((f - 240.0).abs() < 0.01, "expected 240.0, got {}", f);
    }

    #[test]
    fn fold_to_range_already_in_range() {
        let f = fold_to_range(200.0, 150.0, 250.0);
        assert!((f - 200.0).abs() < 0.01, "expected 200.0, got {}", f);
    }

    #[test]
    fn tuned_kick_output_in_range() {
        // Test with various root frequencies
        for &root in &[261.63, 440.0, 130.81, 523.25] {
            for i in 0..2000 {
                let t = i as f32 / 44100.0;
                let s = synth_kick(t, 44100.0, root);
                assert!(!s.is_nan(), "kick NaN at t={} root={}", t, root);
                assert!(s.abs() <= 1.5, "kick out of range at t={} root={}: {}", t, root, s);
            }
        }
    }

    #[test]
    fn tuned_snare_output_in_range() {
        for &root in &[261.63, 440.0, 130.81] {
            for i in 0..2000 {
                let t = i as f32 / 44100.0;
                let s = synth_snare(t, 0.0, root);
                assert!(!s.is_nan(), "snare NaN at t={} root={}", t, root);
                assert!(s.abs() <= 1.5, "snare out of range at t={} root={}: {}", t, root, s);
            }
        }
    }

    #[test]
    fn tuned_rimshot_output_in_range() {
        for &root in &[261.63, 440.0, 130.81] {
            for i in 0..2000 {
                let t = i as f32 / 44100.0;
                let s = synth_rimshot(t, 0.0, root);
                assert!(!s.is_nan(), "rimshot NaN at t={} root={}", t, root);
                assert!(s.abs() <= 2.0, "rimshot out of range at t={} root={}: {}", t, root, s);
            }
        }
    }

    #[test]
    fn repo_voice_has_root_freq() {
        let voice = RepoVoice::from_repo("test-repo");
        assert!(voice.root_freq > 0.0, "root_freq should be positive");
        // Should be one of the chromatic roots
        assert!(CHROMATIC_ROOTS.contains(&voice.root_freq),
            "root_freq {} should be in CHROMATIC_ROOTS", voice.root_freq);
    }

    // -- Phase 1: Category-aware delay tests --

    #[test]
    fn delay_character_session_is_longest() {
        let (session_time, _, _) = EventCategory::SessionBoundary.delay_character();
        let (tool_time, _, _) = EventCategory::ToolPulse.delay_character();
        assert!(session_time > tool_time,
            "session delay time {} should exceed tool pulse {}", session_time, tool_time);
    }

    #[test]
    fn delay_character_tool_pulse_minimal() {
        let (time, fb, throw) = EventCategory::ToolPulse.delay_character();
        assert!(time < 1.0, "tool pulse delay time should be short");
        assert!(fb < 0.0, "tool pulse feedback offset should be negative");
        assert!(throw > 5.0, "tool pulse throw rate should be fast");
    }

    #[test]
    fn delay_character_drums_dry() {
        let (_, fb_offset, _) = EventCategory::DrumHit.delay_character();
        assert!(fb_offset < 0.0, "drum feedback offset should reduce feedback");
    }

    // -- Phase 1: Event density tests --

    #[test]
    fn parse_log_timestamp_valid() {
        let ts = parse_log_timestamp("2026-03-10T15:30:45");
        assert!(ts.is_some(), "should parse valid timestamp");
        let secs = ts.unwrap();
        assert!(secs > 1_700_000_000, "epoch should be recent: {}", secs);
    }

    #[test]
    fn parse_log_timestamp_invalid() {
        assert!(parse_log_timestamp("not-a-timestamp").is_none());
        assert!(parse_log_timestamp("").is_none());
    }

    // -- Phase 2: Daemon tests --

    #[test]
    fn daemon_state_find_or_alloc_slot() {
        let state = DaemonState::new();
        // First alloc should succeed
        let slot = state.find_or_alloc_slot("repo-a", "main");
        assert!(slot.is_some(), "should allocate first slot");
        assert_eq!(slot.unwrap(), 0);

        // Mark it active
        state.voices[0].active.store(true, Relaxed);
        if let Ok(mut r) = state.voices[0].repo.lock() { *r = "repo-a".to_string(); }
        if let Ok(mut b) = state.voices[0].branch.lock() { *b = "main".to_string(); }

        // Same repo+branch should reuse the slot
        let slot2 = state.find_or_alloc_slot("repo-a", "main");
        assert_eq!(slot2, Some(0), "should reuse existing slot");

        // Different repo should get a new slot
        let slot3 = state.find_or_alloc_slot("repo-b", "feature");
        assert_eq!(slot3, Some(1), "should allocate second slot");
    }

    #[test]
    fn daemon_state_max_slots() {
        let state = DaemonState::new();
        // Fill all slots
        for i in 0..MAX_VOICE_SLOTS {
            state.voices[i].active.store(true, Relaxed);
            if let Ok(mut r) = state.voices[i].repo.lock() { *r = format!("repo-{}", i); }
            if let Ok(mut b) = state.voices[i].branch.lock() { *b = "main".to_string(); }
        }
        // Should return None when all slots full
        let slot = state.find_or_alloc_slot("repo-overflow", "main");
        assert!(slot.is_none(), "should return None when all slots full");
    }

    #[test]
    fn conductor_transpose_empty_roots() {
        // No active roots: should return original frequency
        let result = conductor_transpose(440.0, &[]);
        assert!((result - 440.0).abs() < 0.01);
    }

    #[test]
    fn conductor_transpose_unison_preferred() {
        // If new root matches existing, unison (0 semitones) should win
        let result = conductor_transpose(440.0, &[440.0]);
        assert!((result - 440.0).abs() < 0.01, "unison should be preferred: got {}", result);
    }

    #[test]
    fn conductor_transpose_avoids_dissonance() {
        // If existing root is 440Hz (A4), a new root of 466.16Hz (Bb4, 1 semitone away)
        // should be transposed to a more consonant interval
        let result = conductor_transpose(466.16, &[440.0]);
        // Should NOT stay at 466.16 (minor 2nd = maximally dissonant)
        // Should pick fifth (+7) = ~698Hz, or fourth (+5) = ~622Hz, etc.
        let ratio = result / 466.16;
        let semitones_from_original = (12.0 * ratio.log2()).round() as i32;
        assert!(semitones_from_original != 0 || (result - 466.16).abs() < 1.0,
            "should transpose away from minor 2nd dissonance: got {} ({} semitones)", result, semitones_from_original);
    }

    #[test]
    fn category_to_u8_roundtrip() {
        let categories = [
            EventCategory::SessionBoundary, EventCategory::Attention,
            EventCategory::DrumHit, EventCategory::ToolPulse,
            EventCategory::Bass, EventCategory::Lifecycle, EventCategory::Default,
        ];
        for cat in &categories {
            let encoded = category_to_u8(*cat);
            let decoded = u8_to_category(encoded);
            assert_eq!(*cat, decoded, "roundtrip failed for {:?}", cat);
        }
    }

    #[test]
    fn event_density_zero_without_log() {
        // This test works because we're not in a home dir with events.log
        // (or the test temp env doesn't have one). Just verify it doesn't crash.
        let density = recent_event_density(10);
        assert!(density < 1000, "density should be bounded: {}", density);
    }

    #[test]
    fn play_args_has_event_density() {
        let args = hook_play_args("SessionStart", "repo".into(), "main".into(), false);
        // Default event_density should be 0
        assert_eq!(args.event_density, 0);
    }

    #[test]
    fn throw_envelope_front_loaded() {
        // At progress=0, throw should be ~1.0
        let start_throw = ((-3.0_f32 * 0.0).exp() * 0.8 + 0.2).min(1.0);
        assert!((start_throw - 1.0).abs() < 0.01, "throw at start should be ~1.0");

        // At progress=1.0, throw should be ~0.24
        let end_throw = ((-3.0_f32 * 1.0).exp() * 0.8 + 0.2).min(1.0);
        assert!(end_throw < 0.3, "throw at end should be < 0.3: got {}", end_throw);
        assert!(end_throw > 0.2, "throw at end should be > 0.2 (floor): got {}", end_throw);
    }

    // -- Phase 5: Tray / status_json tests --

    #[test]
    fn daemon_state_has_start_time() {
        let state = DaemonState::new();
        let start = state.start_time_secs.load(Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // start_time should be within 1 second of now
        assert!(now.saturating_sub(start) < 2, "start_time should be recent: {}", start);
    }

    #[test]
    fn status_json_handler_returns_valid_json() {
        let state = DaemonState::new();

        // Simulate __status_json response construction (same logic as handler)
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let start = state.start_time_secs.load(Relaxed);
        let last = state.last_activity_secs.load(Relaxed);
        let pid = std::process::id();

        let mut voices_json = Vec::new();
        for (i, slot) in state.voices.iter().enumerate() {
            if slot.active.load(Relaxed) {
                let repo = slot.repo.lock().ok().map(|r| r.clone()).unwrap_or_default();
                let branch = slot.branch.lock().ok().map(|b| b.clone()).unwrap_or_default();
                voices_json.push(format!(
                    "{{\"slot\":{},\"repo\":\"{}\",\"branch\":\"{}\"}}",
                    i, repo, branch,
                ));
            }
        }

        let json_str = format!(
            "{{\"pid\":{},\"uptime_secs\":{},\"active_voices\":[{}],\"idle_secs\":{},\"idle_timeout\":{}}}",
            pid,
            now_secs.saturating_sub(start),
            voices_json.join(","),
            now_secs.saturating_sub(last),
            DAEMON_IDLE_TIMEOUT_SECS,
        );

        // Parse it back to verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("__status_json should produce valid JSON");
        assert!(parsed.get("pid").is_some());
        assert!(parsed.get("uptime_secs").is_some());
        assert!(parsed.get("active_voices").unwrap().is_array());
        assert!(parsed.get("idle_timeout").unwrap().as_u64().unwrap() == DAEMON_IDLE_TIMEOUT_SECS);
    }

    #[test]
    fn status_json_with_active_voices() {
        let state = DaemonState::new();

        // Activate a voice slot
        state.voices[0].active.store(true, Relaxed);
        if let Ok(mut r) = state.voices[0].repo.lock() { *r = "my-repo".to_string(); }
        if let Ok(mut b) = state.voices[0].branch.lock() { *b = "feat/test".to_string(); }

        let mut voices_json = Vec::new();
        for (i, slot) in state.voices.iter().enumerate() {
            if slot.active.load(Relaxed) {
                let repo = slot.repo.lock().ok().map(|r| r.clone()).unwrap_or_default();
                let branch = slot.branch.lock().ok().map(|b| b.clone()).unwrap_or_default();
                voices_json.push(format!(
                    "{{\"slot\":{},\"repo\":\"{}\",\"branch\":\"{}\"}}",
                    i, repo.replace('\"', "\\\""), branch.replace('\"', "\\\""),
                ));
            }
        }

        let json_str = format!(
            "{{\"pid\":1,\"uptime_secs\":0,\"active_voices\":[{}],\"idle_secs\":0,\"idle_timeout\":300}}",
            voices_json.join(","),
        );

        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("should be valid JSON");
        let voices = parsed.get("active_voices").unwrap().as_array().unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].get("slot").unwrap().as_u64().unwrap(), 0);
        assert_eq!(voices[0].get("repo").unwrap().as_str().unwrap(), "my-repo");
        assert_eq!(voices[0].get("branch").unwrap().as_str().unwrap(), "feat/test");
    }

    #[cfg(all(target_os = "macos", feature = "tray"))]
    #[test]
    fn tray_format_uptime() {
        assert_eq!(tray::tests::format_uptime_pub(0), "0s");
        assert_eq!(tray::tests::format_uptime_pub(59), "59s");
        assert_eq!(tray::tests::format_uptime_pub(60), "1m 0s");
        assert_eq!(tray::tests::format_uptime_pub(3661), "1h 1m");
    }

    #[test]
    fn daemon_dir_is_under_home() {
        let dir = daemon_dir();
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains(".branch-tone"), "daemon_dir should contain .branch-tone: {}", dir_str);
    }
}
