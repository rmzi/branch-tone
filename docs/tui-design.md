# TUI Design: Terminal Dashboard

How branch-tone's terminal dashboard visualizes the daemon as a live system
monitor — architecture, rendering techniques, and design decisions.

> **See also**: [Sound Design](sound-design.md) — synthesis engine details.
> [Jazz Ensemble](jazz-ensemble.md) — event-to-voice mapping.

## Motivation

branch-tone runs as a daemon receiving hook events from Claude Code sessions.
The tray app (macOS) shows status in the menu bar, but offers no sense of
*flow* — you can't feel the rhythm of your development sessions. The TUI
exists to make the entire system visible: every event, every voice, every
heartbeat.

## Architecture

```
┌───────────────────────────────────────────────────────┐
│  Claude Code sessions (1–5 workstreams)               │
│  ├── branch-tone hook (stdin JSON)                    │
│  └── writes to ~/.branch-tone/events.log              │
│       sends to daemon via Unix socket                 │
└───────────────┬───────────────────────────────────────┘
                │
    ┌───────────▼───────────────────────────┐
    │  branch-tone daemon                    │
    │  ├── 8 voice slots (cpal audio)        │
    │  ├── Unix socket (__status_json)       │
    │  └── state files (seed, mute, etc.)    │
    └───────────┬───────────────────────────┘
                │ polls every 2s
    ┌───────────▼───────────────────────────┐
    │  branch-tone tui (this)               │
    │  ├── ratatui + crossterm              │
    │  ├── read-only client (no audio)      │
    │  └── ~5fps render, 200ms input poll   │
    └───────────────────────────────────────┘
```

The TUI is a **pure observer** — it never touches the audio engine. It
connects to the daemon the same way the tray app does: polling `__status_json`
over the Unix socket and reading state files from `~/.branch-tone/`. This
means you can open and close the TUI without affecting playback.

## Layout

```
┌─ branch-tone ──────────────────────────────────────────────┐
│ ● RUNNING  PID 1234  up 2h  3 voices  seed: shadow  1/16  │
├─ Activity ─────────────────────────────────────────────────┤
│ █▇▅▃▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▂▃▅█▇▅▃▂▁▁▁▁▁▁▂▅█▇▅ │
│ ■tool ■prompt ■session ■agent         60 min │ 234 events  │
├─ Stream ───────────────────────────────────────────────────┤
│░ 21:58:28 READ    branch-tone/tui [Read]                    │
│░ 21:58:29 BASH    universe/issue_improvements [Bash]       │
│▒ 21:58:30 PROMPT  branch-tone/tui                          │
│▒ 21:58:31 WRITE   universe/issue_improvements [Edit]       │
│▓ 21:58:32 READ    branch-tone/tui [Grep]                   │
│▓ 21:58:33 AGENT+  universe/issue_improvements <worker>     │
│█ 21:58:34 FAIL    branch-tone/tui [Bash]                   │
│                                              28/80 [r]eset │
├────────────────────────────────────────────────────────────┤
│ [m]ute [d]rone [S]top [g]rid:1/16 [p]alette [q]uit        │
└────────────────────────────────────────────────────────────┘
```

Four vertical zones:

| Zone | Height | Purpose |
|------|--------|---------|
| **Status bar** | 3 rows | Daemon state, voice count, seed, grid, mute |
| **Activity** | 5 rows | 60-minute stacked histogram by event category |
| **Stream** | Remaining | Unified voice+event feed with colored lanes |
| **Footer** | 1 row | Keybind hints |

### Design Decision: No Separate Voice Panel

Earlier iterations had a split layout: voices on the left, seeds on the right,
events at the bottom. The user's feedback was clear: "I'll never run more than
5 workstreams — give me more events." The redesign merged voices into the event
stream via colored lane prefixes, and moved seeds to a popup overlay.

## Rendering Techniques

### Voice Lane Colors

Each active voice slot gets a stable color from an 8-color palette:

```rust
const VOICE_COLORS: [Color; 8] = [
    Cyan, Green, Yellow, Magenta, Blue, Red, LightCyan, LightGreen,
];
```

Every event line starts with a colored block character (`█▓▒░`) matching its
repo's voice slot. This creates a visual "lane" effect where you can track
activity per workstream without explicit headers. The color assignment is
positional (slot 0 = Cyan, slot 1 = Green), so it's stable as long as the
voice remains active.

### Recency Gradient

Events don't have a binary "new/old" state. Instead, each line's brightness
is computed from its position in the visible list:

```
recency = line_index / visible_count    (0.0 = oldest, 1.0 = newest)
```

| Recency | Time color | Type color | Repo color |
|---------|------------|------------|------------|
| > 0.8 | DarkGray | Full type color | Voice color |
| > 0.5 | DarkGray | DarkGray | Voice color |
| < 0.5 | DarkGray | DarkGray | DarkGray |
| Newest + flash | **White** | **White + Bold** | **White** |

The newest event also gets a 2-second "flash" where the entire Stream border
turns Green and the line renders in White+Bold. This creates a waterfall
effect: events light up bright at the bottom and dim as they scroll up.

### Activity Histogram (Stacked Per-Category)

The Activity panel renders a 60-column bar chart where each column = 1 minute.
Bars are **stacked by event type category**, drawn bottom-up with direct
buffer writes:

```rust
for (category, buckets) in &state.activity.per_category {
    // Scale value to available column height
    let bar_h = (val * col_height) / max_val;
    // Draw colored block characters bottom-up
    for _ in 0..bar_h {
        buffer[(x, y)].set_char('█').set_fg(category.color());
        y -= 1;
    }
}
```

| Category | Color | Events |
|----------|-------|--------|
| Tool | Green | PreToolUse, PostToolUse, PostToolUseFailure, FileChanged |
| Prompt | Yellow | UserPromptSubmit, Elicitation, ElicitationResult |
| Session | Cyan | SessionStart, SessionEnd, Setup |
| Agent | Blue | SubagentStart, SubagentStop, TaskCompleted, TaskCreated, TeammateIdle |
| Other | DarkGray | Everything else |

This is one of the few places where ratatui's immediate-mode buffer is
accessed directly rather than through widgets — the `Sparkline` widget only
supports single-color bars, so stacked coloring requires cell-level control.

### Seed Overlay

The seed picker is a modal popup rendered on top of the main view:

1. Compute centered `Rect` (50 wide, 20 tall, clamped to terminal size)
2. Render `ratatui::widgets::Clear` to erase the background cells
3. Render the seed `List` widget with `ListState` for selection tracking

The overlay captures all keyboard input while open (j/k navigate, Enter
applies, Esc/p closes). This pattern avoids the complexity of focus
management — there are only two states: overlay open or closed.

### Voice Activity Decay

Each repo's last-event time is tracked. The lane prefix character decays
through block densities over ~20 seconds:

| Age | Char | Color |
|-----|------|-------|
| < 1s | `█` | White (flash) |
| < 2s | `▓` | Voice color |
| < 10s (recency > 0.7) | `▒` | Voice color |
| Older | `░` | Voice color |

## Event Lifecycle

Events persist in `~/.branch-tone/events.log` indefinitely — the daemon
appends one line per hook invocation:

```
2026-04-01T21:58:29 PreToolUse branch-tone tui
2026-04-01T21:58:30 PreToolUse branch-tone tui	Read	
2026-04-01T21:58:31 SubagentStart branch-tone feat		worker
```

The format is backward-compatible: 4 space-delimited fields (`timestamp
event repo branch`), with optional tab-delimited enrichment after the branch
(`\ttool_name\tagent_type`). Old parsers see `"branch\ttool\tagent"` as
the branch field (harmless); new parsers split on tab to extract tool and
agent context.

### Tool & Agent Badges

When enrichment is present, the Stream panel appends colored badges:

- **Tool badges** `[Read]` `[Bash]` `[Edit]` — color-coded by tool family
  (Cyan for reads, Yellow for writes, Red for Bash, Blue for Agent, Magenta
  for web tools)
- **Agent badges** `<worker>` `<researcher>` — Blue, showing which agent
  type spawned

### Tool-Aware Labels

ToolPulse events (PreToolUse/PostToolUse) show tool-specific labels instead
of generic `TOOL`:

| Tool | Label |
|------|-------|
| Read, Glob, Grep | `READ` |
| Write, Edit | `WRITE` |
| Bash | `BASH` |
| Agent | `AGENT` |
| WebSearch, WebFetch | `WEB` |
| (other) | `TOOL` |

The TUI reads the last 500 lines for the activity histogram and displays
the most recent 80 in the Stream panel. Press `[r]` to reset the view
(clears displayed events and voice activity, starts counting from zero).

## Controls

| Key | Action | Scope |
|-----|--------|-------|
| `m` | Toggle mute (all audio) | File: `~/.branch-tone/mute` |
| `d` | Toggle drone (ambient bed) | File: `~/.branch-tone/no-drone` |
| `s` | Start daemon | Spawns `branch-tone daemon --detach` |
| `S` | Stop daemon | Sends `__shutdown` via socket |
| `g` | Cycle quantize grid | File: `~/.branch-tone/quantize` (auto→32→16→8→4) |
| `p` | Open seed palette | Overlay panel |
| `r` | Reset event view | Clears display, restarts counting |
| `q` / Esc | Quit | (Esc also closes seed overlay) |
| `j`/`k` | Navigate seeds | Only when overlay open |
| Enter | Apply selected seed | Only when overlay open |
| `x` | Clear seed | Only when overlay open |

## Feature Gating

The TUI is behind `--features tui` to keep the default binary lean:

```toml
[dependencies]
ratatui = { version = "0.29", optional = true, features = ["crossterm"] }

[features]
tui = ["dep:ratatui"]
```

ratatui brings ~25 transitive dependencies (cassowary, compact_str, itertools,
lru, etc.) that are only compiled when opted in. The hook binary — which fires
on every Claude Code event — stays fast and small.

Unlike the tray feature (`cfg(target_os = "macos")`), the TUI has no platform
gate. ratatui + crossterm is cross-platform, so `branch-tone tui` works on
macOS, Linux, and Windows.

## Development Diary

### Session 1: Initial Implementation (2026-04-01)

**Research phase** — surveyed the musical TUI landscape:
- **textStep** (Rust/ratatui): Full step sequencer + drum machine at 60fps.
  Proved the pattern works. Two-thread architecture: UI + audio over lock-free
  channels.
- **scope-tui**: Terminal oscilloscope using braille characters.
- **cava**: The gold standard terminal audio visualizer (C).

**Key decision**: branch-tone's TUI should be a **pure client**, not embed
audio. The daemon already owns the audio thread. The TUI connects over the
existing Unix socket — same protocol the tray app uses. No cpal, no audio
thread, no lock-free channels needed.

**v1 — Basic dashboard**: Status bar, voice table, seed list, event log,
keybind footer. Split layout (voices left, seeds right). Feature-gated behind
`--features tui`. 7 new tests.

**v2 — Color and activity**: Parsed event log lines with per-type colors
(TOOL=green, START=cyan, PROMPT=yellow, FAIL=red). Added 60-minute activity
sparkline. Voice slots pulse with block-density decay (██→▓▓→▒▒→░░). Event
border flashes green on new arrivals. Recency gradient fades older events.

**v3 — Unified stream**: User feedback: "I'll never run more than 5
workstreams. Make the events bigger. Make things light up."

Redesigned from the ground up:
- Merged voices + events into a single **Stream** panel with colored lane
  prefixes (stable per-repo colors from an 8-color palette)
- Replaced the single-color sparkline with **stacked per-category bars**
  using direct buffer writes for cell-level color control
- Moved seeds to a **popup overlay** ([p] to open, Esc to close)
- Added **[r]eset** to clear the view and start fresh
- 155 tests total with TUI feature

### Session 2: Deep Hook Integration (2026-04-02)

**Goal**: Extract and surface the 30+ fields in Claude Code hook JSON that
were being thrown away. Each tool type should sound different, each agent
type should occupy a different register, and errors should sound dissonant.

**6 work streams** across extraction → protocol → dispatch → synthesis:

1. **HookContext struct** — extracts `tool_name`, `agent_type`, `error_msg`,
   `session_id` from hook JSON. All fields default to "" for backward compat.

2. **27 events** — expanded from 18. Added `StopFailure`, `PermissionDenied`,
   `PostCompact`, `Setup`, `TaskCreated`, `CwdChanged`, `FileChanged`,
   `Elicitation`, `ElicitationResult`. Each with unique seed (19–27).

3. **Socket protocol** — `send_to_daemon` now serializes all HookContext
   fields. Daemon extracts `tool_name`, `agent_type`, `error` with backward
   compat for old clients.

4. **TUI enrichment** — ParsedEvent parses tab-delimited enrichment from
   logs. Stream shows `[Read]` tool badges and `<worker>` agent badges.
   Tool-aware labels: `READ`, `WRITE`, `BASH` instead of generic `TOOL`.

5. **Dispatch wiring** — QueuedNote carries `tool_filter`, `tool_detune`,
   `tool_harmonics`, `is_error`. VoiceSlot gets matching atomics. Agent
   octave shift applied to bass event frequencies.

6. **Audio synthesis** — ToolPulse branch reads new atomics: applies filter
   multiplier, detuned second voice, conditional harmonics/saw boost per
   tool type. Error dissonance: tritone + minor 2nd + pitch drop.

**Result**: 169 tests (22 new), zero new warnings, all build targets pass
