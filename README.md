# branch-tone

Generate unique musical tones from git branch names. Each repo gets its own harmonic identity (key, scale, timbre) while each branch gets its own melodic identity (pattern, rhythm, envelope) — so you can *hear* your context.

## Why?

Ever lose track of which terminal is on which branch? Now you can hear it. Same branch + repo always produces the same musical phrase, so your ears learn to recognize your context.

Each repo sounds fundamentally different — one might be in C# Minor Pentatonic while another is in A# Lydian. Branches within a repo share that harmonic character but vary in melody, rhythm, and envelope shape. The result is ethereal, spaceship-like tones that ring out and overlap.

## Install

```bash
# One-liner (requires Rust)
cargo install --git https://github.com/rmzi/branch-tone

# Or clone and build
git clone https://github.com/rmzi/branch-tone
cd branch-tone
cargo install --path .
```

## Usage

```bash
# Play tone for current branch (auto-detects repo)
branch-tone

# Play tone for specific branch
branch-tone feature/auth

# Specify repo explicitly
branch-tone main --repo my-project

# Sound options
branch-tone --pad              # Warm chord instead of arpeggio
branch-tone --chorus           # Extra detuned layers for richness
branch-tone --tremolo          # Volume wobble
branch-tone --steps 5          # 5-note phrase instead of 3

# Adjust duration (ms) and volume (0.0-1.0)
branch-tone main -d 800 -v 0.5

# Just show parameters, don't play
branch-tone feature/auth --dry-run
```

## How It Works

Repo and branch are hashed **separately** with SHA-256 so they contribute independently:

**Repo hash → harmonic identity:**
- Root note from all 12 chromatic pitches (C through B)
- Scale type (major pentatonic, minor pentatonic, dorian, lydian, mixolydian, minor)
- Octave register and harmonic timbre blend

**Branch hash → melodic identity:**
- Arpeggio pattern (8 per step count)
- Swing timing, envelope shape, chorus detune
- Tremolo rate/depth, interval spread

Same repo + branch → same hashes → same sound. Every time.

Notes ring out with exponential decay and overlap for an ethereal quality, with a sub-octave layer and subtle pitch shimmer always present.

## Integration

### Shell: Play on worktree switch

Add to `~/.zshrc`:

```bash
# Play tone when changing worktrees
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

### Claude Code: Play when agent finishes

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "branch-tone \"$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo claude)\" --repo \"$(basename \"$(git rev-parse --show-toplevel 2>/dev/null)\" 2>/dev/null || echo unknown)\" --pad --chorus --steps 5 -d 800 -v 0.2"
          }
        ]
      }
    ]
  }
}
```

This plays a warm 5-note chord when Claude finishes responding — each repo/branch combination has its own sound, so you can tell which agent is ready just by listening.

## Technical Details

- **Audio**: CPAL (Cross-Platform Audio Library)
- **Hashing**: SHA-256, two-layer (repo + branch hashed independently)
- **CLI**: clap with derive macros
- **Synthesis**: Sine waves with configurable harmonics, sub-octave layer, shimmer, exponential decay envelopes, optional chorus/tremolo
- **Scales**: 6 pentatonic-safe scale types across all 12 chromatic roots

## Development

```bash
cargo build           # Debug build
cargo run -- main     # Run with args
cargo build --release # Optimized build
cargo test            # Run tests (12 tests)
```

## License

MIT
