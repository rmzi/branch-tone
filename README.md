<p align="center">
  <h1 align="center">branch-tone</h1>
  <p align="center">
    <strong>Hear your git context. Every repo has a voice. Every branch has a melody.</strong>
  </p>
  <p align="center">
    <a href="https://github.com/rmzi/branch-tone/releases"><img alt="Version" src="https://img.shields.io/badge/version-0.9.0-blue?style=flat-square"></a>
    <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/rust-2024_edition-orange?style=flat-square&logo=rust"></a>
    <a href="https://github.com/rmzi/branch-tone/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/license-MIT-green?style=flat-square"></a>
    <a href="#"><img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey?style=flat-square"></a>
    <a href="#claude-code-hooks"><img alt="Claude Code" src="https://img.shields.io/badge/Claude_Code-hooks-blueviolet?style=flat-square"></a>
  </p>
</p>

---

A CLI synthesizer that generates deterministic musical phrases from git branch names. Each repository gets its own harmonic identity — key, scale, timbre, pad shape — while each branch gets its own melodic identity — pattern, rhythm, envelope. Same inputs always produce the same sound.

Comes with a built-in **interactive step sequencer** featuring 10 classic drum breaks, a chromatic piano keyboard, and 7 synth presets inspired by jungle/liquid DnB hardware (Roland Juno-106, JP-8000, Korg M1).

## Why?

Ever lose track of which terminal is on which branch? Now you can hear it.

One repo might be **A# Lydian** with a Swell pad shape. Another lands on **E Mixolydian** with a Pulse. Branches within each repo share that harmonic character but diverge in melody, rhythm, swing, and envelope — so `main` and `feature/auth` in the same repo sound like siblings, while the same branch in a different repo sounds like a stranger.

The result is ethereal, spaceship-like tones that ring out and overlap — inspired by the pad sounds of 90s jungle and liquid drum & bass.

## Install

```bash
curl -sSL https://raw.githubusercontent.com/rmzi/branch-tone/main/install.sh | sh
```

Then register the Claude Code plugin:

```bash
branch-tone init                        # User scope (all projects)
branch-tone init --scope project        # Project scope (team-shared)
```

Or manually:

```bash
cargo install --git https://github.com/rmzi/branch-tone
branch-tone init
```

## Quick Start

```bash
branch-tone                    # Play tone for current branch
branch-tone feature/auth       # Specific branch
branch-tone --dry-run          # Show parameters without playing
```

```
🎵 Repo: my-project | Branch: feature/auth [Arpeggio]
   Key: E Mixolydian | Octave: 1.5x | Shape: Swell
   Timbre: harmonic=0.13, 3rd=0.11
   Notes: ["494Hz", "881Hz", "623Hz"]
   Pattern: #4 | Envelope: Soft | Swing: 30%
   Stagger: [0.00, 0.02, 0.06, 0.06, 0.12]
   Spread: 0.92 | Duration: 600ms
```

## Usage

```bash
# Sound modes
branch-tone --pad              # Warm subtractive chord
branch-tone --chorus           # BBD-style detuned layers
branch-tone --tremolo          # Volume wobble
branch-tone --bulldozer        # Layered pad (70%) + arp shimmer (30%)
branch-tone --spooky           # Thin sines, dark filter, eerie resonance

# Parameters
branch-tone --steps 5          # 5-note phrase (default: 3)
branch-tone --reverse          # Descending instead of ascending
branch-tone -d 800 -v 0.5     # Duration (ms) and volume (0.0–1.0)
branch-tone --repo my-project  # Explicit repo name
```

### Environment Variables

| Variable | Default | Effect |
|----------|---------|--------|
| `BRANCH_TONE_VOLUME` | `1.0` | Global volume multiplier |
| `BRANCH_TONE_TEMPO` | `1.0` | Global duration multiplier |

## Interactive Step Sequencer

A real-time drum machine and piano keyboard that uses your repo's harmonic identity as its scale.

```bash
branch-tone player                    # Amen break at native BPM
branch-tone player --pattern 3        # Apache break
branch-tone player --bpm 140          # Custom BPM
```

```
 branch-tone player --- Amen @ 136 BPM --- [PLAYING]

    1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16
K   ●  ·  ·  ·  ●  ·  ·  ●  ·  ·  ●  ·  ·  ·  ·  ·
S   ·  ·  ·  ·  ●  ·  ·  ·  ·  ·  ●  ·  ·  ○  ·  ·
H   ●  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●  ●
O   ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·  ·

                        ▲

 Piano: Oct 0 | Synth: Iceman | Shape: Swell

    W E   T Y U   O P
   A S D F G H J K L       [A# Lydian]

 [enter] toggle  [i] ghost  [</>] move  [^/v] row
 [1-0] pattern   [+/-] BPM  [space] play/pause  [q] quit
 [A-L] piano     [r] record  [z/x/c/v] rec K/S/H/O
 [[\]] octave   [,/.] synth  [;/'] pad shape  [tab] sustain
```

**Drum grid** — 16 steps, 4 rows (Kick, Snare, Hi-hat, Open hat). Toggle full hits (●) and ghost notes (○) with velocity cycling.

**10 classic breaks:**

| # | Break | Native BPM |
|---|-------|------------|
| 0 | Amen | 136 |
| 1 | Think | 120 |
| 2 | Funky Drummer | 115 |
| 3 | Apache | 107 |
| 4 | Skull Snaps | 105 |
| 5 | One Drop | 75 |
| 6 | Steppers | 140 |
| 7 | Rockers | 130 |
| 8 | Dancehall | 100 |
| 9 | Two-Step | 138 |

**Piano keyboard** — chromatic keys mapped to your repo's scale. A–L for natural notes, W/E/T/Y/U/O/P for accidentals.

**7 synth presets:**

| Preset | Voices | Character |
|--------|--------|-----------|
| Juno | 3 | Classic warm BBD chorus |
| Supersaw | 7 | Massive JP-8000 detune |
| Iceman | 5 | LTJ Bukem liquid pad |
| M1 | 3 | Clean digital PCM |
| Bulldozer | 5 | Heavy filtered saw |
| Raw | 1 | Pure unprocessed sawtooth |

**6 pad shapes** — Swell, Cascade, Bloom, Pulse, Drift, Stab — each hashed per-repo so the same project always sounds the same.

**Record mode** — press `r` to arm, then `z`/`x`/`c`/`v` to live-record kick/snare/hat/open during playback.

## How It Works

Repo and branch are hashed **separately** with SHA-256, contributing independently to the final sound:

```
                    SHA-256                          SHA-256
  ┌──────────┐    ─────────►  ┌─────────────────┐  ─────────►  ┌─────────────────┐
  │ repo name│                │ Harmonic identity│              │ Melodic identity │
  └──────────┘                │                  │  ┌────────┐  │                  │
                              │  Root note (C–B) │  │ branch │  │  Arpeggio pattern│
                              │  Scale type      │  │  name  │  │  Swing timing    │
                              │  Octave register │  └────────┘  │  Envelope shape  │
                              │  Timbre blend    │              │  Chorus detune   │
                              │  Pad shape       │              │  Interval spread │
                              └─────────────────┘              │  Stagger offsets │
                                                               └─────────────────┘
```

Same repo + branch = same hashes = same sound. Every time. When used as Claude Code hooks, a per-event seed rotates the pattern, pad shape, and hit type — so each event sounds distinct while preserving the repo/branch identity.

### Synthesis Engine

The audio engine is a from-scratch soft synth written in Rust:

- **Oscillators** — Sawtooth-dominant with sine blend, 3–7 detuned voices per note with per-voice phase spreading
- **Filter** — 24dB/oct Butterworth LPF (cascaded biquads) with envelope-driven cutoff sweep and resonance control
- **Reverb** — Schroeder reverb: 4 comb filters + 2 allpass filters for depth and space
- **Chorus** — BBD-style delay modulation with phase-inverted L/R LFO (90° offset) for stereo width
- **Dub delay** — Tape-style echo with wow/flutter, filtered feedback, and gradual decay
- **Drums** — Fully synthesized: kick (sine pitch sweep 150→50Hz + click), snare (sine body + noise burst), rimshot (click + resonant ring), hi-hats (noise + metallic sines at 6/8/10kHz)
- **Single hits** — Short percussive events use a single drum hit (repo-deterministic type) with fast ~125ms decay
- **Envelopes** — 4 shapes (Punchy, Soft, Pluck, Swell) with exponential decay and note overlap for ethereal ringing

## Integration

### Claude Code Hooks

```bash
branch-tone init
```

One command registers hooks for 10 Claude Code events in `~/.claude/settings.json`. Each event gets a distinct sound that varies by repo and branch — you can tell which event fired *and* which project you're in:

| Group | Events | Sound | Duration |
|-------|--------|-------|----------|
| **Session** | `SessionStart` | Pad + chorus + dub | 2s |
| | `SessionEnd` | Pad + tremolo + dub | 2s |
| **Rhythm** | `Stop` | Single drum hit | 300ms |
| | `UserPromptSubmit` | Single drum hit (different type) | 250ms |
| **Alerts** | `PermissionRequest` | Pad + tremolo + dub + reverse | 1.5s |
| | `Notification` | Pad + chorus + tremolo | 1.2s |
| **Ambient** | `SubagentStart`, `SubagentStop` | Randomized pad | 500ms |
| | `PreCompact` | Pad + chorus + echo | 1.1s |
| **Waiting** | `TeammateIdle` | Clean gentle ping | 600ms |

Every event uses a unique **event seed** that rotates the note pattern, pad envelope shape, and drum hit type — so `SessionStart` and `SessionEnd` play different melodies in the same key, `Stop` and `UserPromptSubmit` use different percussive hits, and alerts each have their own character. The actual content (which notes, which hit type) still varies by repo+branch.

Run `branch-tone test` in any repo to preview all hook sounds.

### Shell: Worktree Switcher

Add to `~/.zshrc` for audio feedback when switching worktrees:

```bash
function wt() {
  local selection=$(git worktree list 2>/dev/null | \
    awk '{path=$1; branch=$NF; gsub(/\[|\]/, "", branch); n=split(path, parts, "/"); dir=parts[n]; printf "%-20s  %-30s  %s\n", branch, dir, path}' | \
    fzf --height 40% --reverse)

  if [[ -n "$selection" ]]; then
    local dir=$(echo "$selection" | awk '{print $NF}')
    local branch=$(echo "$selection" | awk '{print $1}')
    command -v branch-tone &>/dev/null && (cd "$dir" && branch-tone "$branch") &>/dev/null &
    cd "$dir"
  fi
}
```

### Demo: Hear All Your Repos

```bash
./scripts/demo.sh                    # Scan current directory for git repos
./scripts/demo.sh /path/to/repos     # Scan a specific directory
./scripts/demo.sh --all              # Include worktrees
```

## Development

```bash
cargo build                   # Debug build
cargo run -- main             # Run with args
cargo run -- player           # Step sequencer
cargo build --release         # Optimized build
cargo test                    # 57 tests
```

## Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust 2024 edition |
| Audio | [CPAL](https://github.com/RustAudio/cpal) (Cross-Platform Audio Library) |
| Hashing | SHA-256 (two-layer: repo + branch) |
| CLI | [Clap](https://github.com/clap-rs/clap) with derive macros |
| Terminal | [Crossterm](https://github.com/crossterm-rs/crossterm) for the step sequencer |
| Synthesis | Saw/sine oscillators, Butterworth LPF, Schroeder reverb, BBD chorus, tape delay |
| Scales | 10 types across 12 chromatic roots |

## Sound Design

The synthesis is modeled after the pad sounds of 90s jungle and liquid drum & bass — hardware like the **Roland Juno-106** (BBD chorus), **Roland JP-8000** (Supersaw), **Roland JD-800** (the "Iceman" patch from LTJ Bukem's productions), and **Korg M1** (PCM pads). The synth presets in the step sequencer are named after these references.

See [`docs/sound-design.md`](docs/sound-design.md) for the full synthesis research, frequency tables, hardware references, and Camelot key mappings.

## License

MIT
