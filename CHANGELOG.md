# Changelog

What changed in each release of pdfxray, newest first. The section of a release is also its
notes in the app's update dialog and on its GitHub release, so it is written for the people who
use pdfxray. A release cannot be published without one (see `docs/releasing.md`).

## [Unreleased]

### Added

- **Paint order while comparing.** The comparison's toolbar has Paint order (`P`): it steps one
  side at a time, painting that side's page up to the step and the other whole, side by side,
  overlaid or as a mask. An A | B switch in the Paint order tab picks the side.
- The paint-order track marks where each page starts. **Page start** and **Page end** (`Home`
  and `End`) replace the first and last operator buttons: at the edge of a page they go on to
  the page before or after.
- **Go to paint order** on the selection bar and in right-click menus: the stepper starts at the
  first operator that paints the element.
- An **Unpainted boxes** switch in the Paint order tab hides the outlines of what is not painted
  yet.
- In the Inspector's *Stack at this point*, *+N more* rows list twenty more operators each, and
  *Show fewer* folds them again.
- **Save image…** writes an image of the document to a PNG at its own size, with its
  transparency: from the image's preview card, the Inspector, or the right-click menu of the
  image or of the `Do` that paints it.

### Changed

- The Inspector's page bbox is a row per coordinate, each named for its edge (`x0 · left` …),
  instead of one `[x0 y0 x1 y1]` array.
- With two elements selected, *Between A and B* shows how deep they overlap as well as how far
  apart they are, the gap or overlap along each axis, the shared area, and the size in screen
  pixels, flagging one under a pixel. Small lengths keep four decimals. The tiles that repeated
  the gap in other units are gone; the status bar's unit switch does that.

## [0.4.0] - 2026-11-05

### Added

- **Compare two PDFs.** Open a second version of a file with File › Compare With… (⌥⌘O), by
  dropping it with ⌥ held, or with `pdfxray-app a.pdf --compare b.pdf`. See both side by side,
  overlaid in two inks, or as a mask of what changed. A new Changes list, grouped by page,
  takes you to each change. ⌘R reads the compared file again after you edit it.
- Elements are matched by what they say and where they are, never by object number, so a file
  that was re-saved with every object renumbered still compares cleanly.
- `pdfxray diff a.pdf b.pdf` reports what changed (modified, moved, added, removed) as JSON,
  with `--page`, `--scope all|tags|ink`, `--include-unchanged` and a per-page visual score
  with `--visual`. It exits 0 when nothing changed, 1 when something did and 2 on error.
- **An agent skill** teaches coding agents to inspect PDFs with the `pdfxray` CLI instead of
  guessing: `npx skills add matheusAle/pdfxray`.
- `pdfxray render` draws the View menu's annotations (`--tags`, `--labels`, `--artifacts`,
  `--nesting`, `--reading-order`) and compares a page with another file (`--against`, with
  `--mode side-by-side|overlay|mask`).

### Changed

- **Paint order steps through the whole document**, not only the current page. The slider,
  the arrows and play carry on to the next page, and the tab reads like
  `211 / 1120 ops   page 3 · op #42`.
- **Much less memory.** The page, thumbnail and preview threads now share one copy of the
  document instead of opening their own: a 200-page file takes about 410 MB instead of 1.4 GB.

### Removed

- `pdfxray compare`. Use `pdfxray render <file> <page> --reference pdfium --mode mask`, which
  is what it did.

### Known limits

- An untagged file is compared page by page, by the ink the pages share, not element by element.
- An encrypted file is compared on its structure only.
- Comparing files of more than 300 pages a side is slow and takes a lot of memory. The app
  warns, then compares anyway.

## [0.3.0] - 2026-11-02

### Added

- The Tag roles card lists a table's rows (TR), header cells (TH) and data cells (TD) under
  *Table*, so each can be focused on its own.

### Changed

- **Tag labels work without the tag boxes**, and the boxes without the labels. Labels now
  start off: ⇧T shows the boxes, L the labels.

### Fixed

- A long value that wraps in the inspector, such as a page bbox in a narrow panel, no longer
  has the next row drawn over its second line.

## [0.2.0] - 2026-11-02

### Added

- **Select several elements at once**: drag a marquee on empty space or ⌘-drag, ⇧ adds,
  ⌥ subtracts, ⌘A selects all. An action bar and an inspector summary act on them all, and
  `[` and `]` step through them.
- Table header chips show which way each header reads (row, column or both), whether that was
  declared or inferred, and when the two conflict. Hovering or selecting a header shades the
  cells it heads.
- The Tag roles card focuses roles: click a role to show it alone, click again to show all,
  ⇧-click to add or remove one.
- Inspector rows can be reached with Tab and copied with ⌘C (⇧⌘C copies the key too). Long
  arrays wrap and fold.
- Objects get preview cards: images, fonts and colours.
- Space-drag pans the canvas.

### Changed

- ⌘-scroll zooms toward the pointer without scrolling the page.
- Scrolling no longer changes the paint-order step. *Follow step*, on by default, scrolls
  what each step paints into view.
- Validation moved to the side rail, after Revisions. The Validate button in the title bar is
  gone.
- Keyboard focus is visible on every control, and error messages say what to do next.

### Fixed

- The navigator, inspector and drawer lists no longer bounce past their end, which showed
  empty space after a fast scroll.

## [0.1.0] - 2026-10-31

The first release, for Apple Silicon Macs.

- Every layer of a PDF, linked: raw objects and revisions, the page tree and boxes, content
  operators and glyphs, marked content and artifacts, the structure tree and resources.
- A click-to-inspect canvas with a hit stack, rulers, a measure tool and page-box overlays.
- Debug views: X-ray outlines, invisible content, isolate and hide, an operator-by-operator
  paint stepper, clip and transparency views.
- Tags, reading order and what a screen reader announces, with table headers.
- Diagnostics from the syntax layer up, and validation with veraPDF (PDF/UA, WCAG 2.2, PDF/A),
  each finding mapped to the node it concerns.
- The `pdfxray` command line, with the same queries as JSON.
