# pdfxray architecture

pdfxray shows what is actually in a PDF. Two rules shape everything below:

1. **Never repair.** Readers silently fix broken files; a debugger must show the break. The
   raw layer reports every deviation as a diagnostic tied to exact bytes and still shows what
   is there.
2. **One query API.** The desktop app, the CLI and the tests all ask `pdfxray-core` the same
   questions. The UI computes nothing about the PDF itself.

## Crates

```
pdfxray-raw ──► pdfxray-core ──► pdfxray-render ──► pdfxray-app ◄── pdfxray-macos
                     ▲      └──────────────────────► pdfxray-cli
pdfxray-verapdf ─────┘
pdfxray-fixtures (tests only)        xtask (dev tasks)
```

| Crate | Responsibility |
|-------|----------------|
| `pdfxray-raw` | Lossless lexer and object parser with byte spans; xref tables/streams, hybrid files, object streams, `/Prev` revision chain; stream filters; 40+ raw diagnostics. Never repairs. |
| `pdfxray-core` | `Document`: object index, page tree with box provenance, content capture, structure tree, resources, diagnostics, veraPDF mapping. Exposes `info / tree / node / hit / hit_rect / bbox / breadcrumb / marked_content / captured`. |
| `pdfxray-render` | hayro rasterisation into a page's own pixel grid or a given one, page↔pixel transform, overlays (one `vello_cpu` compositor), the annotation layer (tag boxes, chips, reading-order badges and path, a diff's status boxes and legend, laid out and measured once for the app and the export), the visual diff (side by side, overlay, mask over one composite bitmap), X-ray, clip/alpha views, isolate/hide (of one node or a set)/stepper filtering, optional PDFium reference renderer and image diff, PNG/JPEG output. |
| `pdfxray-verapdf` | Finds or installs veraPDF (pinned, SHA-256 checked), runs it as a separate Java process, parses its XML report, parses context paths into locators. |
| `pdfxray-cli` | `pdfxray` binary: JSON over the core API; `render` writes PNG/JPEG, alone or compared `--against` a file or a `--reference` engine. |
| `pdfxray-app` | egui desktop app. |
| `pdfxray-macos` | The macOS integration that needs Objective-C: the application delegate that receives files from Finder and the Dock, and the system's recent documents. No-op stubs elsewhere. |
| `pdfxray-fixtures` | Deterministic generator of small PDFs, each with machine-readable expected facts. |
| `xtask` | Corpus fetch/verify, vendored-corpus integrity, PDFium fetch, the macOS bundle and the app icon. |

## The app and the operating system

One window is one process. A file that arrives while the window shows a document (Finder's
"Open With…", the Dock, `open -a`, File › Open…, Open Recent) starts a new process of the same
executable with the file as its argument; File › New Window starts one without. A drop onto the
window replaces its document. The routing lives in `pdfxray-app/src/instances.rs`, with the
process start injected (`AppState::spawner`) so tests check it without starting anything. One
egui viewport per process keeps every panel's state single-window; egui's multi-viewport would
need per-viewport state everywhere.

`pdfxray_app::run` builds the winit event loop itself instead of letting eframe do it. On macOS
the files a launch was asked to open arrive as an Apple Event while the loop starts, and `AppKit`
hands them to the application delegate's `application:openURLs:`. winit 0.30 installs its own
delegate when the event loop is built (and panics if it is replaced), without that method, so
`pdfxray-macos` adds the method to the class of the installed delegate between `EventLoop::new`
and `run_app`. Files and menu clicks go into a shared queue (`platform::Inbox`) that wakes egui
and is emptied at the start of the next frame, so what arrives before the first frame is not
lost. The native menu bar (macOS only) is built with `muda`, with winit's default menu switched
off; its item ids map to one `MenuCommand` enum (`commands.rs`), the single place that carries
commands out, shared with the keys the window handles itself where there is no menu bar.

### Unsafe code

The workspace denies `unsafe_code`. The one exception is `pdfxray-macos`, which allows it at the
crate root: adding a method to an Objective-C class at run time cannot be done without it. The
surface is one file (`crates/pdfxray-macos/src/macos.rs`) with two blocks: the cast of the Rust
function `open_urls` to an Objective-C method implementation (`Imp`), and the
`class_addMethod` call that adds it to the delegate's class. The crate denies
`clippy::undocumented_unsafe_blocks` and `clippy::missing_safety_doc`, so each carries its
`// SAFETY:` reasoning. Everything else (the menu bar, the recent documents, the window icon, the
title) goes through safe APIs of `muda`, `objc2` and eframe.

## Opening a document

`Document::from_bytes` is eager (decision 13: typical documents up to ~200 pages):

1. `RawDocument::parse`: header, startxref, xref chain, offset verification, diagnostics.
2. `ObjectIndex`: every indirect object of the newest revision, plus a reverse-reference map.
3. Page tree walk: page objects, five boxes each with provenance (own / inherited from a
   `/Pages` node / spec default), `/Rotate`, `/UserUnit`, resources owner.
4. hayro loads the file for rendering and interpretation. Encrypted files stop here: their
   structure is shown, their content is not interpreted (v1).
5. Content capture for every page (see below).
6. Structure tree: role maps, namespaces, attributes, ParentTree/IDTree, cross-checked against
   captured marked content.

## Content capture

hayro's `Device` reports what is drawn but not which operator drew it. Capture tokenises each
content stream with hayro's own tokenizer, then interprets a copy in which every operator is
preceded by an invisible `/PdfxrayOp <</MCID n>> BDC EMC` marker. The recording device reads
`n` from `begin_marked_content`, so every draw call is attributed to its operator exactly. A
walker tracks graphics state, text state, clipping, marked content and Form nesting itself.
Pages where the two views disagree are marked `Degraded` with a reason instead of guessing.

The walker (`capture/walker.rs`) is a coordinator over small collaborators, one file each under
`capture/`: `engine` (the hayro run and its warnings), `gstate` (graphics state, `q`/`Q`, the
current font), `path` (path construction and `W` clips), `marked` (marked-content sequences),
`scopes` (Form XObject scopes and the recursion guard), `font_cache`, plus the pure helpers
`text`, `color` and `geom`. Degradation reasons are a typed `DegradeReason` whose `Display` is
the text users read. Inside capture, geometry is core `Matrix`/`Rect`/`Point`; `kurbo` appears
only where the engine hands geometry over and inside `path` (exact curve bounds).

## Render modes

hayro's renderer device is private, so isolate, hide and the paint-order stepper render a
modified copy of the file: an in-memory incremental update that replaces the page's
`/Contents` (and affected Form XObjects) with filtered streams. Painting operators outside the
allowed set become no-ops (`n`, invisible text render mode, dropped `Do`/`sh`/inline images);
all state operators are kept so positions stay exact. The original bytes are never modified.

## The annotation layer

Tag boxes, the chips that name them, reading-order badges, the path between them and its notes
are laid out by `pdfxray_render::annotate`: a `Scene` (what to mark, in what colour, with what
chip) on a `Grid` (page space onto output units, the size of an annotation unit, the device pixel)
becomes a resolved list of `Annotation`s, each an `Overlay` shape with the `NodeId` it stands for.
The render crate is the only text measurer (Inter Bold and `JetBrains Mono` via `skrifa`), so the
chip nudging is decided once. The app draws that list with egui and binds its buttons, tooltips
and accessible names to the rects it was given; `render_page` rasterises the same list with
`vello_cpu`. Measurement ignores kerning, which only tightens text, so a label drawn by egui never
outgrows its rect; labels are drawn clipped to it anyway.

## The visual diff

`pdfxray_render::composite` puts two pages into one bitmap: side by side (each on its own grid,
one wide bitmap), overlaid (A's ink and B's in two tints, multiplied) or as a mask (the pixel
diff). For the last two, B is drawn **into A's pixel grid** (`RenderSpec::grid`): a page is
`trunc(W × scale)` pixels wide, so without it a page 0.1 pt wider would change the bitmap's size
and every pixel would compare against the wrong one. A page whose size (beyond 1 pt) or `/Rotate`
differs is refused with the reason, and drawn scaled only on request; the pairing's global offset
is removed only on request, and the output says when either happened. The annotations go on top
through `Composite::annotate`: B's boxes are carried into A's page space so both sides are laid
out together, and the legend gets a band of its own below the pages. A diff colours by status,
never by role; the role is the chip's text. The app and the export call the same function, so
they draw the same picture.

In the app a comparison is `AppState::diff` (`state/diff.rs`): the B document beside the open
one (A), and the pairing, computed on a thread of its own and recomputed whenever either side is
read again (a drop, ⌘R). B arrives by File › Compare With…, an ⌥-drop or
`pdfxray-app a.pdf --compare b.pdf`; in a comparison a plain drop replaces B and an ⌥-drop A.
The canvas (`center/canvas/diff.rs`) shows the composite of one page pair, drawn on its own
thread (`diff_frames.rs`), and draws `composite::layout` over it: the scenes and the layout are
the export's. Each shape says which scene it was laid out for (`Annotation::scene`), because the
two files' element ids can be equal, so a chip is bound to its side. Only what is about a
comparison knows sides: the selection, the hit stack and the breadcrumb. The Changes list is the
primary view; the canvas and the inspector's pair card follow its selection.

## Node identity

Every inspectable thing has a `NodeId` with a stable string form, used by the CLI:

| Form | Meaning |
|------|---------|
| `doc` | the document |
| `trailer` | newest trailer |
| `rev:N` | revision N (0 = original) |
| `obj:N:G` | indirect object |
| `page:P` | page P (zero-based) |
| `box:P:media\|crop\|bleed\|trim\|art` | a page box |
| `struct:N:G` | structure element |
| `mc:P:MCID` | marked-content sequence(s) with that MCID on page P |
| `artifact:P:SEQ` | artifact sequence SEQ (paint order) on page P |
| `op:P:IX` | content operator IX (execution order, including nested forms) |
| `glyph:P:OP:G` | glyph G of text operator OP |
| `res:N:G:Kind:Name` | named resource of owner object N G, e.g. `res:3:0:Font:F1` |
| `diag:N` | diagnostic N |
| `val:N` | validation finding N |
| `group:N` | grouping row in a tree (no properties) |

## Coordinate spaces

* **Page space**: PDF default user space of the page, in points, origin bottom-left, y up,
  before `/Rotate` and `/UserUnit`. All core geometry (`bbox`, hit-testing) uses it.
* **Display space**: after `/Rotate`, origin top-left, as shown on screen.
* **Physical lengths**: page-space lengths × `/UserUnit` (rulers and the measure tool).

## Testing

| Layer | Where |
|-------|-------|
| Unit + property tests | each crate's `src` (`proptest` on the lexer and parser) |
| Fixture facts | `pdfxray-fixtures` generates PDFs whose facts are re-derived by an independent byte scanner |
| Integration | `crates/*/tests`, checked against fixture facts |
| Corpus | 38 vendored veraPDF files (CC BY 4.0) always; 33 fetched files (`cargo xtask corpus fetch`) when present |
| Visual | PNG baselines in `crates/pdfxray-render/tests/snapshots` |
| UI / e2e | `egui_kittest` snapshots and flows in `crates/pdfxray-app/tests` |
| External tools | veraPDF (`PDFXRAY_VERAPDF=required`), PDFium (`PDFXRAY_PDFIUM=required`) |
