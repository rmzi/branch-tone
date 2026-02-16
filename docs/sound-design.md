# Sound Design Reference

Synthesis research and parameter notes for `branch-tone`'s audio engine.

## Frequency Ranges for Ambient Pads

| Range | Hz | Character |
|-------|----|-----------|
| Sub-bass | 30–80 | Felt more than heard, rumble/weight |
| Bass | 80–200 | Warmth, body, foundation |
| Low-mid | 200–500 | Fullness, can get muddy fast |
| Mid | 500–2000 | Presence, definition |
| Upper-mid | 2000–5000 | Clarity, edge, harshness zone |

For ambient pads, the sweet spot is **80–500 Hz** for fundamentals. Sub-bass layering (half the fundamental) adds weight without clutter. The pad generator drops notes one octave (`freq * 0.5`) to push everything into warmer territory, then adds a sub layer at `freq * 0.25`.

## Harmonic Ratios

Harmonics above the fundamental define warmth vs eeriness:

| Harmonic | Ratio | Level | Effect |
|----------|-------|-------|--------|
| 2nd (octave) | 2:1 | 0.15–0.25 | Warmth, fullness |
| 3rd (fifth) | 3:1 | 0.03–0.08 | Slight character, hollow at higher levels |
| 4th (2 octaves) | 4:1 | avoid | Too bright for pads |
| 5th (major 3rd) | 5:1 | avoid | Adds tension |

The pad uses a steep harmonic rolloff: fundamental dominant, 2nd at 0.2, 3rd at 0.06. This keeps things warm without brightness creeping in. The arpeggio mode uses repo-hashed values (`harmonic_blend` 0.05–0.35, `third_harmonic` 0.0–0.15) for per-repo timbre variation.

## Envelope Shaping

### Pad Envelope
Slow sine-curved attack and release (45% each) with a short sustain plateau. The sine curve (`sin(progress * pi/2)`) gives a natural-feeling fade rather than the mechanical feel of linear ramps.

### Arpeggio Envelope
The arpeggio uses a "ringing notes" approach — each note triggers at its boundary and decays exponentially (`exp(-t)`) over the remaining phrase. Notes overlap and blend rather than cutting off, creating an ethereal, reverb-like quality without actual reverb processing. A global 15% fade-out at the end prevents hard cutoffs.

### Available Shapes (arpeggio mode)

| Shape | Attack | Decay | Character |
|-------|--------|-------|-----------|
| Punchy | 5% | 15% | Fast hit, quick ring |
| Soft | 25% | 30% | Gentle swell |
| Pluck | 2% | 20% | Instant attack, medium ring |
| Swell | 40% | 10% | Very slow bloom |

## Detuning and Chorus

Detuning multiple oscillators creates width and movement. Parameters:

| Context | Detune | Voices | Effect |
|---------|--------|--------|--------|
| Pad (tight) | 0.6–2.4 cents | 3 per note | Lush, subtle shimmer |
| Chorus (explicit) | 4–16 cents | 5 per note | Wide, obvious movement |
| Default arpeggio | 1.2–4.8 cents | 2 per note | Slight spaciousness |

Cents-to-frequency: `f * 2^(cents/1200)`. At 440 Hz, 10 cents is only ~2.5 Hz of difference, but the phase interference creates audible width.

Phase spreading (`(voice_idx + i) * offset`) ensures detuned voices don't start aligned, which would cause initial volume spikes before the beating pattern develops.

## Layered Drone Techniques

The pad generator layers three elements per note:

1. **Detuned triad** — 3 voices at [-detune, center, +detune] cents with fundamental + 2 harmonics each. This is the main body.
2. **Sub layer** — Pure sine one octave below the (already-dropped) base frequency. Adds weight without harmonic complexity.
3. **Breath modulation** — Very slow amplitude wobble (0.03–0.05 Hz) at 8% depth. Not tremolo — more like the pad is "breathing." Keeps static drones from feeling dead.

## Parameter Comparison

| Parameter | Warm Pad | Halloween/Eerie |
|-----------|----------|-----------------|
| Fundamental range | 80–300 Hz | 200–600 Hz |
| 2nd harmonic | 0.15–0.25 | 0.3–0.5 |
| 3rd harmonic | 0.03–0.08 | 0.15–0.3 |
| Detune | 1–3 cents | 8–20 cents |
| Envelope attack | 30–45% | 5–15% |
| Sub layer | Yes, prominent | Minimal or absent |
| Movement | Slow breath (0.03 Hz) | Tremolo (3–9 Hz) |
| Octave | Drop 1 (-12 semitones) | Stay or raise |

`branch-tone` aims for the warm column. The "eerie" column is useful reference for what to avoid — or for a future Halloween mode.

## Blue Mar Ten / Jungle Pad Aesthetic

The jungle pad sound (circa mid-90s liquid drum & bass) has specific qualities:

- **Dark and pillowy** — fundamentals sit low, harmonics are heavily filtered
- **Wide stereo** — achieved through tight detuning rather than hard panning
- **Slow evolution** — parameters drift over time rather than staying static
- **No sharp attacks** — everything materializes gradually
- **Warm, not bright** — the 2nd harmonic adds fullness but the 3rd and above are nearly absent
- **Sub weight** — a pure sub-bass layer grounds everything

The `branch-tone` pad aims for this aesthetic in a 1.5-second window: slow fade in, brief sustain, slow fade out. The tight detuning (0.6–2.4 cents) and steep harmonic rolloff are key to the warm-not-harsh character. Wider detuning or stronger upper harmonics would push toward trance or ambient techno territory.

## Shimmer (Arpeggio Mode)

A slow pitch wobble on each note (2.5 Hz + 0.3 Hz per voice index, at 0.3% depth) adds an ethereal, slightly unstable quality to arpeggiated notes. This is subtle enough to not sound like vibrato but adds life to sustained ringing tones. Each voice wobbles at a slightly different rate, preventing phase-lock between overlapping notes.
