# Lazy loading plan (closes the open point of plan decision 13)

Status: **§4.1 implemented** (D1 approved, option b). **§4.2 on hold** (D2, a product decision).
**§4.3 deferred** (D3). Nothing is committed.

Goal: (a) a large file is usable before every page has been captured; (b) holding more than one
open document in a process is affordable. Never-repair and the one query API stay as they are:
whatever `info` / `diag` / the snapshots report stays complete and reproducible.

## 1. Measurements

Apple Silicon, 10 cores, `--release`, branch fast-forwarded to `origin/main` `f8c7135`
(Release 0.2.0).

> A first round of measurements (on `90650f6`) ran while the machine was busy with other builds
> and came out 2.5–3× slower, with stages that looked superlinear in the page count. Repeated on
> a quiet machine, the stages scale roughly linearly; the numbers below are the repeated ones
> (two runs each, they agree within ~10 %). The proportions, and every conclusion drawn from
> them, are the same in both rounds.

Existing budgets (`cargo test --release -p pdfxray-core --test perf -- --ignored --nocapture`):

| Test | Result | Budget |
|------|--------|--------|
| `long_doc_opens_within_budget` (40 pages) | 1.7 ms | 150 ms |
| vendored veraPDF corpus (38 files) | total 9 ms, slowest 0.7 ms | 200 ms / file |
| `synthetic_200_pages_open_within_budget` | 174 ms (best of 3) | 2 s |
| `synthetic_200_pages_memory_within_budget` | 364 MiB | 500 MiB |
| `synthetic_200_pages_queries_within_budget` | slowest `tree(Tags)` 1.2 ms, cold `hit` 0.5 ms | 5 ms |
| `dense_page_stays_linear` | failed on a clean `origin/main`; **fixed** (F2, §9): open 227 ms, queries ≤ 1.7 ms | 1 s / 5 ms |

All of them are the same after §4.1 (it does not touch opening).

Per-stage split, from timestamps and `ps` RSS taken in the `OpenStage` progress hook of
`Document::from_bytes_with_progress` (one cold run; `BuildStructureTree` split with a temporary
timer around `TagTree::build` and `warm_trees`, since removed):

| Stage | long-doc (40 p) | synthetic 200 p | synthetic 1000 p |
|-------|-----------------|-----------------|------------------|
| ReadStructure (raw parse) | 0.3 ms | 15 ms, +6 MiB | 80–90 ms, +36 MiB |
| IndexObjects | 0.3 ms | 18 ms, +34 MiB | 91–95 ms, +164 MiB |
| ReadPageTree (+ hayro load) | 0.3 ms | 0.7 ms, +1 MiB | 3 ms, +7 MiB |
| **CaptureContent** | **2.3 ms**, +4 MiB | **133–139 ms, +278 MiB** | **0.88–0.92 s, +1.33 GiB** |
| TagTree::build (incl. content cross-check) | 0.3 ms | 29–34 ms | 176–194 ms |
| warm_trees (Tags/Objects/Resources) | ~0 (below threshold) | 13 ms | 71–89 ms |
| BuildStructureTree total | 0.3 ms | 42–47 ms, +34 MiB | 247–282 ms, +39–186 MiB |
| **Open total / RSS after** | 13–15 ms / 8 MiB | **221–235 ms / 364 MiB** | **1.32–1.41 s / 1.6–1.8 GiB** |
| Dropping the `Document` | 0.3 ms | 37–39 ms | 197–224 ms |

What the numbers say:

1. **Capture is the dominant cost**: ~60–65 % of open time and **~80 % of resident memory**
   (≈1.35 MiB per synthetic page; `CapturedPage` keeps every operator with its operands as
   `String`s, every paint item and every marked sequence). The direction of the task holds.
2. **The tag cross-check is second** (~14 % of time) and it reads captured content of every
   page, so it cannot become lazy on its own (see §3).
3. **Tree warming is small** (~5–6 %) and dropping it would not help the app:
   `AppState::install` asks for `tree(Tags)` on the UI thread the moment the document arrives,
   so the cost would only move from the loader thread to a frame. Not done; it rides with §4.2.
4. **The app multiplied all of this** (fixed by §4.1). Besides the UI's `Document`, three worker
   threads each ran a full eager `Document::from_bytes` on their own copy of the bytes:
   `page_cache.rs` (render thread, started by the first page request), `thumbnails.rs` (started
   by the Pages strip) and `previews.rs` (started by the first object preview). Measured: one
   200-page document with all three running cost **1432 MiB in the app, 4.04× the document
   alone**. And for (a): the render thread started its own full open only *after* the UI's open
   finished, so the first page bitmap appeared after ≈ 2 × open time, although a plain render
   (`render_page` with `ContentFilter::All`) needs nothing but the engine page, available after
   ReadPageTree (41 ms at 200 pages, 196 ms at 1000).

So the cheapest, largest win for both (a) and (b) was not lazy capture but **sharing one
document across the app's threads** (§4.1). Lazy capture (§4.2) comes second.

## 2. What attached validation to `Document`, and why the app re-opened

`Document` had exactly one mutating method: `set_validation(&mut self, &Report)`
(`document/report.rs`), storing `validation: Option<ValidationRun>`, read by
`Document::validation()`, the tree badges and `node(Validation(i))` / `bbox(Validation(i))`
(`document/queries.rs`) and the diagnostics tree (`document/diagnostics.rs`). The run is
resolved against the document (`locate` reads captured pages and resources) but nothing else in
the document depends on it. The app called it in one place (`validation.rs::finish_validation`),
the CLI in one (`validate`), and the app read it in ~15.

Because of that single `&mut`, the UI owned its `Document` by value and the workers could not
hold an `Arc` to it; the comment in `page_cache.rs` said exactly this. Everything else was
already `Send + Sync` (`document_can_be_shared_between_threads` tests it).

## 3. The determinism decision

Document-wide facts come out of capture: `mcid-not-in-content`, `mcid-not-in-structure`,
`mcid-duplicate-in-content`, `marked-content-in-artifact`, `untagged-content` and
`reading-order-*` (`tags/checks.rs`, `tags/order.rs`), plus the resource usage that the
Resources tree needs for *every* page (`resources/usage.rs`). If capture ran on demand, these
would depend on which pages had been visited. Rejected outright.

Options weighed:

| Option | Verdict |
|--------|---------|
| A cheap eager scan instead of full capture | No such scan exists: bounding boxes for reading order and untagged runs need the interpreter, i.e. the capture itself. A tokeniser-only scan would give MCID sets but not geometry, so it would report fewer findings than today. Rejected. |
| Typed "complete / still filling" diagnostics that every consumer handles, with partial lists visible meanwhile | Partial lists are exactly the non-determinism we refuse. Rejected in that form. |
| CLI forces full capture, UI stays lazy | Fine for the CLI, but the UI's navigator badges and Problems drawer would still show partial facts. Not enough alone. |
| **Background completion, published atomically** | **Recommended.** |

**Recommended rule:** the document-wide analysis (capture of every page, tag cross-check,
diagnostics, warm trees) always runs to completion, but **off the critical path**. Its result is
published once, as a whole, through a typed state; no consumer can ever observe a partial list.

* `Document::info()`, `tree(Diagnostics)`, `tree(Tags)`, `tree(Resources)` and every other
  document-wide query either return the complete answer or, before the analysis finished,
  `Analysis::Pending { done, total }`. There is no third state.
* The CLI and the tests open with the blocking call (today's `Document::open` /
  `from_bytes`), which returns only after the analysis completed. **CLI output and every insta
  snapshot stay byte-identical**; the existing tests prove it unchanged.
* The app opens with a two-phase call: it receives a usable `Document` after ReadPageTree
  (pages, boxes, objects, engine), renders the canvas at once, and shows the existing Opening
  card's `CaptureContent` / `BuildStructureTree` progress in place of the panels that need the
  analysis until it completes. `OpenStage` already carries that progress; nothing new to invent.
* Per-page queries (`captured(p)`, `hit`, `tree(Content(p))`, `bbox` of content on `p`) may
  capture page `p` on demand before the background pass reaches it. Capture is deterministic, so
  a page captured on demand is identical to the one the pass would produce; it fills the same
  per-page slot and the pass skips it.

## 4. Design

### 4.1 Share one document across the app's threads — **implemented**

Option (b) of the two weighed (the other, (a), moved the run out of `Document` and made the
queries take it; kept as a later option should runs ever need to live apart from documents).

What changed:

1. `pdfxray-core`: `Document.validation` is a private `RwLock<Option<Arc<ValidationRun>>>`;
   `set_validation(&self, &Report)` builds the run outside the lock and swaps it in;
   `validation() -> Option<Arc<ValidationRun>>` hands out a handle to the run as it was when
   asked (a later `set_validation` does not change it). The internal readers (`tree` badges,
   `node`/`bbox` of `Validation(i)`, the diagnostics tree) go through `validation()`. A poisoned
   lock is read through (`PoisonError::into_inner`): the only writer is one assignment.
2. `pdfxray-cli`: `validate` serialises `doc.validation().as_deref()`, so the JSON is unchanged
   (no serde `rc` feature needed); `let mut doc` → `let doc`.
3. `pdfxray-app`: `LoadedDocument.doc: Arc<Document>`; `AppState::install` still takes a
   `Document` and wraps it (one place, so `open.rs` and the test helpers are unchanged). The ~15
   readers of `validation()` hold the `Arc` handle instead of a borrow (`&run` where a function
   takes `&ValidationRun`; three `.and_then(..).map(..)` chains reshaped so they do not return a
   borrow of the temporary).
4. `page_cache.rs`, `thumbnails.rs`, `previews.rs`: `get` and `Worker::spawn` take
   `&Arc<Document>`; the thread is given an `Arc::clone` and renders from it. The thread exits
   when the UI drops its sender (`clear()`), releasing the clone.
5. Dead code removed: `RenderFailure::Open` and `PreviewFailure::Open` with their
   `Arc<CoreError>` payloads and `CoreError` imports, the `Document::from_bytes("render" |
   "thumbnails" | "previews", bytes)` re-opens and their `Err` branches, the `doc.ok()`
   blank-thumbnails path and its comment, the per-thread `doc.bytes().to_vec()` copies, and the
   module docs that justified the copies (now: "shares the UI's document").
6. Tests (fluent assertions plus a smoke assertion each):
   * `page_cache`, `thumbnails`, `previews` unit tests: the first ask goes to the thread,
     `Arc::strong_count` is 2 while the worker lives (UI + thread, one document), back to 1
     after `clear()`; smoke: page 0's `pixel_digest` equals that of `render_page` on a
     separately opened copy (what the thread rendered before), the thumbnail is 120 px on its
     longest side, the preview produced a bitmap.
   * `pdfxray-verapdf/tests/attach.rs`: a run attached from another thread through a shared
     `Arc<Document>` reaches every holder, re-attaching replaces it while an earlier handle
     keeps its run; smoke: `node(Validation(0))` resolves on the other holder.
   * `pdfxray-app/tests/perf.rs` (new, `#[ignore]`d, release): `APP_DOCS_RSS`, below.

Measured (`cargo test --release -p pdfxray-app --test perf -- --ignored --nocapture`): the
probe opens the synthetic 200-page document in the app (kittest harness), waits for page 1 on
the canvas, a Pages-strip thumbnail and the preview of the image `XObject`, and compares how
much the process grew with what `Document::open` alone grows, each in a child process.

| | document alone | app, all three threads running | ratio |
|---|---|---|---|
| before (`origin/main` `f8c7135`, same probe) | 353–355 MiB | 1432–1433 MiB | **4.04–4.06×** (3 runs) |
| after | 353–354 MiB | 408–414 MiB | **1.15–1.17×** (5 runs) |

The remaining ~55 MiB is the app's own state for the document (navigator models of the 20 000
element tag tree, textures), not a second load. Budget `APP_DOCS_RSS` = **1.25×** (the measured
1.15–1.17× plus margin for noise; the projection of 1.15× would have been flaky).

Time to first page: the render thread no longer opens anything, so the first bitmap waits for
one open instead of two (by construction; not separately measured).

User-visible output: unchanged. `cargo test --workspace` (1755 tests, insta and kittest
snapshots included) passes untouched; no snapshot was updated.

### 4.2 Two-phase open with background analysis (step 2) — **on hold (D2)**

Refactors:

1. `ContentIndex.pages: Vec<OnceLock<Option<PageData>>>` (today `Vec<Option<PageData>>`).
   `captured(&self, p) -> Option<&CapturedPage>` keeps its signature: `OnceLock::get_or_init`
   returns a shared borrow, so on-demand capture needs no API change and no locking on reads.
   `ContentIndex::build` becomes "fill every empty slot, in parallel, reporting progress", and a
   page already filled on demand is skipped.
2. `Document` splits its fields by when they are known:
   * known after ReadPageTree: `name, bytes, raw, objects, engine, pages`, structural
     diagnostics;
   * known after the analysis: `tags: OnceLock<TagTree>`, `diagnostics: OnceLock<Vec<…>>`
     (structural + tag diagnostics, published together), the warm `trees`.
3. New public open API: `Document::open_staged(path, progress, cancel) -> Result<Arc<Document>,
   CoreError>` returns after ReadPageTree and starts the analysis on a core-owned thread (rayon
   pool already in `crate::parallel`); `Document::analysis(&self) -> Analysis`
   (`Pending { stage, done, total }` / `Complete`). `open` / `from_bytes` keep their meaning:
   they run the same stages and return when `Complete`. §4.1 already made the app hold
   `Arc<Document>`, so this slots in without another app-wide change.
4. Document-wide queries during `Pending`: `info()` gains `analysis: Analysis` in `DocInfo`
   (serialised only by the app; the CLI always sees `Complete`, and the field is skipped when
   complete so CLI JSON is unchanged). `tree(Tags | Resources | Diagnostics | Artifacts)` return
   an empty tree while pending, and the app gates those panels on `analysis()` — the app never
   shows a pending tree as if it were empty.
5. App: `open.rs` uses `open_staged`; the Opening card closes after ReadPageTree; the navigator
   tabs for Tags / Resources / Problems show the existing stage progress until `Complete`.
   `AppState::install` stops calling `tree(Tags)` synchronously (it reads `tagged` from the
   structure-tree root presence, a ReadPageTree-time fact, or waits for `Complete`).
6. Cancel: dropping the last `Arc<Document>` raises the analysis' cancel flag so closing a file
   mid-analysis stops the workers.

### 4.3 Bounded memory after analysis (step 3) — **deferred (D3), measure right after step 2**

After step 2, RSS after open is still ~eager once the analysis finished. To make a *second*
1000-page document affordable (≈1.6–1.8 GiB each today), the analysis would keep a compact
per-page summary (MCID set, per-MCID bbox and first paint op, untagged runs, duplicate MCIDs,
resource usage) and drop the `CapturedPage`; `captured(p)` would re-capture into a small LRU.
That changes `captured` to return an owned handle (`Arc<CapturedPage>`), which touches
`pdfxray-render` (filter, xray, rewrite) and a dozen core modules.

A PDF-diff feature is being designed in parallel and will want two documents live at once in
one process. Step 3 is the step that decides whether that is affordable for large files, so it
must be measured right after step 2 lands (two 1000-page documents open, after analysis), not
dropped.

### 4.4 The document-wide paint-order stepper (a document-wide fact)

The paint-order stepper walks the whole document (`Document::op_total`, `paint_order_index`,
`op_at`, `next_visible_change_in_document`; `ContentIndex::starts` holds where each page's
operators start). The positions of a page's operators depend on how many operators **every
earlier page** runs, so the track, the `211 / 1120 ops` count and "past this page's end" are
document-wide facts in the sense of §3: they need every page captured. Under §4.2 they belong
with the diagnostics, behind `Analysis::Complete` (the stepper is unavailable, or shows the
analysis progress, until then); they must never be computed from the pages captured so far,
which would move positions as pages arrive. Today, with eager loading, they are always complete.

## 5. What stays eager, and why

| Stays eager | Why |
|-------------|-----|
| Raw parse, xref chain, raw diagnostics | Every node id and every diagnostic span depends on it; ~7 % of open. |
| Object index + reverse refs | Objects tree, `node(Object)` and diagnostics' object attribution need it; ~8 % of open, needed before any query. |
| Page tree + box provenance + engine load | Needed to show page 1; < 1 % of open time. |
| Document-wide analysis (capture all pages, tag cross-check, diagnostics) | Determinism (§3): it always completes, only later (§4.2). |

| Becomes lazy | |
|--------------|---|
| The app's three worker copies | **Done (§4.1):** they share the UI's `Arc<Document>`. |
| Capture of the page the user is looking at | §4.2: on demand, ahead of the background pass. |
| Tree warming | §4.2: part of the background analysis instead of the critical path. |

## 6. Decisions

* **D1 — sharing (§4.1):** approved, option (b). Implemented.
* **D2 — determinism and two-phase open (§3, §4.2):** on hold for the product owner. The
  visible change: for the first moments of a large file the Tags / Resources / Problems panels
  show stage progress instead of content, while the canvas is already usable. The API side:
  `open_staged` / `analysis()` and the `DocInfo.analysis` field (skipped when complete).
* **D3 — step 3 (§4.3):** deferred; measure right after step 2.

## 7. Perf budgets

Existing budgets (`LONG_DOC_OPEN`, `SYNTH_OPEN`, `SYNTH_RSS_MB`, `CORPUS_FILE_OPEN`, `QUERY`)
stay as they are and still pass after §4.1.

| Budget | Value | Proves | Status |
|--------|-------|--------|--------|
| `APP_DOCS_RSS` (`pdfxray-app/tests/perf.rs`) — app with one 200-page doc and the page, thumbnail and preview threads running, ÷ the document alone | ≤ 1.25× (measured 1.15–1.17×; before 4.04×) | §4.1 | **added** |
| `SYNTH_FIRST_PAGE` — `open_staged` returns + page 0 rendered, 200 pages | 100 ms (ReadPageTree is at 41 ms) | (a) | with §4.2 |
| `SYNTH_1000_FIRST_PAGE` — same, 1000 pages | 400 ms (ReadPageTree is at 196 ms) | (a) at scale | with §4.2 |
| `SYNTH_RSS_STAGED_MB` — RSS after `open_staged`, before analysis, 200 pages | 80 MiB (52 MiB at ReadPageTree today) | (b) | with §4.2 |
| `QUERY` | 5 ms (unchanged), measured after `Complete` | no regression | with §4.2 |
| Determinism | `info()`/`tree(Diagnostics)` after `Complete` equal across: blocking open, staged open with no visits, staged open after visiting pages in random order | §3 | with §4.2 |

## 8. Risks

* **Re-entrancy of on-demand capture (§4.2).** A per-page `OnceLock` blocks a second caller
  while the first captures; a UI-thread `hit` on a page the pool is capturing waits for that
  page (≤ one page's capture, a few ms). Acceptable; documented.
* **Panics in the background analysis (§4.2).** Today a capture panic aborts the open. In the
  staged design it must surface as a failed analysis (typed), not a silently `Pending` forever.
* **`DocInfo` shape (§4.2).** Adding `analysis` must not change CLI JSON; covered by keeping it
  skipped when `Complete` and by the existing insta snapshots.
* **Validation readers under a lock (§4.1, done).** Every read takes a read guard just long
  enough to clone an `Arc`; the run is replaced only when a veraPDF run finishes, so contention
  is nil. The `QUERY` budgets are unchanged.
* **Workers outliving `clear()` (§4.1, done).** A thread busy with a long render keeps its
  `Arc<Document>` until that render ends; the document is then freed. Opening a second file
  meanwhile briefly holds both — the same as before, when the thread held its own copy.

## 9. Follow-ups

* **F1 — `render_preview` and the filtered render modes re-parse the file.**
  `pdfxray-render/src/preview.rs:201` and `rewrite.rs:46` call
  `RawDocument::parse(Arc::new(doc.bytes().to_vec()))` per preview / per filtered render. Not a
  long-lived copy (dropped after the call), so not in `APP_DOCS_RSS`, but it is a full raw parse
  each time. Out of scope here; worth measuring.
* **F2 — `dense_page_stays_linear` failed on a clean `origin/main` (`f8c7135`). Fixed by
  changing the fixture**, see below.
* Plan decision 13 still says "Loading hidden behind a trait"; no such trait exists. Update
  decision 13 and `docs/architecture.md` § Opening a document once D2 is decided.

### F2 in detail (fixed)

Confirmed on a temporary clean worktree of `origin/main` `f8c7135` (removed afterwards), with
`cargo test --release -p pdfxray-core --test perf -- --ignored --nocapture --exact
dense_page_stays_linear`:

* Test: `crates/pdfxray-core/tests/perf.rs`, `dense_page_stays_linear`.
* Failing assertion (line 248): `assert!(doc.info().diagnostics.is_empty(), …)` — it expects
  **0** diagnostics.
* It gets **11**, all `reading-order-backtrack` (Warning): "P (step 1800) is read before P
  (step 1801), but P sits above it on page 1, so a screen reader jumps back up the page", and
  the same for steps 3600, 5400, …, 19800 (every 1800th step).
* The open-time assertion after it is never reached (open itself: 233 ms, budget 1 s).

Why: `dense_page_pdf(n)` (`crates/pdfxray-core/tests/common/mod.rs`) places sequence `i` at
`x = 10 + (i % 20) * 29`, `y = 780 - (i / 20 % 90) * 8`: 20 columns × 90 rows = 1800 sequences
fill the page, then `y` wraps back to 780 and the next 1800 are painted over the first ones,
from the top again. With 20 000 sequences that wrap happens 11 times, and each wrap really is
content read later that sits higher on the page. The check reports exactly those 11 wraps and
nothing else.

Reading: **the fixture is what is wrong, not the check.** The check does what it documents
(`goes_back_up`: the next box's top is above the previous one's by more than the tolerance); the
fixture cannot fit 20 000 lines of 8 pt on one 792 pt page without wrapping, and its expectation
of "no diagnostics" was written without a reading-order check in mind. History cannot say which
came first: the fixture, the test and the check all arrive in the same squashed commit
`56ec9c8`. The test is `#[ignore]`d and CI (`.github/workflows/ci.yml`) does not run
`--ignored`, which is how it stayed red unnoticed; but `docs/spec/pdfxray-plan.md` § Status
claims "release-mode perf budgets" as part of a green gate, and that claim is currently false.
Fix (the fixture, not the check): `dense_page_pdf` in `crates/pdfxray-core/tests/common/mod.rs`
now lays the paragraphs out on a page as tall as their rows need (`dense_page_height(n)`:
1000 rows × 8 pt + 2 × 12 pt = 8024 pt for 20 000), so they read strictly top to bottom, left to
right. The magic numbers became named constants (layout and object numbers); the two identical
`kids` / `refs` strings became one `paragraphs` string; a `# Panics` section and an `assert!`
guard the 14 400 pt page-side limit of ISO 32000-2 Annex C (35 940 paragraphs). The function's
signature is unchanged, so `benches/open.rs` keeps working as is.

A new test that runs in every `cargo test` (the budget itself stays `#[ignore]`d),
`dense_page_reads_top_to_bottom_without_diagnostics`, opens 2 000 paragraphs (past the 1 800
where the old layout wrapped). It asserts no diagnostic codes, a `/MediaBox` height equal to
`dense_page_height`, and the last paragraph below the first; the smoke assertion is that all
2 000 marked-content items resolve. With the old `y` formula restored temporarily, it fails
with `["reading-order-backtrack"]`, so it guards the regression.

After the fix the whole release perf suite passes: `dense_page_stays_linear` opens in 227 ms
(budget 1 s); `marked_content` 1.7 ms, `bbox` 1.0 ms, `node` 0.13 ms, `tree(Tags)` 1.1 ms
(budget 5 ms).
