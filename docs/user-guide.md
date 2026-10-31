# pdfxray user guide

## The window

| Area | What it shows |
|------|---------------|
| **Title bar** | One chip, `PDF 1.7 · Tagged · rev 1/1 ▾` (UA-1 / UA-1 failed after a validation run, damaged, encrypted, read-only revision): it opens the file summary and the revision timeline. |
| **Tags** | The structure tree. Rows show a short text preview; the object ref shows on the hovered or selected row. Dots use the tag-role colours. Hover a custom type for its RoleMap mapping. Leaves are MCIDs and annotation references (`↳ MCID 2  p1`): dim and not clickable, because they point into content, not at a tag. Select their tag, or open Content. |
| **Content** | Operators of the current page, nested by marked content and Form XObjects. |
| **Artifacts** | Artifact sequences per page with `/Type` and `/Subtype`. |
| **Resources** | Fonts, images, forms, colour spaces, patterns, shadings and graphics states per page or form, with usage counts and an "unused" badge. |
| **Objects** | Trailer, revisions and every indirect object; the toggle switches to a Pages view with all five boxes. Images, fonts, forms, colour spaces, shadings, patterns and pages show a picture of themselves (see [Object previews](#object-previews)). |
| **Canvas** | Every page in one vertical scroll. The page that fills most of the view is the *current* page (page field, Content tree, Operators, stepper, isolation follow it); clicking a page also makes it current. The page arrows and field scroll to a page. Fit fits the largest page. Click to select what is under the cursor. |
| **Inspector** | Breadcrumb (long paths fold into "…"), a summary strip, the selection's problem first, then its properties; default values fold into "+ n default values" and sections that do not matter in the current view start collapsed. Blue values are links. Right-click the title or a value for actions (Object, Raw bytes, Copy id, Copy JSON). |
| **Diagnostics** | Problems found while reading the file, with severity. |
| **Drawer** | A 34 pt strip of tabs until it is needed: a click on the page opens the Hit stack, a finished validation opens Validation, a tab click opens that tab (click the active tab to fold it). It starts open in Content, Objects, Validation, Paint order and for damaged files. |
| **Hit stack** | Everything under the last click, most specific first. Shift-click adds a second selection for comparison. |
| **Speech** (`⌘5`) | What a screen reader would announce, in order, for a chosen reader: Structure (spec), Acrobat (NVDA/JAWS), Firefox or Chrome/Edge. The readers are labelled approximations (their rules and sources are in the tooltip); "Show skipped" lists artifacts, Private content and content hidden by alternate text, with the reason. |
| **Validation** | veraPDF results mapped onto the nodes they concern. |
| **Status bar** | Cursor, "file info" (hover: counts, open time, veraPDF), units, diagnostics. |

Drag the edge between the navigator, the canvas and the inspector to make either side panel
wider or narrower; the canvas always keeps room for the page. Drag the drawer's top edge to
change its height.

Tree badges mark problems only: a red dot for a validation finding, a warning mark and a short
word (`Alt`, `order`, `header`) for a diagnostic.

## Selecting things

A click selects the smallest painted item under the cursor (a glyph, a path, an image). A small
bar under the selection offers Isolate · Hide · Show in tree · Object · Copy id; right-click on
the canvas, a tree row or a value offers the same. The
breadcrumb climbs from it through its operator, marked content and structure elements to the
document. The **Pick** menu of the canvas toolbar (keys `1`–`6`) restricts what a click may
select: Tags, Content, Boxes or Artifacts. The Hit stack drawer names the mode while it is not
All. Annotations are listed but not hit-tested before v1.1.

## Measuring

* **Rulers** start at 0 at the top-left corner of the page as shown and count right and down, like
  the `display` coordinates of the cursor in the status bar (the `x`/`y` there stay in PDF
  coordinates, with y up).
* The status bar shows the cursor in page space (y up) and display space (after `/Rotate`) in
  pt, mm, in or px (96 dpi).
* **Measure** (`M`): drag or click twice to get the distance, Δx and Δy. Hold `Shift` after the
  first point to snap the line to 45° (horizontal, vertical or diagonal; Δy or Δx becomes exactly
  0). `Esc` clears.
* Select one node, then Shift-click a second to see the gaps and edge alignment between them.
* **Page boxes** draws Media, Crop, Bleed, Trim and Art boxes; dashed ones are defaults the
  file does not define.

Lengths are physical: page coordinates multiplied by `/UserUnit`.

## Debug views

The **View** control of the toolbar toggles the active page-wide mode (X-ray when none is on);
its **▾** opens the menu with X-ray, Paint order, Clip and Alpha and their options. **Isolate**
and **Hide** act on the selection, so they are toolbar buttons of their own, next to View: they
stay disabled, and say why, until something with content is selected. In a very narrow window
they leave the toolbar; the selection's action bar and their keys still have them.

* **X-ray** (`X`) outlines every painted item at half strength (full on hover and selection),
  coloured by kind, by tag role or as artifact / tagged / untagged. **Invisible items** also marks
  invisible text, white-on-white and off-page content.
* **Isolate** (`I`) paints only the selection's content; **Hide** (`H`) paints everything else.
  Both are also on the selection's action bar.
* **Paint order** (`P`) replays the page operator by operator; the Inspector shows each
  operator's state.
* **Clip** (`C`) shows the clip in effect for the selected operator.
* **Alpha** (`A`) tints transparent, soft-masked and blended content.

Floating cards (legends, page boxes, the measure readout) sit at the canvas's top-right and
fold into chips when there is no room beside the page.

## Tags, reading order and tables

* **All tag boxes** (`⇧T`) outlines every structure element in its role colour; **Tag labels**
  (`L`) adds a role chip; **Nesting** shows grouping elements; **Artifacts** marks artifacts.
  Each role has one fixed colour, shown in the Tag roles legend, the tree dots and the x-ray.
* **Reading order** (`R`) numbers the page's elements and joins them with a path, read from the
  structure tree, from paint order (roughly what Chrome/Edge read) or from the Speech reader.
  A step that goes back up the page is amber and becomes the `reading-order-backtrack`
  diagnostic; a new column and a footnote detour are not backtracks. Select the page for the
  ordered list and where it differs from paint order.
* Selecting a table cell outlines the table and links the cell to its headers, labelled by the
  rule that found them (`Headers`, `Scope`, `position`; PDF 2.0 algorithm or PDF 1.7 rules).
  Cells without a header are amber. The Inspector shows the grid.
* References and their footnotes are linked by a dashed connector with the distance in reading
  steps.

## Object previews

In the **Objects** entry, an object that can be pictured gets a **Preview** section in the
Inspector: a card with the picture and one chip, and the facts that go with it as rows under it.
When the object has no drawing instructions or text of its own to list, a larger card fills the
area under its dictionary in the object view. Its header names the facts in one line and has a
**Fit · 1:1** switch; its footer lists the facts as chips.

| Object | Preview | Facts |
|--------|---------|-------|
| Image `XObject` | its pixels, nearest-neighbour, on a checkerboard (transparency shows through), with a hairline around them and a caption such as `2 × 2 px · shown ×90 · nearest` | pixel size, bits per component, colour space, alpha (soft mask) |
| Font | on a dark canvas, in the text colour: `Aa Bb Cc 0123` (when the program holds every character: embedded, not a subset, not Type 3), then **Glyphs used in this file**, a cell per code the document shows, each glyph with its hexadecimal code under it; all drawn with the font's own program | program (embedded TrueType, Type 1, CFF / OpenType, or Type 3 glyph procedures), distinct codes shown, encoding |
| Form `XObject` | what it paints, over its `/BBox` after `/Matrix`; a form without `/Resources` borrows those of a page that paints it | area, borrowed resources |
| Colour space, ICC profile | a row of swatches in the colours the renderer paints: every `/Indexed` palette entry (**Indexed palette · N colours**, each with its index), a tint ramp for `/Separation`, each ink full and half for `/DeviceN`, primaries for RGB, CMYK and Lab spaces, a ramp for one-component spaces | the space, components, palette |
| Shading | an axial shading as a bar along its axis, with a marker per stop (its ends and the bounds of a stitching function) and the stop's `t` and colour, such as `0 · .17 .42 .69`; other shadings over the area they cover (`/BBox`, the domain or the circles) | type, colour space, function |
| Pattern | a few tiles of a tiling pattern, or the area of a shading pattern | pattern type |
| Page | the page as the canvas draws it | |

**Fit** sizes the picture to the card: an image grows by a whole factor so every pixel is as wide
as the next (it shrinks by a fraction only when it is larger than the card), and swatches shrink
when there are too many. **1:1** shows one preview pixel per screen pixel, and the card scrolls.
The caption under an image states the factor in screen pixels per image pixel. An image preview
has at most 2048 pixels along its longest side; a larger image is scaled down to that, and the
factor says so. The Inspector's card is always fitted.

A preview never fakes a font. A font with no embedded program gets an amber **Not embedded**
frame: readers substitute their own font, so text can reflow or lose glyphs. The dimmed
specimen in it is drawn with pdfxray's own font and labelled *fallback specimen · not the file's
font*; it is the only text on the card not drawn by the file. A program the renderer cannot
read, a font kind it does not support, or a mesh shading without `/BBox` (whose extent is only
known by decoding the mesh) says **No preview** and why.

Previews are drawn on a thread of their own, the object looked at last first, and the last 24
are kept; the card says "Rendering preview…" until the picture is there. They show the newest
revision only: an older revision opened read-only has no Preview section.

## Validation

Pick a profile (PDF/UA-1, PDF/UA-2, WCAG 2.2, PDF/A, …) and press **Validate**. veraPDF runs
as a separate Java process (Java 11+). If it is not installed, the tab offers a one-time
download of the pinned release (about 34 MB, checksum verified) into
`~/Library/Application Support/pdfxray/verapdf`. Clicking a finding selects the node it
concerns.

## Using pdfxray on macOS

pdfxray is one window per document. Each window is its own copy of the app (its own process, its
own Dock icon), so two files can sit side by side, go to separate Spaces or full-screen apart, and
a slow file never holds up another window.

**Opening files from Finder.** pdfxray registers as a viewer for PDFs without taking over from
Preview. To open a PDF in it, right-click the file › **Open With** › **pdfxray**, drop the file on
pdfxray's Dock icon, or run `open -a pdfxray file.pdf` in Terminal. To make pdfxray the default
for every PDF, select a PDF in Finder, choose **File › Get Info** (⌘I), pick pdfxray under
**Open with**, then click **Change All…**.

**Which window a file goes to.** A file sent from Finder, the Dock, `open -a`, **File › Open…**
(⌘O) or **File › Open Recent** opens in the current window while it is empty; when that window
already shows a document, a new window opens with the file. Several files at once: the first can
take the empty window, each of the others gets its own. A file that the window already shows is
not opened twice. **Dropping a file onto a window** is different: it replaces the document of
that window, because the drop says which window you mean.

**New windows.** **File › New Window** (⌘N) opens an empty window. **File › Close Window** (⌘W)
closes this window and ends its process; **pdfxray › Quit** (⌘Q) does the same for the window you
are in, and leaves the other windows open. The window's title (`file.pdf — pdfxray`) names it in
Mission Control, the **Window** menu and the Dock.

**Recent files.** The empty window lists the files you opened lately, and so does **File › Open
Recent**; **Clear Menu** empties both. Files you open also appear in the Dock icon's menu and in
Apple menu › **Recent Items**.

**The menu bar.**

| Menu | Items |
|------|-------|
| **pdfxray** | About pdfxray (version, licence, website), Services, Hide (⌘H), Hide Others (⌥⌘H), Show All, Quit (⌘Q) |
| **File** | New Window (⌘N), Open… (⌘O), Open Recent ▸, Close Window (⌘W), Validate (⇧⌘V) |
| **View** | Zoom In (⌘=), Zoom Out (⌘−), Actual Size (⌘0), Fit Page (⌘9), Enter Full Screen (⌃⌘F) |
| **Window** | Minimize (⌘M), Zoom, Bring All to Front |
| **Help** | pdfxray User Guide (this page) |

There is no Edit menu on purpose: copying and pasting work with the usual keys inside pdfxray's
own fields and lists, and an Edit menu would take those keys away from them.

On Windows and Linux the same shortcuts use Ctrl: Ctrl+N opens a new window, Ctrl+O opens a file
(in a new window when this one shows a document), Ctrl+W closes the window. There is no menu bar.

## Encrypted files

pdfxray v1 shows the structure of encrypted files (objects, xref, pages and boxes, the
`/Encrypt` dictionary) but does not decrypt, render or interpret their content.

## Command line

Every view is available as JSON:

```sh
pdfxray info file.pdf --pretty
pdfxray tree file.pdf --kind tags            # pages | objects | tags | artifacts | resources | diagnostics | content:0
pdfxray node file.pdf struct:12:0
pdfxray hit file.pdf 0 72 700 --layer tags   # page, x, y in points
pdfxray breadcrumb file.pdf glyph:0:57:3
pdfxray render file.pdf 0 -o page.png --boxes --xray kind --highlight mc:0:3
pdfxray render file.pdf 0 -o step.png --step 120
pdfxray render file.pdf 0 -o only.png --isolate struct:12:0
pdfxray validate file.pdf --profile ua1      # --install downloads veraPDF if missing
pdfxray reading-order file.pdf 0 --from paint
pdfxray tag-boxes file.pdf 0
pdfxray speech file.pdf --preset acrobat --show-skipped
pdfxray table file.pdf struct:30:0
```

Node ids are described in [architecture.md](architecture.md#node-identity).
