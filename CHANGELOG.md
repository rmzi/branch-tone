# Changelog

## [0.13.2] - 2026-04-02

### Fixed
- Gate `LAUNCHD_LABEL` constant behind `#[cfg(feature = "tray")]` to eliminate dead_code warning on non-tray builds

## [0.12.0] - 2026-04-01

### Added
- **Terminal dashboard** (`--features tui`): Live ratatui-powered system monitor for the daemon. Connects as a read-only client over the existing Unix socket — no audio code, no thread sync, open/close freely without affecting playback.
- **Unified Stream panel**: Events and voices merged into a single feed. Each repo gets a stable lane color from an 8-color palette, shown as block-density prefixes (`█▓▒░`) that pulse on activity and decay over ~20 seconds.
- **Stacked activity histogram**: 60-minute bar chart with per-category colored segments (tool=green, prompt=yellow, session=cyan, agent=blue). Direct buffer writes for cell-level color control — ratatui's Sparkline widget only supports single-color bars.
- **Seed palette overlay**: Centered popup ([p] to open, Esc to close) showing all 16 curated seeds with groove params. Renders via `ratatui::widgets::Clear` + `List` with `ListState`.
- **Event view reset**: Press [r] to clear displayed events and start counting fresh.
- **Recency gradient**: Newest events flash white+bold for 2 seconds, then fade through full type color → DarkGray based on positional recency (`line_index / visible_count`).
- **Event type parsing**: 16 hook event types mapped to 5 categories with per-type colors and compact labels (TOOL, PROMPT, START, FAIL, AGENT+, etc.).
- **Voice activity tracking**: Per-repo Instant timestamps for real-time lane pulse effects.
- **`docs/tui-design.md`**: Full design document with architecture, rendering techniques, and development diary.
- 14 new TUI-specific tests (155 total with tui feature, 141 default, 142 tray)

### Development Diary
The TUI was built in a single session across three iterations:
1. **v1 — Basic dashboard**: Split layout (voices left, seeds right, events bottom). Socket client ported from tray module. 7 tests.
2. **v2 — Color and activity**: Event log parsing with per-type colors. 60-minute sparkline. Voice pulse indicators with block-density decay. Border flash on new events.
3. **v3 — Unified stream**: Merged voices+events with colored lane prefixes. Stacked per-category activity bars via direct buffer writes. Seeds moved to popup overlay. Reset view. Inspired by user feedback: "Make this thing light up."

## [0.9.1] - 2026-03-14

### Added
- **macOS menu bar tray** (`--features tray`): Monitor and control the daemon from the system tray — status, start/stop, test sounds, open player, recent events log
- **`__status_json` daemon command**: Structured JSON status endpoint (PID, uptime, active voices, idle state) for programmatic consumers
- **`start_time_secs` on DaemonState**: Tracks daemon boot time for uptime reporting
- **Category-aware articulation**: Stabs, comping, bass lines, and pads per event category
- **Seed-derived ADSR envelopes**: Per-event envelope variety from deterministic seeds
- **Category-aware synthesis**: Distinct timbres per instrument (drums, bass, keys, horns)
- **Event density tracking**: Adjusts behavior based on recent event frequency
- 5 new tests (106 total with tray feature, 105 without)

### Changed
- Sound rebalanced: volumes and envelope shapes rotate per event for more musical variety
- Per-category delay timing for better rhythmic separation

## [0.9.0] - 2026-03-10

### Added
- **Jazz ensemble model**: 18 Claude Code hook events mapped as a jazz band — drums, hi-hat, bass, keys/pad, horn, piano/comping
- **8 new hook events**: PreToolUse, PostToolUse, PostToolUseFailure, InstructionsLoaded, ConfigChange, TaskCompleted, WorktreeCreate, WorktreeRemove
- **3 new event categories**: `ToolPulse` (hi-hat micro-clicks for tool events), `Bass` (low-register agent lifecycle), `Lifecycle` (piano/comping for structural events) — replaces `Ambient`
- **`DrumHitType::OpenHat`**: New drum synthesis variant for open hi-hat sounds
- **Jazz micro-patterns**: Ghost notes, flams, and drags on percussive hits (1–4 hits per event, 15–60ms spacing) — deterministic per repo+event seed
- **Worktree-as-voice**: Subagents in worktrees automatically get unique melodic signatures (same repo key, different branch melody)
- **PDS lifecycle mapping**: Sound design informed by Portable Dev System's 6-phase agent workflow (advisory, not coupled)
- **`docs/jazz-ensemble.md`**: Design documentation for the ensemble model, PDS mapping, and micro-pattern system
- 2 new tests (75 total) for micro-pattern determinism and event seed rotation

### Changed
- **Extended durations**: SessionStart/End 2s→3.5s, SubagentStart/Stop 500ms→1s, PermissionRequest 1.5s→2.5s, Notification 1.2s→2s, PreCompact 1.1s→2s, TeammateIdle 600ms→1.5s
- **Category reassignment**: SubagentStart/Stop moved from Ambient to Bass; PreCompact/TeammateIdle moved from Ambient to Lifecycle
- **EventCategory parameters**: Bass uses octave 0.5x / transpose -5; ToolPulse uses octave 2.0x; Lifecycle uses octave 1.25x / transpose +3
- **Drum hit cycle**: 4-type rotation expanded to 5-type (Kick→Snare→Rimshot→ClosedHat→OpenHat)
- Plugin manifest registers all 18 hook events

## [0.8.0] - 2026-03-02

### Added
- **Event categories**: `EventCategory` enum (SessionBoundary, Attention, DrumHit, Ambient) with per-category octave offset, transposition, and step count
- **Single drum hits**: Short percussive events (Stop, UserPromptSubmit) play a single kick/snare/rimshot/hat (~125–300ms) instead of full melodies
- **Rimshot synthesis**: New `synth_rimshot` — click transient + resonant ring + filtered noise
- **Event seeds**: Each Claude Code hook event gets a unique seed (1–10) that rotates note pattern, pad shape, and drum hit type — you can tell which event fired by ear
- **Per-event sound identity**: SessionStart/End play different pad shapes, Stop/UserPromptSubmit use different hit types, all while varying by repo+branch
- **Dub delay effect**: Tape-style echo with wow/flutter, filtered feedback, gradual decay (`--dub`)
- **Spooky mode**: Thin sines, dark filter, eerie resonance (`--spooky`)
- **Interactive piano keyboard**: Chromatic keys (A–L naturals, W/E/T/Y/U/O/P accidentals) in the step sequencer
- **7 synth presets**: Juno, Supersaw, Iceman, M1, Bulldozer, Raw — each with multi-voice architecture inspired by jungle/liquid DnB hardware
- **Piano controls**: Octave shift (`[`/`]`), synth preset (`,`/`.`), pad shape (`;`/`'`), sustain (`Tab`)
- **TeammateIdle hook**: Clean gentle ping for idle teammate notifications
- 18 new tests (57 total) covering event categories, single hits, rimshot synthesis, drum hit determinism, event seed rotation

### Changed
- **Hook event remapping**: Sessions → pads with chorus/tremolo+dub; Stop/UserPromptSubmit → single drum hits; Alerts → pad+tremolo/chorus; hooks now cover 10 events
- **Synth preset dominance**: Preset timbral weight increased from 70% to 85% over hash-derived values for more dramatic per-preset character
- **SessionEnd** uses tremolo (not chorus) to differentiate from SessionStart
- `scripts/demo.sh` defaults to current directory instead of hardcoded paths
- Cleaned up PDS-specific files from tracked git files for public release

### Fixed
- Event seed collisions resolved (each of 10 events has a unique seed)

## [0.7.0] - 2026-02-16

### Added
- **Low-pass filter**: 24dB/oct Butterworth biquad LPF on pad output for warm, subtractive synthesis
- **Reverb**: Schroeder reverb (4 comb + 2 allpass filters) adds depth and space
- **Stereo chorus**: BBD-style chorus with phase-inverted LFO for L/R width
- **Bulldozer mode**: Layered pad (70%) + arp (30%) played simultaneously (`--bulldozer`)
- **Filter envelope**: LPF cutoff sweeps with pad shape for evolving character
- **6 pad shapes**: Swell, Cascade, Bloom, Pulse, Drift, Stab — hashed per-repo for true uniqueness
- **Staggered note entries**: Notes fade in at branch-derived offsets instead of all at once
- **Per-note envelopes**: Each note in a pad chord has its own amplitude shape
- `scripts/demo.sh`: Discover and play tones from repos in a directory (`--all` for worktrees)
- 5 hook format validation tests (init idempotency, old format migration, hook.sh cleanup, unrelated hook preservation)

### Changed
- Hook default: bulldozer mode, 5 steps, 3s duration, 0.35 volume
- `init` registers hooks for SessionStart, Stop, and PermissionRequest
- Pad oscillators now use saw waves (subtractive) instead of pure additive synthesis
- Repo hash determines pad shape (voice identity); branch hash determines timing/stagger (melody identity)
- Octave drop reduced (0.65x) and filter range raised (400–2200 Hz) for better audibility
- `docs/sound-design.md` expanded with hardware references, Camelot keys, and implementation notes

### Fixed
- `init` migrates old flat hook format `{type, command}` to new matcher-group format `{hooks: [{type, command}]}`
- `init` cleans up stale `hook.sh` references in settings.json

## [0.6.0] - 2026-02-16

### Added
- `hook` subcommand: reads Claude Code hook JSON from stdin, detects branch/repo, plays tone
- `--quiet` flag on play to suppress informational output

### Changed
- `init` now writes `branch-tone hook` directly to settings.json (no bash wrapper)
- `init` cleans up old `hook.sh` entries from settings.json and deletes `~/.config/branch-tone/hook.sh`

### Removed
- `HOOK_SCRIPT` bash/jq wrapper — no longer depends on bash or jq at runtime
- `~/.config/branch-tone/` directory creation and hook.sh file management

## [0.5.0] - 2026-02-16

### Added
- Two-layer hashing: repo and branch are hashed separately for independent contribution
- 12 chromatic root notes (C through B) instead of 5 pentatonic
- 6 scale types: major pentatonic, minor pentatonic, dorian, lydian, mixolydian, minor
- Variable timbre via 2nd and 3rd harmonic blend (derived from repo hash)
- 8 arpeggio patterns per step count (up from 4)
- Swing timing (0–30%) for rhythmic variation
- 4 envelope shapes: punchy, soft, pluck, swell
- Variable chorus detune (4–16 cents) and tremolo rate/depth per branch
- Interval spread multiplier for compact-to-wide melodic leaps
- Ethereal sound: notes ring out with exponential decay and overlap
- Sub-octave layer and pitch shimmer always present for depth
- Light stereo detune even without --chorus flag
- Test suite with 12 tests covering determinism, two-layer hashing, oscillator range, and envelopes

### Changed
- Default duration increased to 600ms (1000ms for pad mode)
- Expanded octave register options from 3 to 5
- Repo determines harmonic identity (key, scale, timbre, octave)
- Branch determines melodic identity (pattern, rhythm, envelope, modulation)

### Fixed
- `.claude/hooks.json` updated to current Claude Code format (matcher as string)
- Added `.worktrees/` to `.gitignore`

## [0.4.0] - 2025-02-04

### Added
- Repo-aware sound generation: different repos with same branch produce different tones
- `--repo` flag to explicitly specify repository name
- `--pad` mode for warm chord instead of arpeggio
- `--chorus` effect with detuned layers
- `--tremolo` effect with volume wobble
- `--steps 5` option for 5-note phrases
- Auto-detection of repo name from git remote or directory

### Changed
- Hash now uses `repo:branch` combination instead of just branch name
- Default duration extended to 800ms in pad mode
- README updated with correct Claude Code hook format

## [0.2.0] - 2025-02-04

### Added
- Musical arpeggios instead of single tones
- 3-note phrases using pentatonic scale
- Multiple arpeggio patterns (rising, falling, stepwise, playful)
- Octave variation based on hash

## [0.1.0] - 2025-02-04

### Added
- Initial release
- Single tone generation from branch name
- SHA-256 hashing for deterministic sounds
- Basic CLI with duration and volume controls
- Auto-detection of current git branch
