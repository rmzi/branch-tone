# TUI Polish Pass Round 2: Menu Jump Fix + Activity Theming

## Context

Round 1 (S1–S4, B1–B3) is implemented and passing. Two follow-up issues:
1. **Bug**: "on rerender the menu jumps" — seed overlay list offset resets every frame because `render_seed_overlay` clones `seed_list` state
2. **Feature question**: Should the activity histogram adopt seed-themed colors?

## F1. Fix seed overlay list jump

**Root cause**: `render_seed_overlay` takes `&TuiState` (immutable) and calls `frame.render_stateful_widget(list, overlay, &mut state.seed_list.clone())`. The `clone()` discards the scroll offset that ratatui computes each frame. On short terminals (< 20 rows), not all 16 seeds fit — ratatui recalculates the offset every frame, and since it starts from scratch each time, the list can visually oscillate.

**Fix**:
- Change `fn render_seed_overlay(frame, state: &TuiState, ...)` → `state: &mut TuiState`
- Replace `&mut state.seed_list.clone()` → `&mut state.seed_list`
- No borrow conflicts: `theme` is an owned `ResolvedTheme`, not borrowing from `state`
- Location: `render_seed_overlay` (~line 8929), call site in `ui()` (~line 8523)

## F2. Activity histogram theming

**Current state**: 5 histogram modes with different color sources:
| Mode | Color source | Seed-aware? |
|------|-------------|-------------|
| Category | `EventTypeCategory::color()` — fixed ANSI: Green/Yellow/Cyan/Blue/DarkGray | No |
| Repo | `voice_color_for_repo()` from seed palette | **Yes** |
| Session | `voice_colors[i % 8]` from seed palette | **Yes** |
| Branch | `voice_colors[i % 8]` from seed palette | **Yes** |
| Tool | `ToolFamily::color()` — fixed ANSI: Cyan/Yellow/Red/Blue/Magenta/DarkGray | No |

**Approach**: Tint Category and Tool fixed colors toward the seed accent — 25% blend. This preserves inter-category/tool hue distinctness while adding seed cohesion. The `Other`/`DarkGray` entries get the seed's `dim_text` RGB instead.

**Implementation**:
- Add a `tint_toward(base: Color, accent: Color) -> Color` that blends 25% toward accent (using existing `blend_color`)
- In `build_activity_data`, pass `accent: Color` and apply `tint_toward` for Category and Tool modes
- The accent comes from `seed_theme().accent` which is already available at the call site in `rebuild_activity()`
- The `Other` / `DarkGray` category gets `Rgb(90,90,90)` minimum (matching S3 fix)

**Risk**: Monochromatic seeds (monolith, tundra) could reduce distinctness. At 25% blend, category hues stay recognizable — Green tinted 25% toward gray is still identifiably green. Safe for at-a-glance reading.

**Files**: `src/main.rs`
- `build_activity_data` signature: add `accent: Color` param (~line 7513)
- `EventTypeCategory::color` / `ToolFamily::color`: apply tint in series construction
- `rebuild_activity` call site: pass accent (~line 7966)
- Tests: `activity_data_*` tests pass the accent

## Implementation Order

1. **F1** (overlay fix) — 2 min
2. **F2** (activity tint) — 5 min
3. Tests + build + install

## Verification

```bash
cargo test --features tui     # all 203+ tests pass
cargo build --features tui    # zero warnings
cargo install --path . --features tui
```

Visual: Open `branch-tone tui`, press `p` — seed overlay doesn't jump on short terminals. Switch seeds — activity histogram bars pick up seed tint. Category mode still clearly distinguishable (Green/Yellow/Cyan/Blue remain identifiable).
