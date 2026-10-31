# UI revamp — implementation plan

**Status (2026-10-29): R0–R7 implemented.** Gate green twice in a row (1,138 tests incl. real
veraPDF + PDFium, clippy `-D warnings`, fmt, `cargo deny`, `cargo machete`, no dead code). Not committed.

Inputs: `docs/spec/ui-revamp.md`, `design/exports/00…11-*.png` (behaviour contract = `00`).
Rules: plan includes refactors; no new UI bolted onto the old structure; dead-code check and the
full gate (fmt, clippy `-D warnings`, tests, `cargo deny`, `cargo machete`) after every
milestone; kittest tests are behavioural (what is selected / shown after an interaction) plus a
smoke expectation; snapshots updated on purpose and compared with the PNGs. No commits.

## 1. What changes structurally

The current app is an `egui_tiles` dock of ten equal tabs. The design is a **fixed shell**:

```
TitleBar(44)  file · chips · rev chip ▾ · Validate   (layer mode → canvas toolbar "Pick ▾", see §5)
Rail(52) │ Navigator(300) │ Center: Canvas | ObjectView       │ Inspector(340)
         │                ├──────────────────────────────────┤
         │                │ Drawer(196, resizable, tabs ⌘1–4)│
StatusBar(26)  cursor pt · counts · open time · diagnostics · UNITS · veraPDF
```

So the refactor is a replacement of the dock, not a reskin:

| Old | New |
|-----|-----|
| `dock.rs`, `tab.rs`, `egui_tiles` dependency | `shell/` (title_bar, rail, status_bar, layout) — `egui_tiles` removed |
| `panels.rs` (all tab bodies) | `navigator/{tags,content,pages,objects,validation}.rs` |
| `inspector.rs` (one generic grid) | `inspector/{mod,header,sections,popovers}.rs` built from components |
| `view_bar.rs`, `modes.rs` | `toolbar.rs` (canvas toolbar) + `modes.rs` (kept: render requests) |
| `canvas.rs`, `rulers.rs`, `overlay.rs`, `measure.rs` | `center/canvas/*` (same logic, new chrome: legend cards, labels, floating stepper) |
| — | `center/object_view.rs` (Decoded / Raw bytes, Explain, history) |
| validation UI inside a tab | `navigator/validation.rs` + `drawer/validation.rs` + inspector "Why/How" |
| — | `drawer/{hit_stack,diagnostics,validation,operators,references,paint_order}.rs` |
| — | `states/{empty,opening,encrypted,broken}.rs` |
| ad-hoc styling | `theme/{tokens,fonts,style}.rs` (tokens from PNG 00; Inter + JetBrains Mono, OFL) |
| ad-hoc widgets | `components/{tree_row,prop_row,tool_button,chip,crumb,segmented,popover,section,badge,icon}.rs` |
| `state.rs` (one big struct) | `state/{mod,selection,nav,history,drawer,canvas}.rs` — one sub-struct per area so milestone work does not collide |

`tree_view.rs` (virtualised tree) is kept and re-skinned through `TreeRow`; `units.rs`,
`geometry.rs`, `page_cache.rs`, `selection.rs`, `open.rs`, `validation` worker logic are kept.

Icons: `egui-lucide` 0.2 (MIT AND ISC, egui 0.37) — the design uses Lucide glyphs.
Fonts: Inter (UI) and JetBrains Mono (values) embedded from `assets/fonts` with their OFL
licences, pinned by SHA-256.

## 2. Data the UI needs (spec list) vs core today

| # | Need | Core today | Action |
|---|------|------------|--------|
| 1 | Plain-language note per operator | none | `core::explain::describe(op, operands) -> String` static table, all ISO 32000-2 operators |
| 2 | CTM decomposition + units | CTM is a `[f64;6]` in `GState` | `core::geom::Decomposed { scale_x, scale_y, rotation_deg, skew_deg, uniform, ratio }` + `Matrix` helpers |
| 3 | Glyph metrics (code, Unicode via ToUnicode, ActualText, width/advance, ascent/descent/bbox, embedding/subset) | partly in Glyph props | structured `GlyphInfo` API: `Document::glyph(&NodeId) -> Option<GlyphInfo>` |
| 4 | Coverage per page (tagged / artifact / untagged) | not exposed | `Document::coverage(page) -> Coverage { tagged, artifact, untagged }` (counts of painted items + op ranges) |
| 5 | Object used-by, resources-it-uses, last-changed revision | referrers yes (props); not structured | `Document::object_refs(r) -> ObjectRefs { used_by, uses: Vec<(name, ObjRef, kind)> }`, `Document::object_revision(r)`, `changed_in_later_revision` |
| 6 | Paint order: step, covered-later-by, visibility % (bbox approx) | items only | `Document::paint_step(page, op) -> PaintStep { step, total, paints_over, covered_by, visible_pct (approx) }` |
| 7 | Validation why / how-to-fix per rule | none | `pdfxray-verapdf::explain(rule) -> Option<RuleText { title, why, how }>` (our words; PDF/UA-1/2 + WCAG common rules) |
| 8 | Page boxes provenance, box-outside-media, print figures | provenance + diagnostic yes | `PageInfo::print_figures() -> PrintFigures { bleed_each_side, trim_size, safe_area }` |
| 9 | Opening progress checklist + cancel | `open` is one call | `Document::open_with_progress(path, &mut dyn FnMut(Stage, done, total), cancel)` (stages: structure, objects, page tree, content, structure tree) |
| 10 | Encrypt dict summary (filter, V/R, algorithm, permissions, user-password-empty?) | dict only | `Document::encryption() -> Option<EncryptionInfo>` (no decryption) |
| 11 | Revisions timeline + per-revision object list | revisions in raw | `Document::revisions() -> Vec<RevisionInfo { index, offset, objects_changed }>` |
| 12 | Diagnostic plain-language explanation + byte range | code/message/span | `DiagnosticInfo::explain() -> &str` table keyed by code |

All added to core/verapdf **first, with tests**, before the screens that use them.

## 3. Milestones (each ends with the full gate + snapshots reviewed against the PNGs)

| M | Scope | Parallel |
|---|-------|----------|
| R0 | Core data 1–12 above | with R1 |
| R1 | Theme + tokens + fonts + icons; components; new shell (title bar, rail, navigator host, center host, inspector host, drawer host, status bar); global units; layer-mode 1–6; Esc chain; remove `egui_tiles`, `dock.rs`, `tab.rs`, `panels.rs`; port existing features into the new hosts so nothing regresses | with R0 |
| R2 | Tags + Content: structure tree + filter + RoleMap footer; content tree + coverage bar; Artifacts filter; Hit stack drawer; Operators drawer; tree hover preview / dbl-click zoom / arrow keys; canvas hover outline, alt-click cycle; inspector sections (Geometry + CTM popover, Graphics state + clip tooltip + swatches, Glyph, Font, Text state, Structure); breadcrumb crumbs | after R1 |
| R3 | Objects: Document / Resources / All objects navigator; ObjectView (Decoded default with Explain, Raw bytes hex with range highlight), header chips (filter/size, changed in rev, used as), back/forward history ⌘[ ⌘]; References drawer; diagnostic cards with "Show bytes"; rev chip popover | parallel with R2 (own files) |
| R4 | Measure + Pages/Boxes: thumbnails, 5 box rows with eye toggles + provenance, transform section; box legend card; measure labels + crosshair readout; A/B gap + alignment + print boxes inspector | after R2 |
| R5 | Validation: rail entry, profile picker + counters, findings grouped by rule, canvas fail badges + "On this page" card, drawer table, inspector Why/How/Where, Previous / Next finding (⌘]) | parallel with R4 |
| R6 | Paint order: drawer stepper (play/pause, step, shift-jump to visible change, skip state-only ops, scrubbable track with op cards), canvas ghosting of not-yet-painted, legend card, inspector Paint order section | after R4/R5 |
| R7 | App states: Empty (drop zone + recents persisted + Try a sample), Opening (background open with progress checklist + cancel), Encrypted (explainer + encrypt table + Browse objects / Page boxes), Broken (damage banner + Diagnostics drawer) | after R6 |

## 4. Known egui limits to flag (will be reported, not hacked around)

* No real blur / drop shadows on popovers (egui has simple shadows only).
* Text rendering is egui's (no font hinting control); Inter/JetBrains Mono metrics will be close,
  not pixel-identical.
* Native traffic-light title bar: on macOS the window uses a full-size content view with a
  transparent, untitled system title bar, so the native traffic lights sit inside our 44 pt bar
  (which reserves their room and moves / zooms the window on drag / double click). macOS places
  the lights a few points higher than the design's centred ones; moving them needs AppKit calls
  through `unsafe`, which the workspace denies.

## 5. Deviations from the design file (user decisions after R7)

* **Layer mode moved out of the title bar** into the canvas toolbar as a "Pick: <mode> ▾" menu
  next to Measure. It only filters what a click on the page selects, so it lives with the page
  (hidden in the Objects view). The menu lists every mode with its key (`1`–`6`); Annotations is
  greyed out with "v1.1" (and `4` does nothing) because annotations are not hit-tested yet. The
  Hit stack's header note starts with `picking <mode>` while the mode is not All.
* **No Open button in the title bar** (the design has none): ⌘O, the drop zone and drag and drop.
* **Status bar** shows only the design's items plus a slot for real messages (validation
  progress, errors); "Loaded: …" and the render-mode summary are gone (the title bar and the
  canvas cards say the same).
* **Continuous page scroll** (user request): the canvas is one vertical scroll of every page,
  16 pt apart and centred across; only pages in view are rendered (the render thread renders
  the newest request of each visible page). The *current* page is the one filling most of the
  view after the user scrolls, or the page last clicked; the page field, the Content tree, the
  Operators drawer, the stepper, isolation, the clip and the hit stack belong to it. The page
  arrows and field scroll to a page's top. Fit fits the largest page, so the zoom does not jump
  between pages of different sizes. Page boxes, validation badges, hover and preview outlines
  are drawn on every visible page; the stepper's ghosts only on the current one.
* **Tags tree MCID / OBJR rows** (design lead request): shown as dim `↳ MCID n  pN` text rows,
  not clickable, focusable or reachable with ↑/↓; selecting marked content elsewhere
  highlights its parent tag row.
