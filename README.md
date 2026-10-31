# pdfxray

A PDF debugger for **layout and logical structure**. Navigate tags, artifacts, resources and raw
objects; click anything on the page to inspect its attributes, measurements and rendering
properties.

pdfxray shows what is actually in the file. It never repairs, re-tags or optimises anything:
broken cross-reference tables, wrong offsets, orphaned marked content and role-map cycles are
reported where they are, with the exact bytes involved.

## Features

* **Every layer of the file, linked**: raw objects and revisions, page tree and boxes, content
  operators and glyphs, marked content and artifacts, the structure tree, resources. Select a
  glyph and the inspector climbs to its operator, marked content, structure element and page.
* **Click-to-inspect canvas** with a hit stack, layer filter, rulers, measure tool, page-box
  overlays and comparison of two selections.
* **Debug views**: X-ray outlines, invisible-content detection, isolate / hide, an
  operator-by-operator paint stepper, clip and transparency views.
* **Diagnostics** from the syntax layer up: xref and offset errors, stream lengths, duplicate
  keys, page-tree problems, boxes outside the MediaBox, ParentTree mismatches, orphan MCIDs,
  role-map cycles, untagged content.
* **Validation** with veraPDF (PDF/UA-1/2, WCAG 2.2, WTPDF, Tagged PDF, PDF/A), run as a
  separate process, with every finding mapped onto the node it concerns.
* **A CLI** that exposes the same queries as JSON, for scripts, CI and agents.

See the [user guide](docs/user-guide.md) and the [architecture](docs/architecture.md).

## Status

v1 for macOS. Not in v1: decryption of encrypted files (their structure is still shown),
annotation / form / outline panels (planned for v1.1). Plan: [`docs/spec/pdfxray-plan.md`](docs/spec/pdfxray-plan.md).

## Layout

| Crate | Role |
|-------|------|
| `pdfxray-raw` | Lossless, never-repair syntax layer with byte spans |
| `pdfxray-core` | Document model + query API (the only API the UI and CLI use) |
| `pdfxray-render` | hayro rendering, overlays, debug render modes, optional PDFium reference |
| `pdfxray-verapdf` | veraPDF discovery, install, runner, report → node mapping |
| `pdfxray-cli` | `pdfxray` binary: JSON over the core API |
| `pdfxray-app` | egui desktop app |
| `pdfxray-fixtures` | Deterministic PDF fixture builder (tests only) |
| `xtask` | Corpus, PDFium and release tasks |

## Development

Toolchain is pinned to Rust 1.99.1 (`.tool-versions` for asdf, `rust-toolchain.toml` for rustup);
Java 21 is pinned for the veraPDF tests.

```sh
cargo run -p pdfxray-app -- file.pdf
cargo run -p pdfxray-cli -- info file.pdf --pretty

cargo test --workspace                   # unit, property, fixture, corpus, visual, UI/e2e tests
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check                         # licenses: no GPL/AGPL in the dependency graph

cargo xtask corpus fetch && cargo xtask corpus verify   # external corpus (not committed)
cargo xtask vendored verify              # integrity of fixtures/third_party
PDFXRAY_VERAPDF=required cargo test -p pdfxray-verapdf  # with a real veraPDF
cargo xtask bundle                       # target/bundle/pdfxray.app + zip (see docs/releasing.md)
```

Snapshot changes are reviewed with `cargo insta review`; UI and render baselines are generated on
macOS (`UPDATE_SNAPSHOTS=1` regenerates PNG baselines).

## License

MIT, see [LICENSE](LICENSE). Files under `fixtures/third_party/` keep their own licenses (the
veraPDF sample is CC BY 4.0, see its `ATTRIBUTION.md`). veraPDF and PDFium are never bundled:
they are downloaded on request and run or loaded separately.

pdfxray is an independent project designed from ISO 32000-2 and ISO 14289. It is not affiliated
with any commercial PDF product.
