# diff (pdfxray 0.4 or later)

`pdfxray diff <A> <B> [flags]` compares two PDFs, usually two versions of one file. Structure
elements are matched by what they say and where they are, **never by object number**, so a
re-saved file whose objects were all renumbered still compares cleanly.

## Exit status

* `0`: nothing in scope changed.
* `1`: something did.
* `2`: an error, such as a file that cannot be opened.

`--exit-zero` exits 0 when the files differ too (errors still exit 2). The visual score never
changes the exit status.

## Flags

* `--page <N>`: only page N of A (zero-based), and what corresponds to it in B.
* `--scope all|tags|ink`:
  * `all` (default): element pairs, each page's residue, and the diagnostics fixed and introduced;
  * `tags`: element pairs and the page correspondence;
  * `ink`: only the residue of each page pair, meaning painted content outside the structure tree.
* `--include-unchanged`: also list the pairs and pages that did not change.
* `--visual`: also score each page pair visually. Both pages are drawn on one pixel grid and
  compared perceptually. Two renders per page pair. Tuned with `--visual-scale` (pixels per
  point, default 2), `--threshold` (0 to 1, default 0.1), `--include-aa`,
  `--max-diff-pixels` and `--max-diff-ratio`.

## Output

```json
{ "identical": false, "scope": "all",
  "summary": { "same": 24, "modified": 1, "moved": 0, "added": 0, "removed": 0 },
  "sides": { "a": { "name": "a.pdf", "pages": 3, "elements": 25, "structure_only": false },
             "b": { … } },
  "tolerance": 1.0,
  "pairs": [ {
    "kind": "element", "status": "modified", "a": "struct:16:0", "b": "struct:16:0",
    "role": "Figure", "page_a": 0, "page_b": 0, "bbox_a": { … }, "bbox_b": { … },
    "delta": [0.0, 0.0],
    "changed": [{ "field": "alt", "a": "Bar chart of revenue by region", "b": null }],
    "rows": [{ "group": "Element", "key": "Alt", "a": "Bar chart of revenue by region", "b": null }],
    "confidence": { "rung": "role_box", "method": "anchor" } } ],
  "pages": [],
  "diagnostics": {
    "core": { "fixed": [], "introduced": [ { "code": "figure-alt-missing", "severity": "error",
      "message": "…", "on": { "kind": "element", "pair": 5, "a": "struct:16:0", "b": "struct:16:0" } } ],
      "unchanged": 1 },
    "validation": { "state": "not_run" } } }
```

* `status` is `modified`, `moved`, `added` or `removed` (`same` too with
  `--include-unchanged`). An added element has `"a": null` and `"page_a": null`; a removed one
  has `"b": null` and `"page_b": null`. `a` is a node id in A and `b` one in B, ready for
  `node` on that file.
* `changed` lists each field that differs, with A's and B's values; `rows` gives the same as
  the inspector's rows. `delta` is how far the element moved, in points. A `moved` pair also
  carries its `text`.
* `offset` (`{ dx, dy, pairs }`) appears when the whole page content shifted.
* `pages` lists page pairs: `{ a, b, pairs, residue: { a, b, only_a, only_b }, visual? }`. A
  page with only `b` was inserted, one with only `a` dropped. `residue` counts painted content
  outside the structure tree; `only_a` and `only_b` are what the other side lacks. With
  `--visual`, a page pair also has `visual` (`differing_pixels`, `ratio`, `ink_added`,
  `ink_removed`, `anti_aliased`, `exceeds`), and the top level has the settings used.
* `diagnostics.core` says which of pdfxray's diagnostics a change fixed or introduced.

## Limits

* An untagged file has no structure elements: its pages pair by the ink they share, and each
  page pair reports only its residue (and its visual score with `--visual`).
* An encrypted file's content is not interpreted, so it is compared on structure only (no
  text, no geometry). Its side says `"structure_only": true`.
* A side lists `degraded_pages` when some of a page's painted items could not be fully
  attributed to their operators: items on those pages may be missing or carry uncertain
  details.
