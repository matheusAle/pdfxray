# UI revamp v2: token and type audit (V1)

Design values come from `design/exports/13-foundations.png`, `14-premium-low-burden-spec.png`,
`11-components.png` and `ui-revamp-v2.md`. Where the foundations sheet (older) and the premium
rules disagree, the premium rules win and the row says so. "Fixed" is about V1's own files
(`theme/*`, `components/*`, `shell/layout.rs`, `shell/rail.rs`, `shell/mod.rs` separator); anything
in another area is **pending V3/V4** (the milestone that restyles that area).

## Colours and sizes

| Token | Design value | Implemented value | Fixed? |
|---|---|---|---|
| `BG` | `#0B0D10` (foundations "bg") | `#0B0D10` | yes (kept for window and wells) |
| `SURFACE` | `#121519` | `#121519` | yes (no longer used as a panel fill) |
| `SURFACE_2` | `#181C21` | `#181C21` | yes |
| `SURFACE_3` | `#20252C` | `#20252C` | yes |
| `BORDER` | `#252A31` | `#252A31` | yes (kept for controls; not between areas) |
| `BORDER_STRONG` | `#323842` | `#323842` | yes |
| `TEXT` | `#E7EAEE` | `#E7EAEE` | yes |
| `TEXT_2` | `#A2AAB6` | `#A2AAB6` | yes |
| `TEXT_3` | `#6B7380` (reads `#687380` at export resolution) | `#6B7380` | yes (re-check against the .pen) |
| `ACCENT` | `#3DD6F5` | `#3DD6F5` | yes |
| `ACCENT_DIM` | `#3DD6F51F` | `#3DD6F51F` | yes |
| `KIND_TAG` / `KIND_CONTENT` / `KIND_ARTIFACT` / `KIND_ANNOT` | `#B48CFF` `#5AA9FF` `#8A93A3` `#FFB547` | same | yes |
| `OK` / `WARN` / `ERR` | `#4ADE80` `#FBBF24` `#F87171` | same | yes |
| `PINK` (second selection, measure) | not in the foundations sheet | `#FF50C8` | yes (kept from v1) |
| `CANVAS` | canvas `#0A0C0F` | `#0A0C0F` | yes, applied in `shell/layout.rs` |
| `PANEL` | side panels `#101317` | `#101317` | yes, navigator and inspector |
| `TOOLBAR_BG` | toolbar `#0C0F12` | `#0C0F12` | yes, the canvas toolbar panel (`center/canvas/mod.rs`) |
| `DRAWER_BG` | drawer `#0E1115` | `#0E1115` | yes, drawer panel in `layout.rs` |
| `RAIL_BG` / `STATUS_BG` | rail and status `#0B0D10` | `#0B0D10` | yes; the status bar panel uses `STATUS_BG` (V2) |
| `TITLE_BG` | `#0F1216` at 85 %, 24 px blur | `#0F1216` at 85 % (`D9`), no blur | yes; blur is impossible in egui (translucent fill instead); the title bar panel uses it (V2) |
| `NEUTRAL_ACTIVE` | `#FFFFFF12-14` | `#FFFFFF14` | yes |
| `NEUTRAL_HOVER` | around `#FFFFFF0A` | `#FFFFFF0A` | yes |
| `HAIRLINE` | `#FFFFFF0A` | `#FFFFFF0A` | yes (title-bar edge, egui separators) |
| `POPOVER_HAIRLINE` | `#FFFFFF17` | `#FFFFFF17` | yes |
| `POPOVER_FILL` | `#1A1E24` at 92 % | `#1A1E24` at 92 % (`EB`) | yes, also egui's `window_fill` |
| `CARD_FILL` | `#12161B` at 90 %, radius 10 | `#12161B` at 90 % (`E6`), radius 10 | yes (`effects::floating_card`), every canvas card and the stepper (`center/canvas/cards.rs`) |
| `ROLE_HEADING` `ROLE_P` `ROLE_FIGURE` `ROLE_CAPTION` `ROLE_TABLE` `ROLE_LIST` `ROLE_LINK` `ROLE_INLINE` `ROLE_GROUP` | `#B48CFF` `#5AA9FF` `#FF8A4C` `#F472B6` `#2DD4BF` `#A3E635` `#FACC15` `#94A3B8` `#CBD5E1` | same | yes; consumers **pending V5** |
| artifact | `#8A93A3` | `KIND_ARTIFACT` `#8A93A3` | yes |
| `PRIMARY_TOP` / `PRIMARY_BOTTOM` | `#6FE3F9` to `#2CC6E8` | same | yes, `effects::primary_gradient` |
| Primary highlight / glow | `#FFFFFF40` top 1 px; `0 4 14 #3DD6F533` | same | yes |
| Page shadow | `0 28 70 #000000B8` and `0 2 6 #00000066` | `effects::page_shadow`, same values | yes, drawn under every page in `center/canvas/mod.rs` (the grey 1 px stroke is gone) |
| Grid | 24 px `#FFFFFF05` | `effects::grid`, same | yes, behind the pages, panning with the scroll (`center/canvas/mod.rs`) |
| Popover shadow | `0 18 44 #000000CC` (premium; foundations says `0 10 28 #00000083`) | `0 18 44 #000000CC` | yes (premium wins) |
| `ROW_H` | 28 (premium; foundations "rows 22-26") | 28 | yes |
| `RAIL_W` / `NAVIGATOR_W` / `INSPECTOR_W` / `DRAWER_H` / `TITLE_H` | 52 / 300 / 340 / 196 / 44 | same | yes |
| `STATUS_H` | 26-28 | 28 | yes |
| Radii | 4 / 6 / 8, card 10 | `RADIUS_SM/MD/LG`, `CARD_RADIUS` 10 | yes |
| Page-box colours | Media `#F87171`, Crop `#FBBF24`, Bleed `#4ADE80`, Trim `#5AA9FF`, Art `#B48CFF` (design legend) | `overlay::box_color`: `tokens::ERR` / `WARN` / `OK` / `KIND_CONTENT` / `KIND_TAG`, the same values; Media was `#FF6B81` | yes |
| Hard-coded `Color32` outside `theme/` | none | `center/*` is clean: the crosshair's white and halo (`overlay.rs`), `Color32::GRAY` / `LIGHT_RED` (`canvas/mod.rs`) and the card frames use tokens and `effects`; the legend swatches in `cards.rs` draw the x-ray palette of the render crate (data, not UI). `inspector/graphics.rs` (the colour of a PDF swatch) and `navigator/pages.rs` (white page thumbnails) are page pixels, exempt; `inspector/*`, `navigator/*` and `drawer/*` use tokens only | yes for `center/*` and for the V4 files |
| egui `panel_fill` | `#101317` | `PANEL` (was `SURFACE`) | yes |
| egui `window_fill` / `window_stroke` / shadows | popover `#1A1E24` 92 %, `#FFFFFF17`, `0 18 44` | `POPOVER_FILL`, `POPOVER_HAIRLINE`, `POPOVER_SHADOW` | yes |
| egui `extreme_bg_color`, `text_edit_bg_color` | canvas tone | `CANVAS` | yes |
| egui `faint_bg_color`, `code_bg_color` | neutral white tints | `NEUTRAL_HOVER`, `NEUTRAL_ACTIVE` | yes |
| egui widget `bg_fill` / `weak_bg_fill` (inactive, hovered, active, open) | neutral | `NEUTRAL_HOVER`, `NEUTRAL_ACTIVE` | yes |
| egui widget `bg_stroke` | hairlines, no accent | none / `HAIRLINE` / `POPOVER_HAIRLINE`; the accent stroke on `active` is gone | yes |
| egui `selection` | accent-dim fill | `ACCENT_DIM` fill, accent stroke kept for keyboard focus rings | yes |
| Active tool, segmented selected, rail active | `#FFFFFF14` fill, white icon and label, no accent | `NEUTRAL_ACTIVE`, `Color32::WHITE`, no stroke | yes |
| Primary button | gradient + highlight + glow | `effects::primary_gradient` | yes |
| Section headers | no divider, 14 px padding, 10 px +0.9 text-3, dimmed chevron | same | yes |
| Tree chevrons | dimmed | `TEXT_3` at 70 % | yes |
| Dividers in inspector (`ui.separator()` in `inspector/*`, `center/object_view.rs`) | none | removed from `inspector/*` (title block, sections, measure, finding); the CTM popover keeps its rule inside the popover; `center/object_view.rs` still has a `HAIRLINE` | `inspector/*` yes; `center/object_view.rs` **pending V3** |
| Drawer and navigator lines | none (tone) | tab strip bottom border, content coverage rule and RoleMap rule removed; the drawer is `DRAWER_BG` with no border | yes (V4) |
| Title/status bar edge | hairline `#FFFFFF0A` | `HAIRLINE` (`shell/mod.rs::separator`) | yes; both bars draw it (V2). `separator` still strokes `BORDER`, not `HAIRLINE`: one token in `shell/mod.rs` |

## Font roles

Weights available (`theme/fonts.rs`): Inter 400 `ui`, 500 `medium`, 600 `semibold`, 700 `bold`;
JetBrains Mono 400 `mono`, 500 `mono_medium`, 600 `mono_semibold`, 700 `mono_bold`.

| Role | Family | Weight | Size | Tracking | Implemented |
|---|---|---|---|---|---|
| Screen title | Inter | 700 | 24 | -0.25 | helper `bold(24)` + `tracked`; call sites **pending V4** (`states/*`) |
| Inspector title | Inter | 600 | 17 (premium; foundations says 15) | -0.25, wraps | `fonts::inspector_title`, `text::INSPECTOR_TITLE`; call site `inspector/header.rs::block` (object, finding and measure titles use it too) | yes (V4) |
| Panel / navigator title | Inter | 600 | 15 | -0.25 | `fonts::title(text, text::TITLE, ..)`; `navigator/mod.rs` header now uses it (was a 13 px untracked `RichText`) | yes (V4) |
| UI label | Inter | 400 | 12 | 0 | yes (`TextStyle::Body`) |
| Control label | Inter | 500 | 12 | 0 | yes (`ToolButton`, selected `segmented`) |
| Tooltip name | Inter | 600 | 12 | 0 | yes |
| Section header | Inter | 600 | 10 (premium; foundations says 11 +0.4) | +0.9, upper case, text-3 | yes, `fonts::section_header` in `section.rs`, and in the Objects and Pages lists of the navigator (were 11 px untracked) |
| Tag chips, order badges, summary counts | Inter | 700 | per design | 0 | helper `bold`; the ruler corner (`.small().weak()`) is now `mono::TINY` in text-3; order badges come with V5; V4: the inspector's summary strip uses `Chip` (mono 10 px, as in the design's `1360×600`), count badges use `mono_semibold` |
| Values, ids, operators | JetBrains Mono | 400 | 11 | 0 | yes |
| Meta, keys, keycaps | JetBrains Mono | 400 | 10 | 0 | yes (`Keycap`, tree annotations) |
| Selection labels, `23 0 obj`, operator names | JetBrains Mono | 600 | 11 | 0 | helper `mono_semibold`; the selection label chips on the canvas (`marks::pill`) use it; V4: operator names in the Content tree (`TreeRow::mono_label`), the `N G obj` rows of the Objects list, the operator column of the drawer and the rule numbers of the Validation tab |
| Diagnostic codes, validation counts | JetBrains Mono | 700 / 600 | 10-11 | 0 | `Badge` counts use `mono_semibold`; `mono_bold` on the codes of the Diagnostics tab, the failed / rules / passed tiles and the rule counts of the Validation list (V4); rule ids in the list `mono_semibold` |
| Numerals | JetBrains Mono for anything that must align | 400 | as the role | 0 | egui cannot enable OpenType `tnum`; documented in `theme/fonts.rs` |

## Shell (V2)

| Item | Design | Implemented | Fixed? |
|---|---|---|---|
| Title chip | one chip `PDF 2.0 · UA-2 · rev 3/3 ▾`, history icon, `11-components.png` | `shell/title_bar.rs::title_chip`: neutral fill (`NEUTRAL_HOVER`, `NEUTRAL_ACTIVE` on hover/open), `POPOVER_HAIRLINE`, mono 10; runs coloured only for what needs a look (Untagged, Encrypted, `damaged`, `UA-n failed`, `read-only`) | yes |
| Old chips | version, Tagged/UA, damaged, rev chips | removed; `states::encrypted::chip_text` became `cipher` (popover) | yes |
| Chip popover | file summary and revisions | frosted `popover::show`, 320 px: name, size, header, pages, tagged, PDF/UA, encryption, damage, then the timeline (pick = read-only view) | yes |
| File name | tracked title | `fonts::title(.., LARGE)` | yes |
| Status bar | cursor · file info · units · problems | cursor in a fixed 290 px slot (the text no longer pushes "file info" around), `file info` item, units, `N diagnostics` (warning colour only above 0) | yes |
| File info hover | counts, open time, veraPDF | tooltip card (fade only: `tooltip::show` is private, so no scale); the veraPDF dot moved here | yes; scale-in **pending** a public `tooltip::rich` |
| Status messages | not in the design | `Rendering…` and non-`Ready` messages stay after "file info" | kept |
