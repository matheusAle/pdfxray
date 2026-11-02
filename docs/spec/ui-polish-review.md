# pdfxray UI polish review

A `better-interface` review of the app, run on 2026-10-31 over the uncommitted `main` checkout
(zoom, stepper and sidebar fixes, table header scope, Tag roles focus, marquee multi-selection
included). The high- and medium-priority refinements were fixed in the same pass; the
**Status** column says which. Redesigns, layout changes and product decisions are listed under
[Skipped](#skipped) and [For the designer](#for-the-designer).

## Scope and coverage

**Scope:** the whole desktop app at its one supported size (1440 × 900 window, dark only):
Tags, Content, Objects with previews, Pages, Validation, the canvas with its overlays (tag
boxes, table header scope, Tag roles card, reading order, validation badges), the marquee and
multi-selection, and the empty, opening, encrypted and broken states.

**Stack:** Rust, egui 0.37 with AccessKit (`WidgetInfo`, `Response::accessible_name`,
`labelled_by`), custom-painted components in `crates/pdfxray-app/src/components`, tokens in
`crates/pdfxray-app/src/theme/tokens.rs`, Inter and JetBrains Mono. Rendered evidence comes from
the kittest snapshots in `crates/pdfxray-app/tests/snapshots/*.png`.

**Convention documents read:** `docs/spec/ui-revamp-v2.md`, `design/exports/*.png` (boards 01–28),
`docs/user-guide.md`, the project `CLAUDE.md`. `components/focus.rs` documents the focus
convention: a control that paints itself must call `focus::ring`.

**Boundary:** no web viewport, so 320 px reflow and browser zoom do not apply; window resizing,
VoiceOver and real keyboard runs were not inspected live (the app was not launched).

| Domain | Evidence inspected | Result |
| --- | --- | --- |
| Accessibility | Every `Sense::click()` / `click_and_drag()` / `drag()` site in `src/` paired with its `focus::ring`; AccessKit names via a Tab walk of the Tags screen (`every_stop_of_the_tab_key_has_a_name`); text fields; drag-only regions | 4 findings |
| Layout | Snapshots of every main screen; inspector section and sub-header edges; empty states; truncation (crumbs, tree details, defaults line) | 2 findings |
| Writing | Inventory of error, empty-state, hint and status strings in `src/`; terms (“Open file…”, “Objects”, “Content”) | 4 findings |
| Typography | Punctuation in rendered strings (apostrophes, ellipses, ranges); mono and UI scales in `tokens.rs`; wrapping of empty states | 1 finding |
| Colors | Measured token pairs (WCAG 2.x relative luminance) on PANEL, SURFACE, DRAWER, popovers and selected rows | 1 finding (decision pending, not changed) |
| UI polish | Focus, hover and selected states of the custom components; ring colour on the white page | Folded into the accessibility findings |

## Findings

Ranked by severity, then by reach. **Status**: Fixed, Skipped (see below) or Open (LOW, left as work).

| # | Severity | Domain | Location | Before | After | Why | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | HIGH | Accessibility | `components/section.rs:77`, `components/crumb.rs:149`, `components/prop_row.rs:249`, `components/step_track.rs:93`, `components/tree_row.rs:236` (chevron), `inspector/defaults.rs:66`, `inspector/paint_order.rs:242`, `inspector/reading_order.rs:208`, `inspector/table.rs:200`, `navigator/pages.rs:337,411,412`, `navigator/validation.rs:612,677,724`, `drawer/paint_order.rs:299,350`, `center/object_view/ops.rs:160`, `center/object_view/decoded.rs:230`, `states/opening.rs:210`, canvas: `tag_overlay.rs:440,613`, `connector.rs:161`, `table_overlay.rs:324`, `validation_badges.rs:52` | 25 custom controls sense `Sense::click()` (Tab-focusable) but never call `focus::ring`: a keyboard user cannot see where focus is | `focus::ring(ui, &response, rect, radius)` after the fill, as `focus.rs` prescribes; on the page `focus::page_ring` (accent ring 2 pt off the chip plus a new `FOCUS_HALO` token, since the accent is 1.7:1 on white) | WCAG 2.4.7 Focus Visible; trigger “keyboard-reachable control with no visible focus indicator” | Fixed |
| 2 | HIGH | Accessibility | `components/prop_row.rs:246` | Link and clickable values (`/H1 12 0 R`, `Page 1`, header-cell links) sensed `Sense::CLICK`, which egui does not make focusable: a pointer-only path | `Sense::click()` on the value’s text, with a ring; `Enter`/`Space` follow it | WCAG 2.1.1 Keyboard; trigger “pointer but not keyboard” | Fixed |
| 3 | HIGH | Accessibility | `navigator/mod.rs:200`, `center/toolbar.rs:201`, `navigator/validation.rs` (path field) | The navigator filter and the page field are Tab stops with no name (hint text only); the veraPDF path field’s visible label was not tied to it | `.accessible_name("Filter")`, `.accessible_name("Page number")`, `.labelled_by(label.id)` | WCAG 4.1.2 Name, Role, Value; trigger “control with no accessible name” | Fixed |
| 4 | HIGH | Accessibility | `shell/title_bar.rs:87`, `center/canvas/page.rs:117` | The window-drag strip (`click_and_drag()`) and the marquee region (`drag()`) were focusable, unnamed Tab stops | `Sense::CLICK \| Sense::DRAG` and `Sense::DRAG`: pointer gestures, not stops (Delete) | WCAG 2.4.3 Focus Order, 4.1.2; unnamed stop | Fixed |
| 5 | HIGH | Writing | `commands.rs:131`, `validation.rs:451`, `drawer/diagnostics.rs:201`, `center/canvas/mod.rs:64` | “Cannot open X: it is no longer at P” · “Installing veraPDF failed: …” · “Rendering engine: Encrypted” (a Debug dump) · “The rendering engine could not read this document.” | “… Find it with Open file… (⌘O).” · “… Try again, or enter the path of a veraPDF you installed yourself.” · the canvas sentence via `engine_message` · “… Its objects are still in the Objects view.” | Trigger “error that names no way to recover” | Fixed |
| 6 | MEDIUM | Writing | `navigator/mod.rs:39`, `navigator/objects.rs` (`Line::ClearFilter`), `navigator/pages.rs`, `navigator/validation.rs` | “No rows/objects/pages/findings match “x”.” with no exit (the field has no clear control) | Same text plus a **Clear filter** button (one shared `no_match` helper) | A filtered empty state names the query and offers an exit | Fixed |
| 7 | MEDIUM | Writing | `center/canvas/mod.rs:59` | “Encrypted: not rendered in v1” (release jargon; the encrypted card says “in this version”) | “This file is encrypted: pdfxray can’t draw its pages in this version.” | One voice, one vocabulary | Fixed |
| 8 | MEDIUM | Writing | `navigator/tags.rs:21` | “This file has no structure tree.” | “…, so screen readers have no tags to follow. Content lists what each page draws.” | Empty states point forward | Fixed |
| 9 | MEDIUM | Typography | 22 strings in `src/` (`states/broken.rs`, `states/opening.rs`, `states/encrypted.rs`, `modes.rs`, `action_bar.rs`, `table_overlay.rs`, `object_preview/mod.rs`, `object_facts/naming.rs`, …) and 22 in `pdfxray-core` explanation catalogues; `center/canvas/mod.rs:80`; `navigator/validation.rs:698` | Straight `'` beside the project’s curly quotes; “Rendering...”; “+4 more p2-p5” | `’`, “Rendering…”, “p2–p5” (`more_text`) | Typographic punctuation, applied consistently | Fixed |
| 10 | MEDIUM | Colors | `theme/tokens.rs` `TEXT_3` (195 uses in 61 files) | `#6B7380`: 3.89:1 on PANEL, 3.83:1 on SURFACE, 3.96:1 on DRAWER, 3.50:1 on popovers, 3.11:1 on a selected row; chevrons at 70 % reach 2.52:1 | A decision: lighten `TEXT_3`, or keep it for non-essential text only | WCAG 1.4.3 (4.5:1 for 10–12 px text), 1.4.11 for chevrons | Skipped (user decision) |
| 11 | MEDIUM | Accessibility | `components/prop_row.rs:333` | The per-row copy icon exists only while the row is hovered, so Tab never reaches it | A keyboard path to copy a value (designer/product) | WCAG 2.1.1 | Skipped (designer) |
| 12 | LOW | Accessibility / UI | `components/crumb.rs`, `inspector/table.rs`, `inspector/paint_order.rs`, `drawer/paint_order.rs` | On an already selected item (current crumb, selected cell, “this op”, current card) the focus ring is the same 1 px accent as the selection outline | A distinct focus style (offset or 2 px) | Focus on a selected item is hard to tell | Skipped (designer) |
| 13 | LOW | Layout | `inspector/table.rs:300,352` | “NAMED BY /HEADERS” and “HEADERS OF THE CELL” start on the key edge, 12 px left of every section header’s text | Align them to the section headers’ text edge, store them in natural case | Shared edges | Open |
| 14 | LOW | Writing | `center/canvas/role_legend.rs`, `roles.rs:37` | “Table…” in the Tag roles card reads as a cut word and is spoken as “Table…, 0 elements” | Boards disagree (12: “Table…”, 27: “Table”) | Consistency | Skipped (designer) |

## Skipped

- **`TEXT_3` contrast (finding 10).** Not changed: the decision is the user’s. Instances that
  carry information at 10–12 px: property keys (`components/prop_row.rs:188`), section headers
  (`theme/fonts.rs:182`), tree-row details and annotations (`components/tree_row.rs:292,332`),
  navigator empty states (`navigator/mod.rs`, `objects.rs`), drawer hints such as “sorted by
  severity” (`drawer/*`), the status bar (“file info”, cursor), `ROLE_OTHER`, page-number
  captions in Pages, the multi-selection footnote, preview captions. Worst pairs: popovers
  (3.50:1) and selected rows (3.11:1).
- **The copy icon (finding 11)**, **the focus style on selected items (finding 12)** and
  **“Table…” (finding 14)**: see the designer’s list.
- **Finding 13** is LOW and was left open.

## For the designer

Board-level items, none changed in code:

1. **Copy without a pointer** (all inspector boards): the copy icon only exists on hover. Decide a
   keyboard path, such as a permanently focusable icon on the focused row or a Copy item on the
   value’s menu reachable with the keyboard.
2. **Focus on a selected item** (11 Components, 13 Foundations): define a focus style that
   differs from the 1 px accent selection outline (current crumb, selected table cell, “this op”,
   current paint card).
3. **Page-control focus ring** (12, 26, 27): the code now draws an accent ring 2 pt off a chip
   or badge on the page with a dark `#0A0C0F` 80 % halo (`FOCUS_HALO`) so it reads on white;
   please confirm or redraw it.
4. **“Table…” in the Tag roles card**: board 12 says “Table…”, board 27 says “Table”.
5. **Not embedded font card** (23 Objects · Preview cards): about 70 px of empty card under the
   fallback specimen in the centre preview.
6. **Multi-selection “By role” rows** (28): the count and “Only P” sit at the far right, 200 px
   from the role chip.
7. **Decoded object view** (03, 23): long arrays such as `/Widths [600 600 …` run off the right
   edge with no wrap and no cue that more is there.
8. **Validation summary tiles** (05): the amber “3 rules” tile has no verb (failed? checked?).
9. **Table inspector sub-headers** (finding 13): confirm their edge before it is aligned.

## Verification

Passed:

- New behaviour tests, each confirmed to fail with its fix reverted:
  `polish_panels::a_header_a_crumb_and_a_link_value_show_the_ring_only_while_the_keyboard_is_on_them`,
  `a_link_value_is_reached_and_followed_with_the_keyboard`, `every_stop_of_the_tab_key_has_a_name`,
  `a_filter_that_matches_nothing_offers_the_way_back`;
  `navigator::validation::tests::the_more_row_names_a_page_range_with_an_en_dash`;
  `windows::open_recent_follows_the_window_rules_and_clear_menu_empties_the_list`,
  `e2e::empty_trees_say_so_and_populated_ones_list_their_nodes`,
  `e2e::an_encrypted_document_lists_its_pages_but_the_canvas_says_it_is_not_rendered`,
  `validation::install::a_failed_install_says_why_and_offers_it_again` (updated).
- Contrast: computed from the token values with the WCAG relative-luminance formula.
- 13 snapshots changed, each compared with its previous version (only the edited strings moved):
  `app_inspector_page`, `app_measure_active`, `app_minimal_opened`, `app_page_boxes_defaults`,
  `app_page_boxes_overlay`, `app_xray_graphics_state` and `app_state_broken` (the Tags empty
  state; broken also the banner’s apostrophes), `app_objects_missing_font_preview` (“file’s
  font”), `app_objects_raw_bytes`, `app_objects_stream_decoded`, `app_rev_popover` (“Page 1’s
  drawing instructions”), `app_state_encrypted` (“can’t”), `app_state_opening` (“they’re”).
- Gate (all with `CARGO_INCREMENTAL=0`): `cargo fmt --all` (then `--check` clean);
  `cargo clippy --workspace --all-targets` and `--all-features`: 0 warnings each;
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`: clean; `cargo machete`: no
  unused dependencies; `cargo test --workspace --no-fail-fast`: 1718 passed, 0 failed,
  8 ignored; `cargo xtask bundle`: `target/bundle/pdfxray.app` signed and valid,
  `pdfxray-0.1.0-macos-aarch64.zip`.

Not verified:

- VoiceOver output and a full keyboard walk of every screen in the running app (not launched).
- Truncated tree-row details rely on egui’s elided-label tooltip; not exercised.

## Verdict

`Approve`: the HIGH findings are fixed. MEDIUM finding 10 (`TEXT_3`) waits for the user,
finding 11 and the LOW findings wait for the designer. All six domains were inspected; none is
`Not reviewed`.
