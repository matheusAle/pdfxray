# Node ids

Every node of a PDF has an id. The commands print them (`id`, `node`, `link`, `target`, `a`,
`b`…) and `node`, `breadcrumb`, `table`, `render --highlight/--isolate/--hide` take them. Pages,
operator indexes and MCIDs in ids are zero-based.

| Id | What | Example |
|---|---|---|
| `doc` | The document as a whole | `doc` |
| `trailer` | The newest trailer dictionary | `trailer` |
| `rev:N` | An incremental-update revision, oldest = 0 | `rev:1` |
| `obj:NUM:GEN` | A raw indirect object in the newest revision | `obj:12:0` |
| `page:P` | A page | `page:0` |
| `box:P:KIND` | A page box: `media`, `crop`, `bleed`, `trim` or `art` | `box:0:crop` |
| `struct:NUM:GEN` | A structure element (tag), by its object number | `struct:11:0` |
| `mc:P:MCID` | A marked-content sequence with an MCID on page P | `mc:0:3` |
| `artifact:P:SEQ` | An artifact sequence, numbered in paint order on the page | `artifact:0:2` |
| `op:P:IX` | A content-stream operator, by execution index on the page, Form XObjects included | `op:0:57` |
| `glyph:P:OP:G` | Glyph G drawn by text operator `op:P:OP` | `glyph:0:57:3` |
| `res:NUM:GEN:KIND:NAME` | A named resource of object NUM GEN (page, form or pattern). KIND is `Font`, `XObject`, `ColorSpace`, `Pattern`, `Shading`, `ExtGState` or `Properties` | `res:5:0:Font:F1` |
| `diag:N` | A diagnostic, by index into `info`'s `diagnostics` | `diag:0` |
| `val:N` | A validation finding, by index into the last report | `val:9` |
| `group:N` | A grouping row in a tree. It has no properties | `group:100` |

`struct:11:0` and `obj:11:0` name the same PDF object: the first as a tag, the second as raw
dictionary.

## Chaining

* `tree --kind tags` → `struct:` ids → `node`, `table`, `render --highlight`.
* `hit` at a point → the ids under it, innermost first → `node` and `breadcrumb` on each.
* `reading-order` and `tag-boxes` give each element's `rect` → `hit` at a point inside it.
* `speech` items give `node` and `marked_content` → `node`, and `render --highlight` on the
  `mc:` ids.
* `tree --kind content:P` gives `op:` ids → `render --step IX` or `--clip IX`.
* A `node`'s rows carry `link` ids to the parent, children, page, object and diagnostics.
