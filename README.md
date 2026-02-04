# branch-tone

Generate unique musical tones from git branch names. Each branch gets its own sonic identity — different repos with the same branch name produce different sounds.

## Why?

Ever lose track of which terminal is on which branch? Now you can *hear* it. Same branch + repo always produces the same musical phrase, so your ears learn to recognize your context.

Each branch gets a unique **3-note arpeggio** using the pentatonic scale (C, D, E, G, A) — always pleasant, never harsh.

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
branch-tone --chorus           # Detuned layers for richness
branch-tone --tremolo          # Volume wobble
branch-tone --steps 5          # 5-note phrase instead of 3

# Adjust duration (ms) and volume (0.0-1.0)
branch-tone main -d 500 -v 0.5

# Just show parameters, don't play
branch-tone feature/auth --dry-run
```

## How It Works

1. **Hash** the `repo:branch` combination with SHA-256
2. **Map** hash bytes to musical parameters:
   - Root note from pentatonic scale (C, D, E, G, A)
   - Octave (low, mid, or high)
   - Arpeggio pattern (rising, falling, stepwise, or playful)
3. **Synthesize** a 3 or 5-note phrase with soft attack/decay
4. **Play** via your system's audio output

Same repo + branch → same hash → same melody. Every time.

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
- **Hashing**: SHA-256 for deterministic randomness
- **CLI**: clap with derive macros
- **Synthesis**: Sine wave with harmonics, ADSR envelope, optional chorus/tremolo

## Development

```bash
cargo build           # Debug build
cargo run -- main     # Run with args
cargo build --release # Optimized build
cargo test            # Run tests
```

## License

MIT
