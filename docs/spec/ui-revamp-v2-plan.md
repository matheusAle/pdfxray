# UI revamp v2: implementation plan

**Status (2026-10-29):** V0, V0.1, V1–V5, V7 and V8 implemented; V6 polish done (dense code lines, one-line breadcrumbs, no action bar over validation badges, tidier table overlay); V9 (pdf.js oracle, transcript corpus) not started. Not committed.
**Changed after V5 (2026-10-30, user decision):** the "Edit tag colours…" editor and its saved overrides are removed; every role keeps the design's colour (`roles::role_color`, `pdfxray_render::ROLE_COLORS`). The navigator and the inspector are resizable (drag their edge; minimums 240 and 280 pt, the canvas keeps 360 pt).

Inputs: `docs/spec/ui-revamp-v2.md`, `design/exports/14-premium-low-burden-spec.png` (rules),
`01-tags.png` (reference screen), `12-tags-overlay-reading-order.png` + spec §7/§9 in `00-…png`,
`11-components.png`, `13-foundations.png`.

Rules as before: refactors are part of the plan; dead-code check and the full gate (fmt, clippy
`-D warnings` with and without `pdfium`, all tests incl. veraPDF/PDFium, `cargo deny`,
`cargo machete`) after every milestone; behavioural kittest tests with a smoke expectation;
snapshots regenerated on purpose and compared with the PNGs. No commits, no co-author trailer.

## Milestones

| M | Scope | Depends on |
|---|-------|-----------|
| V0 | **Core.** `RoleClass` (heading, p, figure, caption, table, list, link, inline, group, artifact, other) resolved through the RoleMap and PDF 2.0 namespaces; tag boxes per page (every StructElem with a box on the page, its depth and class); reading order per page from the structure tree (document order of elements that own content on the page) and from paint order (first painted op); backtrack detection (a step that starts above the previous one); new diagnostic `reading-order-backtrack`; a short text preview per StructElem. CLI `pdfxray reading-order <file> <page> [--from structure|paint]`. | — |
| V1 | **Theme, effects, motion.** Tokens: surface tones (canvas `#0A0C0F`, panels `#101317`, toolbar `#0C0F12`, drawer `#0E1115`, rail/status `#0B0D10`), neutral-active fill `#FFFFFF14`, hairlines, role colours, frosted surfaces. New `theme/effects.rs` (frosted panel, soft shadows, two-layer page shadow, faint grid, gradient primary button) and `theme/motion.rs` (eased, interruptible animations keyed by id; honours Reduce motion read from macOS `com.apple.universalaccess reduceMotion` + `PDFXRAY_REDUCE_MOTION`). Typography: tracked titles (egui `extra_letter_spacing`), 10 px +0.9 section headers, 17 px wrapping inspector title, tabular numerals. Components: `Keycap`; ToolButton/PickButton/rail neutral active; rows 28 px; Section without dividers; popovers/tooltips frosted with fade/scale. | — (parallel with V0) |
| V2 | **Shell.** Merged title chip `PDF 2.0 · UA-2 · rev 3/3 ▾` → file summary + revisions popover (replaces version/tag/UA/rev chips); translucent title bar; status bar = cursor · file info (hover: counts, open time, veraPDF) · units · problems. | V1 |
| V3 | **Toolbar + canvas.** One **View ▾** control replaces the six tool buttons (menu = modes with keycaps + §7 options: colour by, show all tag boxes ⇧T / labels L / reading order R / nesting / artifacts, reading order from); floating cards anchored top-right 14 px in, never over a page, collapsing to chips (X-ray legend → "4 kinds ▾"); X-ray outlines at 50 % (full on hover/selection); page shadow + grid; selection action bar (Isolate · Hide · Show in tree · Object · Copy id) + the same actions on right-click (canvas, tree, values); motion hooks (selection grows in, zoom/jump ease, paint-step fade). | V1 |
| V4 | **Inspector + trees + drawer.** Inspector: summary strip, problem-first card with Details, "+ n default values", mode-irrelevant sections start collapsed, footer removed (actions moved to V3 bar/right-click; Validation keeps its nav bar), value-change flash. Tags tree: text previews, refs only on selected/hovered row, problem-only badges, dimmed chevrons, RoleMap as hover, RoleMap footer removed. Drawer: 34 px strip by default; expands on page click / validation finished / tab click; open by default in Content, Objects, Validation, Paint order, Broken; count badges coloured only for warnings/errors. | V1 (V3 for the moved actions) |
| V5 | **Tag overlay + reading order.** Role colours shared by tree dots, canvas boxes and legend; "Edit tag colours…" with persisted overrides (data dir, next to recents); all-tag boxes, tag label chips, nesting, artifacts on the canvas; reading-order badges + 2 px path drawn 1→n (300 ms), backtracks amber with inline note; Tag roles legend card (counts, zero dimmed); Inspector "Page n · reading order" list + "vs paint order"; diagnostic listed in Diagnostics. | V0, V3, V4 |
| V0.1 | **Core fixes from the research** (`docs/research/screen-reader-reading-order.md`): steps classified First / Forward / ColumnBreak / Detour / Backtrack, the diagnostic only for true backtracks (no false alarms on columns, footnotes, sidebars); `text_preview` in announcement order (Alt → ActualText → painted text). Fixtures `reading-order-columns`, `reading-order-footnote`. | V0 |
| V6 | Snapshot pass against every PNG, docs (user guide, this plan's status), final gate twice. | all |
| V7 | **Speech order + checks + tables (core, approved).** `Document::speech_order` — the document-wide announcement sequence built with the documented rules (artifacts/Private skipped, Alt/ActualText/E substituted and Alt hiding children, OBJR placed at their tree position, table headers resolved, /Lang changes marked), each item keeping its NodeId. **Reader presets** as labelled approximations where the research shows reliable differences: Acrobat on Windows (NVDA/JAWS), Firefox pdf.js, Chrome/Edge default (content order, tags ignored), plus "Structure (spec)" as the neutral default. Cheap checks: `alt-hides-content`, `figure-alt-missing`, `alt-and-actualtext`, `actualtext-on-structure`, `rolemap-multi-hop`, `private-hides-content`. **Table model**: grid from TR/TH/TD with RowSpan/ColSpan, header association via `/Headers`+`/ID`, then `/Scope`, then position — PDF 2.0's header-finding algorithm *and* the PDF 1.x rules (no namespaces, TH by position/Scope); table checks mapped to Matterhorn 15-001…15-005 / PDF/UA-2. CLI `speech`, `table`. One fixture per edge case. | V0.1 |
| V8 | **Speech UI (approved).** "Speech" drawer tab: transcript linked to the canvas (hover/click item ↔ outline), "show skipped" toggle, "approximation" label, reader preset picker; canvas path/badges/Inspector list can use speech order as a source; table header-association overlay + Inspector mini grid; footnote reference → note connectors. | V5, V7 |
| V9 | **Oracle + corpus (good to have).** ~20 generated edge-case fixtures with exact transcript snapshots; pinned pdf.js run in headless Firefox as a nightly reference (xtask, never a runtime dependency); PDFium structure-tree cross-check. | V7 |

## Colour and type audit (user request, folded into V1/V3/V4)

* V1: every token against the design (13-foundations, v2 spec), egui visuals mapped onto the
  tokens (panel/window/faint/extreme fills, widget states, selection) so nothing renders a step
  lighter; Inter Bold and JetBrains Mono SemiBold/Bold added (OFL, pinned in
  `assets/fonts/SOURCES.md`) with `bold()` / `mono_semibold()` / `mono_bold()`; tracking on titles
  and section headers. Tables in `docs/spec/ui-revamp-v2-audit.md`.
* V3/V4: page-box colours to the design legend (Media `#F87171`, Crop `#FBBF24`, Bleed `#4ADE80`,
  Trim `#5AA9FF`, Art `#B48CFF`); every hard-coded UI `Color32` moved to tokens (page pixels and
  thumbnails exempt); call sites that bypass the theme helpers (`.small()`, bare `RichText`
  headings) switched to them; heavy weights applied where the design uses them.
* V6: snapshots compared with the PNGs at 1× and 2× (text gamma / feathering noted, not hacked).

## Refactors (part of the milestones above)

* `theme/` gains `effects.rs` and `motion.rs`; ad-hoc `Stroke` borders between areas go (tone separates).
* `center/toolbar.rs`: the six `ToolButton`s and `TOOLS`/`labelled_width` become one `view_menu.rs` (new) —
  the old per-tool buttons are deleted, their keys stay in `shell/keys.rs`.
* `inspector/footer.rs` deleted; its actions move to `center/canvas/action_bar.rs` (new) and a shared
  `components/context_menu.rs` (new) used by tree rows, the canvas and prop rows.
* `shell/title_bar.rs`: chip cluster → one `title_chip` + `file_summary` popover; `status_bar.rs` trimmed.
* `drawer/mod.rs`: open/collapsed state per mode (`state/drawer.rs`), strip rendering.
* `navigator/tags.rs` RoleMap footer removed; `tree_row` gains preview text + hover-only ref.
* `center/canvas/cards.rs`: anchoring/collapse logic shared by all cards; legend becomes a chip.
* New `center/canvas/tag_overlay.rs` (boxes, labels, reading order) fed by core V0.
* Dead after V3/V4: separate tool buttons, `XRAY_LABEL`… constants only used by them, inspector footer,
  RoleMap footer, the old chip cluster helpers. Checked with clippy/machete and a grep pass.

## Known egui limits (fallbacks, reported)

* No background blur: frosted surfaces are solid tinted fills at the given alpha with a hairline.
* Shadows: egui `Shadow` (offset/blur/spread) — close to the CSS values, single layer per call (two calls for the page).
* Tabular numerals: egui cannot enable OpenType `tnum`; numbers that must align use JetBrains Mono (already tabular).
* Gradients: drawn as a vertex-coloured mesh (vertical only).
