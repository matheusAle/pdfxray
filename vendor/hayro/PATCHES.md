# hayro 0.8.0, patched for pdfxray

This is [hayro](https://github.com/LaurenzV/hayro) 0.8.0 as published on crates.io
(Apache-2.0 OR MIT, see `LICENSE-APACHE` and `LICENSE-MIT`), with one addition. The workspace
uses it through `[patch.crates-io]`.

## Why

pdfxray emulates how other viewers place what lands between device pixels (see
`crates/pdfxray-render/src/emulate.rs`). That has to happen between the interpreter and the
rasteriser, and upstream keeps both its renderer and the glyphs of a run private, so the hook
cannot be added from outside the crate.

## What changed

- `src/rules.rs` (new): the public `PixelRules` trait. Its methods get device-space geometry just
  before it is rasterised and may move it: fills (and how far they overlap), strokes and their
  width, clips, the origins of a run's glyphs, and an image's sample grid. The defaults change
  nothing.
- `src/lib.rs`: `render_into_with`, which is `render_into` with a `PixelRules`. `render_into`
  calls it with rules that change nothing, so it draws exactly as upstream. The rules live in
  `GlobalState`, so soft masks drawn by child renderers get them too.
  `Renderer::rules()` says whether they apply to what is drawn now: not inside a tiling
  pattern's cell, nor to glyph outlines or Type 3 glyph content.
- `src/path.rs`: fills and strokes go through the rules. A fill's overlap is a stroke of the same
  path and paint, twice the overlap wide. `draw_rect` becomes a path when rules apply.
- `src/clip.rs`: clip paths and clip rectangles go through the rules.
- `src/glyph.rs`: each run's glyph transforms are computed once, moved to the origins the rules
  give, and used wherever the run's own transforms were.
- `src/image.rs`: an image's transform goes through the rules.
- `src/mask.rs`: `GlobalState` has a lifetime.
- `Cargo.toml`: the example (not shipped) is dropped.

## Updating

Copy the new release's `src`, `Cargo.toml`, licences and README over this directory, then apply
the changes above again. `diff -ru` against the registry copy shows them.
