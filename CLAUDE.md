# branch-tone

## Architecture

Single-file Rust application (`src/main.rs`, ~8000 lines). Everything — CLI, hashing, synthesis, audio output, step sequencer, menu bar tray, terminal dashboard — lives in one file.

## Build & Test

```bash
cargo test                              # 147 tests, must pass with zero warnings
cargo build --release                   # Optimized binary
cargo install --path .                  # Install to ~/.cargo/bin/
cargo test --features tray              # Tray tests also pass (148 total)
cargo build --release --features tray   # Build with macOS menu bar support
cargo install --path . --features tray  # Install with tray support
cargo test --features tui               # TUI tests (169 total)
cargo build --release --features tui    # Build with terminal dashboard
cargo install --path . --features tui   # Install with TUI support
```

## Key Concepts

- **Two-layer hashing**: repo name → harmonic identity (key, scale, timbre, pad shape); branch name → melodic identity (pattern, rhythm, envelope)
- **Event seeds**: each Claude Code hook event gets a unique seed that rotates pattern, pad shape, and drum hit type — same repo sounds different per event
- **Deterministic**: same inputs always produce the same sound
- **Tray app** (macOS, `--features tray`): menu bar icon for daemon monitoring/control via `objc2-app-kit`. Separate process communicating over the daemon's Unix socket. Feature-gated to keep default build lean
- **TUI dashboard** (`--features tui`): ratatui-powered terminal dashboard showing live daemon state (voices, seeds, events, controls). Connects as a client to the daemon's Unix socket. Cross-platform, feature-gated

## Runtime Files

All daemon state lives in `~/.branch-tone/`:

| File | Purpose |
|------|---------|
| `pid` | Daemon process ID |
| `socket` | Unix socket for hook → daemon communication |
| `seed` | Active sound seed name (e.g. "shadow") |
| `mute` | Presence = global mute (all audio silenced) |
| `no-drone` | Presence = drone muted (event sounds still play) |
| `quantize` | Grid override: "4", "8", "16", or "32" (absent = auto) |
| `log` | Recent hook event log |

Seed presets carry groove parameters (swing, humanize, quantize subdivision) that shape timing feel independently of harmonic identity. Custom seed strings use hash-derived groove defaults.

## After Code Changes

Run `/install` to build and install the latest binary. The Claude Code hooks call the installed binary, not the dev build — stale installs mean the user hears old sounds.
