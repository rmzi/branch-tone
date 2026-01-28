# branch-tone 🎵

[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

Generate unique musical tones from git branch names. Each branch gets its own sonic identity.

## Why?

Ever lose track of which terminal is on which branch? Now you can *hear* it. Same branch always produces the same tone, so your ears learn to recognize your context.

Uses a **pentatonic scale** (C, D, E, G, A) so every branch sounds pleasant — no dissonance.

## Install

```bash
# From source
cargo install --path .

# Or build locally
cargo build --release
cp target/release/branch-tone /usr/local/bin/
```

## Usage

```bash
# Play tone for a branch
branch-tone feature/auth

# Current branch (auto-detect)
branch-tone

# Adjust duration (ms) and volume (0.0-1.0)
branch-tone main --duration 500 --volume 0.5

# Just show the frequency, don't play
branch-tone feature/auth --dry-run
```

## How It Works

1. **Hash** the branch name with SHA-256
2. **Map** hash bytes to musical parameters:
   - Note from pentatonic scale (C, D, E, G, A)
   - Octave (3, 4, or 5)
   - Attack/decay envelope shape
3. **Synthesize** a sine wave with those parameters
4. **Play** via your system's audio output

Same branch name → same hash → same tone. Every time.

## Integration

### Play on branch switch (shell)

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
    branch-tone "$branch" &  # Play in background
    cd "$dir"
  fi
}
```

### Claude Code hook

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "onAssistantResponse": [
      {
        "command": "branch-tone $(git branch --show-current 2>/dev/null || echo 'default') &",
        "description": "Play branch tone on response"
      }
    ]
  }
}
```

## Technical Details

- **Audio**: CPAL (Cross-Platform Audio Library)
- **Hashing**: SHA-256 for deterministic randomness
- **CLI**: clap with derive macros
- **Synthesis**: Pure sine wave with ADSR envelope

## Development

```bash
# Build
cargo build

# Run
cargo run -- feature/auth

# Release build (optimized)
cargo build --release

# Run tests
cargo test
```

## License

MIT
