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
