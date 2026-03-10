# Jazz Ensemble Sound Design

How branch-tone maps Claude Code hook events to a jazz ensemble, where each
voice signals a different aspect of the agent lifecycle.

## The Ensemble

```
Voice             Hook Events                        Character
────────────────  ─────────────────────────────────── ──────────────────────
Drums (kick/snr)  UserPromptSubmit, Stop              Downbeat / backbeat
Hi-Hat (tools)    PreToolUse, PostToolUse,            Rapid micro-clicks
                  PostToolUseFailure
Bass (agents)     SubagentStart, SubagentStop,        Ascending / descending
                  WorktreeCreate, WorktreeRemove      notes (voices enter/leave)
Keys/Pad (sess)   SessionStart, SessionEnd            Full chord bloom / fade
Horn (attention)  PermissionRequest, Notification     Melodic phrase, look here
Piano (lifecycle) InstructionsLoaded, ConfigChange,   Arpeggios, chord shifts,
                  TaskCompleted, PreCompact,           resolution cadences
                  TeammateIdle
```

## Event Categories

Each voice corresponds to an `EventCategory` that shapes the sound:

```
Category         Octave  Transpose  Steps   Register
───────────────  ──────  ─────────  ──────  ─────────────
SessionBoundary  1.0x    0          5       Center — full chord
Attention        1.0x    +5         max(3)  Bright via transpose, not octave
DrumHit          1.0x    0          1       Center — single punch
ToolPulse        2.0x    0          1       High — barely audible click
Bass             0.5x    -5         3       Low — foundation
Lifecycle        0.75x   +3         3       Below pad — comping texture
Default          1.0x    0          base    Center — fallback
```

## Duration & Volume Map

```
Event                Dur(ms)  Vol    Voice           Seed
───────────────────  ───────  ─────  ──────────────  ────
SessionStart         3500     0.30   Keys/Pad        1
SessionEnd           3500     0.25   Keys/Pad        4
Stop                 400      0.12   Drums (snare)   3
UserPromptSubmit     350      0.08   Drums (kick)    5
PreToolUse           120      0.05   Hi-Hat (closed) 11
PostToolUse          150      0.05   Hi-Hat (open)   12
PostToolUseFailure   250      0.08   Hi-Hat (rim)    13
PermissionRequest    2500     0.18   Horn            2
Notification         2000     0.15   Horn            6
SubagentStart        1000     0.10   Bass (ascend)   7
SubagentStop         1000     0.10   Bass (descend)  8
WorktreeCreate       1200     0.10   Bass (ascend)   14
WorktreeRemove       1200     0.10   Bass (descend)  15
InstructionsLoaded   1500     0.10   Piano (arp)     16
ConfigChange         1800     0.10   Piano (shift)   17
TaskCompleted        2500     0.15   Piano (resolve) 18
PreCompact           2000     0.10   Piano (sweep)   9
TeammateIdle         1500     0.08   Piano (hold)    10
```

## Worktree-as-Voice

branch-tone hashes `repo + branch` to produce a sound identity. Since a
worktree IS a different git branch, subagents running in worktrees
automatically get their own melodic signature:

- **Same repo key** (harmonic identity) — they're in the same project
- **Different branch melody** — their worktree branch name produces unique
  arpeggio patterns, rhythms, and envelopes

During a PDS swarm with 3 workers in 3 worktrees, you hear 3 distinct bass
voices entering and leaving the mix. No extra code needed — it falls out of
the two-layer hashing architecture.

## PDS Lifecycle Mapping

The sound design maps naturally to the Portable Dev System's 6-phase
workflow. This mapping is **advisory and interoperable, not coupled** —
branch-tone works with any Claude Code workflow, but the design is informed
by PDS's agent orchestration patterns.

```
PDS Phase       Hook Events                       What You Hear
──────────────  ────────────────────────────────── ──────────────────────
1. Plan         UserPromptSubmit, SubagentStart    Kick + bass entry
                (researcher)
2. Decompose    WorktreeCreate x N                 Bass runs ascending
3. Dispatch     SubagentStart x N (workers)        Multiple bass voices enter
4. Validate     SubagentStop, SubagentStart        Bass exit/entry
                (validator)
5. Consolidate  SubagentStart (reviewer, doc),     Bass, then resolved cadence
                TaskCompleted
6. Knowledge    SubagentStart (scout),             Bass + held waiting note
                TeammateIdle
```

## Jazz Micro-Patterns (Ghost Notes)

Drum and hi-hat events don't play a single sterile hit. Each repo gets a
deterministic micro-pattern (hash bytes 24–25) that adds jazz feel:

- **`hit_count`** (1–4): Primary hit + ghost notes. 1 = clean single, 2 = flam,
  3 = drag, 4 = buzz roll.
- **`hit_spacing_ms`** (15–60ms): Tighter = flam feel, wider = drag feel.
- **Ghost note velocity**: Primary at 100%, ghosts at 60%/50%/40% (decreasing).
- **Event seed rotation**: Each hook event gets a different hit count from the
  same repo, so Stop might be a clean snare while UserPromptSubmit is a flammed
  kick.

```
hit_count=1  ●               Clean single
hit_count=2  ○●              Flam (ghost + primary)
hit_count=3  ○○●             Drag (two ghosts + primary)
hit_count=4  ○○○●            Buzz (three ghosts + primary)
             └─┘ spacing_ms
```

## Design Principles

1. **Frequency maps to frequency**: High-firing events (tool calls) get
   high-pitched, ultra-short sounds. Low-firing events (session start) get
   full chord pads. Mirrors a real drum kit.

2. **Volume encodes importance**: Tool pulses at 0.05, drums at 0.08-0.12,
   bass at 0.10, attention at 0.15-0.18, session pads at 0.25-0.30.

3. **Direction encodes intent**: Ascending = something starting (SubagentStart,
   WorktreeCreate). Descending/reversed = something ending (SubagentStop,
   WorktreeRemove, SessionEnd).

4. **Deterministic identity**: Same repo+branch+event always produces the
   same sound. You learn to recognize your projects by ear.

5. **PDS-informed, not PDS-coupled**: The ensemble voices map naturally to
   agent lifecycle phases, but nothing in the code references PDS directly.
   branch-tone's design direction is driven by PDS's evolution.
