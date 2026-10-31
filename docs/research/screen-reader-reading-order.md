# How screen readers read PDFs: reading order and speech, with a pdfxray proposal

Research note for the reading-order feature (`docs/spec/ui-revamp-v2.md` §3, `docs/spec/ui-revamp-v2-plan.md`
milestones V0/V5, `design/exports/12-tags-overlay-reading-order.png`).

- **Date:** 2026-10-29.
- **Status:** research only. Nothing in this note is decided; part 2 is a proposal.
- **Clean-room:** every claim comes from public standards, vendor documentation and help pages, public issue
  trackers, published test results, or the public source of open-source viewers (Chromium, PDFium, pdf.js, WebKit,
  androidx, NVDA). Those viewers were *read to describe behaviour*; no code was copied.
  - NVDA is GPL. It is only described here and must never be vendored or linked.
  - Nothing was decompiled.
- **Confidence:**
  - **H**: read in a primary source, or confirmed by two independent sources.
  - **M**: from one secondary source, or inferred from open-source code we read but did not run.
  - **L**: from a search snippet, a single community report, or our own inference.
- **Citations** are keys in square brackets, such as [A-SDK-MSAA]. §8 lists every key with its URL and section.
- **Versions:** viewer behaviour changes between releases, so each claim gives the version or date we saw it at.
  Code observations are from each project's `main` branch as read in October 2026.

---

## 0. Summary

**What decides what a user hears (H)**

- What a user hears depends far more on the **viewer** than on the screen reader. The screen reader can only speak
  what the viewer exposes through the platform accessibility API.
  - Acrobat/Reader on Windows exposes the structure tree through MSAA plus its own DOM [A-SDK-MSAA].
  - Firefox (pdf.js) exposes it as ARIA in a hidden DOM [PDFJS-STL].
  - Preview and Safari pass it on through closed-source PDFKit [WK-AX].
  - **Chrome, and Edge builds on the Chromium viewer, do not use tags by default.** The tag-reading code exists but
    sits behind the `PdfTags` feature, which is `FEATURE_DISABLED_BY_DEFAULT` [CR-FEAT]. By default these viewers read
    text in content-stream order and guess at structure [CR-HEUR][IAG][PM].
  - Android's built-in viewer exposes one flat block of text per page [ANDROIDX].

**Tag-aware viewers walk the tree the way the standard says (H)**

- They do a depth-first, pre-order walk of `/K` [ISO1 §14.8.2.3.1][BPG §3.2.2].
- Artifacts are not exposed [ISO1 §14.8.2.2][A-SDK-MSAA][PDFJS#20550].
- Acrobat, and therefore NVDA and JAWS in Acrobat, treat `/Alt` or `/ActualText` on **any** structure element as
  replacing its whole subtree: "the child count is always zero" [A-SDK-MSAA][NVDA#15012].
  - So an `/Alt` on a `Sect` or on the `Document` silences everything under it.
  - pdfxray does not show this today.

**Where tag-aware viewers disagree (H/M)**

- **Alt vs ActualText precedence.** Acrobat-based AT speaks Alt and drops ActualText [NVDA-SRC]. The Best Practice
  Guide asks for both [BPG §5.5.2].
- **Alt on a Figure that has children.** pdf.js replaces the children only under specific conditions.
  Chromium's experimental tag mode keeps them [CR-STRUCT].
- **Role-map chains.** pdf.js and PDFium follow only one RoleMap hop [PDFJS-CORE][PDFIUM-ST].
- **Table headers.**
  - Chromium marks every TH as a row header [CR-ROLES].
  - pdf.js uses Scope, then position.
  - NVDA in Acrobat uses Headers/Scope, but RowSpan and ColSpan are buggy [NVDA#5140][NVDA#21020].

**Footnotes are never moved (H for Acrobat+NVDA, M otherwise)**

- No viewer or screen reader we found relocates a Note to its reference. A note is read where it sits in the
  structure tree [NVDA#14819].
- The best practice is to put the Note directly after the element that holds the reference
  [BPG §4.2.8][ISO1 §14.8.4.4]. That placement *legitimately* jumps down the page and back up, which pdfxray's
  `reading-order-backtrack` will flag as a false positive. So will a correct two-column layout (§4.1).

**`/Tabs` sets keyboard focus order, not reading order (H)**

- `/Tabs` only orders annotations for the Tab key [ISO1 §12.5 Table 30][WCAG-PDF3].
- PDF/UA-1 requires `/S` [MH 28-008/28-009]. PDF/UA-2 allows `A`, `W` or `S` [WTPDF §8.9.3.3].

**Acrobat's reading preferences (H)**

- "Infer reading order", "Left-to-right, top-to-bottom" and "raw print stream" apply to **untagged** files only.
- "Override the reading order in tagged documents" applies the chosen method to tagged files and ignores the tags
  [A-READ][A-SRG].
- For documents over 50 pages, Acrobat by default exposes only the visible pages [A-READ][A-SRG][NVDA#20466].

**Proposal in one line**

Add a **speech order**: a linear "what would be announced" transcript, derived from the structure tree with the
documented substitution and skip rules. It is a third reading-order source next to Structure and Paint, with a few
clearly labelled viewer profiles, table-header resolution, note linking, and about 20 new diagnostics tied to
Matterhorn and WCAG. Part 2 has the details.

---

## 1. Three orders, and which one assistive technology uses

| Order | Defined by | pdfxray today |
|---|---|---|
| **Paint (content) order** | Content stream execution order. Artifacts are included. ISO calls this "page content order" [ISO1 §14.8.2.3.1] | `ReadingOrderSource::Paint` (first painted operator per element) |
| **Logical structure order** | "Defined by a depth-first traversal of the page's logical structure hierarchy"; artifacts "shall be included in the page content order but not in the logical structure order" [ISO1 §14.8.2.3.1] | `ReadingOrderSource::Structure` (elements that own content, in tree order) |
| **Speech / AT order** | What a given viewer exposes, then what the screen reader linearises: structure order *after* substitutions (Alt, ActualText, /E), skips (artifacts, Private, Alt-covered descendants), annotation placement and table semantics. For untagged files or tag-ignoring viewers, it is the content order or a layout heuristic | **Missing.** This note proposes it |

- **PDF 1.7.** "An accessibility application extracts content by traversing the structure hierarchy and presenting
  the contents of each node" [ISO1 §14.9.1]. (H)
- **PDF 2.0.**
  - The PDF 1.7 `TagSuspect` / `/Suspects` ordering escape hatch was removed [ISO2-ERR].
  - PDF/UA-2 says the logical content order "shall be semantically correct" and equates this with WCAG SC 1.3.2
    [WTPDF §8.2.3][VP-UA2]. (H)
- **The requirement is human-judged.** The PDF/UA-1 requirement ("Tags not in logical reading order") is
  Matterhorn **09-001**, which is *not* machine-checkable [MH][MH-PM]. pdfxray's order views are therefore tools for
  human judgement. They are not validation verdicts. (H)

---

## 2. Findings by topic

### 2.1 Where the order comes from

**Adobe Acrobat / Reader (Windows): NVDA, JAWS, Narrator**

- Acrobat "uses the logical structure of a Tagged PDF document to determine word order" [A-SDK-ACC]. (H)
- The MSAA subtree under the Document object "reflects the logical structure of the document" [A-SDK-MSAA, "PDF
  Document"]. (H)
- Read Out Loud also follows the structure tree in tagged files [A-READ][A-REFLOW]. (H)
- **Reading preferences** (Edit > Preferences > Reading) [A-READ][A-REFLOW][A-SRG]. (H)

  | Option | Effect |
  |---|---|
  | *Infer reading order from document* (recommended) | "advanced method of structure inference layout analysis" for **untagged** files. Acrobat adds **temporary tags** at open; these must be rebuilt each time, and AT cannot interact while tagging runs |
  | *Left-to-right, top-to-bottom* | Layout order. "Form fields are ignored and tables aren't recognized" |
  | *Use reading order in raw print stream* | Content-stream order. Same limits as above |
  | *Override the reading order in tagged documents* | Applies the chosen method to tagged files, ignoring the tags. Adobe: use it "only when you encounter problems in poorly tagged PDFs" |
  | *Page vs Document* | Default: "For large documents, only read the currently visible pages". The threshold defaults to 50 pages [A-SRG]. In that mode AT cannot leave the visible page until it is scrolled into view; NV Access calls this "a feature, not a bug" [NVDA#20466] |
  | *Confirm before tagging documents* | Prompts before temporary tagging |

  - An NVDA developer reported in 2017 that Acrobat ignores tagging entirely whenever any mode other than "Infer"
    is selected. A 2019 tester could not reproduce this [NVDA#1216]. (L, conflicting)
- **The Order panel (Reading Order tool)** reorders highlighted regions for reflow and repair [A-ORDER]. Adobe does
  not say whether it rewrites the tag tree or only the content order (undocumented). Community experts say AT
  follows the tags tree [WEBAIM-18527]. (M)
- **Malformed or empty trees.** "Malformed structure trees can also make a document appear empty." AT then gets
  "Alert: Empty document" [A-SDK-MSAA, "Processing empty documents"]. (H)
- **Not documented:**
  - What Acrobat does with a structure tree when `/MarkInfo /Marked` is false or missing.
  - Whether the Order panel affects AT.

**NVDA and JAWS in Acrobat** consume Acrobat's MSAA tree plus its DOM (`IPDDomNode`, `IPDDomElement::GetStdName`).
They do not reorder content [A-SDK-MSAA][NVDA-SRC]. (H for NVDA, from source; M for JAWS, since JAWS is closed and
documented only through release notes [JAWS-WN].)

**Chrome, and Edge on the Chromium viewer (PDFium)**

- **Default mode: no tags.** Text comes from PDFium's text page, which walks page objects in content order. Paragraph
  and heading boundaries are guessed from font size and line spacing [CR-HEUR]. (H, from code)
  - **Heading guessing is switched off for tagged files** (`mark_headings_using_heuristic = !is_tagged`), so a tagged
    file loses its headings entirely [CR-HEUR]. (H) This matches the reports of "one heading, wrong level" [PM]
    and of reading in "visual stacking order, not the tags" [IAG]. (H)
  - "Tagged" is decided by `FPDFCatalog_IsTagged`, which follows the PDF 1.7 definition of a tagged PDF
    [PDFIUM-CAT]. (M)
- **Tag mode** (`--enable-features=PdfTags`, off by default, not in chrome://flags, written January 2025 to March
  2026) [CR-FEAT][CR-CL]. (H)
  - Builds one tree per page from `/StructParents` and the ParentTree. Children keep `/K` order.
  - Untagged text on a tagged page is attached after the preceding tagged text [CR-STRUCT]. (M, from code)
- **Untagged or image-only files.** An on-device OCR feature ("Screen AI") announces "This PDF is inaccessible.
  Extracting text, powered by Google AI" [CR-OCR][CR-WAI-IG]. (H) We found no Chrome version for its rollout.

**Microsoft Edge with the Adobe-powered reader**

- The engine switch was announced on 2023-02-08 [EDGE-BLOG]. The policy page promises "greater accessibility"
  [EDGE-POL]. Edge's PDF page claims screen-reader support on Windows and macOS [EDGE-PDF]. (H)
- Nothing public says whether it exposes the tag tree.
- PowerMapper's December 2025 results give JAWS and NVDA a 25% reliability score with Edge 143, the same as with
  Chrome 143 [PM]. That suggests Edge still behaved like Chromium for PDFs at that point. (L, inferred)

**Firefox (pdf.js)**

- A hidden structure DOM is built inside the `<canvas>`. `aria-owns` pulls the text-layer spans into **structure
  order** [PDFJS#13171][PDFJS-STL]. (H)
- Untagged text stays in text-layer DOM order, which is content order. (M, inferred from the architecture)
- Annotations were placed by visual position [PDFJS#15237]. Since August 2026 they are placed by their OBJR position
  in the tag tree [PDFJS#21768]. Files without a ParentTree are handled since October 2026 [PDFJS#22115]. (H)

**Apple: Preview, Safari and PDFKit (VoiceOver)**

- Safari delegates the accessibility tree to PDFKit (`[_pdfDocument accessibilityChildren:]`) [WK-AX]. (H)
- PDFKit's rules are undocumented. A 2022 developer-forum question about tags got no answer [APPLE-FORUM]. (H)
- **Tests show PDFKit follows tags today.**
  - Preview 11 and Safari 15.4 (May 2022) followed the tag order and voiced headings, alt text and tables with
    header labels [IAG].
  - VoiceOver with macOS Safari 26 and iOS Safari 18.6 scored 75% reliability (December 2025) [PM]. (H as a test
    result, undocumented as behaviour)
- Older reports (2014–2020) said Apple ignored tags [WEBAIM-2014]. The behaviour changed at an undocumented point. (L)

**Android: TalkBack**

- Chrome for Android opens PDFs with androidx.pdf, not PDFium [CR-ANDROID]. (H, from code)
- The androidx.pdf viewer exposes each page as **one virtual view**: "Page N: …" followed by all of the page's text,
  plus OCR text. Links and form fields are separate views. There are no headings, no tables and no alt text
  [ANDROIDX]. (H, from code)
- The platform `PdfRenderer` API exposes text, image alt text and links, but no tag tree [ANDROID-PDF]. (H)
- **Adobe Acrobat Reader mobile**
  - Reads tagged paragraphs, headings, links and alt text. Lists and tables were "not currently fully supported" as
    of 2017 [A-MOBILE17]. (H, dated)
  - Liquid Mode re-derives structure with AI [A-LMODE]. Adobe recommends keeping the original layout for tagged
    files [A-MOBILE20]. (H)

**Does `/Tabs /S` matter for reading? No; it sets focus order.**

- `/Tabs` orders annotations for keyboard Tab navigation. With `S`, annotations "shall be visited in the order in
  which they appear in the structure tree" [ISO1 §12.5 Table 30]. (H)
- **PDF/UA-1** requires `/Tabs /S` on every page with annotations: Matterhorn 28-008 (missing) and 28-009 (not S)
  [MH][VP-UA1]. (H)
- **PDF/UA-2** allows `A`, `W` or `S` [WTPDF §8.9.3.3][VP-UA2]. (H)
- **Acrobat** has a preference, "Use Document Structure For Tab Order When No Explicit Tab Order Is Specified"
  [A-READ]. (H)
- Browse-mode reading order is unaffected by `/Tabs` in every viewer we examined. (M, inferred; no viewer documents a
  link between the two)

**How the tree is walked (tag-aware viewers)**

- **Depth-first pre-order over `/K`**, in array order [ISO1 §14.7.2 Table 323, §14.8.2.3.1][BPG §3.2.2]. (H)
  - Content items are leaves. The hierarchy "is represented entirely by the K entries… not by nesting of the
    associated content items" [ISO1 §14.7.4.1]. (H)
- **Inside a marked-content sequence**, text is in content-stream order. PDF 1.7 also defines a
  `ReversedChars` tag for reverse-order show strings [ISO1 §14.8.2.3.3]. (H)
- **OBJR** (annotations, links, widgets) sits at its position among the parent's `/K` entries.
  - "Annotations… shall be determined from the document's logical structure" [ISO1 §14.8.2.3.2]. (H)
  - An annotation's element placed immediately before or after a marked-content element is treated as preceding or
    following it [ISO1 §14.8.2.3.2]. (H)
  - pdf.js now follows OBJR placement [PDFJS#21768]. (H)
  - Chromium's default mode places links and widgets next to the nearest text run, ignoring OBJR [CR-HEUR]. (M)
- **ParentTree, `/StructParent(s)`**
  - Chromium builds its per-page tree from the page's `/StructParents` through the ParentTree, using PDFium's
    `LoadPageTree`, which gives up beyond 32 levels of nesting [CR-STRUCT][PDFIUM-ST]. (M)
  - pdf.js needed an explicit fix for files with no ParentTree [PDFJS#22115]. (H)
  - Acrobat's dependence on the ParentTree is undocumented. Its repair tools ("Fix parent tree") suggest that it
    matters. (L)
  - **Consequence for pdfxray:** a ParentTree that disagrees with `/K` (`parent-tree-mismatch`) can make browser
    viewers expose a *different* tree from the one pdfxray walks. That is a speech-order risk, not just a structural
    nit.

### 2.2 What gets skipped (and what does not)

| Content | Spec | Acrobat (NVDA/JAWS) | pdf.js | Chromium default | PDFKit |
|---|---|---|---|---|---|
| Artifacts (pagination header, footer, page number, watermark, layout, background) | Not in logical order; a TTS engine "probably should not speak running heads or page numbers" [ISO1 §14.8.2.2] | Not exposed: "Artifacts are not read by a screen reader or by the Read Out Loud feature" [A-EDIT]; headers and footers outside the structure are "not presented through the MSAA interface" [A-SDK-MSAA]. (H) | `aria-hidden` since January 2026 [PDFJS#20550] (H); exposed before that | **Read**: an artifact image is announced "unlabeled graphic" [PM]; no artifact check in the code [CR-HEUR]. (H) | Skipped [PM]. (H, test) |
| Untagged content on a tagged page | Forbidden by PDF/UA (Matterhorn 01-005) | Content outside the logical structure is not presented [A-SDK-MSAA]. (H) | Stays in the text layer in DOM order (M) | Read in content order | Unknown |
| Hidden text (`Tr 3`, white-on-white, clipped, off-page) | Still content: "a text-to-speech engine may choose to speak text that is not visible" [ISO1 §14.8.2.2.3]. An invisible OCR layer is endorsed [BPG §5.5.3.1] | Not documented. Presumably spoken, since it is text. (L, inferred) | Text layer includes `Tr 3` text; that is how OCR layers become selectable (M) | Spoken (PDFium extracts it) (M) | Unknown |
| Empty elements | Allowed only for TD, LI, Span, Div, Document, NonStruct and Private; empty TD cells must still be conveyed [BPG §3.5] | Empty TH shifts the headers in some reports [NVDA#13889] (M) | — | — | — |
| `NonStruct`, `Div`, `Part`, `Sect` | NonStruct "shall not be interpreted… descendants shall be processed normally" [ISO1 Table 333]; grouping only | Grouping role, no announcement (M) [A-SDK-MSAA] | `NonStruct` → `none`, Part/Sect/Div → `group` [PDFJS-STL] (H) | Div/NonStruct → generic; Part flattened [CR-ROLES] (H) | Unknown |
| `Private` | "Neither the Private element nor any of its descendants shall be interpreted" [ISO1 Table 333][BPG §4.1.7] | Undocumented | No mapping (generic span) (M) | Unknown tag → generic container (content read) [CR-ROLES] (M) | Unknown |
| Content marked both artifact and tagged | Forbidden: 01-003 (artifact inside tagged content) and 01-004 (tagged content inside an artifact), both machine-checkable [MH] | Undocumented which one wins | Artifact spans hidden (M) | Read | Unknown |
| Descendants of an element with `/Alt` or `/ActualText` | "Alternate description of the structure element and its children" [ISO1 Table 323]; ActualText is "exact replacement for the structure element and its children" [ISO1 §14.9.4] | **Hidden**: "child count is always zero" [A-SDK-MSAA]; "Adding alternate text to a parent tag prevents a screen reader from reading any of that tag's child tags" [A-EDIT]; Alt on Document hides the whole file [NVDA#15012] (H) | Figure with Alt + BBox + content child: replaced by `role=img`. Otherwise the label goes on the element (`aria-label`, or `aria-description` for name-prohibited roles) and the children stay [PDFJS-STL] (H) | Tag mode: Figure named from Alt, children still walked; on text elements Alt becomes the description [CR-STRUCT] (M) | Unknown |

### 2.3 Tables

**Standard model** [ISO1 §14.8.4.3.4 Table 337, §14.8.5.7 Table 349][ISO2-ERR]. (H)

- **Structure:** Table → (THead | TBody | TFoot)? → TR → TH | TD.
- **Caption** is the Table's first or last child. PDF 2.0 limits it to one Caption [ISO2-ERR Table 372].
- **Attributes** (owner `/Table`):
  - `RowSpan` and `ColSpan` (TH/TD; default 1).
  - `Headers`: array of TH IDs, resolved recursively.
  - `Scope`: Row | Column | Both (TH).
  - `Summary`: on the Table, "for use in non-visual rendering such as speech or braille".
  - `Short` (PDF 2.0): abbreviated header text.
- **PDF 2.0 header-finding algorithm** [ISO2 §14.8.4.8.3, Table 371][ISO2-ERR]:
  - It applies when there is no `Headers` attribute.
  - Search from the cell toward the start of the table in the WritingMode directions.
  - Stop at the table edge, at a data cell found after a header cell, or at a header that has its own Headers.
  - A Scope that conflicts with the search direction does not stop the search.
  - Results are ordered from most specific to most general.
  - (M: read through the errata, not the full body text.) **pdfxray can implement this exactly.**
- **Split tables.** A table spanning pages is one Table element. Header rows repeated on later pages are artifacts
  [BPG §4.2.6][WTPDF §8.2.2]. THead and TFoot "may be redrawn" on each page fragment, but there is only one of each
  [ISO1 §14.8.4.3.4]. (H)
- **Simple vs complex.** Use Scope for simple tables and Headers/ID for complex ones. Reserve Summary for information
  AT cannot otherwise get [BPG §5.4.1–5.4.2]. Empty cells are TD, never TH [BPG §4.2.6]. (H)
- **PDF/UA-2** [WTPDF §8.2.5.26]. (H)
  - Tables and row groups shall be regular.
  - Scope is required where the default algorithm is insufficient.
  - Headers is required on all header-bearing cells where Scope cannot express the relationships.

**Matterhorn 1.1 table failure conditions** (PDF/UA-1 §7.5) [MH][MH-PM]

| ID | Condition | How checked |
|---|---|---|
| 15-001 | A row header cell is not tagged TH | Human |
| 15-002 | A column header cell is not tagged TH | Human |
| 15-003 | In a table without Headers/IDs, a TH has no Scope | Machine |
| 15-004 | Content tagged as a table is not tabular | Human |
| 15-005 | A cell's headers cannot be determined unambiguously | Human |
| 09-004 | Table structure nesting does not follow ISO 32000-1 Table 337 | Machine |

**How it is announced**

- **Acrobat + NVDA** (H, from source and issues)
  - NVDA reads `Summary`, `Headers`, `Scope`, `ColSpan` and `RowSpan`. Explicit Headers override implicit headers
    [NVDA-SRC].
  - Navigation: Ctrl+Alt+Arrows; the "row/column headers" document formatting setting [NVDA-UG].
  - Known failures:
    - RowSpan/ColSpan header errors, still reproducible with Acrobat 2023 [NVDA#5140][NVDA#5141].
    - A TD in THead announced as a column header [NVDA#9046].
    - A cell beside a RowSpan unreachable in a Headers/IDs table, open in 2026; JAWS could reach it [NVDA#21020].
    - Repeated headers caused by bad Headers arrays [NVDA#18184].
- **Acrobat + JAWS** (H, release notes; details M)
  - Keys: T / Shift+T, Ctrl+Alt+Arrows, Ctrl+Alt+NumPad5 [A-SRG].
  - Release-note history [JAWS-WN]:
    - JAWS 12: "now correctly reads table headers in PDF documents".
    - JAWS 2020: fixed columns misread and the table summary not read.
    - JAWS 2023: fixed multi-page tables.
  - Community reports [A-COMM-TH][A-COMM-AT]:
    - In 2018, JAWS announced column headers only after Scope was set.
    - JAWS read cells with ActualText or Alt as "blank".
- **Acrobat MSAA roles:** TABLE, ROW, CELL, ROWHEADER, COLUMNHEADER. Cells support spatial `accNavigate`
  [A-SDK-MSAA]. (H)
- **pdf.js** [PDFJS-STL][PDFJS#21702]. (H)
  - TH maps to `rowheader` or `columnheader` from Scope. Without Scope, a TH in a TR inside TBody is a row header;
    otherwise it is a column header.
  - Since August 2026: RowSpan/ColSpan map to `aria-rowspan`/`aria-colspan`, Headers to `aria-describedby`
    (recursive), Summary to `aria-description`, and `Short` to a hidden abbreviation.
  - Before that fix, first-column cells were repeated [IAG].
- **Chromium tag mode** maps every TH to `rowHeader`, regardless of Scope [CR-ROLES]. Scope, Headers and spans are
  not handled (inferred from code [CR-STRUCT], M).
  - Default mode: row cells merged, no table announced [IAG][PM]. (H)
- **PDFKit/VoiceOver** announces a table with header labels [IAG][PM]. On iOS, column headers are read by Read Item
  but not by Read Screen [PM]. (H as a test)
- **Things no source covers:**
  - Nested tables (only JAWS 9's "reports nested tables with level and size" [JAWS-WN]).
  - Layout tables (ARIA has no "presentation" route in PDF; Matterhorn 15-004 says don't tag them as Table).
  - Behaviour with a missing TH beyond "no headers".

### 2.4 Footnotes, endnotes and references

**Standard model**

- **PDF 1.7**
  - `Note` is an inline element. It "may be included as a child of the structure element in the body text that
    refers to it, or… elsewhere… accessed by… Reference". "Tagged PDF does not prescribe the placement of
    footnotes" [ISO1 §14.8.4.4 Table 338]. (H)
  - Footnotes may be "interspersed" or placed "at the end of the page" in content order [ISO1 §14.8.2.3.1]. (H)
- **PDF/UA-1 §7.9**
  - Notes shall be Note (19-001, human) and references shall be Reference (19-002, human).
  - A Note needs an `/ID` (19-003, machine) that is unique (19-004, machine) [MH][VP-UA1]. (H)
- **PDF 2.0 and PDF/UA-2** [WTPDF §8.2.5.14][VP-UA2]. (H)
  - `FENote` replaces Note: "The Note standard structure type shall not be present."
  - The referring element uses `/Ref` to the FENote, and the FENote uses `/Ref` back.
  - Interactive references use a Link annotation with a **structure destination** (§8.8).
  - Attribute owner FENote has `NoteType` = Footnote | Endnote | None (default None).
  - A Lbl is a descendant of the element that groups the label with what it labels (§8.2.5.16).
- **Best practice** [BPG §4.2.8]. (H)
  - "Locate the note directly following the reference in the logical reading order."
  - The well-known alternative: the Note "occurs immediately after the structure element containing the Reference".
  - Consumers should search forward for a Note whose Lbl matches. They *may* skip Notes during sequential reading and
    let users return to the Reference.

**What AT actually does**

- No viewer or screen reader moves a note to its reference. Each is read at its tree position.
  - NVDA + Acrobat Reader 2020: the footnote link at the marker is announced but not followed, and the footnote is
    read where it sits, at the end [NVDA#14819]. (H)
  - No source documents special handling in JAWS. (Undocumented)
  - In NVDA's Acrobat backend, `Note` and `Reference` have no role mapping [NVDA-SRC]. (H)
- **pdf.js** maps Note, FENote, Aside and Annot to ARIA `note` [PDFJS-STL]. (H) Whether NVDA or JAWS announce a `note`
  boundary in Firefox is undocumented. (L)
- **Chromium** has no Note mapping, so a note is a generic container [CR-ROLES]. (H)
- **Consequence:** where a note sits in the tree *is* where it is heard. "Footnote far from its reference" is a
  meaningful, measurable diagnostic. The BPG-recommended placement creates a spatial jump that the backtrack
  heuristic must not flag (§4.1).

### 2.5 Images, Alt, ActualText, /E and math

- **Semantics** (H)
  - **Alt** [ISO1 §14.9.3]:
    - A "complete (or whole) word or phrase substitution for the current element".
    - Consecutive Alt elements are separated by a word break.
    - Allowed on **any** structure element already in PDF 1.7, not only Figure and Formula.
    - Also allowed on a marked-content `Span`, and as an annotation's `/Contents`.
  - **ActualText** [ISO1 §14.9.4]:
    - A "character substitution", an "exact replacement" for the element and its children.
    - No word break between consecutive runs.
    - PDF 2.0 changed the definition in Table 355 [WTPDF §8.5.2]. The exact new wording is unconfirmed.
  - **E (expansion)**: a word or phrase substitution with an implied word break [ISO1 §14.9.5][WCAG-PDF8].
- **Figure requirements** (H)
  - PDF/UA-1 §7.3: Figure needs Alt or ActualText (13-004, machine). ActualText used where Alt is appropriate is
    13-005 (human) [MH].
  - PDF/UA-2 §8.2.5.28: the Figure encloses all of its appearance and has Alt or ActualText [WTPDF].
  - Since PDF/UA-2, an image *may* be a Span with ActualText [WTPDF §8.2.2].
  - Decorative images are artifacts [WCAG-PDF4].
- **Captions** (H)
  - Caption is the first child (read before) or last child (read after) of the captioned element
    [WTPDF §8.2.5.27][ISO2-ERR Table 372].
  - For PDF/UA-1 Figures, the BPG prefers an *adjacent sibling* Caption, preferably after, "since current AT doesn't
    expect children". Present both the Caption and the Alt [BPG §4.1.3, §4.3.1].
- **Precedence and children in practice**
  - **Acrobat/MSAA:** an element with Alt or ActualText has zero children. Its value is the attribute
    [A-SDK-MSAA]. (H)
  - **NVDA:** Alt wins over ActualText and replaces the descendants [NVDA-SRC]. This works around Acrobat still
    including descendant text [NVDA#2174]. (H)
  - **ActualText on Figure** is still announced as "graphic: …" by NVDA [NVDA#18843]; closed "by design", disputed.
    (M)
  - **ActualText on H1/H2** stops Acrobat exposing them as headings [NVDA#18148]. (M: diagnosis by a commenter)
  - **BPG** advice [BPG §5.5.2–5.5.3]. (H)
    - Prefer ActualText on a Span, not on Figure, TH or TD. Some APIs replace the whole TD/TH, which breaks the table.
    - If both Alt and ActualText are present, both must be available.
  - **pdf.js** falls back from Alt to ActualText as a single `alt` value [PDFJS-CORE]. (H)
  - **Chromium tag mode** ignores struct-level ActualText (a `TODO` in the code). PDFium applies *marked-content*
    `/ActualText` in its text extraction, in both modes [CR-STRUCT]. (M)
- **Empty Alt.** PowerMapper's "blank alt text" case is in its "expected to fail" set [PM]. Behaviour per viewer is
  not documented here. (L)
- **Images of text and scans.** Put OCR text underneath in render mode 3. Alt or ActualText on a whole scanned page is
  "almost always semantically inappropriate" [BPG §5.5.3.1]. (H)
- **/E support** (H unless marked)
  - Acrobat's DOM exposes E, and "alternate text, actual text, and expansion attributes are included and may
    override text" [A-SDK-MSAA, DOM].
  - NVDA's backend has no /E handling (inferred from source, L).
  - Chromium tag mode puts E in the description [CR-STRUCT] (M).
  - Whether JAWS speaks /E is undocumented.
- **Math**
  - The MathML namespace is `http://www.w3.org/1998/Math/MathML`, the only domain-specific namespace in PDF 2.0.
    `math` sits only under Formula [ISO2-ERR §14.8.6.3][WTPDF §8.2.5.29]. (H)
  - NVDA + Acrobat supports "Tagged PDF with MathML structure elements" through MathCAT [NVDA-UG]. (H)
  - Formula Alt is silent without a math reader [NVDA#18405]. (M)
  - pdf.js reads MathML from associated files or `MSFT_MathML`, hides the matching text, and drops Alt
    [PDFJS#20384][PDFJS#20567][PDFJS#20551]. (H)
  - Chromium maps Formula to `math` [CR-ROLES]. (H)

### 2.6 Other edge cases

- **Lists** (H unless marked)
  - Structure: L → LI → (Lbl, LBody). Nested lists go inside LBody [ISO1 §14.8.4.3.3][WCAG-PDF21].
  - The marker belongs in Lbl. NVDA will not synthesise numbers from `ListNumbering` [NVDA#2177].
  - PDF/UA-2 requires `ListNumbering` (not None) when Lbl is present [WTPDF §8.2.5.25].
  - Matterhorn 16-001 to 16-003 [MH].
  - Mappings: Chromium Lbl → `listMarker` [CR-ROLES]. pdf.js gives Lbl and LBody no role [PDFJS-STL].
  - A Lbl that is a bullet glyph is spoken as the glyph's Unicode, for example "bullet". (L, inferred)
- **Headings** (H unless marked)
  - PDF/UA-1:
    - The first numbered heading must be H1 (14-002).
    - No skipped levels (14-003).
    - At most one H per node (14-006).
    - No mixing H with Hn (14-007) [MH].
  - PDF/UA-2 forbids `H`, requires H1–Hn, has no sequential-level rule, and adds `Title`, which "shall not be
    identified as a heading" [WTPDF §8.2.5.12–13].
  - pdf.js gives Hn `aria-level=n` and H a heading with no level [PDFJS-STL].
  - Chromium tag mode makes every heading a plain heading with no level (inferred from code, M) [CR-ROLES].
- **Links** (H unless marked)
  - Structure: Link → (text content items + OBJR). One OBJR, plus QuadPoints, when the link wraps lines
    [ISO1 §14.8.4.4.2][WCAG-PDF11].
  - **Name:** Alt or ActualText if present, "otherwise… the value of the first content child" [A-SDK-MSAA, "PDF
    Link"].
  - **`/Contents` is generally not spoken:** NVDA does not read it, and the reporter says JAWS doesn't either
    [NVDA#13982][NVDA#15229][BPG §4.2.12].
  - PDF/UA-1 still requires it (28-012) [MH]. PDF/UA-2 relaxes it to "should" and requires link content to be
    contiguous in reading order [WTPDF §8.9.2.4.2].
  - PDF/UA-2 puts internal-target links in Reference [WTPDF §8.2.5.20].
- **Forms**
  - A Widget belongs in a Form element (28-010) with one widget per Form [MH][WTPDF §8.10]. (H)
  - The name comes from `TU` [WCAG-PDF10][WCAG-PDF12]. (H)
  - Tab order is `/Tabs` (§2.1).
  - pdf.js read form fields out of Acrobat's order before OBJR placement [PDFJS#16564]. (H)
- **TOC/TOCI.** TOCI holds Reference(Lbl, Span), optionally with Link + OBJR. Leaders are artifacts
  [BPG §4.1.4][WTPDF §8.2.5.8]. Chromium maps TOC → `docToc` and TOCI → `listItem` [CR-ROLES]. (H)
- **Multi-column layouts.** A mis-tagged two-column page is read straight across [WCAG-PDF3]. (H)
  - Content-order viewers (Chromium default, Android) read columns in whatever order the producer painted them.
  - A correct structure order jumps from the bottom of column 1 to the top of column 2.
- **Aside and sidebars.** PDF 2.0 Aside is content outside the main flow. Its parent is the deepest element it
  relates to [WTPDF §8.2.5.9]. pdf.js maps it to `note` [PDFJS-STL]. (H)
- **Across pages**
  - One element keeps its content items on several pages [BPG §3.4][WTPDF §8.2.2]. (H)
  - Chromium builds **per-page** trees: each page is a region "Page N", so cross-page elements are split
    [CR-STRUCT]. (M)
  - pdf.js loads the structure tree per page, so the same is likely there. (L, inferred)
  - Acrobat's visible-pages mode limits AT to rendered pages [A-READ]. (H)
- **Role mapping**
  - AT gets the **standard** role. Acrobat's `GetStdName` "returns the standard role for this element" [A-SDK-MSAA].
    (H)
  - pdf.js (`roleMap.get(name) ?? name`) and PDFium (`GetType`) follow **one** RoleMap hop only
    [PDFJS-CORE][PDFIUM-ST]. A chain such as `MyHead → Heading1 → H1` therefore reaches browsers as the
    intermediate custom name, which maps to a generic role. (M) pdfxray resolves full chains.
- **/Lang**
  - Resolved hierarchically: catalog → structure element → marked content → in-string escape [ISO1 §14.9.2]. (H)
  - NVDA passes per-element Lang down for automatic voice switching [NVDA-SRC][NVDA#1737]. (H)
  - PowerMapper (December 2025) [PM]. (M: per-test pages)
    - Phrase language: only VoiceOver passes.
    - Document language: Firefox passes; Chrome, Edge and Safari fail.
- **ActualText on spans (ligatures, hyphenation).**
  - Marked-content ActualText is applied by PDFium and Acrobat (M).
  - Soft hyphens shall be U+00AD [ISO1 §14.8.2.2.3]. (H)
- **Span, Quote, Code.** Span has no semantics [WTPDF §8.2.5.17]. The BPG suggests TTS signal quotes with "voicing
  changes or beeps" [BPG §4.1.2.3]. Mappings: Chromium Span → `staticText`; pdf.js Code → `code` and Em → `emphasis`.
  (H)
- **ARIA and platform role mappings**

  | Tag | Acrobat MSAA [A-SDK-MSAA] | pdf.js [PDFJS-STL] | Chromium tag mode [CR-ROLES] |
  |---|---|---|---|
  | Document | DOCUMENT (value = root Alt) | — | generic |
  | Part / Sect / Div | GROUPING | group | docPart / section / generic |
  | Art | GROUPING | article | article |
  | H1–H6 / H | (implicit, DOM) | heading + level / heading | heading (no level) |
  | P | (implicit) | paragraph | paragraph |
  | L / LI / Lbl / LBody | (implicit) | list / listitem / — / — | list / listItem / listMarker / none |
  | Table / TR / TH / TD | TABLE / ROW / ROW-, COLUMNHEADER / CELL | table / row / row-, columnheader / cell | table / row / rowHeader / cell |
  | THead / TBody / TFoot | — | rowgroup | rowGroup |
  | Figure | GRAPHIC | figure (+ `img` for Alt) | figure + image |
  | Formula | — | — (MathML) | math |
  | Link | LINK | link | link |
  | Caption | — | caption (under Table/Figure) | caption |
  | BlockQuote | — | blockquote | blockquote |
  | Note / FENote / Aside | — | note | generic |
  | TOC / TOCI | — | — | docToc / listItem |
  | Span | TEXT | — | staticText |
  | Artifact | not exposed | hidden | not filtered |

### 2.7 Known divergences between structure order and what users hear

1. **Chromium default ignores tags** (Chrome, and Edge on Chromium as of 143). It reads in content order, guesses
   paragraphs, has no headings on tagged files, merges table cells and reads artifacts [CR-HEUR][IAG][PM]. (H)
   pdfxray's **paint order** view is therefore a reasonable proxy for "what Chrome says", which gives that view real
   user meaning.
2. **Acrobat "Override the reading order in tagged documents"** makes Acrobat use the untagged methods on tagged
   files [A-READ][A-SRG]. (H) Exactly which methods honour tags at all is conflicting [NVDA#1216]. (L)
3. **Acrobat large-document mode** exposes only the visible pages (default threshold 50 pages) [A-READ][NVDA#20466].
   (H)
4. **Alt on a container hides its subtree** in Acrobat-based AT [A-SDK-MSAA][NVDA#15012]. (H) It does not in Chromium
   tag mode, or in pdf.js except for the Figure path. (M)
5. **One-hop role mapping** in pdf.js and PDFium vs full chains in Acrobat (M).
6. **Notes are read in place**, never moved [NVDA#14819]. (H)
7. **pdf.js ≥ 5.4.530 in Chromium browsers**: `display:contents` on marked-content spans breaks `aria-owns`, so
   headings, tables and lists have no content [PDFJS#20790]. Open. (H)
8. **Android's built-in viewer flattens each page to one text block** [ANDROIDX]. (H)
9. **A malformed structure tree reads as an "empty document" in Acrobat** [A-SDK-MSAA]. (H)
10. **ActualText on a heading drops the heading role in Acrobat** [NVDA#18148]. (M)

---

## 3. Comparison table (tagged PDF unless stated)

✓ = conveyed, ✗ = not conveyed, ~ = partial or buggy, ? = undocumented or untested. Each cell gives its source and
confidence.

| Behaviour | NVDA/JAWS + Acrobat (Win) | NVDA/JAWS + Firefox (pdf.js) | NVDA/JAWS + Chrome/Edge (Chromium, default) | VoiceOver + Preview/Safari (PDFKit) | TalkBack + Chrome/androidx | TalkBack/VO + Acrobat mobile |
|---|---|---|---|---|---|---|
| Order source | Structure tree [A-SDK-ACC] H | Structure (aria-owns); untagged in content order [PDFJS-STL] H | Content stream + heuristics [CR-HEUR][IAG] H | Structure [IAG][PM] H (test only) | Per-page text block [ANDROIDX] H | Structure, partial [A-MOBILE17] M |
| Untagged file | Temporary tags via "Infer" [A-READ] H | Text layer, content order (M) | Heuristics + OCR [CR-OCR] H | ? | Text + OCR [ANDROIDX] H | Liquid Mode (AI) [A-LMODE] H |
| Artifacts skipped | ✓ [A-EDIT] H | ✓ since 2026-01 [PDFJS#20550] H | ✗ [PM] H | ✓ [PM] H | n/a (all text) | ? |
| Headings + level | ✓ [IAG] H | ✓ [PM] H | ✗ wrong or missing [PM] H | ✓ [PM] H | ✗ | ✓ [A-MOBILE17] M |
| Figure Alt | ✓ [IAG] H | ✓ (JAWS: "image ignored" [PM]) M | ~ [IAG][PM] M | ✓ [IAG] H | ✗ [ANDROIDX] H | ✓ [A-MOBILE17] M |
| Alt hides children | ✓ any element [A-SDK-MSAA] H | Figure path only [PDFJS-STL] H | ✗ (tag mode) [CR-STRUCT] M | ? | n/a | ? |
| ActualText (struct) | ✓, Alt wins [NVDA-SRC] H | ✓ (fallback) [PDFJS-CORE] H | ✗ (MC ActualText ✓) [CR-STRUCT] M | ? | ? | ? |
| Table headers | ✓, span bugs [NVDA#5140][NVDA#21020] H | ✓ since 2026-08 [PDFJS#21702] H | ✗ [PM] H | ✓ [IAG][PM] H | ✗ | ~ [A-MOBILE17] M |
| Headers/IDs | ✓ (NVDA) [NVDA-SRC] H; JAWS ~ [A-COMM-TH] L | ✓ aria-describedby [PDFJS#21702] H | ✗ | ? | ✗ | ? |
| Notes relocated to reference | ✗ [NVDA#14819] H | ✗ (role note) H | ✗ | ? (assume ✗) | ✗ | ? |
| Link name from text | ✓; `/Contents` ignored [A-SDK-MSAA][NVDA#15229] H | ✓ [PDFJS-STL] M | Link role only in tag mode [CR-STRUCT] M | ? | Separate view [ANDROIDX] H | ✓ [A-MOBILE17] M |
| OBJR placement honoured | ✓ (implied by tree exposure) M | ✓ since 2026-08 [PDFJS#21768] H | ✗ (nearest text) [CR-HEUR] M | ? | ✗ | ? |
| /Lang switching | ✓ NVDA [NVDA#1737]; JAWS fixed 2019 [JAWS-WN] H | Document ✓, phrase ✗ [PM] M | ✗ [PM] M | Phrase ✓, document ✗ [PM] M | ? | ? |
| Role-map chains | Full (GetStdName) [A-SDK-MSAA] H | One hop [PDFJS-CORE] M | One hop [PDFIUM-ST] M | ? | n/a | ? |
| Math (MathML) | ✓ with MathCAT [NVDA-UG] H | ✓ AF/MSFT_MathML [PDFJS#20384] H | math role (tag mode) M | ? | ✗ | ? |
| Cross-page elements | Whole doc, or visible pages for large docs [A-READ] H | Per page (L) | Per page [CR-STRUCT] M | ? | Per page H | ? |

**Edge with the Adobe-powered reader:** nothing is documented. Treat it as unknown. PowerMapper's Edge 143 results
match Chromium [PM] (L).

---

## 4. Proposal: how pdfxray can support this

### 4.1 What exists (V0) and two issues worth fixing first

**What V0 provides** (`crates/pdfxray-core/src/tags/order.rs`)

- `Document::reading_order(page, ReadingOrderSource::{Structure, Paint}) -> ReadingOrder` lists the elements that
  *directly own content* on the page, once each.
  - Structure order: first `/K` position.
  - Paint order: first operator painted.
- `reading_order_comparison` compares the two orders position by position.
- `tag_boxes` gives every element's box on a page.
- `text_preview` gives a short text preview of an element.
- Diagnostic `reading-order-backtrack`: the step's top edge rises by more than `max(2 pt, 0.75 × min height)`.
- The model already parses `/Alt`, `/ActualText`, `/E`, `/Lang`, `/ID`, attributes (owner + entries) and
  `RoleClass`. Captured `MarkedSeq` has `actual_text`, `alt`, `lang` and `artifact`. `GState` has `render_mode`.

**Issues found while grounding this research**

1. **Backtrack false positives on correct documents.** `goes_back_up` compares only top edges, so these are all
   flagged:
   - (a) every move from the bottom of column 1 to the top of column 2;
   - (b) a correctly placed footnote (BPG puts the Note right after the referencing paragraph, so the order runs body
     → bottom note → back up to the body);
   - (c) a sidebar `Aside` placed at its anchor.

   **Proposal:** classify each step instead of keeping a single boolean.

   ```rust
   pub enum StepKind {
       /// The usual move: same column, further down.
       Forward,
       /// Top of the next column. LTR: the new box starts right of the previous box's right edge and the vertical
       /// overlap is below threshold; mirrored for RTL `/Lang` or WritingMode.
       ColumnBreak,
       /// The step enters, or returns from, a Note / FENote / Aside / Caption whose anchor is adjacent in the tree.
       Detour { kind: DetourKind },
       /// Goes back up the page with none of the reasons above.
       Backtrack,
   }
   ```

   Only `Backtrack` raises `reading-order-backtrack`. `ColumnBreak` and `Detour` draw the path in a neutral colour on
   the canvas, with an explanatory note.
2. **The preview prefers painted text over Alt.** `text_preview` falls back to `/Alt` only when the element has no
   text. Acrobat-based AT does the reverse [A-SDK-MSAA]. The Inspector should show what is **spoken**:
   - the Alt or ActualText when present;
   - the painted text as a secondary line when the two differ;
   - an `alt-hides-content` marker when they differ (§4.4).

   (This is a recommendation for the V0/V5 owners. This note changes no code.)

### 4.2 The speech-order model

**What it is**

A document-scoped, linear sequence of *speech items*: what a tag-following screen reader would encounter in browse
mode, in order. It is built from the structure tree with the documented rules. It is **not** a TTS string. It keeps
node identity, so every item maps back to a `NodeId` and a box on the canvas.

**How it relates to the existing orders**

- **Structure order** answers "which elements, in which sequence" (element granularity).
- **Speech order** answers "what is announced, in which sequence" (content granularity, after substitutions and
  skips).
- **Per-page projection.** Projected onto a page, consecutive speech items with the same owning element collapse into
  one `OrderStep`. The canvas path, badges, backtrack classification and Inspector list all work unchanged. Add a
  third `ReadingOrderSource::Speech(SpeechProfile)`.
- **Where they differ.** For a well-formed tree the Speech projection equals Structure order, except where:
  - Alt or ActualText swallows a subtree (fewer steps);
  - artifacts or `Private` are skipped;
  - an OBJR-only element takes a step at its annotation rectangle;
  - `Spec` and the Acrobat profile drop untagged content (Chromium-tag would keep it).

  Those differences are exactly what users need to see, so the "vs paint order" card gains a "vs speech" row.
- **Paint order** stays as is. Per §2.7 it is the closest proxy for the Chromium default and Android. The UI can say so
  (§4.5).

**Rules for the `Spec` profile** (normative where possible)

| # | Rule | Source |
|---|---|---|
| S1 | Depth-first pre-order over StructTreeRoot `/K`, then each element's `/K` in array order. A cycle guard and multiple-parent guard reuse existing diagnostics | [ISO1 §14.7.2, §14.8.2.3.1] |
| S2 | A content item (MCID/MCR) yields the Unicode text of its glyphs in paint order. Marked-content `/ActualText` replaces that sequence's glyphs; marked-content `/Alt` (on Span) replaces it as a phrase. `ReversedChars` is honoured | [ISO1 §14.9.3–14.9.4, §14.8.2.3.3] |
| S3 | Artifacts (`/Artifact` marked content, the PDF 2.0 `Artifact` element) yield nothing and are recorded as `Skip::Artifact`. Tagged content nested in an artifact also yields nothing, plus the existing `marked-content-in-artifact` (01-004) | [ISO1 §14.8.2.2][MH 01-003/01-004] |
| S4 | `Private` yields nothing for its whole subtree (`Skip::Private`). `NonStruct`, `Div`, `Part`, `Sect` and `Document` are transparent | [ISO1 Table 333][BPG §4.1.6–4.1.7] |
| S5 | An element with `/Alt` and/or `/ActualText` yields **one** substitution item. Its descendants are recorded as `Skip::CoveredBy(elem)`. When both are present, the Spec profile emits Alt and attaches ActualText as `also` | [ISO1 Table 323, §14.9.3–14.9.4][A-SDK-MSAA][BPG §5.5.2] |
| S6 | `/E` yields the expansion, with the original text kept on the item | [ISO1 §14.9.5][WCAG-PDF8] |
| S7 | An OBJR yields an annotation item at its `/K` position. Name: Link → Alt/ActualText, else the text of the content siblings (the Link's own text). Widget → `TU`, else Alt. Annot → `Contents`/Alt. `/Contents` is kept as `fallback_name` only | [ISO1 §14.8.2.3.2, §14.8.4.4.2][A-SDK-MSAA][WCAG-PDF10/11] |
| S8 | Table: one `TableStart { rows, cols, summary }` item. Each TH/TD item carries `(row, col, row_span, col_span)` and the resolved headers (explicit `Headers` recursively, else the PDF 2.0 algorithm). Caption is emitted where it sits | [ISO1 Table 337/349][ISO2 §14.8.4.8.3] |
| S9 | Note/FENote/Aside are emitted where they sit in the tree. Reference ↔ Note links are computed separately (§4.3) | [ISO1 Table 338][NVDA#14819][BPG §4.2.8] |
| S10 | List: `ListStart { items }`. Lbl text verbatim. Headings: Hn gives level n; `H` gives level `None`. `Title` is not a heading in PDF 2.0 | [ISO1 §14.8.4.3.3][WTPDF §8.2.5.12–13] |
| S11 | Language: effective `/Lang` (catalog → element → marked content). Emit a `LangChange` item when it changes | [ISO1 §14.9.2] |
| S12 | Untagged content on a tagged page yields nothing (`Skip::Untagged`; it is already `untagged-content`) | [A-SDK-MSAA][MH 01-005] |
| S13 | Hidden but tagged content (`Tr 3`, fully clipped, outside the CropBox, fill = background) is **emitted** with `hidden: Hidden::…`. The spec says it is content, so pdfxray marks it rather than dropping it | [ISO1 §14.8.2.2.3] |
| S14 | Role: the fully resolved standard type, plus `hops` (the RoleMap chain length), so profiles can model one-hop viewers | [A-SDK-MSAA][PDFJS-CORE][PDFIUM-ST] |

### 4.3 Data model sketch (Rust, `pdfxray-core::tags::speech`)

```rust
/// Which reading model to apply. Profiles other than `Spec` are approximations, labelled as such in every output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum SpeechProfile {
    /// ISO 32000 + PDF/UA rules (§4.2). The reference model.
    Spec,
    /// Acrobat/Reader on Windows as exposed through MSAA/DOM (what NVDA and JAWS consume):
    /// Alt/ActualText zero children on any element, Alt wins, `/Contents` never the link name,
    /// full role-map chains, untagged content dropped.
    AcrobatWin,
    /// Firefox pdf.js struct-tree layer (pinned version): one-hop RoleMap, Figure-only Alt replacement,
    /// ActualText as Alt fallback, TH row/column from Scope then position, notes kept with role `note`.
    Pdfjs,
    /// Chrome/Edge PDFium viewer *default*: content-stream order of text runs with image Alt by MCID,
    /// artifacts read, no headings on tagged files. Projects onto pdfxray's paint order.
    ChromiumDefault,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeechOrder {
    pub profile: SpeechProfile,
    pub items: Vec<SpeechItem>,
    /// Everything that was *not* spoken, and why. Drives "show skipped" in the UI.
    pub skipped: Vec<Skipped>,
    /// Reference → note pairs, with their distance in items and pages.
    pub notes: Vec<NoteLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeechItem {
    pub index: usize,
    /// The element that owns this item (innermost), or the marked content / annotation for leaves.
    pub node: NodeId,
    pub owner: Option<NodeId>,
    pub page: Option<PageIx>,
    pub rect: Option<Rect>,
    pub kind: SpeechKind,
    /// What is announced, after substitution.
    pub text: String,
    /// What was painted, when it differs (Alt/ActualText/E substitutions).
    pub painted: Option<String>,
    pub lang: Option<String>,
    pub hidden: Option<Hidden>,
    /// The rule that produced the item (S1–S14), for the "why" tooltip.
    pub rule: SpeechRule,
}

pub enum SpeechKind {
    Text,
    RoleStart { role: String, standard: String, class: RoleClass, level: Option<u8>, hops: u8 },
    RoleEnd { class: RoleClass },
    Substitution { source: SubstSource /* Alt | ActualText | Expansion */, also: Option<String> },
    TableStart { rows: u32, cols: u32, summary: Option<String> },
    Cell { row: u32, col: u32, row_span: u32, col_span: u32, header: bool, headers: Vec<HeaderRef> },
    ListStart { items: u32 },
    Annotation { subtype: String, name: Option<String>, fallback_name: Option<String>, target: Option<LinkTarget> },
    LangChange { from: Option<String>, to: Option<String> },
}

pub struct HeaderRef { pub th: NodeId, pub via: HeaderSource /* Headers | Scope | Algorithm | Position */ }
pub enum Hidden { InvisibleMode, Clipped, OffPage, SameAsBackground }
pub struct Skipped { pub node: NodeId, pub page: Option<PageIx>, pub reason: SkipReason }
pub enum SkipReason { Artifact, Private, CoveredBy(NodeId), Untagged, EmptyElement, OutsideVisiblePages }
pub struct NoteLink {
    pub reference: NodeId,
    pub note: NodeId,
    pub via: NoteLinkSource, // Ref | StructDest | IdTree | LblMatch
    pub item_distance: usize,
    pub page_distance: u32,
}

/// Table model shared by speech, diagnostics and the overlay.
pub struct TableGrid {
    pub table: NodeId,
    pub rows: u32,
    pub cols: u32,
    /// Row-major; `None` where a span covers the slot.
    pub cells: Vec<Option<GridCell>>,
    pub irregular: Vec<IrregularRow>,
}

impl Document {
    /// Speech order for the whole document (or one page), for a profile.
    pub fn speech_order(&self, scope: SpeechScope, profile: SpeechProfile) -> SpeechOrder;
    /// The per-page projection used by the canvas path (collapses items to owning elements).
    pub fn reading_order(&self, page: PageIx, source: ReadingOrderSource) -> ReadingOrder; // + Speech(profile)
    /// A plain-text transcript ("heading level 1, Annual Report 2025") for CLI/UI/snapshots.
    pub fn speech_transcript(&self, scope: SpeechScope, profile: SpeechProfile) -> String;
    /// Grid of a Table element, with spans resolved and irregular rows reported.
    pub fn table_grid(&self, table: &NodeId) -> Option<TableGrid>;
    /// Header cells of a TH/TD, most specific first (explicit Headers, else the PDF 2.0 algorithm).
    pub fn cell_headers(&self, cell: &NodeId) -> Vec<HeaderRef>;
    /// Reference ↔ note links in the document.
    pub fn note_links(&self) -> Vec<NoteLink>;
}
```

**Supporting work this needs** (refactors to fold into the plan)

- **An annotations model in core:** `/Annots`, `/Subtype`, `/Rect`, `/QuadPoints`, `/Contents`, `/TU`,
  `/StructParent`, `/F`, and the page's `/Tabs`. The Annotations panel is v1.1 [pdfxray-plan §1 row 9]. Speech S7
  needs the data model now and the panel later.
- **A content walk per `Kid::Content`** that yields glyph Unicode in paint order, extending `mc_text_each`. It must
  also carry render mode, clip and CropBox containment, for S13.
- **`Elem.attrs` parsed into typed table, list and note attributes** (`Scope`, `Headers`, `RowSpan`, `ColSpan`,
  `Summary`, `Short`, `ListNumbering`, `NoteType`, PDF 2.0 `/Ref`). Today they are generic `(key, value)` strings.
- **`text_preview`** delegates to the speech substitution rules (§4.1 issue 2), so there is one source of truth.
- **Dead-code check:** once Speech exists, `owners_by_page` may become a projection of it. Decide that in the plan; do
  not keep two walkers.

### 4.4 Profiles: only where differences are reliable

| Profile | Differs from Spec in | Evidence | Confidence |
|---|---|---|---|
| `AcrobatWin` | Alt wins over ActualText, which is dropped (not `also`). `/Contents` never names a link. Hidden text is spoken. A Formula without a math reader speaks its Alt. A "visible pages" mode is opt-in (`OutsideVisiblePages` beyond 50 pages; off by default in pdfxray) | [A-SDK-MSAA][NVDA-SRC][NVDA#2174][NVDA#15229][A-READ] | H for MSAA rules; M for what NVDA/JAWS then voice |
| `Pdfjs` | One RoleMap hop. Alt replaces children only on the Figure path (Alt + BBox + content first child); elsewhere Alt is a label or description and the children are read. ActualText is used as Alt when Alt is missing. TH header kind from Scope, then position. Annotations at the OBJR position | [PDFJS-STL][PDFJS-CORE][PDFJS#21768] | H for code at a pinned version |
| `ChromiumDefault` | Ignore the tree. Text runs in content order (pdfxray paint order), artifacts included, image Alt by MCID, no table or heading semantics on tagged files, links next to the nearest text | [CR-HEUR][CR-FEAT][IAG][PM] | H for order; M for grouping details |

- **No profile** for VoiceOver/PDFKit, JAWS-specific behaviour, Edge-on-Adobe or TalkBack + Acrobat. They are
  undocumented. The UI should say "no reliable model: Spec is the closest documented approximation".
- **Chromium `PdfTags` mode** waits until it ships enabled.
- **Every profile output carries** `approximation: true`, the evidence keys and the pinned version (for example
  "pdf.js v5.x as of 2026-10"). The CLI prints a one-line disclaimer.

### 4.5 New checks and diagnostics

- **Severity policy:** *Warning* for things that change what is heard; *Info* for best-practice gaps.
- **Not duplicating veraPDF:** where veraPDF already reports a rule, pdfxray's diagnostic says "also veraPDF <rule>"
  and focuses on showing the location and the speech effect.

| Code | Trigger | Speech effect | Ties to |
|---|---|---|---|
| `alt-hides-content` | An element other than Figure/Formula (or a Figure whose descendants include text or elements) has `/Alt` or `/ActualText` while its subtree holds real content. **Error-level wording** when it is on `Document`, `Part` or a root child | Acrobat-based AT never reaches the subtree | [A-SDK-MSAA][A-EDIT][NVDA#15012]; WCAG 1.3.1 |
| `figure-alt-missing` | Figure (resolved) without Alt or ActualText, and not inside an Alt-bearing ancestor | "graphic" or "unlabelled graphic" | MH 13-004; UA-2 §8.2.5.28; WCAG PDF1, 1.1.1 |
| `formula-alt-missing` | Formula without Alt/ActualText and without MathML (namespace or AF) | Silent or garbled math | MH 17-002; UA-2 §8.2.5.29 |
| `alt-empty` | Alt present but empty or whitespace on a non-artifact | Viewer-dependent | [PM] "blank alt"; WCAG 1.1.1 |
| `alt-and-actualtext` | Both on one element | ActualText lost in Acrobat/NVDA | [NVDA-SRC]; [BPG §5.5.2] |
| `actualtext-on-structure` | ActualText on H*, TH, TD, Figure, L or LI (not Span) | Heading or cell semantics lost | [NVDA#18148][BPG §5.5.3]; MH 13-005 (Figure) |
| `rolemap-multi-hop` | A custom role needs more than one RoleMap hop to reach a standard type | pdf.js and Chromium see a generic element | [PDFJS-CORE][PDFIUM-ST]; MH 02-001 context |
| `table-no-header` | Table with no TH at all (and not a 1-row/1-column layout) | No header context | MH 15-001/15-002 (heuristic); WCAG PDF6, 1.3.1 |
| `table-th-scope-missing` | TH with no Scope in a table without Headers/IDs | Ambiguous direction | MH 15-003 (machine) |
| `table-headers-unresolved` | A `Headers` ID that is missing from the IDTree, not a TH, or in another table | Headers silently dropped | MH 15-005; UA-2 §8.2.5.26; [NVDA#18184] |
| `table-irregular` | Row widths differ after spans, or spans overlap | NVDA/JAWS navigation errors | UA-2 §8.2.5.26; [NVDA#5140][NVDA#21020] |
| `table-cell-no-headers` | A TD for which neither Headers nor the algorithm finds a header (Info) | Data read without context | ISO2 §14.8.4.8.3; MH 15-005 |
| `table-empty-th` | An empty TH | Header shift | [BPG §4.2.6][NVDA#13889] |
| `table-caption-position` | Caption not first or last child of Table, or more than one Caption | Read mid-table | ISO1 §14.8.4.3.4; UA-2 §8.2.5.27 |
| `table-repeated-header-tagged` | On a later page, a TR whose text equals THead's and that is tagged (not an artifact) | Headers read twice | [BPG §4.2.6] |
| `caption-detached` | Caption whose nearest Figure/Table is not its parent (first/last child) and not an adjacent sibling | Caption heard far from its object | [BPG §4.1.3]; UA-2 §8.2.5.27; MH 13-003 context |
| `note-far-from-reference` | NoteLink with `item_distance` above N items (default 1 element) or `page_distance` ≥ 1 | Note heard away from its reference | [BPG §4.2.8]; MH 19-001/19-002 (human) |
| `note-id-missing` | Note without `/ID`, or with a duplicate (PDF/UA-1 files) | Cannot be linked | MH 19-003/19-004 |
| `note-type-pdf2` | `Note` in a PDF 2.0 / UA-2 file, or FENote without a `/Ref` pair | Not linked | UA-2 §8.2.5.14 |
| `note-unreferenced` | Note/FENote with no Reference, Link or Ref pointing at it | Orphan note | [BPG §4.2.8] |
| `objr-outside-context` | Link annotation OBJR not under Link/Reference; Widget not under Form; other annotation not under Annot | Wrong role or out of flow | MH 28-002/28-010/28-011; ISO1 §14.8.4.4.2 |
| `annotation-far-from-content` | An annotation's speech position is on another page from its `/Rect`, or more than K items from the nearest content intersecting its rectangle | Link or field heard out of context | MH 28-001 (human); UA-2 §8.9.3.2 |
| `link-not-contiguous` | One Link element's content items are separated by other speech items | Link text split | UA-2 §8.9.2.4.2 |
| `link-name-empty` | Link with no text, Alt or ActualText (`/Contents` alone does not count) | "link" with no name | WCAG PDF11/PDF13, 2.4.4; MH 28-012 |
| `tabs-not-structure` | Page has annotations and `/Tabs` is missing or not `S` (UA-1), or not A/W/S (UA-2) | Focus order differs from reading order | MH 28-008/28-009; UA-2 §8.9.3.3; WCAG PDF3, 2.4.3 |
| `hidden-text-spoken` | Tagged text with render mode 3, fully clipped, off the CropBox, or the same colour as the backdrop, and **no** visible image/figure beneath (an OCR layer over an image is fine) | Heard but not seen | ISO1 §14.8.2.2.3; [BPG §5.5.3.1] |
| `private-hides-content` | Private element with real content | Never read | ISO1 Table 333 |
| `empty-element` | An element with no content, outside BPG's allowed list | Empty stops in navigation | [BPG §3.5] |
| `list-label-outside-lbl` | LI whose first painted glyph is a bullet or number but sits in LBody or is untagged | Marker lost or misread | [NVDA#2177]; UA-2 §8.2.5.25 |
| `heading-level-skip` / `heading-h-mixed` | Shown in speech context (veraPDF covers UA-1 14-003/14-007) | Navigation confusion | MH 14-003/14-007 |
| `untagged-in-flow` | Speech annotation on existing `untagged-content`: untagged text *between* two tagged items on the page | Silent in Acrobat; inserted by Chromium tag mode | MH 01-005; [CR-STRUCT] |

**Changes to the existing diagnostic:** `reading-order-backtrack` becomes "backtrack after classification" (§4.1).
`ColumnBreak` and `Detour` steps are never reported.

### 4.6 UI ideas that fit the v2 design

- **View ▾ → "Reading order from"**
  - Options become: Structure tree · Paint order · **Speech ▸** (Spec · Acrobat (Windows) · Firefox pdf.js ·
    Chrome/Edge default).
  - Each profile entry has a keycap-free subtitle "approximation".
  - "Chrome/Edge default" can simply alias Paint order, with a note: "Chromium reads content order (tags ignored by
    default)".
- **Canvas path and badges** (same 2 px path and numbered badges)
  - **Alt-substituted steps** get a small "Alt" or "AT" glyph in the badge. Their covered descendants are drawn with a
    hatched 30% overlay and the chip "not spoken: covered by Alt".
  - **Skipped content** (artifacts, Private, untagged) shows when "Show skipped" is on. It is dimmed and uses the
    artifact grey `#8A93A3`.
  - **Step kinds:**
    - Backtrack: amber, as in the design.
    - ColumnBreak: neutral dashed segment.
    - Detour (note or aside): neutral dotted segment, with a return arrow.
  - **OBJR steps** put the badge on the annotation `/Rect`, with a link-chain icon.
  - **Hidden-but-spoken text** gets an "eye-off" badge.
- **Inspector: "Page n · reading order" list**
  - Rows stay at the element level, with a disclosure arrow that shows the speech items under each row (text, Alt
    substitution, cell with headers).
  - The "vs paint order" card gains "vs speech (profile)": same count, first difference, and the number of skipped
    items by reason.
  - The problem-first card shows the new diagnostics.
- **Speech preview drawer tab**
  - A new drawer tab, "Speech", that opens by default when Reading order from = Speech.
  - It is a transcript with one line per item, for example:
    - `heading 1 · Annual Report 2025`
    - `graphic · "Revenue by quarter" (Alt)`
    - `table · 4 rows, 3 columns`
    - `row 2, Revenue · Q1 · 1.2 m`
    - `[de] Gesamtumsatz`
  - **Interactions:**
    - Hover highlights the canvas box; click selects the node.
    - The profile picker sits in the tab header.
    - "Copy transcript".
    - A "Show skipped" toggle interleaves greyed lines such as `(artifact: page number "4")`.
  - **Header line:** "Approximation from the structure tree. Not recorded from a screen reader."
  - Monospace, tabular numerals, as in existing drawer tabs.
- **Table header-association overlay** (Tag overlay option "Table headers", shortcut to be decided)
  - When a TD or TH is selected, draw thin connectors from the cell to each header cell, coloured by `HeaderSource`:
    - Headers attribute: solid.
    - Scope: dashed.
    - Algorithm or position: dotted.
  - Cells with no header get a red outline.
  - The Inspector gets a mini-grid for the Table node (rows × columns with spans), showing each cell's computed
    `(row, col)` and an "irregular" marker.
- **Footnote links on the canvas**
  - A curved, dotted connector from each Reference or Lbl to its Note/FENote, labelled with the speech distance (for
    example "heard 37 items later · p. 5").
  - `note-far-from-reference` uses the problem colour.
  - "Show in tree" jumps between the two.
- **Tab-order overlay (later)**
  - The `/Tabs` value is a chip on the page.
  - Focus order of annotations as small square badges, distinct from the round reading-order badges.

### 4.7 Fixtures and headless verification

**Generated fixtures** (`crates/pdfxray-fixtures/src/catalog/`, same style as `reading_order.rs`)

Each fixture documents its expected facts, including the expected **Spec transcript**, in `*.expected.json`.

| Fixture | Content |
|---|---|
| `speech-alt` | Figure with Alt + child text + Caption (first child); P with Alt over a Span; `Sect` with Alt over two paragraphs; a second file variant with `/Alt` on `Document` |
| `speech-alt-actualtext` | Figure with both; H1 with ActualText; marked-content Span ActualText for an "ﬁ" ligature; soft-hyphen U+00AD line break; Code with Alt |
| `speech-expansion-lang` | `/E` on an abbreviation Span; catalog `/Lang en-US`, a `de` paragraph, a `fr` marked-content span; an empty `/Lang` |
| `speech-artifacts` | Header/footer/page-number pagination artifacts with Subtypes; watermark; artifact nested in tagged content; MCID inside an artifact; PDF 2.0 `Artifact` structure element |
| `speech-hidden` | `Tr 3` OCR text under an image (fine); `Tr 3` text with no image; white-on-white text; text clipped away; text outside the CropBox |
| `speech-grouping` | NonStruct/Div wrappers; a Private subtree with text; empty P and empty TD |
| `table-scope` | Simple 3×3 with column and row TH and Scope; Summary; Caption first |
| `table-headers-ids` | Complex table with `Headers` arrays (including a dangling ID and a non-TH target) |
| `table-spans` | RowSpan/ColSpan; one irregular row; empty TH |
| `table-split` | One Table across 2 pages; THead repeated on page 2 as an artifact (variant: tagged) |
| `table-no-th`, `table-caption-middle`, `table-nested`, `table-layout` | One problem each |
| `notes-ua1` | Reference + Lbl + Note with ID placed (a) right after the paragraph, (b) at page bottom in tree order, (c) at document end; a Note without ID |
| `notes-pdf2` | PDF 2.0: FENote with NoteType Footnote/Endnote; `/Ref` both ways; Link with a structure destination |
| `links-objr` | Link (text + OBJR); a link wrapping two lines (QuadPoints); Link with only `/Contents`; OBJR Link under P; Widget in Form and not in Form; annotation OBJR on the wrong page |
| `tabs-variants` | Four pages with `/Tabs` absent, R, C and S, each with two widgets whose structure order differs from their geometry |
| `lists` | Bullet glyph in Lbl; bullet in LBody; nested L inside LBody; ListNumbering present or absent |
| `headings` | H/Hn mix; skipped level; PDF 2.0 Title |
| `columns-aside` | Two-column page tagged correctly (expect ColumnBreak, no backtrack) and tagged across (expect backtracks); Aside sidebar |
| `rolemap-chain` | `MyHead → Heading1 → H1` (two hops) next to a one-hop mapping |
| `formula-mathml` | PDF 2.0 Formula with MathML-namespace children; Formula with Alt only |

**Verification tiers**

1. **Exact (CI, every PR).** `insta` snapshots of the `speech_order` JSON and `speech_transcript` for `Spec` and
   `AcrobatWin` on every fixture. Use fluent expectations (role, text, skip reasons) plus one smoke assertion per test,
   as the plan requires.
   - The existing tier-3 corpus (PDF/UA Reference Suite) gets reviewed snapshots of transcript length, skip counts
     and diagnostics.
2. **pdf.js oracle (xtask, nightly).**
   - `cargo xtask speech-oracle pdfjs` runs a pinned pdf.js release (Apache-2.0, fetched and SHA-pinned like the
     corpus; never vendored into the app) in headless Firefox through Playwright or WebDriver BiDi.
   - It dumps the accessibility tree (the role/name/level tree) of each page's struct-tree layer and diffs it against
     `SpeechProfile::Pdfjs`.
   - Use Firefox, not Chromium, because of [PDFJS#20790]. (Feasibility: M; needs a spike.)
3. **PDFium structure cross-check (feature `pdfium`, existing).**
   - Use `FPDF_StructTree_GetForPage` and the `FPDF_StructElement_*` functions (`GetType`, `GetAltText`,
     `GetActualText`, `GetMarkedContentIdAtIndex`) through `pdfium-render`'s raw bindings.
   - Compare the per-page element order and role-after-one-hop with pdfxray's tree. Divergences are exactly what
     Chromium's tag mode would see. (M: the bindings exist in PDFium's public header [PDFIUM-ST]; whether
     `pdfium-render` re-exports them needs checking.)
4. **Chromium (experimental, manual).** Run Chrome with `--enable-features=PdfTags` and `--force-renderer-accessibility`
   and dump the accessibility tree through the DevTools protocol (`Accessibility.getFullAXTree`). The PDF viewer runs
   in an out-of-process frame, so reaching its tree needs a spike. (L)
5. **macOS PDFKit (local, opt-in).**
   - A small helper uses the macOS Accessibility API (`AXUIElement`) to dump Preview's accessibility tree for a fixture.
     It needs the Accessibility permission, so it is not for CI.
   - Optionally, script VoiceOver through AppleScript, for example with the MIT-licensed Guidepup, for spot transcripts.
   - This turns "?" cells in §3 into observed facts per macOS version. (M)
6. **Manual AT recordings (Windows).**
   - Record NVDA's Speech Viewer log or JAWS's speech history, reading each fixture in Acrobat Reader (versions
     noted), into `fixtures/at-recordings/<fixture>.<at>-<version>.<viewer>-<version>.txt`.
   - A test compares them loosely with the `AcrobatWin` transcript: order of the substantive strings only. They are
     recorded evidence, never regenerated by CI.

### 4.8 Phased plan (cheap and high-value first)

| Phase | Scope | Why first | Depends on |
|---|---|---|---|
| **P0** | Backtrack step classification (ColumnBreak/Detour); speech-aware `text_preview`; `alt-hides-content`, `figure-alt-missing`, `alt-and-actualtext`, `actualtext-on-structure`, `rolemap-multi-hop`, `private-hides-content` | Removes false positives in V0/V5. Everything else here uses data already parsed | V0 |
| **P1** | `speech_order` for the Spec profile (S1–S6, S10–S14, without annotations or tables); CLI `pdfxray speech <file> [--page N] [--profile …] [--skipped]`; Speech drawer tab; `ReadingOrderSource::Speech`; fixtures `speech-*`, `headings`, `lists`, `columns-aside`, `rolemap-chain` | The core value: "what is heard" | P0, V5 canvas path |
| **P2** | Typed table attributes; `table_grid`; `cell_headers` (PDF 2.0 algorithm); table diagnostics; header-association overlay and Inspector mini-grid; table fixtures | The most failure-prone area in published tests | P1 |
| **P3** | Annotations model (`/Annots`, `/Tabs`, `/StructParent`, names); S7; link, form and tab diagnostics; OBJR badges; `links-objr`, `tabs-variants` | Shares work with the v1.1 Annotations panel | P1 |
| **P4** | Notes: NoteLink (Ref, structure destination, ID, Lbl match); note diagnostics; canvas footnote connectors; `notes-*` | Needs speech distance from P1 and Link targets from P3 | P1, P3 |
| **P5** | Profiles `AcrobatWin`, `Pdfjs`, `ChromiumDefault`; pdf.js oracle in xtask; PDFium structure cross-check | Only worthwhile once Spec is solid | P1–P4 |
| **P6** | Hidden-text detection (`hidden-text-spoken`), MathML/Formula, `/E` display; macOS PDFKit dump helper; manual AT recordings for the fixture set | Lower frequency, or needs platform work | P1 |

Each phase follows the existing process: plan first (including refactors), dead-code check, the full gate, no commits
without the user's permission.

### 4.9 What we cannot know without running real assistive technology

- **What JAWS actually voices.** It is closed. Release notes only list fixes [JAWS-WN]. This includes JAWS's handling
  of Headers vs Scope, `/E`, `Note`, ActualText in cells, and its verbosity settings.
- **How NVDA or JAWS verbosity settings change the announcement.** For example: reporting headers only when they
  change, "figures and captions", list item counts, and the "clickable" setting [NVDA-UG]. The transcript shows full
  association, not the speech after filtering.
- **VoiceOver/PDFKit rules** for Alt on containers, ActualText, notes, OBJR placement, role-map chains and `/E`. Only
  black-box tests exist [IAG][PM].
- **Edge's Adobe-powered reader:** whether it exposes tags at all, and how.
- **Acrobat on macOS** (NSAccessibility mapping): undocumented. User reports describe partial support.
- **Acrobat's treatment of corner cases:** `/MarkInfo /Marked false` with a tree, a broken ParentTree, tagged content
  inside an artifact, `Tr 3` text, and off-page text. All are undocumented.
- **Pronunciation and voice:** how ligatures, symbols, bullets and private-use glyphs are spoken (depends on the
  synthesiser and its punctuation level), and whether language switches actually change the voice (depends on the
  installed voices).
- **Timing and user behaviour:** quick navigation (H, T, K keys), focus vs browse mode, and the visible-pages
  boundary in long documents depend on the user's interaction, which a static model cannot reproduce.
- **Mobile Acrobat and Liquid Mode:** AI-derived structure that is not reproducible.
- **Chrome's OCR output** for untagged or image-only pages: a model, varying by version and language.

---

## 5. Open questions

1. **ISO 32000-2 body text.** The exact wording of the 2.0 "logical content order" clause (14.8.2.5?) and of the
   revised ActualText definition (Table 355) were seen only through the errata and WTPDF. Someone with the free
   sponsored copy [ISO2] should confirm both before the Spec profile cites them. The same applies to the normative
   `/Tabs` A/W definitions (Table 31) and the 2.0 artifact Subtype list.
2. **Spec profile when both Alt and ActualText are present.** Spec emits Alt plus ActualText as `also` (BPG says both
   should be available); AcrobatWin drops ActualText. Is that the right default in the UI?
3. **`note-far-from-reference` threshold.** Measure in speech items, in elements, or "not the next sibling of the
   referencing block"? BPG gives two acceptable placements [BPG §4.2.8].
4. **Should Paint order be relabelled** "Paint order (≈ Chrome/Edge default)" in the View menu, or should the
   ChromiumDefault profile stay separate?
5. **pdf.js and Chromium move quickly** (8 relevant pdf.js PRs in 2026 alone). How often should the profiles and
   oracle pins be refreshed, and who owns that?
6. **Speech scope.** Is document scope needed in the UI (transcript across pages), or is page scope enough until P4
   (notes on other pages)?
7. **Annotation data in core** (P3) pulls part of the v1.1 Annotations panel forward. Is that acceptable for the
   roadmap?
8. **Dynamic XFA, optional content (OCG) visibility, and reading order inside Form XObjects** (`/Stm`) are not
   covered here. Matterhorn checkpoints 20 and 30 apply.

---

## 6. Notes on corrected premises

- **Alt on non-Figure elements** is not new in PDF 2.0. PDF 1.7 already allowed `/Alt` on any structure element
  [ISO1 Table 323, §14.9.3].
- **Acrobat's "Infer reading order from document"** is for **untagged** files. It does not override tags unless
  "Override the reading order in tagged documents" is also checked [A-READ][A-SRG].
- **Chrome's tagged-PDF reading** did not arrive in Chrome 114–120. The structure-tree mode was written in 2025–2026
  and is still disabled by default [CR-FEAT][CR-CL]. Chrome 85's 2020 work was about **writing** tagged PDFs.
- **No dedicated "OBJR" Matterhorn checkpoint** exists. OBJR use is covered by 28-002, 28-010 and 28-011 through
  ISO 32000-1 §14.8.4.4.2 [MH][MH-PM].

---

## 7. Glossary of terms used

- **AT:** assistive technology.
- **MSAA:** Microsoft Active Accessibility.
- **OBJR:** object reference (a structure-tree leaf pointing at an annotation or XObject).
- **MCID/MCR:** marked-content identifier and marked-content reference.
- **BPG:** PDF Association, Tagged PDF Best Practice Guide.
- **MH:** Matterhorn Protocol.
- **UA-1 / UA-2:** ISO 14289-1 / ISO 14289-2.
- **WTPDF:** Well-Tagged PDF.

---

## 8. References

Retrieval dates are October 2026 unless noted. "Wayback" means it was read through web.archive.org because the
origin blocks scripted access. Code links point to `main` or `master` as read in October 2026.

**Standards and best practice**

- **[ISO1]** ISO 32000-1:2008 (PDF 1.7), Adobe's free copy:
  https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf
  - §12.5 Table 30 (`Tabs`).
  - §14.7.2 Table 323 (`K`, `Alt`, `ActualText`, `E`, `Lang`).
  - §14.7.4.1–14.7.4.4 (content items, ParentTree).
  - §14.8.2.2 Tables 330 (artifacts); §14.8.2.2.3 (hidden page elements).
  - §14.8.2.3.1–3 (page content order, annotations, reversed chars).
  - §14.8.4.2 Table 333 (grouping, NonStruct, Private).
  - §14.8.4.3.3–4 Table 337 (lists, tables).
  - §14.8.4.4 Table 338 (Note, Reference, Link §14.8.4.4.2).
  - §14.8.4.5 Table 340 (Figure/Formula/Form).
  - §14.8.5.7 Table 349 (table attributes).
  - §14.9.1–14.9.5 (accessibility support).
- **[ISO2]** ISO 32000-2:2020 (PDF 2.0), no-cost sponsored access: https://pdfa.org/sponsored-standards/
  (announcement: https://pdfa.org/announcing-no-cost-access-to-iso-32000-2-pdf-2-0/). §14.8.4.8.3 Table 371 (table
  header algorithm), Table 372 (Caption), §14.8.6.3 (MathML).
- **[ISO2-ERR]** PDF Association, ISO 32000-2:2020 errata, clause 14:
  https://pdf-issues.pdfa.org/32000-2-2020/clause14.html
- **[MH]** Matterhorn Protocol 1.1 checkpoint table (Tagged PDF School):
  https://taggedpdfschool.pdfa.org/api/db/md/matterhorn (checkpoints 01, 02, 09, 13–19, 28, 30). Its group titles
  for 26–28 are shifted.
  - **[MH-PM]** Cross-check: https://www.powermapper.com/products/sortsite/rules/accmatterhorn/
  - **[MH-ANN]** Release note: https://pdfa.org/rules-for-accessible-pdf-matterhorn-protocol-1-1-is-now-available/
- **[VP-UA1]** veraPDF PDF/UA-1 rules: https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFUA-Part-1-rules
- **[VP-UA2]** veraPDF PDF/UA-2 rules: https://github.com/veraPDF/veraPDF-validation-profiles/wiki/PDFUA-Part-2-rules
- **[WTPDF]** PDF Association, Well-Tagged PDF 1.0 (February 2024, CC BY 4.0; clause 8 mirrors PDF/UA-2):
  https://pdfa.org/wp-content/uploads/2024/02/Well-Tagged-PDF-WTPDF-1.0.pdf (Wayback). Errata:
  https://pdf-issues.pdfa.org/wtpdf-1/
  - §8.2.2–8.2.5.32, §8.3, §8.4.3–8.4.5, §8.5.2, §8.8, §8.9.2.2, §8.9.2.4.2, §8.9.3.2–3, §8.9.4.2, §8.10.
- **[BPG]** PDF Association, Tagged PDF Best Practice Guide: Syntax, v1.0.1:
  https://pdfa.org/wp-content/uploads/2023/07/Tagged-PDF-Best-Practice-Guide.pdf (Wayback)
  - §3.2.2, §3.4, §3.5, §3.7, §4.1.2.3, §4.1.3, §4.1.4, §4.1.6–7, §4.2.2, §4.2.5, §4.2.6, §4.2.8, §4.2.12–13,
    §4.3.1–3, §5.4.1–2, §5.5.2–5.5.4.
- **[WCAG-PDFn]** W3C WCAG 2.2 PDF Techniques: https://www.w3.org/WAI/WCAG22/Techniques/pdf/PDF1 through
  …/PDF23. Cited here: PDF1, PDF3, PDF4, PDF6, PDF8, PDF10, PDF11, PDF12, PDF13, PDF21.

**Adobe**

- **[A-SDK-ACC]** Acrobat SDK, Accessibility, "Determining rendering order and logical order":
  https://opensource.adobe.com/dc-acrobat-sdk-docs/library/accessibility/index.html
- **[A-SDK-MSAA]** Acrobat SDK, "Reading PDF Files Through MSAA":
  https://opensource.adobe.com/dc-acrobat-sdk-docs/library/accessibility/MSAA&PDF.html
  - Sections: "IAccessible object types for PDF > PDF Document" (logical structure; headers and footers not
    presented); "PDF Structure Element" and "PDF Link" (Alt/ActualText makes the child count zero; link name).
  - DOM accessibility attributes; "Processing empty documents".
  - Quotes verified directly.
- **[A-READ]** Adobe, "Accessibility features, Adobe Acrobat Reader", section "Accessibility preferences in Reading
  panel": https://helpx.adobe.com/reader/using/accessibility-features.html (Wayback 2022-02-25; page updated
  2021-08-04).
- **[A-REFLOW]** Adobe, "Reading PDFs with reflow and accessibility features":
  https://helpx.adobe.com/acrobat/using/reading-pdfs-reflow-accessibility-features.html (Wayback 2026-09-10).
- **[A-SRG]** Adobe, "Accessing PDF Documents with Assistive Technology: A Screen Reader User's Guide" (2008):
  https://www.adobe.com/accessibility/pdfs/accessing-pdf-sr.pdf (Setup Assistant; JAWS commands; 50-page default).
- **[A-ORDER]** Adobe, "Reading Order tool for PDFs":
  https://helpx.adobe.com/acrobat/using/touch-reading-order-tool-pdfs.html (Wayback 2026-09-30).
- **[A-EDIT]** Adobe, "Editing document structure with the Content and Tags panels":
  https://helpx.adobe.com/acrobat/using/editing-document-structure-content-tags.html (Wayback 2026-10-01; covers
  parent alt text hiding children and artifacts not read).
- **[A-MOBILE17]** https://blog.adobe.com/en/publish/2017/06/27/adobe-improves-support-for-accessible-pdf-documents-on-mobile-devices
- **[A-MOBILE20]** https://blog.adobe.com/en/publish/2020/12/10/adobe-delivers-enhanced-pdf-reading-experience-for-accessibility
- **[A-LMODE]** Adobe, Liquid Mode (Android): https://www.adobe.com/devnet-docs/acrobat/android/en/lmode.html
- **[A-COMM-TH]** Adobe Community thread "JAWS reader order of Acrobat tables header data cells", message 10232680,
  2018 (seen in search results only; full URL not verified). (L)
- **[A-COMM-AT]** Adobe Community thread "PDF actual text does not get read by screen reader", message 10637884,
  September 2019 (seen in search results only; full URL not verified). (M)

**Screen readers**

- **[NVDA-UG]** NVDA 2026.3 User Guide: https://download.nvaccess.org/documentation/userGuide.html ("Browse Mode",
  "Reading Mathematical Content", "Document Formatting").
- **[NVDA-SRC]** NVDA's Acrobat support, public GPL source, described not copied:
  https://github.com/nvaccess/nvda/blob/master/nvdaHelper/vbufBackends/adobeAcrobat/adobeAcrobat.cpp and
  https://github.com/nvaccess/nvda/blob/master/source/NVDAObjects/IAccessible/adobeAcrobat.py
- **[NVDA#n]** https://github.com/nvaccess/nvda/issues/n for n = 1216, 1737, 2174, 2177, 5140, 5141, 9046, 13889,
  13982, 14819, 15012, 15229, 18148, 18184, 18405, 18843, 20466, 21020.
- **[JAWS-WN]** Freedom Scientific, JAWS "What's New" and previous-version feature pages:
  https://support.freedomscientific.com/Downloads/JAWS/JAWSWhatsNew (per version:
  …/Downloads/JAWS/PreviousFeatures/?version=N).
- **[WEBAIM-18527]** WebAIM mailing list thread on Acrobat tag order vs reading order:
  https://webaim.org/discussion/mail_message?id=18527 (M)
- **[WEBAIM-2014]** WebAIM list, 2014, Apple and PDF tags: https://webaim.org/discussion/mail_message?id=27054 (L)

**Chromium, PDFium, Edge**

- **[CR-FEAT]** https://raw.githubusercontent.com/chromium/chromium/main/pdf/pdf_features.cc:
  `BASE_FEATURE(kPdfTags, base::FEATURE_DISABLED_BY_DEFAULT)` (verified directly).
- **[CR-ROLES]** https://raw.githubusercontent.com/chromium/chromium/main/pdf/pdf_accessibility_constants_helper.cc
- **[CR-STRUCT]** Chromium `components/pdf/renderer/pdf_accessibility_tree_builder_structure.cc`, `ShouldUseStructureMode` in
  `pdf_accessibility_tree_builder.cc`, and `pdf/pdfium/pdfium_page.cc`:
  https://source.chromium.org/chromium/chromium/src/+/main:components/pdf/renderer/
- **[CR-HEUR]** Chromium `components/pdf/renderer/pdf_accessibility_tree_builder_heuristic.cc` (same directory).
- **[CR-CL]** Example tag-reading commit, "(7/7) Retrieves the entire structure tree", 2025-10-10:
  https://chromium.googlesource.com/chromium/src/+/f4abba09e5218240897a367a8f6cde39bdac16e0%5E%21/ (tracking bug
  40707542).
- **[CR-OCR]** ChromeOS.dev, "Frictionless PDF accessibility with ML and OCR" (September 2024):
  https://chromeos.dev/en/posts/frictionless-pdf-accessibility-with-ml-and-ocr
- **[CR-WAI-IG]** W3C WAI-IG list, October 2025, a JAWS user hearing Chrome's OCR message:
  https://lists.w3.org/Archives/Public/w3c-wai-ig/2025OctDec/0037.html
- **[CR-ANDROID]** Chromium `chrome/browser/ui/android/pdf/` (`PdfEntryPointImpl.java`) using androidx.pdf:
  https://source.chromium.org/chromium/chromium/src/+/main:chrome/browser/ui/android/pdf/
- **[PDFIUM-ST]** PDFium `public/fpdf_structtree.h`:
  https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_structtree.h (also
  `core/fpdfdoc/cpdf_structtree.cpp`, `cpdf_structelement.cpp`).
- **[PDFIUM-CAT]** PDFium `public/fpdf_catalog.h` (`FPDFCatalog_IsTagged`):
  https://pdfium.googlesource.com/pdfium/+/refs/heads/main/public/fpdf_catalog.h
- **[EDGE-BLOG]** https://blogs.windows.com/msedgedev/2023/02/08/adobe-acrobat-microsoft-edge-pdf/
- **[EDGE-POL]** https://learn.microsoft.com/en-us/deployedge/microsoft-edge-browser-policies/newpdfreaderenabled
- **[EDGE-PDF]** https://learn.microsoft.com/en-us/deployedge/microsoft-edge-pdf ("Accessibility").

**pdf.js**

- **[PDFJS-STL]** https://github.com/mozilla/pdf.js/blob/master/web/struct_tree_layer_builder.js
  (`PDF_ROLE_TO_HTML_ROLE`, TH rule, alt handling, verified directly).
- **[PDFJS-CORE]** https://github.com/mozilla/pdf.js/blob/master/src/core/struct_tree.js (RoleMap lookup, Alt and
  ActualText).
- **Pull requests:** https://github.com/mozilla/pdf.js/pull/N for N = 13171, 15237, 20384, 20472, 20492, 20550, 20551,
  20567, 21702, 21768, 22115.
- **Issues:** https://github.com/mozilla/pdf.js/issues/N for N = 6269, 14261, 16564, 18090, 18297, 20790.

**Apple and Android**

- **[WK-AX]** WebKit `WKAccessibilityPDFDocumentObject.mm`:
  https://github.com/WebKit/WebKit/blob/main/Source/WebKit/WebProcess/Plugins/PDF/WKAccessibilityPDFDocumentObject.mm
- **[APPLE-FORUM]** https://developer.apple.com/forums/thread/713855
- **[CG-TAG]** https://developer.apple.com/documentation/coregraphics/cgpdftagtype
- **[ANDROID-PDF]** https://developer.android.com/develop/ui/views/layout/pdf/pdf-viewer
- **[ANDROIDX]** androidx.pdf `PdfViewAccessibilityManager.kt`, in AOSP `platform/frameworks/support`, branch
  androidx-main, `pdf/` module: https://android.googlesource.com/platform/frameworks/support/ (exact file path not
  re-verified).

**Published test results**

- **[IAG]** Information Access Group, "Comparing how well PDF viewer applications work with screen readers", tested
  2022-05-18:
  https://informationaccessgroup.com/comparing-how-well-pdf-viewer-applications-work-with-screen-readers/ (verified
  directly).
- **[PM]** PowerMapper, "Screen reader support for PDF", updated 2025-12-14:
  https://www.powermapper.com/tests/screen-readers/pdf/ (reliability per combination verified directly; per-test,
  per-browser details from its linked test pages, M).
- **[S508]** Section508.gov, PDFs: https://www.section508.gov/create/pdfs/ (names Acrobat as the test tool).
