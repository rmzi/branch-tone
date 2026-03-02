# branch-tone

## Architecture

Single-file Rust application (`src/main.rs`, ~4000 lines). Everything — CLI, hashing, synthesis, audio output, step sequencer — lives in one file.

## Build & Test

```bash
cargo test              # 57 tests, must pass with zero warnings
cargo build --release   # Optimized binary
cargo install --path .  # Install to ~/.cargo/bin/
```

## Key Concepts

- **Two-layer hashing**: repo name → harmonic identity (key, scale, timbre, pad shape); branch name → melodic identity (pattern, rhythm, envelope)
- **Event seeds**: each Claude Code hook event gets a unique seed that rotates pattern, pad shape, and drum hit type — same repo sounds different per event
- **Deterministic**: same inputs always produce the same sound

## After Code Changes

Run `/install` to build and install the latest binary. The Claude Code hooks call the installed binary, not the dev build — stale installs mean the user hears old sounds.
