---
name: pdfxray
description: Inspect and reason about a PDF with the pdfxray CLI instead of guessing from its bytes or its look. Use when a task involves a PDF's structure, tags, reading order, what a screen reader announces, objects and resources, what sits at a point on a page, tables and their header cells, rendering a page to look at it, comparing two versions of a PDF, or PDF/UA, WCAG or PDF/A validation.
---

# pdfxray

`pdfxray` answers questions about a PDF as JSON: its structure tree, reading order, speech,
tables, objects, what is under a point, and validation findings. It also renders a page to a
PNG you can look at, and compares two PDFs. Run it rather than inferring structure from text
extraction or from how a page looks.

## Before you start

Check that the CLI is there: `command -v pdfxray && pdfxray --version`. If it is missing, read
[references/install.md](references/install.md). It runs on Apple Silicon Macs only. Ask the
user before you install anything.

`diff`, and `render`'s annotation and comparison flags (`--tags`, `--labels`, `--artifacts`,
`--nesting`, `--reading-order`, `--against`, `--reference`, `--mode`), need pdfxray 0.4 or
later; `render`'s Windows viewer emulation (`--emulate`, `--dpi`, `--screen`, `--zoom`) needs
0.5. On an older version, `pdfxray <command> --help` shows what yours has.

## How it works

* Every command prints JSON. Add `--pretty` to indent it. Pipe it through `jq` or a script
  rather than reading large output whole.
* Pages are zero-based on the command line (`0` is the first page). The JSON's human labels say
  `Page 1`.
* Coordinates are PDF points in page space, origin at the bottom left. Boxes are
  `{x0, y0, x1, y1}`.
* Everything is addressed by a **node id** such as `page:0`, `struct:11:0`, `mc:0:2` or
  `op:0:4`. The ids one command prints are the ids the next one takes. See
  [references/node-ids.md](references/node-ids.md).
* Exit status: 0 on success, 1 when the command fails (a file it cannot open, an unknown node
  id), 2 for a bad command line. `diff` is different: 0 when nothing in scope changed, 1 when
  something did, 2 on any error.

## Commands

| Command | Answers | Reference |
|---|---|---|
| `info` | Version, pages, objects, revisions, encryption, diagnostics | [inspect.md](references/inspect.md) |
| `tree --kind …` | The pages, objects, tags, artifacts, resources, diagnostics or content-stream tree | [inspect.md](references/inspect.md) |
| `node`, `breadcrumb` | Every property of one node; the path to it from the root | [inspect.md](references/inspect.md) |
| `hit` | Everything under a point, most specific first | [inspect.md](references/inspect.md) |
| `reading-order`, `tag-boxes` | The order a page is read in; every element's box | [tags.md](references/tags.md) |
| `speech` | What a screen reader announces, item by item | [tags.md](references/tags.md) |
| `table` | A table's grid and each cell's header cells, with the rule that found them | [tags.md](references/tags.md) |
| `render` | A page as PNG or JPEG, with highlights, x-ray, tag boxes, reading order, compared with another file, or as Chrome, Edge or Acrobat on Windows draws it | [render.md](references/render.md) |
| `diff` | What changed between two PDFs, matched by content and position, never by object number | [diff.md](references/diff.md) |
| `validate` | veraPDF findings (PDF/UA, WCAG 2.2, PDF/A…) mapped to node ids | [validate.md](references/validate.md) |

Load only the reference you need. Each lists the command's flags and output fields as the CLI
prints them. For worked tasks (a screen reader skipping something, proving a fix, looking at a
page), see [references/recipes.md](references/recipes.md).

## Ground rules

* Quote what the CLI printed: node ids, rule ids, diagnostic codes. Do not paraphrase them into
  something it did not say.
* When you need to see a page, render it and open the PNG. Do not describe a page you have not
  rendered.
* `speech` presets other than `structure` approximate real viewers; the output says so with
  `"approximation": true`. Say that when you report them.
* `validate` exits 0 even when the file fails. Read `compliant` and `findings`.
