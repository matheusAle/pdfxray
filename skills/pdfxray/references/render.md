# render

`pdfxray render <FILE> <PAGE> -o <OUTPUT> [flags]` draws one page to `.png`, `.jpg` or
`.jpeg` and prints a JSON summary. Open the image to look at it.

```json
{ "output": "page0.png", "width": 600, "height": 800 }
```

`--scale` is pixels per point (default `2`, so a 300 × 400 pt page is 600 × 800 px).

## What to draw on the page

| Flag | Draws |
|---|---|
| `--boxes` | Every defined page box |
| `--highlight <ID>` | An outline around these nodes (repeat the flag for more) |
| `--xray kind\|tag\|artifact` | An outline around every painted item, coloured by kind (glyph, path, image, shading), by innermost marked-content tag, or by artifact vs tagged vs untagged |
| `--invisible` | With `--xray`, also mark invisible items: invisible text, white on white, off the page |
| `--isolate <ID>` | Paint only the content of these nodes (repeatable) |
| `--hide <ID>` | Paint everything except the content of these nodes (repeatable) |
| `--step <IX>` | Paint the operators up to execution index `IX` only (the `op:P:IX` ids) |
| `--clip <IX>` | Show the clip in effect after operator `IX` |
| `--alpha` | Tint items with transparency, soft masks or blend modes other than Normal |

`--isolate` cannot be combined with `--hide` or `--step`, and `--hide` not with `--step`.

## The View menu's annotations (pdfxray 0.4 or later)

| Flag | Draws |
|---|---|
| `--tags` | Every structure element's box in its role colour |
| `--labels` | A chip naming each element's type |
| `--artifacts` | The artifact sequences and the PDF 2.0 `Artifact` element |
| `--nesting` | The containers too |
| `--reading-order structure\|paint\|speech` | Numbered badges in that order, joined by a path. `speech` follows `--speech-preset` (`structure` default, `acrobat`, `firefox`, `chromium`) |

For example, `--tags --labels --reading-order structure` is a quick picture of how a page is
tagged and read.

## Comparing a page (pdfxray 0.4 or later)

Against another file, with the elements that changed in status colours and a legend:

```sh
pdfxray render a.pdf 0 --against b.pdf -o compare.png
```

* `--against <FILE>`: side B. `--against-page <N>` picks B's page; by default, the page the
  pairing matches.
* `--mode side-by-side|overlay|mask`: A left and B right (the default against a file); both in
  two ink colours multiplied; or A in grey with the pixels that differ coloured, ink B adds and
  ink B removes apart.
* `--register` removes the global offset the pairing found before overlaying B.
  `--scale-b-to-a` scales B to fit A when the sizes or `/Rotate` differ, instead of refusing to
  overlay.
* `--order-side a|b`: whose reading order an overlay or a mask numbers (default `a`).
* The visual score: `--threshold` (0 to 1, default 0.1, lower counts subtler changes),
  `--include-aa` (count anti-aliasing pixels too), `--max-diff-pixels`, `--max-diff-ratio`
  (above either, the score `exceeds`; with neither, nothing exceeds).

The JSON then adds `against` (`{ file, page }`), `mode`, `summary` (how many elements on the
two pages have each status, e.g. `{ "added": 7, "removed": 6 }`), `differing_pixels`,
`differing_fraction`, `visual` (`score`: `differing_pixels`, `ratio`,
`ink_added`, `ink_removed`, `anti_aliased`, `exceeds`; `settings`), `registration`, `b_scale`,
`notes` and `warnings`.

Against a reference engine, to check that pdfxray draws the page like PDFium:
`--reference pdfium` (default mode `mask`, the differing pixels in red; `--tolerance`, default
48, is the per-channel difference that still counts as equal). It needs a PDFium library:
`--pdfium <file or dir>`, `PDFXRAY_PDFIUM`, or the pdfxray data directory. `--against` and
`--reference` cannot be combined, and neither works with the flags of the first table above.

## As a Windows viewer draws it (pdfxray 0.5 or later)

Draw the page on a Windows screen's device pixels, rounded the way a given viewer rounds what
lands between two pixels:

```sh
pdfxray render f.pdf 0 --emulate acrobat --dpi 120 --zoom 100 -o acrobat.png
pdfxray render f.pdf 0 --emulate chrome --dpi 120 --zoom 100 -o chrome.png
```

* `--emulate chrome|edge|acrobat`: the viewer. Chrome and Edge draw with PDFium: each glyph
  starts on a whole pixel, a rectangular clip grows out to whole pixels, an upright image covers
  a whole number of pixels; nothing else is snapped. Acrobat snaps coordinates within 0.0001 px
  of a pixel, lets fills overlap 0.01 px, makes every stroke a whole number of pixels wide (at
  least 1) with its edges on pixel boundaries, and rounds each glyph advance, so a long text run
  can drift.
* `--dpi <N>`: the screen's density, 48 to 480 (default 96, Windows' 100 % scaling; 120 is
  125 %, 144 is 150 %).
* `--screen <W>x<H>`: its size in device pixels (default `1920x1080`), for the fit zooms.
* `--zoom <PERCENT>|fit-width|fit-page`: the viewer's zoom (default 100). The fit zooms take the
  whole screen as the page area.

Any of these sets the scale to `zoom × DPI / 72` device pixels per point, so they cannot be
combined with `--scale`; without `--emulate`, the page is drawn on that screen with pdfxray's own
rounding. They draw one page: not with `--against` or `--reference`. The debug overlays and
annotations still work. The JSON adds `emulation`:

```json
{ "viewer": "Acrobat", "engine": "Adobe AGM", "screen": [1920, 1080], "dpi": 120,
  "scale_factor": 1.25, "zoom": "100%", "zoom_percent": 100.0, "pixels_per_point": 1.6666666 }
```

Anti-aliasing, font hinting and Acrobat's resolution thresholds are not emulated: compare
rendered images for where things land (a hairline gap, a 1 px vs 2 px line, text drift), not for
the exact grey of an edge.
