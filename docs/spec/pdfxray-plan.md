# pdfxray — Implementation Plan

pdfxray is an MIT-licensed, Rust-native **PDF debugger** for layout and logical structure: navigate tags, artifacts,
resources and raw objects; click anything on the page and inspect its attributes, measurements and rendering properties.

It is **not** a remediation, auto-tagging or optimization tool.

> Clean-room rule: pdfxray is designed from ISO 32000-2, ISO 14289 (PDF/UA) and public documentation. No code, assets or
> decompiled internals of any proprietary product are used.

## Status (2026-10-29)

M0–M6 implemented. Full gate green: fmt, clippy `-D warnings` (with and without `pdfium`),
~960 tests incl. real veraPDF and PDFium runs, `cargo deny`, `cargo machete`, release-mode perf
budgets. Not done: Developer ID signing / notarization never exercised (needs Apple
credentials); universal build untested locally (no x86_64 target installed); the release
workflow has not run on GitHub yet.

## 1. Decisions (grill session, 2026-10-28)

| # | Topic | Decision |
|---|-------|----------|
| 1 | Scope | Read-only v1. v2: small edits written only as **incremental updates** + revision diff view. |
| 2 | Engine | **hayro** stack (syntax / interpret / raster) is the core. Our `Device` impl captures every painted op. **PDFium** (`pdfium-render`) is an optional, feature-gated *reference renderer* for diff views and visual tests. |
| 3 | Parser | Own **lossless, never-repair raw layer** (`pdfxray-raw`) next to `hayro-syntax`. Disagreements become diagnostics. |
| 4 | UI | **egui** + `egui_tiles`; tests via **egui_kittest** (snapshots + AccessKit-driven e2e). |
| 5 | Corpus | 3 tiers: generated (MIT, in repo) · vendored veraPDF sample (CC BY 4.0, no Isartor) · fetched + SHA-256 pinned (PDF/UA Reference Suite, pdf.js, PDFBox). |
| 6 | Harness | Headless **core query API**; UI only renders it. Same API exposed as `pdfxray` CLI with JSON output. |
| 7 | Click model | **Hit stack + breadcrumb**, plus a **layer mode** filter (All / Tags / Content / Annotations / Boxes / Artifacts). |
| 8 | Measurement | Multi-space coordinates + CTM chain · rulers + measure tool · page-box overlay · text metrics. |
| 9 | Panels v1 | Tags · Content + Artifacts · Resources + Objects. (Annotations / Forms / Outline / OCGs → v1.1) |
| 10–11 | Validation | **veraPDF in v1**, subprocess; detect on PATH/config, else one-click pinned download (SHA-256). Java is the user's. |
| 12 | Platform | **macOS first** (arm64 + x64). Visual baselines produced on macOS. |
| 13 | Scale | **Eager** loading, typical docs (≤ ~200 pages / 20k struct elems). Loading hidden behind a trait so lazy can come later. |
| 14 | Render modes | Isolate/hide · X-ray outlines · clip + gstate viz · paint-order stepper. |
| 15 | Sequencing | Walking skeleton first (M0–M6 below). |
| 16 | Name | **pdfxray** |
| 17 | Encryption | Unsupported in v1: show `/Encrypt` dict, refuse to interpret/render. **Revisit:** decided on the belief that hayro cannot decrypt; M0 found `hayro-syntax` 0.8 *does* (stub → `LoadPdfError::Decryption(PasswordProtected)`), so empty-user-password files are nearly free to support. |

Toolchain: Rust **1.99.1** pinned in `.tool-versions` (asdf). Edition 2024. MSRV = pinned version.
Licensing: dependencies must be MIT / Apache-2.0 / BSD / Zlib / ISC / MPL-2.0-as-subprocess. **No AGPL/GPL linking** (MuPDF, iText excluded). Enforced with `cargo-deny`.

## 2. Workspace layout

```
crates/
  pdfxray-raw/        lossless lexer, xref sections/streams, revisions, byte spans, raw diagnostics. No repair.
  pdfxray-core/       Document model + query API: NodeId, struct tree, content capture (hayro Device),
                      hit-testing, geometry/measurement, diagnostics, loader trait.
  pdfxray-render/     hayro rasterization + overlay compositing + render modes; `pdfium` feature = reference renderer.
  pdfxray-verapdf/    veraPDF discovery/download, subprocess runner, XML report → NodeId mapping.
  pdfxray-cli/        `pdfxray` binary: JSON over the core API.
  pdfxray-app/        egui/eframe desktop app (UI only — no PDF logic).
  pdfxray-fixtures/   deterministic PDF fixture builder + generated fixture catalog.
xtask/                corpus fetch/verify, snapshot review helpers, release tasks.
fixtures/
  generated/          output of pdfxray-fixtures (committed, MIT)
  third_party/verapdf/  curated CC BY 4.0 sample + LICENSE + ATTRIBUTION.md
  corpus.toml         manifest of fetched (tier 3) files: url, sha256, license note
```

Dependency direction: `raw ← core ← {render, verapdf} ← {cli, app}`. `fixtures` is a dev-dependency only.
`app` and `cli` must never import `hayro*` or `pdfxray-raw` directly — everything goes through `pdfxray-core`.

## 3. Core data model

**Stable identity** — every inspectable thing has a `NodeId`; all panels, the canvas and the CLI speak NodeIds.

```rust
enum NodeId {
    Object(ObjRef),                              // raw COS object (num, gen) in a given revision
    Revision(u32),
    Page(PageIx),
    StructElem(ObjRef),                          // or synthetic id for direct-object struct elems
    MarkedContent { page: PageIx, mcid: i32 },
    Artifact { page: PageIx, seq: u32 },         // artifact MC sequences have no MCID
    Op { page: PageIx, stream: StreamPath, ix: u32 },   // content op; StreamPath walks Form XObject nesting
    Glyph { page: PageIx, op: u32, glyph: u32 },
    Resource { owner: ObjRef, kind: ResKind, name: Name },
    PageBox { page: PageIx, kind: BoxKind },
    Diagnostic(u32),
    Validation(u32),
}
```

**Captured page** (built by the hayro `Device` impl, eagerly per page):
- `Vec<PaintItem>` in paint order: kind (fill/stroke/glyph/image/shading/xobject-begin/end), source `Op` id, full
  graphics state snapshot (CTM, clip, colors+spaces, alpha, blend, SMask, line params, text state, font ref),
  device-space bbox, page-space bbox, local-space bbox, marked-content stack at paint time.
- An R-tree (`rstar`) over page-space bboxes → hit-testing returns the stack, ordered smallest→largest / topmost first.

**Cross-links** (computed at load): MCID ↔ StructElem via ParentTree **and** via scanning `/K` (both kept, mismatches are
diagnostics) · Op ↔ MarkedContent · Glyph → Font resource → Object · Annotation ↔ StructElem via `/StructParent`.

## 4. Core query API (shared by UI, CLI, tests)

```
open(path) -> Doc                    info(doc)            diag(doc)
objects(doc, rev?)  object(doc, ref) xref(doc)            revisions(doc)
tree(doc, kind = struct|content|artifacts|resources)      node(doc, NodeId) -> Props
hit(doc, page, x, y, layer) -> Vec<NodeId>                breadcrumb(doc, NodeId)
measure(doc, a, b) -> Distance/Offset                     render(doc, page, RenderSpec) -> Image
validate(doc, profile) -> Report (mapped to NodeIds)
```

CLI mirrors it: `pdfxray info|diag|objects|object|xref|tree|node|hit|measure|render|validate <file> ...`, JSON on stdout,
`--pretty` for humans. `render --overlay tags,mcid,boxes,xray --isolate <node> --step <op> -o out.png`.

## 5. Test harness

| Layer | Tool | What |
|-------|------|------|
| Unit | `cargo test` | per-module; fluent assertions + an explicit smoke assertion per test |
| Property / fuzz | `proptest`, `cargo-fuzz` | raw lexer/xref never panics; round-trip of spans |
| Integration | `insta` (JSON/YAML) | CLI/API output per fixture (tier 1 exact, tier 2 reviewed snapshots) |
| Visual | PNG diff (`image` + tolerance) | hayro render + overlays per fixture; hayro-vs-PDFium diff (feature `pdfium`) |
| UI | `egui_kittest` snapshots | panels and canvas at fixed sizes |
| E2E | `egui_kittest` harness | open fixture → click node/canvas → assert inspector contents |
| Corpus | `cargo xtask corpus` | tier 3 fetch + verify; nightly crash/regression run |
| Hygiene | `clippy -D warnings`, `rustfmt`, `cargo-deny`, `cargo-machete`/dead-code check | every PR |

Rules: every fixture documents its *expected* facts next to its generator; snapshot updates require review
(`cargo insta review`); visual baselines only regenerated on macOS.

## 6. Milestones (each has a Definition of Done = all listed tests green in CI)

**M0 — Harness skeleton.** Workspace + crate stubs, `.tool-versions`, CI (macOS: fmt, clippy, test, deny), fixture
builder with first fixtures, CLI stub (`pdfxray --version`, `info` returning placeholder JSON), app shell with dock
layout (egui_tiles; egui_dock lags behind egui 0.37) and empty panels, kittest smoke + first UI snapshot, xtask corpus manifest + fetch/verify, vendored veraPDF sample.

**M1 — Walking skeleton.** raw: lexer, classic xref + xref streams + object streams, revisions, spans, diagnostics.
core: open (eager loader trait), objects/object/xref. render: hayro page raster. app: Objects panel + canvas + click →
hit stack (page-level only) → inspector. CLI: `objects`, `object`, `xref`, `render`. E2E: open → click object → inspector.

**M2 — Content capture.** hayro `Device` capture of all paint items + gstate + bboxes, Form XObject nesting, R-tree
hit-test, glyph level. Content panel (op list ↔ canvas), Artifacts panel, X-ray mode, breadcrumb, layer mode.

**M3 — Tags.** Struct tree, RoleMap (incl. PDF 2.0 namespaces), attributes/classes, ParentTree cross-check, orphan MCID /
untagged content / cycle diagnostics, reading-order overlay, full cross-linking across panels.

**M4 — Measurement + render modes.** Multi-space coordinates + CTM chain, rulers, measure tool, page boxes, text metrics,
isolate/hide, clip + gstate viz, paint-order stepper. Resources panel with inheritance + usage.

**M5 — veraPDF.** Discovery, pinned download, subprocess runner, profile picker, XML report → NodeId mapping, badges.

**M6 — Release.** PDFium reference/diff view, perf pass, docs, signed + notarized macOS build, GitHub release.

## 7. Risks

- hayro is pre-1.0 (no encryption, some blend/knockout gaps) → wrap behind `pdfxray-core`, contribute upstream, PDFium diff view exposes gaps.
- **Confirmed in M0** (hayro-interpret 0.8 `Device`): callbacks give paths, glyph runs, images, clips, transparency
  groups, `DrawProps` (CTM, paint, soft mask, blend) and `begin_marked_content(tag, mcid)`. They do **not** give
  operator index / byte span, marked-content property dicts (e.g. `/Artifact <</Type /Pagination>>`), Form XObject
  begin/end, text state (Tc/Tw/Tz/TL/Ts/Trise) or the font resource name. Plan: in M1 spike a pdfxray op walker over
  `hayro-syntax` content ops that tracks op index, span, MC property stack, XObject nesting and text state, then align
  it with the `Device` callbacks in paint order (both are deterministic). Propose an upstream "op location" hook to hayro
  to remove the alignment step later.
- hayro 0.8 ignores `/UserUnit` and does not expose Bleed/Trim/Art boxes; and it **silently repairs** wrong xref
  offsets (fixture `broken-xref-offsets`). Page boxes and xref truth must come from `pdfxray-raw`, never hayro (M1/M4).
- Eager loading on large files → loader trait from day one.
- egui look → invest in a theme in M6.
