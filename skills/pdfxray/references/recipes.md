# Recipes

Short paths through common questions. Pages are zero-based; `f.pdf` is the file.

## Why does a screen reader skip or misread something?

1. Find what is announced, and what is not:
   `pdfxray speech f.pdf --show-skipped`. Look for the text you expected.
   * It has `skipped`: `artifact` (marked as decoration), `private`, `empty`, or
     `{"covered_by": "struct:…"}` (an ancestor's Alt or ActualText replaces it).
   * It is there but says something else: `substituted_by` names the replacement and
     `painted` holds the text on the page.
   * It is missing altogether: it may not be tagged. Find it on the page and check.
2. To find it on the page: `pdfxray tag-boxes f.pdf 0` for the element boxes, or
   `pdfxray hit f.pdf 0 <x> <y> --layer tags` at a point where it is drawn. Only `page:0` and
   page boxes, or no `struct:` id, means the content there is not tagged.
3. Explain with `pdfxray node f.pdf <id>` (its type, Alt, ActualText, language, parent) and
   `pdfxray breadcrumb f.pdf <id>`.
4. For a specific viewer, repeat step 1 with `--preset acrobat`, `firefox` or `chromium`, and
   say that these presets are approximations.

## Is the reading order right?

`pdfxray reading-order f.pdf 0` lists the steps. `backtracks` counts the jumps back up the
page; steps of `kind` `backtrack` are where they happen. Compare with
`--from paint` to see whether the tags follow the visual order. To show it:
`pdfxray render f.pdf 0 --tags --labels --reading-order structure -o order.png`, then open
`order.png`.

## Look at a page before you describe it

```sh
pdfxray render f.pdf 0 -o page0.png
```

Open `page0.png`. To point at something you found, add `--highlight <id>` (repeatable). To see
what is painted and how it is tagged, add `--xray tag` (or `kind`, or `artifact`). To isolate one
element's content, `--isolate <id>`. Use `--scale 1` for a quick look at a large page.

## Prove a fix with diff

Keep the original, make the change into a new file, then:

```sh
pdfxray diff before.pdf after.pdf --pretty
```

* The change you meant shows up in `pairs`, with its `status` and `changed` fields; nothing
  else should.
* `diagnostics.core.fixed` should list the problem you fixed, and `introduced` should be empty.
* For a file that should only have been re-saved, expect exit status 0. A script can rely on
  `$?`: 0 identical, 1 different, 2 error.
* Add `--visual` to also check that the pages still look the same.
* Then rerun the check that found the problem (`validate`, `speech`, `reading-order`) on the
  new file.

## Check a table's headers

1. `pdfxray tree f.pdf --kind tags` and find the `Table` element's `struct:` id.
2. `pdfxray table f.pdf struct:N:0`. For each data cell, `headers` lists its header cells and
   `via` the rule that found each one. A header cell with `header_scope.source: "conflict"`
   says one direction in its `Scope` and another by its position. A data cell with no headers
   has nothing to be announced with.
3. `pdfxray speech f.pdf` reads each cell with its `cell.headers`, which is what a reader says.

## Validate for accessibility

```sh
pdfxray validate f.pdf --profile ua1   # or ua2, wcag22, wtpdf
```

Group `findings` by `rule`, and use `target` with `node` to see the object each one is about.
Read `compliant`, not the exit status.

## What is this thing at (x, y)?

`pdfxray hit f.pdf 0 <x> <y>` lists everything there, innermost first: glyph, operator, marked
content, structure elements, page boxes. Then `pdfxray node f.pdf <id>` on the one you care
about. An operator's node has its geometry, graphics state and text state; a glyph's node
has its font and Unicode.

## Why does it look different in Chrome or Acrobat?

A hairline gap between two fills, a line one pixel wide in one viewer and two in another, or
text that drifts along a line usually comes from where each viewer puts what lands between two
device pixels. Draw the page as each one does, on the user's screen, and compare:

```sh
pdfxray render f.pdf 0 --emulate chrome --dpi 120 --zoom 100 -o chrome.png
pdfxray render f.pdf 0 --emulate acrobat --dpi 120 --zoom 100 -o acrobat.png
```

Open both and look at the spot. Then `pdfxray hit f.pdf 0 <x> <y>` and `pdfxray node` on the
operator there for its exact coordinates and line width: a coordinate a fraction of a pixel off
the grid at that scale (`zoom × DPI / 72` px per point) is the usual cause.

