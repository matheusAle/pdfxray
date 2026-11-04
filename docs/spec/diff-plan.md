# pdfxray — PDF diff: plan

Compare two PDFs — most often two versions of one file, same tagging and same visual elements but
every object renumbered — and say what changed: which structure elements correspond, which were
added, removed, moved or modified, and where the ink differs. Exposed in the app (side by side or
overlaid) and on the CLI (JSON, and annotated pages as PNG/JPEG).

Base: branch `add-pdf-diff-overlay-cli` at `bca1a4f` (local `main`, unpushed; `origin/main` is at
`5b32137`, v0.3.0).

## 1. Decisions (grill session, 2026-11-03)

| # | Topic | Decision |
|---|-------|----------|
| 1 | What a diff *is* | **A pairing**, not a picture. Element pairs with a status; the visual delta is one attribute of a pair. Rejected: pixels as the artifact — it cannot see a retag with identical ink, which is the main use case. |
| 2 | Anchors | **Two, staged.** Anchor 1: structure elements (v1). Anchor 2: paint clusters over the *leftover* ink, covering artifacts and untagged content (stage 4). Every pair carries `kind` (`element` \| `ink`) from day one so anchor 2 is not a schema break. |
| 3 | Alignment scope | **Document-wide.** Page correspondence is *derived* from the pairing, never matched by a heuristic. Handles insertion, removal and reflow across page boundaries for free. |
| 4 | Algorithm | **Patience-style anchoring** on unique keys, recursing into the gaps, then a **move post-pass** over the leftovers. Cost scales with the number of differences, not document size. Repeated look-alikes can never be anchors, so they are only resolved inside a narrow gap. |
| 5 | Key ladder | `role + text` → `text` → `role + quantised bbox` → `geometry`. A gap with no anchor at any rung degrades to bounded full alignment *inside that gap* and says so, with a confidence per pair. |
| 6 | Change payload | **Two layers from one comparison**: typed summary fields (role, standard, level, alt, actual text, lang, text, bbox + delta, page) over an exhaustive generic `Props`-row diff. A test asserts they cannot disagree. |
| 7 | Geometry | Global offset estimated from matched pairs, **reported once**; every per-pair delta is the **residual** after removing it. `moved` only beyond a stated tolerance. |
| 8 | Output default | Changes only; `--include-unchanged` for the full pairing. `--scope all\|tags\|ink`, default `all`. |
| 9 | Confidence | Mandatory on every pair: which ladder rung matched it, whether text matched without `ToUnicode`, whether either side's page was `Alignment::Degraded`. |
| 10 | Visual language | **Status takes colour, role moves to the chip text**, with a drawn legend. Role colour would collide with status colour on the same box. |
| 11 | Export format | **Raster only** (PNG/JPEG). No SVG, no PDF. |
| 12 | Annotation parity | **One measurer.** `pdfxray-render` owns the label fonts via `skrifa` (Inter Bold for chips; JetBrains Mono Medium for the backtrack note and Bold for the header-chip `#` mark), computes chip rects and badge placement, returns a resolved shape list; the app places its interactive widgets on *those* rects. Two measurers (egui's and skrifa's) would nudge chips differently in the app and the export. |
| 13 | Compositor | **`vello_cpu` replaces the hand-rolled overlay compositor** for every primitive. Already in the graph via hayro (0.3), `skrifa` 0.46 via `hayro-interpret`: zero new crates. Accepts a one-time baseline regeneration. |
| 14 | "As if on the UI" | Same geometry, colours, labels and reading-order path — **not pixel-identical**. No hover, no tooltips; the legend is drawn as its own block. |
| 15 | CLI shape | `pdfxray diff a.pdf b.pdf` for the JSON. `render` gains **one orthogonal axis — what to compare this page against**: `--against <file>` or `--reference <engine>`. The `compare` verb is **deleted**; it *is* `render --reference pdfium --mode mask`. |
| 16 | Reference engines | `pdfium` now. Later: `quartz` (macOS CoreGraphics, a system framework) and PATH-discovered external renderers under the veraPDF precedent — never bundled, never linked, the user's own install. Adobe is not available as a library. |
| 17 | App state | `AppState.diff: Option<DiffSession>` holding the B `LoadedDocument` and the pairing; `document` stays the A side. No `Vec<LoadedDocument>` — that invites a tab feature nobody asked for and touches every call site. |
| 18 | How B arrives | "Compare With…" menu command, ⌥-drop, and `pdfxray-app a.pdf --compare b.pdf` (so e2e tests need no dialog). In diff mode a **plain drop replaces B** — the side being iterated. |
| 19 | Reload | Explicit **⌘R** in v1. mtime watching is a follow-up; nothing in the app watches files today. |
| 20 | Canvas | **One canvas over a composite bitmap** (side by side = a wide bitmap, overlay = blended). `CanvasState` stays single; per-side hit-testing keeps both transforms. The app and the exported PNG are then literally the same picture. |
| 21 | Defaults | Enter in **side by side**. Only **changed** elements outlined (`⇧T` still shows all boxes). **Changes list is primary**, canvas follows selection. Hide `same` and within-tolerance geometry; **artifact changes stay visible, collapsed to one row per page**. Reading order in overlay shows **one side at a time** (A/B toggle); order changes are read in the Changes list. |
| 22 | Page geometry mismatch | **Never scale silently.** Banner + explicit opt-in "scale B to A". Silent scaling is a repair. |
| 24 | Kerning | Measurement **pads by a worst-case widening per character class** (HarfBuzz-derived: Inter Bold widens a pair by ≤ 45/2048 em between letters and digits, ≤ 234/2048 em beside a raised `*`; JetBrains Mono never kerns), rounded up to egui's 1/32 pt grid. Chips end up ~1 px wider than egui alone would draw them. Rejected: reading Inter's pair-kern table in render — that pins the measurement to whichever shaper egui ships. (egui 0.37 shapes with harfrust, a Rust HarfBuzz port that reads GPOS; Inter Bold carries no legacy `kern` table at all. The bound holds whatever egui shapes with next; a parity measurer would have to be re-verified on every egui upgrade.) **The plan's original premise — that unkerned measurement is a safe over-estimate — was wrong**: Inter kerns some pairs apart (`r`–`t` in "Art", by 0.19 pt), caught by the across-fixtures width test. |
| 23 | Scale ceiling | Diff is budgeted to ~200–300 pages per side. Above a threshold it **warns, never refuses**. Large-document diff depends on the lazy-loading plan's **D3**, which is therefore a prerequisite, not an optional step 3. |

## 2. The model

The spine is a pairing, in `pdfxray-core`:

```rust
pub struct Pairing { pub pairs: Vec<Pair>, pub pages: Vec<PageMatch>, pub offset: Option<Offset> }

pub struct Pair {
    pub kind: PairKind,                    // Element | Ink
    pub status: Status,                    // Same | Modified | Moved | Added | Removed
    pub a: Option<NodeId>, pub b: Option<NodeId>,
    pub page_a: Option<PageIx>, pub page_b: Option<PageIx>,
    pub bbox_a: Option<Rect>, pub bbox_b: Option<Rect>,
    pub delta: Option<(f64, f64)>,         // residual, global offset removed
    pub changed: Vec<FieldChange>,         // typed layer
    pub rows: Vec<RowChange>,              // generic Props layer
    pub confidence: Confidence,            // rung, unicode_source, degraded
}
```

`NodeId` is **not** extended with a side. Nothing but `page:P` and `box:P:kind` is stable across
versions — `obj`, `struct`, `mc`, `artifact`, `op`, `glyph` and `res` all shift — so a side-less id
is meaningless on its own and a pair always names both. The JSON uses explicit `a` / `b` fields,
never a prefixed string, so no consumer can parse a side back into a `NodeId` and lose it.

## 3. Alignment

**Linearisation.** The elements in **document pre-order** (the structure tree's own depth-first
walk), with the `Structure` preset's role semantics — *not* `speech_order`, which this plan
originally specified and which does not work for a pairing: it emits one item per run of marked
content, and a container holding only child elements (`Sect`, `Document`) produces no item at all,
so those elements could never be paired. The `Structure` preset is still what supplies
role resolution, so the diff stays independent of any reader-emulation choice.

**Keys are hashes.** Each element's key is a 128-bit hash of its normalised text (`text::squash`)
plus role and class, with a short display prefix kept beside it. Patience anchoring needs equality
and uniqueness only, so hashing makes the uniqueness test an O(1) `HashMap` check and removes the
memory blow-up where a `Document` element's key would hold the whole document's text.

**Keys are built bottom-up, once.** `text_preview` → `preview` → `collect_text` is recomputed per
call and walks the element's whole subtree; an ancestor's text contains every descendant's, so
20 000 independent calls re-walk the same content once per tree level. One memoised bottom-up pass
builds each element's key from its children's: O(content + elements) instead of O(depth × content).

**Then:** unique-key anchors → recurse into gaps → bounded full alignment in anchorless gaps (over
1M cells it gives up and the output says so) → move post-pass pairing leftover removals with
leftover additions by key similarity → **structural container pass**: leftover containers are
paired by where their children's pairs landed, repeated to a fixpoint, because a container's key is
derived from its children and so changes whenever any child does, while it is still the same
container → derive `PageMatch` from the element pairs by mutual majority, pages with no element
evidence aligned by their residue → estimate the global offset and re-express every delta as a
residual.

**Six things this plan had wrong, found in implementation:**

* **No common prefix/suffix trimming.** The usual diff optimisation is unsound for a pairing: cell
  texts and figure boxes repeat from page to page, so on an appended page it paired A's *last* page
  with B's *new* one. Removed, with an aligner test and a synthetic append test holding the line.
* **The global offset cannot come from a plain median.** Two paragraphs moved by +10 pt and +140 pt
  yield a bogus 75 pt "offset". An offset now requires at least 3 pairs and a strict majority
  agreeing within tolerance, or none is reported.
* **A pure container is never `Moved` by geometry alone.** Its box is the union of its children's,
  so the children carry the change and the container does not report a move of its own.
* **Registration and keying are mutually dependent**, so alignment is two passes when an offset
  exists. Box keys computed before the offset is known bucket text-less elements (figures) into
  different buckets under a global shift, which is why `global-shift-2pt` first came out as two
  removals plus two additions rather than empty. When an offset is found the box keys are rebuilt
  with it removed — each side moved half way, so the result still mirrors — and the alignment runs
  again.
* **The generic `Props`-row layer needs the same geometry awareness as the typed layer.** A Layout
  `/BBox` row differs by exactly the global offset, so the generic layer contradicted the empty diff
  the typed layer correctly reported. Those rows are now compared like the element's own box: offset
  removed, within tolerance.
* **Confidence must report the rung a pair was *anchored* on**, not the best rung on which the keys
  happened to be equal. Identical rows told apart only by their boxes were claiming a `role+text`
  match. A pair now never reports better than the anchors bounding its gap — the honesty property
  decision 9 exists for, and it was silently wrong.

## 4. Visual diff

Three modes, one composite bitmap: `SideBySide`, `Overlay` (A and B as two tints, multiply),
`Mask` (reuses `render::diff`). Both sides rasterise at the same scale **into one pixel grid** —
`RenderedPage` is `trunc(W*scale)` px, so a 0.1 pt size difference changes bitmap dimensions;
`RenderSpec` gains an explicit target grid (size + offset) so B can be drawn into A's grid instead
of falling back to `diff`'s union-with-delta-255. Tolerance defaults differ by purpose: the existing
`48` is tuned for cross-engine comparison, same-engine file-vs-file wants 8–16.

## 5. The annotation layer (the largest piece)

`Overlay` is rect-only today, composited by hand-rolled pixel loops (`paint_ring`, `coverage`,
`blend`), while every annotation the UI draws — role boxes, labelled chips with collision nudging,
numbered badges, the reading-order path with its dashed detour segment, nesting, artifact chips —
lives in egui in `app/src/center/canvas/tag_overlay.rs` (784 lines) with its own layout logic. The
CLI cannot produce any of it.

Resolution: the **geometry and label layout move into `pdfxray-render`** as a resolved shape list
(`Overlay::{Frame, Text, Polyline, Dashed, Badge}`), rasterised with `vello_cpu`, with Inter Bold and
JetBrains Mono (Medium and Bold) embedded there via `skrifa`. The app consumes the same shape list, draws it with egui, and
attaches interaction to the chip and badge rects it was given — the chips and badges stay buttons
with hover, tooltips and AccessKit labels, because the e2e tests query them. This also pays off the
architecture rule the app currently bends: chip layout is PDF-derived geometry computed in the UI.

## 6. Degradation — what the diff refuses to guess

| Condition | Behaviour |
|---|---|
| Encrypted side (open stops before capture) | Structure-level diff only, no visual diff, stated in the output. |
| `Alignment::Degraded` page | Pairs on that page carry the degradation in their confidence; the page's visual diff is labelled degraded. |
| No `ToUnicode` | Keys fall back to glyph-code/advance signatures; `UnicodeSource` records that the match was made without Unicode. |
| Page size or `/Rotate` differs | Overlay is not registered; banner and explicit opt-in to scale. Never silent. |
| No anchor at any ladder rung in a gap | Bounded full alignment inside the gap, low confidence, reported. |
| Op streams reshuffled, ink identical | Not a difference. Op-level diff is never done across files — only within one file across revisions. |

## 7. Refactors this requires (not optional)

1. `pdfxray-render`: overlay compositor → `vello_cpu`; `Overlay` gains `Text`/`Polyline`/`Dashed`/`Badge`; **delete** `paint_ring`, `coverage`, `blend` and the hand-rolled rect path. Regenerate 13 render baselines and the affected subset of 39 app PNG snapshots (macOS only, reviewed).
2. `pdfxray-render`: annotation geometry + label layout move in from the app; `RenderSpec` gains the explicit pixel grid.
3. `pdfxray-app`: `tag_overlay.rs` becomes a consumer of the shape list — delete the layout and nudging code it no longer owns; collapse the duplicated role-colour decision (`app/src/roles.rs::role_color` vs `render::role_slot`) onto one.
4. `pdfxray-cli`: **delete** `Command::Compare`, its arm in `run`, and migrate `tests/compare.rs` onto `render --reference`. Add the View-menu flags (`--tags`, `--labels`, `--artifacts`, `--nesting`, `--reading-order {structure|paint|speech}`) to `render` — they are new for single-file render too, and two divergent flag sets would be worse than one.
5. `pdfxray-app`: `DiffSession` slot; thread `Side` through `selection`, the drawer hit stack and `breadcrumb` only — not through state that is not diff-aware.
6. Dead-code sweep after each step, per CLAUDE.md.

## 8. Performance and resources

| Fact | Number |
|---|---|
| One 200-page document | 221–235 ms open, 364 MiB |
| Two, as a diff session | ≈ 730 MiB — within budget |
| One 1000-page document | 1.32–1.41 s, 1.6–1.8 GiB |
| Two, as a diff session | 3.2–3.6 GiB — **out of budget**, needs D3 |
| `CapturedPage` | ≈ 1.35 MiB per page, and the matching keys *require* capture on both sides |
| Pairing itself | 20k pairs, hashes not strings — single-digit MB |

Pairing runs once per pair, off the UI thread, on the existing worker + progress-card pattern, and
is invalidated when either side reloads. The visual diff stays per-page and lazy. The composite
decision means one render-cache entry per visible page instead of three, so `CAPACITY: 12` stays.

**New budgets**, in CI's perf job: pairing time for two 200-page documents · RSS of a live diff
session · composite render time per page · the bottom-up key pass in isolation, so a regression in
it cannot hide inside the total.

## 9. Testing

`pdfxray-fixtures` gains a **variant**: a pair is (base, declared mutation) with the **expected
diff** as machine-readable facts beside the generator, exactly as `expected.rs` works today.

| Variant | Proves |
|---|---|
| `renumber-objects` | **Expected diff: empty.** Object identity is irrelevant — the core claim. |
| `rewrite-content-same-ink` | Expected diff: empty. Op streams are never diffed across files. |
| `global-shift-2pt` | Expected diff: empty, one reported offset. Registration works. |
| `reorder-tags` | Order change, no ink change. |
| `move-paragraph-3pt` | Residual geometry, `moved`. |
| `drop-alt`, `change-heading-level`, `retag-figure-as-formula` | Typed field changes. |
| `insert-page` | Page correspondence derived, not matched. |
| `artifact-page-number` | The collapsed-artifact-row default. |

Plus two cheap, strong invariants run over the whole corpus (38 vendored veraPDF files, and the
fetched tier when present): **identity** — a document paired with itself yields the empty diff —
and **mirror** — pairing A→B and B→A yields mirrored statuses (`added` ↔ `removed`).

Then: insta JSON per pair · exit codes (`0` identical, `1` differences, `2` error, `--exit-zero`) ·
determinism (byte-identical JSON across runs and across page visit orders) · the typed-vs-generic
agreement test · visual baselines for the three modes · kittest snapshots and AccessKit labels for
the Changes list and every new overlay widget · fluent assertions with an explicit smoke assertion
per test, per CLAUDE.md.

## 10. Staging

| Stage | Content |
|---|---|
| **1 — core + CLI** | Pairing spine, bottom-up hashed keys, patience anchoring, key ladder, moves, derived page correspondence, both payload layers, `pdfxray diff`, exit codes, fixture pairs, identity and mirror invariants. No UI, no renderer. |
| **2 — visual + export** | Annotation refactor (§5, §7.1–7.4), `vello_cpu` compositor, baseline regeneration, pixel grid, registration, three modes, `render --against` / `--reference`, `compare` deleted. Also: **fold `Overlay::Rect` into `Frame`** and migrate the CLI's `outline()` (`Rect` survived stage 2's refactor only because that agent could not touch the CLI), and **expose table-header scope from core** (`TagView::scope_of`) so an export can draw the header-scope arrows — the app currently derives them from its own table data, which is the last place the UI computes a PDF fact the exporter cannot reach. Independently: the diagnostics / veraPDF-findings diff ("what got fixed, what broke"), which rides stage 1's pairing and needs no renderer. |
| **3 — app** | `DiffSession`, `Side` threading, Compare With… / ⌥-drop / launch argument, ⌘R, composite canvas, Changes rail, suppression chips, A/B reading-order toggle. |
| **4 — follow-ons** | Anchor 2 (ink pairs over leftovers: artifacts, untagged content, untagged files) · mtime watching · revision-vs-revision diff. |

## 11. Risks

* **Baseline churn** (§7.1) is the one irreversible-feeling step. It is reviewed, macOS-only, and
  the alternative — two anti-aliasing paths in one picture — is a visible defect.
* **One measurer** (§1.12) means the app's chips are positioned by `skrifa` metrics while egui draws
  the glyphs. Labels must be drawn clipped to the rect the layout computed, and a visual test must
  catch overflow.
* **Anchorless documents** (a table of identical rows, a globally retagged file) fall to the bottom
  of the ladder. Bounded and reported, but the output is weaker there, and the fixture set must
  include one so the degradation is exercised rather than theoretical.
* **The ceiling** (§1.23). Two 1000-page documents will swap. Warning without refusing is the right
  behaviour for a debugger, but it must be measured and stated, not discovered.
* **`--reference` beyond pdfium** reintroduces the licence question. The veraPDF precedent (separate
  program, never bundled, never linked, user's own install) is the only shape allowed.

## 12. Follow-ups

* **Revision diff for free.** The diff API takes two *document views*, not two paths, so a revision
  pair plugs in. `Document::from_bytes(&bytes[..rev_end])` on the byte prefix up to a revision's
  `startxref` would give a full model of an earlier revision and reuse everything. Caveat: a prefix
  is not always a valid document (linearised files especially) and never-repair means it may load
  with diagnostics. Spike before committing.
* Lazy-loading **D3** moves from "defer" to prerequisite for large-document diff; **D2** is
  unrelated to this feature.
* Decision 1 of `pdfxray-plan.md` ("v2: small edits … + revision diff view") is partly satisfied by
  the above; update it once the spike lands.
