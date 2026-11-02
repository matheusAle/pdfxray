# Focus you can see, copy without a pointer, long values with a cue

Implements the designer's answers to [the UI polish review](ui-polish-review.md) ("For the
designer" 1–9, findings 11–14), board **2.13 Spec** (`design/exports/29-spec-focus-copy-long-values.png`)
and the re-exported boards 05, 12, 23, 27 and 28. `TEXT_3` is not touched (the user's decision).

Status: **done** (2026-10-31). The plan was written first; each section below says what was
actually done, and where it differs from the plan, why.

## 1. Copy without a pointer

`components/prop_row.rs`, `components/context_menu.rs`, new `components/toast.rs`.

- The **row** is the Tab stop (`Sense::click()` on the row). The copy icon and the text of a link
  are pointer targets only (`Sense::CLICK`), so a link row is one stop, not two. The row is
  announced as `Key: value` (the value cut after 80 characters in the name, never in the copy);
  a link row has the `Link` role, any other row `ListItem`; a row without a key is named by its
  value.
- Focused look: the hover wash, the copy icon and the inset focus ring (§2). The row keeps that
  look while its value menu is open. At rest the icon stays hover-only.
- Keys on the focused row: `⌘C` (macOS sends it as egui's `Event::Copy`; a `C` key event with ⌘ is
  accepted too) copies the value, `⇧⌘C` copies `Key: value`, `Enter` (egui's keyboard click, and
  AccessKit's click) follows a link. The modifiers are read from the key event itself.
- **Value menu**: `⇧F10`, the context-menu key or a right-click anywhere on the row opens it with
  "Copy value ⌘C" · "Copy key and value ⇧⌘C" · "Follow link ↩" (disabled, text-3, when the value
  is not a link) and quiet keycaps. Opened from the keyboard, its first line takes the focus. In
  the inspector the selection's actions follow under a separator (as before); in the Pages
  navigator the menu has the copy lines only.
- After any copy (icon, keys or menu): the toast "✓ Copied <value>" for **1.6 s**
  (`toast::TOAST_FOR`), frosted popover style, at the foot of the panel the copy came from. It is
  one AccessKit node, role `Status`, `live = polite`, named "Copied <value>". The icon's existing
  1.2 s check flip is kept.

**Refactors.** `PropOutput` gained `followed` (click on the link text, Enter on the row, or Follow
link in the menu on the frame before); all callers (`inspector/{font, graphics, paint_order,
structure, table, validation}.rs`) moved from `value.clicked()` to it. The context slot now
carries a `ValueTarget` (row response, key, copy text, link flag, toast area) and
`context_menu::attach_value` draws the menu; the menu lines share one painter (`line`) with the
node menu. The value column no longer needs its own click sensor (the row covers it).
**Dead code removed:** `PropRow::clickable` (no caller), the old `context_menu::item`,
`focus::PAGE_RING_GAP`, the per-column `wide` click response.

**Deviations.** The focused row uses the hover wash (`NEUTRAL_HOVER`, `#FFFFFF0A` over the
panel ≈ `#191C20`) rather than a second `SURFACE_2` fill: the spec says "looks like a hovered
one", and on the panel the two are the same tone within 1 step. The menu lines keep an icon
(copy, copy, ↳) so they align with the node actions under them in the same menu; the board shows
the value lines alone.

## 2. One focus style

`components/focus.rs` and every call site.

- `RING_WIDTH` 2, new `RING_GAP` 2. `ring` draws the accent **outside** the control, 2 px off,
  radius + 2 (`StrokeKind::Outside` from `rect.expand(2)`). New `row_ring` draws it 2 px **inside**
  a full-width row. `ring_rect` / `row_ring_rect` expose the geometry.
- Moved to `row_ring` (16 sites): section headers, property rows, tree rows, context and View /
  Pick menu lines, drawer table rows, inspector rows (defaults, reading order, multi-selection
  elements), decoded operator rows, validation rule groups, nodes, "more" and profile rows, page
  rows, the step track. Everything else (chips, crumbs, cells, cards, buttons, tabs, segments,
  switches, rail, toolbar, links, thumbnails, eye buttons, the fold button) keeps `ring`.

## 3. Page ring

`focus::page_ring`: the accent 2 px wide, 2 pt off the chip, with `FOCUS_HALO` (`#0A0C0F` 80 %)
2 px wide right outside it. It shares `RING_GAP` / `RING_WIDTH`.

## 4. "Table"

`roles.rs`: `class_name(Table)` is "Table"; spoken "Table, 0 elements".

## 5. Not embedded card

`components/object_preview/canvas.rs`: the frame's blocks are measured first; the frame is as
tall as its content plus 16 px under the caption and centred vertically in the preview body
(centre and inspector cards).

## 6. By role rows

`inspector/multi.rs`: tag · count · "Only P" left to right, `ROLE_GROUP_GAP` (10 px) apart
(the frameless button keeps no side padding, so the gaps are between what is drawn).

## 7. Long values

New `components/long_value.rs` (pure `wrap` + `fold_line` painter), used by `PropRow` and the
decoded object view.

- Kinds: array `[…]` (breaks between top-level items; `n g R`, nested arrays, strings and
  dictionaries are one item each), literal string `(…)` (between words, inside an over-long
  word), hex string `<…>` (anywhere), anything else (between words, never folds).
- Hanging indent: the width of the opening bracket. Past `FOLD_AFTER` (3) lines: the
  "⌄ 194 more values" button (surface-2, 1 px `FOLD_HAIRLINE` `#FFFFFF14`, `RADIUS_FOLD` 5,
  Inter 11, focusable, ring outside), then the closer and the mono-10 text-3 count ("224
  numbers"; "references", "names", "strings"… when all items agree, else "items"). Strings say
  "more characters" / "N characters", hex strings "more bytes" / "N bytes".
- Enter or a click unfolds in place (kept per row id). In the inspector the row keeps the focus
  after a keyboard unfold. The painted lines carry one `Label` node with the whole value.
- Decoded view (`center/object_view/decoded.rs`): the list moved from fixed-height `show_rows` to
  measured rows in `show_viewport` (only rows in view are drawn, same 28 px pitch); a wrapped
  entry is one numbered line, continuation lines 20 px, references stay underlined links.
- **Refactor in `pdfxray-core`** (`objects.rs`): object value rows were cut at 160 characters with
  "…"; they are now whole, so Copy value copies the whole array. The one-line summaries of tree
  rows and the encryption dictionary keep the cut.

## 8. Validation tiles

`navigator/validation.rs`: "checks failed" · "rules failed" · "checks passed" (singular for 1).
To keep "checks passed" on one line, the tiles are 4 px apart (was 6) with 8 px left / 2 px right
padding, as on board 05; a caption wraps rather than overflow in a narrower panel.

## 9. Table inspector sub-headers

`components/section.rs` gained `TEXT_X` (20, shared with the header) and `sub_header`;
`inspector/table.rs` uses it with `NAMED_BY` "Named by /Headers" and `HEADERS_OF_THE_CELL`
"Headers of the cell": Inter 11/600 in text-2, on the section headers' text edge.

## Tokens added

`tokens::FOLD_HAIRLINE`, `tokens::RADIUS_FOLD`, `focus::RING_GAP`, `toast::TOAST_FOR`,
`long_value::{FOLD_AFTER, FOLD_LINE_H}`, `multi::ROLE_GROUP_GAP`, `section::TEXT_X`.
`FOCUS_HALO`'s doc now says 2 px.

## Tests

Each new test was checked to fail with its behaviour reverted (three batches of reverts, then the
files restored).

| Behaviour | Test |
| --- | --- |
| Row as Tab stop, focus look, icon not a stop | `components::prop_rows::the_row_is_the_tab_stop_and_looks_hovered_with_an_inset_ring_and_its_icon` |
| ⌘C, `Event::Copy`, ⇧⌘C | `…::command_c_copies_the_exact_value_and_shift_command_c_the_key_with_it` |
| Enter follows a link only | `…::enter_on_a_link_row_follows_the_link_and_on_a_plain_row_does_nothing`, `polish_panels::a_link_value_is_reached_and_followed_with_the_keyboard` |
| Menu from ⇧F10, first line focused, Follow link disabled and dimmed | `…::shift_f10_opens_the_value_menu_on_its_first_line_with_follow_link_off_for_a_plain_value` |
| Menu from right-click and the menu key, Follow link follows | `…::a_right_click_or_the_menu_key_opens_the_value_menu_and_follow_link_follows` |
| Toast text, 1.6 s, live region | `…::a_copy_shows_the_toast_for_1_6_s_and_announces_it`, `toast::tests::…` |
| Ring outside with a gap vs inset, 2 px | `polish_panels::a_control_ring_stands_two_points_off_it_and_a_row_ring_sits_two_points_inside`, `focus::tests::…` |
| Page ring and halo | `polish_panels::a_control_on_the_page_has_the_ring_two_points_off_with_a_two_point_halo_around_it` |
| "Table" | `roles::tests::the_legend_names_tables_without_an_ellipsis`, `tag_overlay` (updated) |
| Not embedded frame | `object_previews::the_not_embedded_frame_hugs_what_it_holds_and_sits_in_the_middle_of_the_canvas` |
| By role grouping | `marquee::a_by_role_row_reads_tag_count_and_only_as_one_group_from_the_tag` |
| Wrap, indent, fold, count, unfold by click | `…::a_long_array_wraps_under_its_first_item_and_folds_past_three_lines`, 10 `long_value::tests` |
| Unfold by Enter, Copy value whole | `…::enter_unfolds_a_long_array_and_copy_value_copies_all_of_it`, `object_previews::a_long_array_of_the_decoded_view_wraps_folds_and_unfolds_in_place`, `pdfxray-core objects::tests::a_long_value_is_kept_whole_and_only_the_tree_summary_is_cut` |
| Tile wording | `navigator::validation::tests::each_tile_names_what_it_counts`, `validation::findings`, `validation::navigation` (updated) |
| Sub-headers | `table_overlay::the_sub_headers_of_the_table_section_are_in_natural_case_on_the_section_headers_text_edge` |

Updated for the new names (`Key: value` on link rows, clicks on the link text): `e2e`,
`table_overlay`, `paint_order`, `polish_panels`, `components::prop_rows`.

## Snapshots

New: `prop_rows_value_menu`, `prop_rows_copied_toast` (board 2.13: focused row, value menu,
toast, folded `/Widths`). Updated after comparing with the boards: `app_multi_selection` (28, By
role), `app_validation_findings` (05, tiles), `app_table_header_scope`,
`app_table_header_scope_ids`, `app_table_overlay` (sub-headers), `app_tag_roles_focus` (12/27,
"Table"), `app_objects_missing_font_preview` (23, frame), `app_objects_font_preview` (the
`/Widths` array now wraps; the dictionary scrolls).

## Not done

- `docs/spec/ui-polish-review.md` is left as written; this file answers its designer list.
- `navigator::thousands` and `object_facts::thousands` are the same function (found while
  reusing one); merging them is outside this change.
- VoiceOver output and a keyboard walk of the running app were not checked (the app was not
  launched).
