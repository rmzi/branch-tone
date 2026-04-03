# TUI Layout Restructure: Repos Top, Stream+Sphere PiP Bottom

## Context

The current layout gives too much prominence to the stream and doesn't match the user's vision. The sphere needs to be a permanent, animated PiP element — not crammed into a sidebar. The stream should show commands contextually but be secondary to repos and the sphere.

## Layout

```
┌─ Status ──────────────────────────────┐
├─ Activity ────────────────────────────┤
├─ Repos (full width, 4-5 lines) ──────┤
│ ■ branch-tone/main  GREP "fn res.."  │
│ ■ branch-tone/dev   EDIT main.rs     │
│ □ other-repo/feat   BASH cargo test  │
├─ Stream ──────────────────────┬──────┤
│ GREP  "fn resolve" src/       │      │
│ EDIT  main.rs:7759            │  ◯   │
│ BASH  cargo test              │ PiP  │
│ PROMPT "fix contrast"         │      │
├───────────────────────────────┴──────┤
└─ footer ─────────────────────────────┘
```

Vertical stack:
1. Status bar (3 lines)
2. Activity histogram (resizable, default 11)
3. Repos panel (full width, `Length(5)`)
4. Stream + Sphere PiP side-by-side (`Min(6)`, stream gets `Min(30)`, sphere gets `Length(22)`)
5. Footer (1 line)

## L1. Layout restructure in `ui()`

Change the vertical constraints:
```rust
Constraint::Length(3),                     // status
Constraint::Length(state.activity_height), // activity  
Constraint::Length(5),                     // repos (full width)
Constraint::Min(6),                       // stream + sphere PiP
Constraint::Length(1),                     // footer
```

Then split chunk[3] horizontally:
```rust
let bottom = Layout::horizontal([
    Constraint::Min(30),      // stream
    Constraint::Length(22),   // sphere PiP
]).split(chunks[3]);
```

- `render_sidebar` → `render_repos` (renamed, full-width, no sphere code)
- `render_sphere_panel` renders in the PiP rect
- `render_stream` renders in the stream rect
- Remove old sidebar toggle / `show_sidebar` horizontal split logic

## L2. Sphere 3D physics — orbit + wander with wall bouncing

Add `SphereState` to `TuiState`:
```rust
struct SphereState {
    // Position in normalized cube space [-1, 1]³
    x: f32, y: f32, z: f32,
    // Velocity
    vx: f32, vy: f32, vz: f32,
    // Orbital parameters (slowly drifting ellipse)
    orbit_phase: f32,
    orbit_radius: f32,
    orbit_tilt: f32,
}
```

**Physics model**: The cube is 2.5× the sphere diameter. The sphere follows an elliptical orbit that slowly precesses, with gentle random perturbations. When it approaches a wall (|coord| > 1.0), it bounces elastically — reflected velocity with slight damping. The orbit_phase advances each tick, creating continuous smooth motion.

**Projection**: The sphere's (x,y,z) position determines:
- Lateral offset in the PiP frame (x maps to horizontal centering)
- Vertical offset (y maps to vertical centering)
- Scale/brightness (z maps to apparent size — closer = bigger/brighter, farther = smaller/dimmer)

The raycast sphere renderer already exists — just pass the offset and scale derived from SphereState.

Update `render_sphere()` to accept position offsets + scale factor so the sphere appears to move within the PiP frame.

## L3. Stream column tightening

Current stream columns: `lane(1) + time(9) + type(8) + repo + branch + badge`

New tighter layout — drop the lane char, make type the leading color stub:
```
TYPE   context                    repo/branch
GREP   "fn resolve_theme" src/   branch-tone/main
EDIT   main.rs:7759              branch-tone/main  
BASH   cargo test --features tui branch-tone/dev
PROMPT "fix the contrast"        branch-tone/main
```

- Type label is the colored stub (5-7 chars, left-aligned, colored by semantic type)
- Context is the most valuable info (file path, command, pattern) — gets the most space
- Repo/branch right-aligned, colored by branch_color_for()
- Timestamp only shown if scrolled (not in live tail mode)
- Drop the `▓░▒` lane chars — type color stub replaces them

**Note**: Context data requires hook enrichment (L4) to populate. Until then, show tool badge as fallback.

## L4. Hook enrichment (context extraction)

In `run_hook()`, extract a context snippet from `tool_input` and append to the log line as a 5th tab-delimited field:

```rust
let context = match ctx.tool_name.as_str() {
    "Read" | "Edit" | "Write" => json_str_field(&tool_input, "file_path"),
    "Grep" | "Glob" => json_str_field(&tool_input, "pattern"),
    "Bash" => json_str_field(&tool_input, "command"),  // truncate to 80 chars
    "Agent" => json_str_field(&tool_input, "description"),
    "WebSearch" => json_str_field(&tool_input, "query"),
    "WebFetch" => json_str_field(&tool_input, "url"),
    _ => String::new(),
};
// For UserPromptSubmit: extract "prompt" from top-level JSON
```

Log format becomes:
```
timestamp event repo branch\ttool\tagent\tsession_id\tcontext
```

Update `ParsedEvent::parse()` to extract the context field.
Update `type_label()` / stream rendering to show context.

## Implementation Order

1. **L1** — Layout restructure (vertical stack, PiP split)
2. **L2** — SphereState physics (orbit + wander + bounce in cube)
3. **L3** — Stream column tightening (type stub + context + repo/branch)
4. **L4** — Hook enrichment (context extraction into log)

## Files

All in `src/main.rs`:
- `ui()` — layout constraints (~line 8500)
- `render_sidebar()` → rename to `render_repos()`, remove sphere
- `render_sphere_panel()` + `render_sphere()` — accept position/scale
- `render_stream()` — new column layout
- `run_hook()` — context extraction (~line 3682)
- `ParsedEvent::parse()` — context field (~line 7152)
- `TuiState` — add `SphereState` field

## Verification

```bash
cargo test --features tui   # all tests pass
cargo build --features tui  # zero warnings  
cargo install --path . --features tui
```

Visual: `branch-tone tui` — repos full-width under activity, stream compact with type stubs, sphere orbiting smoothly in PiP bottom-right, bouncing off frame walls with z-depth scaling.
