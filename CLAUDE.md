# branch-tone

## Architecture

Single-file Rust application (`src/main.rs`, ~7000 lines). Everything — CLI, hashing, synthesis, audio output, step sequencer, menu bar tray — lives in one file.

## Build & Test

```bash
cargo test                              # 105 tests, must pass with zero warnings
cargo build --release                   # Optimized binary
cargo install --path .                  # Install to ~/.cargo/bin/
cargo test --features tray              # Tray tests also pass (106 total)
cargo build --release --features tray   # Build with macOS menu bar support
cargo install --path . --features tray  # Install with tray support
```

## Key Concepts

- **Two-layer hashing**: repo name → harmonic identity (key, scale, timbre, pad shape); branch name → melodic identity (pattern, rhythm, envelope)
- **Event seeds**: each Claude Code hook event gets a unique seed that rotates pattern, pad shape, and drum hit type — same repo sounds different per event
- **Deterministic**: same inputs always produce the same sound
- **Tray app** (macOS, `--features tray`): menu bar icon for daemon monitoring/control via `objc2-app-kit`. Separate process communicating over the daemon's Unix socket. Feature-gated to keep default build lean

## After Code Changes

Run `/install` to build and install the latest binary. The Claude Code hooks call the installed binary, not the dev build — stale installs mean the user hears old sounds.
