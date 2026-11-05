# info, tree, node, breadcrumb, hit

Every command takes `--pretty` to indent its JSON.

## `pdfxray info <FILE>`

Summary facts and diagnostics.

```json
{
  "name": "reading-order.pdf", "byte_len": 1939, "header_version": "1.7", "header_offset": 0,
  "page_count": 1, "object_count": 13, "revision_count": 1, "encrypted": false,
  "engine_error": null,
  "diagnostics": [
    { "code": "reading-order-backtrack", "severity": "warning",
      "message": "Caption (step 3) is read before Figure (step 4), but Figure sits above it on page 1, so a screen reader jumps back up the page",
      "span": [1286, 1379], "object": { "num": 11, "generation": 0 } }
  ]
}
```

`diagnostics` are pdfxray's own checks (broken xref, missing Alt, reading-order backtracks…),
each with a stable `code`, a `severity`, a byte `span` and the `object` it is about. Diagnostic
`N` in this list is node `diag:N`. Start here: it is cheap and often names the problem.

## `pdfxray tree <FILE> [--kind KIND]`

A navigation tree. `--kind` is one of:

* `pages` (default): pages and their boxes.
* `objects`: the trailer, the revisions and every indirect object.
* `tags`: the structure tree. Each element shows its type (a role-mapped one as
  `Heading1 → H1`), a text or Alt preview, and its MCIDs.
* `artifacts`: the artifact sequences of each page, with their type and box.
* `resources`: each page's fonts, XObjects, colour spaces… by category.
* `diagnostics`: the diagnostics, grouped.
* `content:<page>`: page `<page>`'s content stream, one row per operator (`op:` ids), nested
  under `BDC`/`BT`/`q` and Form XObjects.

Each row is `{ "id", "label", "detail"?, "badges"?, "children"? }`. `badges` are diagnostics
on that row: `{ "code", "severity" }`. Absent fields are empty.

```json
{ "id": "struct:11:0", "label": "Figure", "detail": "Alt: \"Grey square\" · 1 MCID",
  "badges": [{ "code": "reading-order-backtrack", "severity": "warning" }],
  "children": [{ "id": "mc:0:2", "label": "MCID 2", "detail": "Page 1" }] }
```

Trees of large files are big: filter with `jq` instead of printing them whole, for example
`pdfxray tree f.pdf --kind tags | jq '.. | objects | select(.label? == "Figure") | .id'`.

## `pdfxray node <FILE> <ID>`

Every property of one node, as the app's inspector shows it.

```json
{ "id": "struct:11:0", "title": "Figure", "kind": "struct-elem",
  "groups": [
    { "name": "Element", "rows": [
      { "key": "Type", "value": "Figure" },
      { "key": "Standard type", "value": "Figure" },
      { "key": "Effective lang", "value": "en-US (from the catalog)" },
      { "key": "Alt", "value": "Grey square" } ] },
    { "name": "Structure", "rows": [
      { "key": "Parent", "value": "Document", "link": "struct:7:0" } ] }
  ] }
```

`groups` and their row keys depend on the node's `kind`: a structure element has *Element*,
*Content*, *Structure*, *Object* and *Diagnostics*; an operator (`op:`) has *Operator*,
*Geometry*, *Graphics state*, *Text state*, *Marked content* and *Painted glyphs*; a glyph has
*Glyph*, *Text state* and *Context*. A row's `link` is a node id to follow.

## `pdfxray breadcrumb <FILE> <ID>`

The path from the document to a node: `["doc", "page:0", "mc:0:2"]`.

## `pdfxray hit <FILE> <PAGE> <X> <Y> [--layer LAYER]`

Everything under a point (PDF points, origin bottom left), most specific first:

```json
["glyph:0:4:1", "op:0:4", "mc:0:0", "struct:8:0", "struct:7:0", "box:0:media", "box:0:crop", "page:0"]
```

`--layer` narrows what may be hit: `all` (default), `tags` (structure elements and marked
content), `content` (operators and glyphs), `annotations`, `boxes` or `artifacts`. With `tags`,
the first `struct:` id is the innermost element owning that spot.

To find coordinates, take a `rect` from `tag-boxes` or `reading-order` and hit its middle.
