# Changelog

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
