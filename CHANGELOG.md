# Changelog

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
