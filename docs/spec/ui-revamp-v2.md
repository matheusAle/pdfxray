# pdfxray UI revamp v2: premium, low-burden, tag overlay

Follow-up to `ui-revamp.md` and `ui-revamp-plan.md`. Source of truth: `design/pdfxray-revamp.pen`.
PNG exports are in `design/exports/` (same names as before, plus 13 and 14).

| Export | What to read |
|---|---|
| `14-premium-low-burden-spec.png` | **The rules list**: premium items 1–7 and low-burden items A–G |
| `01-tags.png` | Reference screen with every treatment applied (the old version is archived in the .pen) |
| `02…10-*.png` | All other screens with the same treatment |
| `12-tags-overlay-reading-order.png` + spec §7 in `00-…png` | Tag-role colours, all-tag boxes, reading-order path |
| `11-components.png`, `13-foundations.png` | Updated components, tokens (incl. `role-*` colours), type scale |

## 1. Premium theme (applies everywhere)

1. **Tone, not lines.** Areas separate by surface tone: canvas `#0A0C0F`, side panels `#101317`, toolbar `#0C0F12`,
   drawer `#0E1115`, rail/status `#0B0D10`. Inspector section dividers are removed; sections get 14 px vertical padding.
   Tree and inspector rows are 28 px tall.
2. **One accent.** Cyan only for the current selection and the primary action (Validate, "Next finding").
   Active tools, the active Pick mode, the active rail item and the active view mode are neutral: fill `#FFFFFF12–14`, white icon and label.
3. **Depth through light.**
   - Title bar: `#0F1216` at 85%, 24 px background blur, hairline `#FFFFFF0A`.
   - Popovers, menus and tooltips: `#1A1E24` at 92%, 1 px `#FFFFFF17`, 20 px blur, shadow `0 18 44 #000000CC`.
   - Floating canvas cards: `#12161B` at 90%, radius 10, blur.
   - Page: two-layer shadow (`0 28 70 #000000B8`, `0 2 6 #00000066`) on a faint 24 px grid (`#FFFFFF05`) behind the pages.
   - Validate: vertical gradient `#6FE3F9 → #2CC6E8`, 1 px top highlight `#FFFFFF40`, glow `0 4 14 #3DD6F533`.
4. **Typography.**
   - Tabular numerals for every number.
   - Titles tracking −0.2 to −0.3. The inspector title is 17 px and wraps.
   - Section headers: 10 px, +0.9 tracking, text-3.
5. **Motion** (all interruptible; honour macOS "Reduce motion"):

   | Event | Animation |
   |---|---|
   | Selection outline appears | grows in, 120 ms ease-out |
   | Popover / menu opens | fade + scale 0.97→1 from its anchor, 140 ms |
   | Zoom, jump-to-node | 220 ms ease |
   | Paint-order step | newly painted item fades in, 90 ms |
   | Reading-order path shown | draws 1→n, 300 ms |
   | A value changes | brief surface-3 flash, 400 ms |

6. **Keycaps** for shortcuts in tooltips and menus. The key sits next to the name, never at the far edge.

## 2. Lower visual burden

- **A. Inspector**
  - A summary strip of 3–4 fact chips sits under the title.
  - The selection's problem comes first, as a card with a Details action.
  - Default values fold into a "+ n default values" line.
  - Sections not relevant to the current mode start collapsed (e.g. Image in Tags, Structure in Content).
  - The action footer (Raw object / Stream bytes / Copy JSON) is removed; those actions move to B/D. Validation keeps its Previous / Rule docs / Next finding bar.
- **B. Trees (Tags)**
  - Object refs show only on the selected or hovered row. Other rows show a short text preview instead (e.g. "H1 Annual Report 2025").
  - Badges appear only for problems. Chevrons are dimmed.
  - RoleMap info moves into a hover on mapped rows; the RoleMap footer is removed.
- **C. Toolbar**
  - The six render tools collapse into one **View ▾** control showing the active mode (X-ray / Paint order / "View" when none). Its menu lists every mode with its key.
  - Pick, Measure, Page boxes, Fit and zoom stay as they are.
- **D. Selection action bar**: a floating frosted bar under the canvas selection with Isolate · Hide · Show in tree · Object · Copy id. The same actions are on right-click in the tree, the canvas and on values.
- **E. Drawer**
  - Collapsed by default to a 34 px tab strip. It expands when the user clicks the page (hit stack), when validation finishes, or when a tab is clicked.
  - It stays open by default in modes where it is the main content: Content (Operators), Objects (References), Validation, Paint order, Broken file.
  - Count badges are coloured only for warnings and errors.
- **F. Canvas**
  - X-ray outlines are drawn at 50%, and at full strength on hover and selection.
  - The X-ray legend becomes a chip ("5 kinds ▾") that expands on hover.
  - The paint stepper shows only while Paint order is on.
  - Floating cards anchor to the canvas's top-right corner, 14 px in and below the ruler, never over a page. They collapse to chips when space is tight.
- **G. Shell**
  - One title-bar chip: "PDF 2.0 · UA-2 · rev 3/3 ▾". It opens the file summary and revisions popover.
  - Status bar: cursor · file info (hover shows counts, open time and veraPDF) · units · problems.

## 3. Tag overlay and reading order (now approved)

Spec §7 and screen 2.2:
- Role colours come from the tokens `role-heading/p/figure/caption/table/list/link/inline/group`. Artifacts stay `#8A93A3`.
  Custom roles use the colour of their RoleMap target. (The "Edit tag colours…" override was built in V5 and later removed: the role colours are fixed.)
- The **View ▾** menu (X-ray options) offers:
  - Colour by: Kind · Tag role · Artifact
  - Show: All tag boxes (⇧T), Tag labels (L), Reading order (R), Nesting, Artifacts
  - Reading order from: Structure tree · Paint order
- Reading order is drawn as numbered badges plus a 2 px path. A step that goes back up the page turns amber.
  This is a new diagnostic, `reading-order-backtrack`. The Inspector lists the order, and a "vs paint order" comparison shows where the two disagree.
- Role colour is shared by the tree dots, the canvas boxes and the legend.

## Process (unchanged)

- Plan first, including refactors (theme module, shared surfaces/effects, animation helper).
- Check for dead code after each refactor.
- Fluent kittest expectations with smoke checks; update snapshots intentionally and compare them with the PNGs.
- Run the full gate after each milestone.
- **No commits without the user's permission. Never add a co-author trailer.**
