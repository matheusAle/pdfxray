# reading-order, tag-boxes, speech, table

These read the structure tree (tags). An untagged file has none: `tree --kind tags` is empty
and `reading-order` has no steps.

## `pdfxray reading-order <FILE> <PAGE> [--from structure|paint]`

The order a page's tagged content is read in: the elements that directly own content.
`--from structure` (default) is the structure tree's order, which a screen reader follows;
`--from paint` is the order the content stream paints them.

```json
{ "page": 0, "from": "structure", "backtracks": 1,
  "steps": [
    { "index": 3, "id": "struct:10:0", "role": "Caption", "class": "caption", "kind": "forward",
      "backtrack": false, "preview": "Figure 1: Revenue by region",
      "rect": { "x0": 40.0, "y0": 177.8, "x1": 166.17, "y1": 187.18 } },
    { "index": 4, "id": "struct:11:0", "role": "Figure", "class": "figure", "kind": "backtrack",
      "backtrack": true, "preview": "Grey square", "rect": { … } } ] }
```

* `role` is the type as written in the file; `class` is its family: `heading`, `p`, `figure`,
  `caption`, `table`, `list`, `link`, `inline`, `group`, `artifact` or `other`.
* `kind` is one of `first`, `forward`, `column_break`, `detour` (a note or aside) or
  `backtrack`. Only a step that goes back up the page within its flow is a backtrack, and those
  are what `backtracks` counts.
* Comparing `--from structure` with `--from paint` shows where the tag order and the visual
  order part.

## `pdfxray tag-boxes <FILE> <PAGE>`

Every structure element that draws on the page, containers included:

```json
[ { "id": "struct:8:0", "role": "Heading1", "standard": "H1", "class": "heading", "depth": 1,
    "rect": { "x0": 40.0, "y0": 354.864, "x1": 208.05, "y1": 377.688 } } ]
```

`standard` is the standard type the role maps to (empty when it reaches none). `depth` counts
ancestors.

## `pdfxray speech <FILE> [--preset P] [--show-skipped]`

What a screen reader announces for the whole document, one item per announcement, in order.

`--preset`: `structure` (default, ISO 32000 as written), `acrobat` (Acrobat on Windows, as NVDA
and JAWS consume it), `firefox` (pdf.js) or `chromium` (Chrome and Edge: tags ignored, content
in paint order). All but `structure` are approximations, and say so.

Top level: `preset`, `label` (e.g. `"Structure (spec)"`), `approximation`, `rules` (for an
approximating preset, each `{ topic, behaviour, evidence }` it applies) and `items`.

Each item has `index`, `node`, `page`, `declared_role`, `role`, `class`, `text`, `lang`,
`invisible` and usually `marked_content` (its `mc:` ids). Then, only when they apply:

* `level`: a heading's level.
* `substituted_by`: `alt`, `actual_text` or `expansion` when that replaced the content; then
  `painted` is the text on the page, and `also_actual_text` when an `ActualText` was dropped.
* `lang_change`: `{ "from": "en-US", "to": "de-DE" }`.
* `cell`: `{ table, row, col, row_span, col_span, header, headers }` for a table cell.
* `note`: a footnote or reference flag, e.g. `{ "role": "reference", "other": "struct:11:0",
  "via": "label_match", "items_apart": 1 }`.
* `link`: `{ "subtype": "Link", "target": { "uri": "https://…" } }`.
* `skipped`: with `--show-skipped`, an item that is **not** spoken, with why: `artifact`,
  `private`, `empty`, or `{"covered_by": "<node id>"}` (hidden under an Alt or ActualText
  above it).

```json
{ "index": 2, "node": "struct:9:0", "page": 0, "declared_role": "Sect", "role": "Sect",
  "class": "group", "text": "Summary of results", "substituted_by": "alt",
  "painted": "Revenue grew in every region. Costs stayed flat this year.",
  "lang": "en-US", "invisible": false }
```

## `pdfxray table <FILE> <ID>`

`ID` is a `Table` structure element (find it with `tree --kind tags`). The table's grid and
each cell's header cells.

Top level: `table`, `rules` (`pdf17`, or `pdf2` for a PDF 2.0 file with namespaces, which
follows ISO 32000-2's header-finding algorithm), `rows`, `cols` (the widest row, spans
included), `cells` (in reading order), `slots` (row-major, `rows × cols`: the index into
`cells` of the cell covering each slot, `null` where none does) and `irregular` (the rows that
are not as wide as the table, each `{ row, width }`).

Each cell: `node`, `header` (true for a header cell), `group` (`head`, `body` or `foot`: in
`THead`, in `TBody` or directly in the table, in `TFoot`), `row`, `col`, `row_span`,
`col_span`, `id` (its `/ID`, when it has one), `headers` and, when it names header ids that do
not exist, `missing_header_ids`. Each of `headers` is `{ "cell": "<node id>", "via": … }`,
where `via` is the rule that found it: `headers` (the cell's `/Headers` list), `scope` (the
header's `/Scope`) or `position` (a header with no `Scope`, by where it sits).

A header cell also has `header_scope`:

```json
{ "scope": "column", "source": "inferred", "place": "top_row",
  "governs": ["struct:17:0", "struct:20:0"], "named_by": ["struct:15:0"] }
```

`source` is `declared` (its `Scope` says so), `inferred` (no `Scope`, read from its place) or
`conflict` (the `Scope` says one direction and its position the other; readers follow the
`Scope`). `place` is `head`, `top_row`, `header_row`, `first_column` or `data_row`. `governs`
lists the cells it heads; `named_by` the cells whose `/Headers` name it.
