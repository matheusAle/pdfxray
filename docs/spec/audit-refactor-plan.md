# Rust audit: refactor plan

Source: the rust-skills audit of 2026-10-30, a static scan with no code changes. The user asked
for its findings to be acted on. Refactors come before new features, and each step ends with the
full gate: fmt, clippy with default and all features, `cargo deny`, `cargo machete`, and the
workspace tests.

The steps run one after another, not in parallel. They share one working tree and one cargo
target, and nothing is committed, so a git worktree would be empty.

| Step | Scope | Findings |
|---|---|---|
| R1 | **Errors that keep their cause, and no silent failures.** Typed error enums (`thiserror`, `#[source]`) instead of `Result<_, String>` in `page_cache.rs`, `previews.rs`, `center/canvas/mod.rs` and `xtask/src/corpus/fetch.rs`, turned into text only at the UI edge. Recover poisoned locks in `capture/walker.rs` instead of dropping warnings, and collect warnings as typed notes. Check every `.ok()` / `is_ok()` that throws away an I/O error (`open.rs`, `commands.rs`, `recents.rs` and others): report it to the user, or say in a comment why dropping it is right. | 1, 2 |
| D1 | **Not from the audit; it runs here because it touches `previews.rs` after R1.** The designed object-preview cards (board "2.9 Objects · Preview cards", `design/exports/23-objects-preview-cards.png`) for the inspector's Preview section and the centre preview. | design |
| R2 | **Workspace hygiene and crate boundaries.** A `[profile.release]` (LTO, one codegen unit, strip). Every dependency version in `[workspace.dependencies]`. One shared fixture-opening helper for inline tests. `Document::engine()` stops being public API. `pdfxray-core` no longer depends on `pdfxray-verapdf`: the validation data types live where core owns them, and the dependency points the other way. A parameter struct replaces the `too_many_arguments` allow in `tag_overlay.rs`. `cargo doc` with `-D warnings` in CI. `missing_errors_doc` and `missing_panics_doc` on for `pdfxray-core` and `pdfxray-raw`. | 6, 8, 9 (part), 4 (allow) |
| R3 | **Numeric conversions.** One audited conversion module per crate that needs it, replacing scattered `as` casts and `#[allow(clippy::cast_*)]`. Float comparisons use a tolerance or `total_cmp`. | 5 |
| R4 | **The capture walker.** A `DegradeReason` enum instead of strings. The core `Matrix` type instead of raw arrays. `Walker` split into collaborators (graphics-state stack, marked content, XObject scopes, path building, font cache) with the same output, proven by the existing capture tests and snapshot JSON. Review its clones. | 3, 4, 7 (walker) |
| R5 | **Large files.** Split by feature: `raw/filters.rs` one file per codec, `raw/xref.rs`, `raw/parser.rs`, `tags/speech.rs`, `object_facts.rs`, `core/document.rs`, `center/canvas/mod.rs`. Split the largest integration test files by feature, and replace `panic!("not a dict")` in tests with helpers or `let … else`. | 4, 10 |

**R4 result:** `capture/walker.rs` went from 1,501 lines, 57 functions and 28 fields to 916
lines, 32 functions and 13 fields. It is now a coordinator over `engine`, `gstate`, `path`,
`marked`, `scopes` and `font_cache`, with pure helpers in `text`, `color` and `geom`, each in its
own file with unit tests. Degradation is a typed `DegradeReason` whose `Display` keeps the old
text. The walker computes with core `Matrix`/`Rect`; `kurbo` stays at the engine edge and inside
`path`. The public data model (`XObjectScope`, `GState`, `Alignment`) keeps its arrays and
strings, because tests and serialized JSON depend on them. Of the walker's clones, eleven are gone
or now happen only when needed; six of those ran on every operator or glyph. A before/after dump
of all 107 fixture and corpus files (363 pages) is byte-identical, and the capture bench shows no
regression.

Not done, and why:

- **Clone and `format!` volume in per-frame UI paths (7):** the audit itself says to profile first, and there is no measured slowness. Clones are reviewed only where R4 and R5 touch the code anyway.
- **A `tracing` facade (8):** a new cross-cutting dependency and a product decision. Warnings already reach the user as diagnostics.
- **miri for `pdfxray-macos` (9):** its unsafe code calls the Objective-C runtime, which miri cannot execute.
- **Repository URL placeholder (8):** needs the real GitHub owner from the user.
- **`Box<dyn FnMut>` aliases (8):** kept as they are; the intent gets a doc line where R1 or R2 passes by.

**R5 result:** nine source files over about 800 lines became directory modules, split by feature
with the same public paths: `raw/filters` (one file per codec, the chain and shared types in
`mod.rs`), `raw/xref` (table, stream, `startxref`, revision chain), `raw/parser` (direct objects
in `mod.rs`, indirect objects, stream data), `raw/document` (object streams, whole-file checks),
`core/document` (navigation queries, the veraPDF report, diagnostics), `core/tags/speech`
(presets, the walk, the passes, footnote links), `core/tags/parse` (catalog, elements, number and
name trees), `app/object_facts` (naming, decoded lines, formatting) and `app/center/canvas`
(layout, scroll, page, pointer). No file of them is over 520 lines now. Inline tests moved with
their code; helpers used across files sit in a `test_util` module (in `object_facts` a one-line
`ObjRef` shorthand is repeated instead). Items that other files now call are `pub(super)`, which
reaches what private reached before; `ns_index` and the `ParsedObject` re-export got tighter
because nothing outside used them.
The seven integration test files over 1,000 lines (`core/tests/ui_data`, `capture`, `content`,
`tags`, `app/tests/components`, `tags_content`, `validation`) are now one test binary each, with
a `main.rs`, a `support.rs` for shared helpers and one module per feature. Every test kept its
name, the count is the same (1,598 passed, 8 ignored), and no snapshot or expected JSON changed.
The 13 `panic!("not a …")` in the parser tests became `as_dict` / `as_array` / `as_stream` helpers
that show what was parsed instead, and the six in `xtask/src/cli.rs` became `matches!` assertions.

Not done, and why:

- **One `Rect::union`:** `content/paint.rs` builds the box edge by edge, and `tags/view.rs` passes
  the same edges through `Rect::new`, which reorders them again. They agree on every box the code
  makes, but not on an inverted one (`x0 > x1`), so they do not match exactly and both stay. The
  copies in `capture/geom.rs` and `content.rs` do match `paint.rs`; merging those three is a
  separate step.
- **Long but cohesive files:** `capture/walker.rs` (926 lines) is the coordinator R4 just
  produced; `fixtures/src/writer.rs` is 691 lines of one PDF writer plus its tests; and
  `raw/src/diagnostic.rs` is 698 lines of one diagnostic enum with its codes and messages.
  Integration tests just under 1,000 lines (`core/tests/resources.rs`, `raw/tests/synthetic.rs`,
  `app/tests/shell.rs`) stay as they are.
