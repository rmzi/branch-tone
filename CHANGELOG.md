# Changelog

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
