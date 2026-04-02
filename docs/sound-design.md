# Sound Design Reference

Synthesis research and parameter notes for `branch-tone`'s audio engine.
Based on Blu Mar Ten's JungleJungle (1989–1999) sample pack and broader
jungle/liquid DnB production techniques.

> **See also**: [Jazz Ensemble](jazz-ensemble.md) — how 18 Claude Code hook
> events map to a jazz ensemble (drums, bass, keys, horn, piano), including
> the PDS lifecycle mapping and worktree-as-voice concept.
> [TUI Design](tui-design.md) — terminal dashboard architecture, rendering
> techniques, and development diary.

## Source Material: Blu Mar Ten JungleJungle Pack

650+ samples from vinyl/CD spanning 1989–1999. The pad samples that define
our target aesthetic (with Camelot keys mapped to standard):

| Pad | Camelot | Key | Notes |
|-----|---------|-----|-------|
| Bulldozer | — | — | Pad + arpeggiated layer, the target sound |
| Reflections | 3B | Db major | Love — lush, evolving |
| Ten Pad | 9B | G major | Amazing — full, warm |
| Traenon | 12A | Db minor | Perfect ambience |
| Sonic Pad 1 & 2 | 5A | C minor | |
| Connected Modu | 6A | G minor | |
| Groove Therapy Lo | 2A | Eb minor | |
| Inner Pad | 11A | F# minor | |
| Love Pad | 10A | B minor | |
| Moomin Loop | 5A | C minor | |
| Night Train Short | 8A | A minor | |
| Portraits | 11A | F# minor | |
| Revelation | 1B | B major | |
| Drumz BassPad | 1B | B major | |
| Shoes Pad | 5B | Eb major | |
| Apollo | 11A | F# minor | |

The heavy lean toward minor keys (Eb, C, G, F#, A, B, Db minor) is
characteristic of jungle/liquid DnB — melancholic, atmospheric tonalities.

## Classic Hardware Behind the Sound

The pads in this era came from a specific set of instruments:

- **Roland JD-800/JD-990** — LTJ Bukem's known synth. The "Iceman" patch
  is one of the most iconic jungle pad sounds (PFM "Dreams", Bukem & Tayla
  "Remnants").
- **Roland JV-1080** — The workhorse ROMpler. Factory string/pad presets
  were sampled and layered extensively.
- **Korg M1** — PCM-based pads that defined late-80s/early-90s dance music.
- **Korg Wavestation** — Vector synthesis and wave sequencing for complex,
  evolving textures. Particularly popular with "intelligent" DnB.
- **Roland Juno-106/Juno-60** — Analog saw/square through the BBD chorus
  circuit. The "Juno pad" is synonymous with warm, wide analog pads.
- **Roland JP-8000** — Introduced the Supersaw (1997): 7 detuned saw
  oscillators in a single waveform.
- **E-mu E4 / Akai S3000 samplers** — Many producers sampled pad chords
  from film soundtracks and other records, then replayed single-key stabs.

A critical insight: **hardware reverb was more important than the synth
itself** for achieving authentic jungle pad sounds. The reverb tail and
how it smeared harmonics was a defining characteristic.

## Frequency Ranges for Ambient Pads

| Range | Hz | Character |
|-------|----|-----------|
| Sub-bass | 30–80 | Felt more than heard, rumble/weight |
| Bass | 80–200 | Warmth, body, foundation |
| Low-mid | 200–500 | Fullness, can get muddy fast |
| Mid | 500–2000 | Presence, definition |
| Upper-mid | 2000–5000 | Clarity, edge, harshness zone |

Sweet spot for jungle pads: **80–500 Hz** fundamentals. The pad generator
drops notes one octave (`freq * 0.5`) then adds a sub layer at `freq * 0.25`.

For notification context (low volume playback), focus energy in **500 Hz–3 kHz**
where human hearing is most sensitive (Fletcher-Munson). Sub-bass is inaudible
at low volumes and wastes headroom.

## Oscillator Architecture

### Sawtooth Foundation
Detuned sawtooth waves are the primary waveform — rich in harmonics, responds
well to filtering. The classic approach:

| Style | Oscillators | Detune | Character |
|-------|-------------|--------|-----------|
| Juno-style | 2 saws | 5–15 cents | Classic warm width |
| Supersaw (JP-8000) | 7 saws | 0.89x–1.11x center | Massive, genre-defining |
| branch-tone pad | 3 saws | 0.6–2.4 cents | Subtle, tight lushness |
| branch-tone arp | 2–5 saws | 1.2–16 cents | Varies by chorus flag |

### JP-8000 Supersaw Reference (Adam Szabo's reverse-engineering)

At maximum detune, the 7 oscillators sit at:
```
0.8908x  0.9382x  0.9811x  1.0000x  1.0204x  1.0633x  1.1077x
```
The sweet spot for pads is 20–40% of max detune. At moderate settings the
spread is much smaller, which is where the warm pad territory lives.

For a Rust implementation, 3–5 detuned saws with the right spread produces
a convincing pad. The center oscillator stays louder (~0.7 gain) while
side oscillators range 0.0–1.0 depending on mix.

### Harmonic Ratios

| Harmonic | Ratio | Level | Effect |
|----------|-------|-------|--------|
| 2nd (octave) | 2:1 | 0.15–0.25 | Warmth, fullness |
| 3rd (fifth) | 3:1 | 0.03–0.08 | Slight character |
| 4th+ | 4:1+ | avoid | Too bright for pads |

The pad uses steep rolloff: fundamental dominant, 2nd at 0.2, 3rd at 0.06.
The arpeggio uses repo-hashed values (`harmonic_blend` 0.05–0.35,
`third_harmonic` 0.0–0.15) for per-repo timbre variation.

## Filter Design

### Low-Pass Filter Settings

| Parameter | Dark Pad | Warm Pad | Bright Pad |
|-----------|----------|----------|------------|
| Cutoff | ~60 Hz | 800–2000 Hz | 2000+ Hz |
| Slope | 24 dB/oct | 24 dB/oct | 12 dB/oct |
| Resonance | Low (0–10%) | Low-Med (10–20%) | Low (5–10%) |

For the jungle pad target: 24 dB/oct LPF at 800–1800 Hz, resonance under
20%. High resonance thins the sound — above 30% introduces nasal/acidic
coloring.

### Filter Envelope Modulation
- Slow attack on filter envelope (200–500 ms) creates gentle "opening"
- Moderate envelope amount (30–50% of range) with slow attack = classic
  breathing pad
- LFO to filter cutoff at 0.1–0.5 Hz, sweeping 200–500 Hz of range

**Note**: `branch-tone` currently uses additive harmonics rather than
subtractive filtering. A proper LPF would be a significant upgrade
toward the authentic sound.

## Envelope Shaping

### Pad Envelope (current)
Slow sine-curved attack and release (45% each) with short sustain plateau.
The sine curve (`sin(progress * pi/2)`) gives a natural-feeling fade.

### Classic ADSR for Jungle Pads
- Attack: 200–400 ms (gentle fade in)
- Decay: 200 ms
- Sustain: 0.7 (keep level high for low-volume audibility)
- Release: 400–600 ms

### Arpeggio Envelope (current)
"Ringing notes" approach — each note triggers at its boundary and decays
exponentially (`exp(-t)`) over remaining phrase. Notes overlap and blend,
creating ethereal reverb-like quality without actual reverb.

### Available Shapes (arpeggio mode)

| Shape | Attack | Decay | Character |
|-------|--------|-------|-----------|
| Punchy | 5% | 15% | Fast hit, quick ring |
| Soft | 25% | 30% | Gentle swell |
| Pluck | 2% | 20% | Instant attack, medium ring |
| Swell | 40% | 10% | Very slow bloom |

## Chorus and Detuning

### Juno-106 BBD Chorus Reference
The definitive chorus sound. Uses bucket-brigade device delay:
- Mode I: LFO 0.5 Hz, triangle, 100% depth
- Mode II: LFO 0.8 Hz, triangle, 100% depth
- Mode I+II: LFO ~1 Hz, sine-like, 8% depth (subtle shimmer)
- Stereo: left/right modulation 180° out of phase
- Delay time: ~0.64–12.8 ms range

### Implementation for Rust
- Short delay line (~2–5 ms base)
- Modulate with triangle LFO at 0.5–1.0 Hz
- Create stereo by inverting LFO phase for L vs R channel
- Mix: 40–50% wet

### Current branch-tone Detuning

| Context | Detune | Voices | Effect |
|---------|--------|--------|--------|
| Pad (tight) | 0.6–2.4 cents | 3 per note | Lush, subtle shimmer |
| Chorus (explicit) | 4–16 cents | 5 per note | Wide, movement |
| Default arpeggio | 1.2–4.8 cents | 2 per note | Slight spaciousness |

Phase spreading (`(voice_idx + i) * offset`) prevents initial volume spikes.

## Stereo Widening Techniques

### Oscillator Detuning + Panning (simplest)
Pan detuned oscillators: sharp slightly left, flat slightly right, center
stays center. Natural stereo spread from phase differences.

### Chorus (most important for jungle pads)
Left/right delay lines modulated with inverted LFO phase. Slightly different
LFO rates for L/R (e.g., 0.5 Hz left, 0.6 Hz right) creates organic,
evolving stereo image.

### Haas Effect (micro-delay)
Delay one channel by 5–15 ms. Creates width perception but can cause phase
issues in mono — be careful with laptop speakers.

**For branch-tone**: detuning + panning is most practical. Keep center
oscillator strong for mono compatibility.

## Reverb (Not Yet Implemented)

Reverb is the single most important effect for authentic jungle pads.
The hardware reverb defined the era more than the synth itself.

### Target Settings
- Type: Large hall or plate
- Size: Medium-large (5–7/10)
- Pre-delay: 20–40 ms (preserves attack clarity)
- Damping: High-frequency rolloff at 4 kHz (keeps tail warm)
- Width: 70–100%
- Wet/dry: 30–40%

### Simplified Options for Rust
- Feedback delay network with diffusion
- A few all-pass filters + comb filters (Schroeder reverb)
- Even a simple filtered feedback delay adds significant depth

## The Bulldozer Approach: Pad + Arp Layer

The standout aesthetic from the sample set — two layers working together:

### Layer 1: The Pad (harmonic bed)
- Sustained chord, slow attack/release
- Warm, filtered, wide stereo
- Low-mid frequency range (200–2000 Hz)
- Lower volume — provides the foundation

### Layer 2: The Arp (rhythmic movement)
- Same chord notes played as rhythmic sequence
- Shorter envelope (fast attack, short decay, low sustain)
- Brighter filter (higher cutoff, ~3–5 kHz)
- High-pass filtered to sit above the pad
- Delay effect for rhythmic cascade
- ~60% of pad volume, or 40/60 pad/arp mix

### For branch-tone (1.5s notification)
- Play pad chord sustained
- Simultaneously play same notes as rapid short pulses (every 100–150 ms)
  at a higher octave
- Arp layer: faster attack (10–20 ms), shorter release (100 ms)
- More delay/echo on arp layer

## Layered Drone Techniques (Current Implementation)

The pad generator layers three elements per note:

1. **Detuned triad** — 3 voices at [-detune, center, +detune] cents with
   fundamental + 2 harmonics each. Main body of the sound.
2. **Sub layer** — Pure sine one octave below the already-dropped base
   frequency. Adds weight without harmonic complexity.
3. **Breath modulation** — Very slow amplitude wobble (0.03–0.05 Hz) at
   8% depth. Not tremolo — more like the pad is "breathing."

## Parameter Comparison

| Parameter | Warm Pad (target) | Halloween/Eerie |
|-----------|-------------------|-----------------|
| Fundamental range | 80–300 Hz | 200–600 Hz |
| 2nd harmonic | 0.15–0.25 | 0.3–0.5 |
| 3rd harmonic | 0.03–0.08 | 0.15–0.3 |
| Detune | 1–3 cents | 8–20 cents |
| Envelope attack | 30–45% | 5–15% |
| Sub layer | Yes, prominent | Minimal or absent |
| Movement | Slow breath (0.03 Hz) | Tremolo (3–9 Hz) |
| Octave | Drop 1 (-12 semitones) | Stay or raise |

## Shimmer (Arpeggio Mode)

Slow pitch wobble per note (2.5 Hz + 0.3 Hz per voice index, 0.3% depth).
Subtle enough to not sound like vibrato but adds life. Each voice wobbles at
a slightly different rate, preventing phase-lock between overlapping notes.

## Potential Improvements (Prioritized)

1. **Low-pass filter** — Move from additive harmonics to subtractive
   synthesis (saw oscillator → LPF). Single biggest upgrade toward
   authentic jungle pad sound.
2. **Reverb** — Even a simple Schroeder reverb would add massive depth.
   Hardware reverb defined the era.
3. **Bulldozer mode** — Layered pad + arp playing simultaneously. Already
   have both generators; need to combine them.
4. **Stereo chorus** — Proper BBD-style chorus with L/R phase-inverted LFO
   modulation. Currently detuning approximates this but isn't true chorus.
5. **Filter envelope** — Slow sweep on LPF cutoff for evolving character
   rather than static filtering.
