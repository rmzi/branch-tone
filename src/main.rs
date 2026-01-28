// =============================================================================
// branch-tone: Generate unique musical tones from git branch names
// =============================================================================
//
// RUST LEARNING NOTES:
// - Lines starting with // are comments
// - Lines starting with //! are "inner doc comments" (document the module)
// - Lines starting with /// are "outer doc comments" (document the next item)
//
// This file demonstrates core Rust concepts:
// - Ownership and borrowing
// - Error handling with Result
// - Structs and implementations
// - Traits (like interfaces)
// - Pattern matching
// - Iterators
// =============================================================================

// -----------------------------------------------------------------------------
// USE STATEMENTS
// -----------------------------------------------------------------------------
// `use` brings items into scope, similar to `import` in JavaScript/Python.
// The `::` is the path separator (like `/` in file paths or `.` in JS).
//
// Convention: group standard library (`std`), then external crates, then local.
// -----------------------------------------------------------------------------

// Standard library imports
use std::f32::consts::PI;        // Mathematical constant π
use std::time::Duration;          // For specifying tone duration

// External crate imports (from Cargo.toml dependencies)
use anyhow::{Context, Result};    // Error handling helpers
use clap::Parser;                 // CLI argument parsing (derive macro)
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};  // Audio traits
use sha2::{Sha256, Digest};       // Hashing

// -----------------------------------------------------------------------------
// CLI ARGUMENT PARSING WITH CLAP
// -----------------------------------------------------------------------------
// Clap uses Rust's "derive" macros to generate argument parsing code.
// The #[derive(...)] attribute tells the compiler to auto-implement traits.
//
// RUST CONCEPT - STRUCTS:
// A struct is like a class without methods (data only).
// Methods are added separately in `impl` blocks.
// -----------------------------------------------------------------------------

/// Command-line arguments for branch-tone
///
/// The triple-slash comments become the --help text!
#[derive(Parser, Debug)]  // Derive Parser (for CLI) and Debug (for printing)
#[command(name = "branch-tone")]
#[command(author = "Ramzi Abdoch")]
#[command(version = "0.1.0")]
#[command(about = "Generate unique musical tones from git branch names")]
#[command(long_about = r#"
branch-tone creates a unique, pleasant tone based on the hash of a branch name.
Each branch gets its own sonic identity - same branch always produces same tone.

The tone uses a pentatonic scale (C, D, E, G, A) so every branch sounds musical.

Examples:
  branch-tone feature/auth      # Play tone for this branch
  branch-tone main --duration 500   # Play for 500ms
  branch-tone $(git branch --show-current)  # Current branch
"#)]
struct Args {
    /// The branch name to generate a tone for
    ///
    /// RUST CONCEPT - OPTION<T>:
    /// Option<String> means "might have a String, might be None"
    /// This replaces null/undefined from other languages.
    /// - Some("value") = has a value
    /// - None = no value
    ///
    /// If not provided, we'll try to detect the current git branch.
    #[arg(value_name = "BRANCH")]
    branch: Option<String>,

    /// Duration of the tone in milliseconds
    ///
    /// The #[arg] attribute configures how clap handles this argument.
    /// `short` means -d, `long` means --duration, `default_value` is fallback.
    #[arg(short, long, default_value = "300")]
    duration: u64,  // u64 = unsigned 64-bit integer (can't be negative)

    /// Volume level (0.0 to 1.0)
    #[arg(short, long, default_value = "0.3")]
    volume: f32,    // f32 = 32-bit floating point

    /// Just print the frequency without playing
    #[arg(long)]
    dry_run: bool,  // bool = true/false, flags are false by default
}

// -----------------------------------------------------------------------------
// MUSICAL CONSTANTS
// -----------------------------------------------------------------------------
// We use a pentatonic scale because it always sounds good - no dissonance.
// These frequencies are in Hz (cycles per second).
// -----------------------------------------------------------------------------

/// Pentatonic scale frequencies starting from C4 (middle C)
///
/// RUST CONCEPT - ARRAYS:
/// [f32; 5] means "array of 5 f32 values"
/// Arrays have fixed size known at compile time.
/// For dynamic size, you'd use Vec<f32> (a vector).
const PENTATONIC_SCALE: [f32; 5] = [
    261.63,  // C4
    293.66,  // D4
    329.63,  // E4
    392.00,  // G4
    440.00,  // A4
];

/// Available octave multipliers (we'll pick based on hash)
/// Octave 3 = half frequency, Octave 5 = double frequency
const OCTAVE_MULTIPLIERS: [f32; 3] = [
    0.5,   // Octave 3 (lower)
    1.0,   // Octave 4 (middle)
    2.0,   // Octave 5 (higher)
];

// -----------------------------------------------------------------------------
// TONE PARAMETERS STRUCT
// -----------------------------------------------------------------------------
// This struct holds all the musical parameters derived from the hash.
// -----------------------------------------------------------------------------

/// Musical parameters derived from branch name hash
///
/// RUST CONCEPT - #[derive(Debug)]:
/// Debug trait lets us print the struct with {:?} format specifier.
/// Useful for development/logging.
#[derive(Debug)]
struct ToneParams {
    frequency: f32,      // The note frequency in Hz
    duration_ms: u64,    // How long to play
    volume: f32,         // 0.0 to 1.0
    attack_ms: u64,      // Fade in time
    decay_ms: u64,       // Fade out time
}

// -----------------------------------------------------------------------------
// IMPLEMENTATION BLOCKS
// -----------------------------------------------------------------------------
// `impl` blocks add methods to structs.
// Methods that take `&self` borrow the struct (read-only access).
// Methods that take `&mut self` borrow mutably (can modify).
// Methods that take `self` consume the struct (take ownership).
// Associated functions (no self) are like static methods.
// -----------------------------------------------------------------------------

impl ToneParams {
    /// Create ToneParams from a branch name
    ///
    /// RUST CONCEPT - ASSOCIATED FUNCTION:
    /// This is called with ToneParams::from_branch(...), not instance.from_branch()
    /// because it doesn't take `self` - it creates a new instance.
    ///
    /// RUST CONCEPT - &str vs String:
    /// - String = owned string, lives on heap, can be modified
    /// - &str = borrowed string slice, just a view into string data
    /// Using &str in parameters is more flexible (accepts both String and &str).
    fn from_branch(branch: &str, duration_ms: u64, volume: f32) -> Self {
        // Hash the branch name using SHA-256
        // This gives us deterministic "random" bytes
        let mut hasher = Sha256::new();
        hasher.update(branch.as_bytes());  // Convert string to bytes
        let hash = hasher.finalize();      // Get the 32-byte hash

        // Extract musical parameters from hash bytes
        // Each byte is 0-255, we map these to our musical values

        // Byte 0: Select note from pentatonic scale (0-4)
        let note_index = (hash[0] as usize) % PENTATONIC_SCALE.len();
        let base_freq = PENTATONIC_SCALE[note_index];

        // Byte 1: Select octave multiplier (0-2)
        let octave_index = (hash[1] as usize) % OCTAVE_MULTIPLIERS.len();
        let octave_mult = OCTAVE_MULTIPLIERS[octave_index];

        // Final frequency
        let frequency = base_freq * octave_mult;

        // Byte 2: Attack time (20-80ms) - how fast the sound fades in
        let attack_ms = 20 + ((hash[2] as u64) % 60);

        // Byte 3: Decay time (50-150ms) - how fast it fades out
        let decay_ms = 50 + ((hash[3] as u64) % 100);

        // RUST CONCEPT - RETURNING VALUES:
        // The last expression without a semicolon is the return value.
        // This is idiomatic Rust (though `return` also works).
        // `Self` refers to the type we're implementing (ToneParams).
        Self {
            frequency,      // Shorthand: same as `frequency: frequency`
            duration_ms,
            volume,
            attack_ms,
            decay_ms,
        }
    }
}

// -----------------------------------------------------------------------------
// MAIN FUNCTION
// -----------------------------------------------------------------------------
// The entry point of the program.
//
// RUST CONCEPT - RESULT RETURN TYPE:
// Returning Result<()> means:
// - Ok(()) = success (unit type () is like void)
// - Err(e) = failure with error e
//
// The `?` operator propagates errors automatically (early return on error).
// -----------------------------------------------------------------------------

fn main() -> Result<()> {
    // Parse command-line arguments
    // Clap handles --help, --version, and validation automatically
    let args = Args::parse();

    // Get the branch name (from args or detect from git)
    let branch = match args.branch {
        // RUST CONCEPT - PATTERN MATCHING:
        // `match` is like switch but way more powerful.
        // Must handle all possible cases (exhaustive).
        Some(b) => b,  // If Some, extract the inner value
        None => get_current_branch()
            .context("No branch specified and couldn't detect current git branch")?,
            // The ? propagates the error if get_current_branch() fails
    };

    // Generate tone parameters from branch name
    let params = ToneParams::from_branch(&branch, args.duration, args.volume);

    // Print info
    println!("🎵 Branch: {}", branch);
    println!("   Frequency: {:.2} Hz", params.frequency);
    println!("   Duration: {} ms", params.duration_ms);
    println!("   Attack: {} ms, Decay: {} ms", params.attack_ms, params.decay_ms);

    // If dry run, stop here
    if args.dry_run {
        return Ok(());  // Early return, success
    }

    // Play the tone
    play_tone(&params)?;  // ? propagates any audio errors

    Ok(())  // Success!
}

// -----------------------------------------------------------------------------
// GIT BRANCH DETECTION
// -----------------------------------------------------------------------------
// Runs `git branch --show-current` and captures the output.
// -----------------------------------------------------------------------------

/// Get the current git branch name
///
/// RUST CONCEPT - RESULT<T, E>:
/// Result has two type parameters:
/// - T = the success type (String here)
/// - E = the error type (anyhow::Error here, written as just Error with anyhow)
///
/// anyhow::Result<T> is shorthand for Result<T, anyhow::Error>
fn get_current_branch() -> Result<String> {
    // std::process::Command runs external commands
    let output = std::process::Command::new("git")
        .args(["branch", "--show-current"])  // Array of arguments
        .output()                             // Run and capture output
        .context("Failed to run git command")?;  // Add context to error

    // Check if command succeeded
    if !output.status.success() {
        // RUST CONCEPT - anyhow::bail!:
        // Macro that creates an error and returns early
        anyhow::bail!("git command failed - are you in a git repository?");
    }

    // Convert output bytes to string and trim whitespace
    let branch = String::from_utf8(output.stdout)
        .context("Git output was not valid UTF-8")?
        .trim()      // Remove whitespace/newlines
        .to_string(); // Convert &str back to owned String

    // Check if we got anything
    if branch.is_empty() {
        anyhow::bail!("No current branch (detached HEAD?)");
    }

    Ok(branch)  // Wrap in Ok for Result return type
}

// -----------------------------------------------------------------------------
// AUDIO PLAYBACK
// -----------------------------------------------------------------------------
// This is where the magic happens! We use CPAL to generate and play audio.
// -----------------------------------------------------------------------------

/// Play a tone with the given parameters
///
/// RUST CONCEPT - BORROWING WITH &:
/// `params: &ToneParams` borrows the struct (read-only reference).
/// The caller keeps ownership, we just borrow it temporarily.
/// This is Rust's way of avoiding unnecessary copies.
fn play_tone(params: &ToneParams) -> Result<()> {
    // Get the default audio host (CoreAudio on macOS, WASAPI on Windows, etc.)
    let host = cpal::default_host();

    // Get the default output device (speakers/headphones)
    let device = host
        .default_output_device()
        .context("No audio output device found")?;

    // Get the device's default audio format
    let config = device
        .default_output_config()
        .context("Failed to get default audio config")?;

    // RUST CONCEPT - MATCH ON ENUM:
    // SampleFormat is an enum with variants like F32, I16, U16.
    // We handle each format differently.
    match config.sample_format() {
        cpal::SampleFormat::F32 => run_audio::<f32>(&device, &config.into(), params),
        cpal::SampleFormat::I16 => run_audio::<i16>(&device, &config.into(), params),
        cpal::SampleFormat::U16 => run_audio::<u16>(&device, &config.into(), params),
        format => anyhow::bail!("Unsupported sample format: {:?}", format),
    }
}

/// Generic audio runner
///
/// RUST CONCEPT - GENERICS:
/// `<T: cpal::SizedSample + cpal::FromSample<f32>>` means:
/// - T is a generic type parameter
/// - T must implement SizedSample trait (it's an audio sample)
/// - T must implement FromSample<f32> (can convert from f32)
///
/// This lets us write one function that works with f32, i16, or u16 samples.
fn run_audio<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    params: &ToneParams,
) -> Result<()>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,  // Trait bounds
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // RUST LESSON - AVOIDING LIFETIME ISSUES:
    // We copy the values we need from `params` into local owned variables.
    // The closure will then capture these owned values, not the borrowed reference.
    // This is a common pattern when closures need to outlive their scope.
    let frequency = params.frequency;
    let duration_ms = params.duration_ms;
    let volume = params.volume;
    let attack_ms = params.attack_ms;
    let decay_ms = params.decay_ms;

    // Calculate total samples
    let total_samples = (sample_rate * duration_ms as f32 / 1000.0) as usize;
    let attack_samples = (sample_rate * attack_ms as f32 / 1000.0) as usize;
    let decay_samples = (sample_rate * decay_ms as f32 / 1000.0) as usize;

    // RUST CONCEPT - INTERIOR MUTABILITY:
    // We need to modify `sample_clock` inside the closure, but closures
    // capture variables by reference. We use a simple counter here.
    let mut sample_clock = 0usize;

    // RUST CONCEPT - CLOSURES:
    // `move |...| { ... }` is a closure (anonymous function).
    // `move` means it takes ownership of captured variables.
    let mut next_value = move || -> f32 {
        // Check if we're done
        if sample_clock >= total_samples {
            return 0.0;
        }

        // Calculate envelope (volume curve over time)
        let envelope = if sample_clock < attack_samples {
            // Attack phase: fade in
            sample_clock as f32 / attack_samples as f32
        } else if sample_clock > total_samples - decay_samples {
            // Decay phase: fade out
            (total_samples - sample_clock) as f32 / decay_samples as f32
        } else {
            // Sustain phase: full volume
            1.0
        };

        // Generate sine wave sample
        // Formula: sin(2π * frequency * time)
        let time = sample_clock as f32 / sample_rate;
        let sample = (2.0 * PI * frequency * time).sin();

        // Apply envelope and volume
        let output = sample * envelope * volume;

        sample_clock += 1;
        output
    };

    // Track if we've finished playing
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let finished_clone = finished.clone();

    // Create the audio stream
    // RUST CONCEPT - ERROR HANDLING IN CLOSURES:
    // The error callback handles audio errors asynchronously.
    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // Fill the audio buffer with our samples
            for frame in data.chunks_mut(channels) {
                let value = next_value();

                // Check if we're done
                if sample_clock >= total_samples && value == 0.0 {
                    finished_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                }

                // Convert f32 to whatever sample format the device wants
                let sample = T::from_sample(value);

                // Write to all channels (usually 2 for stereo)
                for channel_sample in frame.iter_mut() {
                    *channel_sample = sample;
                }
            }
        },
        err_fn,
        None,  // No timeout
    ).context("Failed to build audio stream")?;

    // Start playback
    stream.play().context("Failed to start audio playback")?;

    // Wait for playback to finish
    // RUST CONCEPT - BUSY WAITING:
    // Not ideal, but simple. A better approach would use channels or condvars.
    while !finished.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }

    // Small delay to ensure audio buffer flushes
    std::thread::sleep(Duration::from_millis(50));

    Ok(())
}
