# Marquee multi-selection: plan

Design: board "2.12 Canvas · Marquee multi-selection" (`design/exports/28-canvas-marquee-selection.png`).
The selection model is refactored first; the features are built on the new model, never next to
the old one.

## Step 1: the selection model (refactor)

Today `SelectionState` is `primary` plus an optional `secondary` (the Shift-click compare). It
becomes an **ordered set with a primary**:

* `SelectionState` keeps its nodes private: `nodes()` (in the order they joined, no repeats),
  `primary()`, `len()`, `contains()`, `other()` (the second node of a 2-item selection). Every
  single-selection consumer (inspector, breadcrumb, overlays, references, stepper, isolate,
  clip) reads `primary()` and behaves as before.
* Changes go through `AppState` methods only: `select`, `select_on_canvas`, `select_additional`
  (Shift: add, or remove a node that is in and is not the primary), `deselect` (⌥),
  `select_nodes` (a marquee: replace, add or subtract), `select_all` (⌘A),
  `keep_primary` (Esc), `clear_selection`, `step_into` / `step_out` / `step_selection`
  (the inspector's stepping), `narrow_selection` ("Only P").
* `secondary` is removed. The compare view is the 2-item case: `other()`, `compare_rect()` and
  `comparison()` (gap measured from the primary) read the set.
* Dead code removed with it: the direct writes to `selection.primary`, the "second selection"
  field resets.

## Step 2: core and render (minimal extensions)

* `Rect::intersects` and `PageIndex::hit_rect` in core, and `Document::hit_rect(page, rect,
  layer)`: what a marquee touches, one node per thing touched, filtered by the Pick target, in
  reading order. Tags: the element that owns each touched marked-content sequence (the sequence
  itself when no element owns it), in structure order. Content: operators, in paint order.
  Artifacts: artifact sequences. Boxes: the boxes whose outline the rectangle meets. All: per
  painted item its element, else its artifact, else its operator. Annotations: none (not hit
  tested before v1.1).
* `pdfxray_render::ops_for_all(doc, nodes)`: the union of `ops_for` over a set, so Isolate and
  Hide can act on N nodes. `Isolation` carries the nodes.

## Step 3: features on the new model

* Canvas gestures (Pick tool): drag from empty space draws the marquee, ⌘-drag starts one over
  an element, Shift adds, ⌥ subtracts; Shift-click and ⌥-click do the same for one element.
  Space-drag pans; the Measure tool keeps plain drag and the marquee is off while it is on.
* Marquee look: 1 px accent stroke, accent at 8 % fill, radius 2, a 1 px `#0A0C0F` halo at
  45 %, crosshair cursor, a count chip at the cursor ("4 · Tags"), a 60 % accent pre-highlight
  of what it will catch, auto-scroll past the edge of the view.
* Multi-selection on the canvas: every member gets an outline (1.5 px accent, 6 % fill), no
  per-element labels; a group hairline (accent at 33 %) with an "N selected" badge.
* Action bar for N ≥ 2: "N elements" · Isolate · Hide · Show in tree · Copy ids · × ("Clear
  selection (Esc)").
* Inspector: N ≥ 3 shows the summary (title, "Pick: Tags · page 1", chips, BY ROLE with
  "Only P", ELEMENTS); a row steps in to the normal inspector with "← N elements · 2 of N ‹ ›";
  `[` and `]` step. N = 2 keeps today's "2 selected · gap" view.
* Keys: ⌘A selects all of the Pick target on the page, Esc first drops all but the primary and
  then clears, I and H act on the whole selection. In the tree: Shift+↑/↓ extends, ⌘+Space
  toggles the focused row; the tree highlights every selected row.

## Done

**Model (step 1).** `state/selection.rs`: `SelectionState { nodes, primary, stepped_in, pinned }`
with private fields and the accessors above; every reader of the old `primary` field calls
`primary()`. Removed: the `secondary` field, its resets in `install`, `pointer.rs` and `keys.rs`,
and the direct writes to `primary` (`cycle_page_point` now calls `select_on_canvas`). The
compare inspector (`inspector/measure.rs`) and the canvas's pink outline read `other()`.
`Isolation.node` became `Isolation.nodes`, `SelectedOps.node` became `SelectedOps.nodes`, and
`AppState::view_targets` lists the selection on the primary's page.

**Core and render (step 2).** `Rect::intersects`, `PageIndex::hit_rect`,
`Document::content_touched` (crate-private), `Document::mcid_owner` and `Document::hit_rect`;
`pdfxray_render::ops_for_all`. Tests: `pdfxray-core/tests/tags/hit_rect.rs` (one per target),
`pdfxray-render/tests/modes.rs::ops_for_all_is_the_union_of_every_node_of_a_set`.

**Features (step 3).**

* Gestures: `center/canvas/page.rs` adds a drag-only region under the page image while a marquee
  may start (empty space at the pointer, or ⌘), so the image keeps its clicks and hover; the
  marquee starts once the pointer has really moved. `center/canvas/pointer.rs` and
  `state/marquee.rs` run it; `center/canvas/mod.rs` finishes it when the button is let go.
  `state/pointer.rs` has Shift-click (`shift_click_page_point`) and ⌥-click
  (`alt_click_page_point`).
* Space-drag pan: egui 0.37 only drags a scroll area on touch screens, so a mouse drag never
  panned. While Space is held the canvas's scroll area takes drags (`ScrollSource::ALL`) and the
  page takes none.
* Drawing: `center/canvas/multi.rs` (members, group hairline and badge, marquee with halo,
  pre-highlight, count chip, action bar anchor); auto-scroll in `center/canvas/scroll.rs`
  (`edge_scroll`). Tokens in `theme/tokens.rs` (`MARQUEE_*`, `MULTI_*`, `GROUP_HAIRLINE`).
* Action bar: `center/canvas/action_bar.rs::multi_bar` with `actions::SelectionAction`.
* Inspector: `inspector/multi.rs` (summary and the stepped-in bar), wired in `inspector/mod.rs`.
* Keys: `shell/keys.rs` (⌘A, `[`/`]`, Esc). Trees: `tree_view.rs` (`set_also_selected`,
  Shift+↑/↓, ⌘+Space) and `navigator/mod.rs`.

**Decisions where the board left room.**

* The primary is the last *plain*-clicked node; Shift-click adds without moving it. This keeps
  today's compare view exactly (the gap from the first node, the same signs) and every existing
  compare test unchanged.
* ⌥-click subtracts on a selection of two or more; with one node or none it keeps going down the
  hit stack, as before.
* Esc keeps today's order: first every node but the primary, then the selection. The × clears at
  once.
* A plain drag that starts over an element does nothing (as before); ⌘ starts a marquee there.
  With the Boxes target the whole page is empty space.
* Members of a multi-selection get 1.5 px / 6 % outlines; the single selection keeps its 2 px
  outline, which also marks the primary once the inspector stepped into it.
* The ELEMENTS list shows at most 200 rows ("and N more").

**Tests.** `crates/pdfxray-app/tests/marquee.rs` (gestures per target, Shift/⌥ for drag and click,
⌘-drag, Space pan, Measure, ⌘A, Esc, count chip, pre-highlight, auto-scroll, the bar's actions,
Isolate/Hide on a set, the summary and "Only", stepping, the tree's keyboard path, Tab to the
bar) and three snapshots (`app_marquee_drawing`, `app_multi_selection`, `app_multi_stepped_in`).
Unit tests in `state/selection.rs`, `state/marquee.rs`, `selection.rs`, `inspector/multi.rs`,
`center/canvas/multi.rs` and `center/canvas/scroll.rs`. Each behaviour was checked to fail its
test when reverted.
