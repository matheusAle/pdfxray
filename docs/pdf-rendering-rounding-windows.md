# PDF Rendering: Rounding & Pixel Snapping on Windows

Reference for how Adobe Acrobat and Chromium (PDFium) convert PDF coordinates to
screen pixels on Windows. Covers the full pipeline from PDF user-space to device
pixels, including zoom, DPI, rounding strategies, gap prevention, and text
positioning.

Target use: emulating per-viewer rendering behavior in pdfxray.

---

## Coordinate Spaces

PDF defines content in **user space**: 1 unit = 1/72 inch, origin at the
bottom-left corner, Y-axis pointing up.

Screen devices use **device space**: origin at the top-left corner, Y-axis
pointing down, measured in pixels.

Every renderer must bridge these two spaces through an affine transformation
matrix (the CTM — Current Transformation Matrix) that encodes translation,
rotation, scaling, and the Y-axis flip.

---

## 1. Adobe Acrobat (AGM + CoolType)

### 1.1 Engine Architecture

Adobe's rendering stack has three layers:

| Layer | Component | Role |
|---|---|---|
| 2D graphics | **AGM** (Adobe Graphics Manager) | Path filling, stroking, image compositing, color management |
| Text | **CoolType** | Glyph rasterization, hinting, text layout |
| Compositing | **CompositorDX9** / **CompositorCPU** | Final compositing via DirectX 9 GPU shaders or CPU fallback, with GDI interop |

AGM also has a full **GDI backend** (`WinGDIPort`, `WinGDIGState`,
`GDIBitmapDeviceController`) used for printing and as a fallback when the DX9
path is unavailable.

### 1.2 Coordinate Precision

AGM uses **two coordinate precisions** simultaneously, selected by output
resolution:

| Type | Width | Template Params | Use |
|---|---|---|---|
| `BRVInt16Rect` | 16-bit signed integer | `F`, `G` | Low-resolution screen rendering |
| `BRVIntCoordRect` | 32-bit integer | `H`, `I` | High-resolution and print rendering |

Every scanline rasterizer class (`TAGMRun`, `TAGMRunScanline`,
`TAGMRunRepeatedScanline`, `TAGMRunNonBinaryRect`, etc.) is
**dual-instantiated** as C++ templates for both precisions. The engine picks
which instantiation to use based on the current `DeviceResolution`.

For glyph outlines, CoolType uses a separate `BRVFixedMatrix` (16.16
fixed-point) alongside a `BRVCoordMatrix` (floating-point). The fixed-point
path is used inside the CFF hinter where fractional precision matters for stem
alignment.

### 1.3 Resolution Model

Adobe uses a **discrete resolution table**, not continuous scaling:

- `DeviceResolutionList` — a set of known resolution values
- `DeviceResolutionSelector` — picks the closest entry from the list
- `MinDeviceResolution` — enforced floor; the engine never renders below this
- `DefaultResolution` — fallback when resolution cannot be determined (300 DPI
  for print)

For print output, resolution comes from the printer HDC via `GetDeviceCaps`.
For screen output, the resolution is derived from the display but quantized to
the discrete table.

**Feature thresholds** switch rendering behavior at specific `DevicePixelSize`
values:

| Threshold | Controls |
|---|---|
| `ATCThreshold` | Adobe Type Composer — switches text composition strategy |
| `CIDThreshold` | CID font rendering mode |
| `TrueTypeThreshold` | TrueType rasterization mode (outline vs bitmap) |
| `ConvertAllDashes` | Whether dash patterns are converted to explicit geometry |

These thresholds mean rendering is **not a smooth function of zoom** — behavior
changes abruptly when crossing a threshold boundary.

### 1.4 Snap-to-Device System

AGM implements explicit device-pixel snapping with three mechanisms:

#### 1.4.1 snap_to_device

A coordinate snapping mode that aligns computed positions to exact device
pixels. Controlled by a tolerance:

- **`SnapToPureTolerance = 0.0001`** — when a device-space coordinate is within
  ±0.0001 of an integer pixel, it is rounded to that exact integer.

This tolerance is very tight. At most zoom/DPI combinations, coordinates land
further than 0.0001 from integer pixels and **are not snapped**. The result is
inconsistent: some elements snap and others don't, depending on where they land
after the matrix transform. This inconsistency is most visible at intermediate
zoom levels.

#### 1.4.2 PixelOverlap

- **`PixelOverlap = 0.01`** — adjacent filled regions are allowed to overlap by
  0.01 device pixels.

This prevents hairline gaps between touching shapes. Without it, floating-point
precision limits in the affine transform can produce sub-pixel gaps between
regions that should be seamless.

However, the overlap is fixed at 0.01 regardless of zoom level. At high zoom
the overlap becomes visible as a darkened seam where two fills composite over
each other.

#### 1.4.3 RoundOffsetCallback

A callback mechanism (`RoundOffsetCallback`) that lets different rendering
contexts supply their own rounding function. This means the rounding strategy
is not globally fixed — it varies depending on what is being rendered and
through which port.

### 1.5 Stroke Adjustment

AGM implements the PostScript `setstrokeadjust` operator:

When stroke adjustment is enabled, thin strokes are widened so their edges land
on device pixel boundaries. A 0.5-point line that would straddle two pixels is
widened to fill one full pixel.

Related API:
- `AGMPortSetStrokeAdjustProc` — per-port callback to control stroke adjustment
- `setstrokeadjust` / `currentstrokeadjust` — PostScript operators embedded in
  AGM's interpreter

**Artifact**: at certain zoom levels, a thin line renders as 1 device pixel;
at the next zoom step, the same line jumps to 2 pixels. This produces visible
"breathing" of line weights during zoom transitions.

### 1.6 Text Grid

AGM has a separate grid system for text:

- `AGMPortSetTextGridSizeProc` — sets the text positioning grid size per port

Text characters can be snapped to a grid that differs from the graphics
coordinate grid. This improves text readability at screen resolutions by
aligning baselines and character origins to consistent positions, at the cost
of exact PDF-specified positioning.

### 1.7 Text Rendering (CoolType)

CoolType implements a proprietary CFF/Type1 hinting engine with the full
PostScript hinting vocabulary:

- **Alignment zones**: BlueValues, OtherBlues, FamilyOtherBlues
- **Zone control**: BlueScale, BlueShift, BlueFuzz
- **Stem widths**: StdHW, StdVW, StemSnapH, StemSnapV
- **Force bold**: ForceBold, ForceBoldThreshold
- **Language**: LanguageGroup, ExpansionFactor

The hinter uses `CFFHinter` with a histogram-based stem detector (`HistValue`)
that analyzes stem widths across the font to find the most common widths and
snap stems to consistent device-pixel widths.

#### Glyph Advance Rounding

Glyph advance widths are rounded with a zero-guard:

```
dX round dup 0 ne   — round X advance; keep only if non-zero
dY round dup 0 ne   — round Y advance; keep only if non-zero
```

This prevents a small advance from rounding to zero (which would stack glyphs
on top of each other). If rounding would produce zero, the unrounded value is
used instead.

#### Anti-Aliasing

CoolType uses multiple patented AA algorithms that are **selected dynamically
per-glyph** based on size, resolution, and context:

| Algorithm | Behavior |
|---|---|
| Dynamic AA Selection | Per-glyph choice of AA method based on context |
| Anisotropic AA | Different smoothing in X vs Y directions |
| Contrast Adjustment | Per-glyph contrast tuning in the AA output |
| Glyph Darkening | Edge-detection-based darkening to improve perceived weight |
| LCD Down-sampling | Sub-pixel rendering optimized for LCD panels |
| Hinted Stem Placement | Stems placed on a high-resolution sub-pixel grid, then down-sampled |
| Improved Hinting Policy | Modified hinting behavior specifically for anti-aliased output |

### 1.8 GPU Compositing

AGM composites through DirectX 9 GPU shaders when available:

- Color-managed compositing: CMYK → XYZ → RGB transform chains run as GPU
  shader programs (`GEPShader_TexColor_*_CMYKtoXYZACE_*_XYZtoMatrixRGBACE_*`)
- Spot color handling on GPU
- Highlight and XOR blend modes as shaders
- `CompositorDX9::StartGDICompositing()` — hybrid path that composites GPU
  output with GDI elements

The DX9 → GDI handoff is a known source of compositing seams at layer
boundaries when the GPU and GDI sub-images don't align to the same pixel grid.

### 1.9 Summary of Adobe Rounding Behavior

```
PDF coordinate
  │
  ├─ CTM transform → device-space float
  │
  ├─ if |device_float - round(device_float)| < 0.0001:
  │     snap to integer pixel  (snap_to_device)
  │
  ├─ if stroke and setstrokeadjust:
  │     widen to nearest pixel boundary
  │
  ├─ if text:
  │     align to TextGridSize
  │     apply CoolType hinting (stems → pixel grid)
  │     round advances with zero-guard
  │
  ├─ if adjacent fill:
  │     extend by 0.01 px overlap  (PixelOverlap)
  │
  └─ select Int16 or IntCoord rasterizer based on DeviceResolution
      │
      └─ scanline rasterization at selected precision
```

---

## 2. Chromium / PDFium

### 2.1 Engine Architecture

| Layer | Component | Role |
|---|---|---|
| 2D graphics | **AGG** (Anti-Grain Geometry) | Software rasterization via `CFX_AggDeviceDriver` |
| Text | **FreeType** | Glyph rasterization (not DirectWrite) |
| PDF viewer | **PDFiumEngine** / **PdfViewWebPlugin** | Zoom, DPI, page↔screen coordinate conversion |

There is an experimental **Skia** backend (`pdf_use_skia`), but it is off by
default and produces measurably different output (notably, glyph X positions
are not rounded to integers).

### 2.2 Coordinate Precision

PDFium uses **single-precision `float`** throughout the rendering pipeline.
Coordinates stay as floats through all matrix transforms and are converted to
integers only at the final rasterization step.

The primary rect type is `CFX_FloatRect` (four `float` members: left, bottom,
right, top). Integer rects use `FX_RECT` (four `int` members).

### 2.3 Zoom and DPI

The effective device scale is a continuous multiplication:

```
effective_scale = zoom × device_scale_factor
```

Where:
- `zoom` — the user's chosen zoom level (1.0 = 100%)
- `device_scale_factor` — Windows DPI divided by 96

DPI detection (`ui/display/win/dpi.cc`):

```
device_scale_factor = GetDeviceCaps(hdc, LOGPIXELSX) / 96.0
```

This value is:
- Read once and **cached for the process lifetime**
- The same for all monitors (**no per-monitor DPI** in the PDF path)
- Overridable with `--force-device-scale-factor=X`

Page dimensions in device pixels:

```
device_width  = page_width_pts  × effective_scale / 72
device_height = page_height_pts × effective_scale / 72
```

The viewport↔screen conversion multiplies/divides by `device_scale_` — there
is no discrete resolution table or threshold switching.

### 2.4 The Transformation Pipeline

```
PDF user space (points, Y-up)
  │
  ├─ object's own CTM (cm operator in content stream)
  │
  ├─ page MediaBox / CropBox mapping
  │
  ├─ CPDF_RenderStatus: object2device matrix
  │   (each object multiplies its local matrix into object2device)
  │
  ├─ Form XObjects: form_matrix × object2device, recurse
  │
  └─ CFX_RenderDevice / CFX_AggDeviceDriver
      │
      └─ device bitmap (integer pixels)
```

The `object2device` matrix is a standard 2D affine `[a b c d e f]`. Every page
object's local matrix is composed into it before drawing. Form XObjects
multiply their form matrix in recursively. Text with a non-identity text matrix
gets the CTM's inverse applied before combining.

### 2.5 Float-to-Integer Rounding

PDFium provides **four** methods on `CFX_FloatRect` to convert to integer
coordinates:

#### 2.5.1 GetOuterRect() — Expansion

```
left   = floor(left)
bottom = floor(bottom)
right  = ceil(right)
top    = ceil(top)
```

The integer rect **fully contains** the float rect. Used for clipping regions
and dirty-rect computation.

#### 2.5.2 GetInnerRect() — Contraction

```
left   = ceil(left)
bottom = ceil(bottom)
right  = floor(right)
top    = floor(top)
```

The integer rect **fits inside** the float rect. Used for content boundary
computation.

#### 2.5.3 ToFxRect() — Truncation (legacy)

```
left   = (int)left    // truncate toward zero
bottom = (int)bottom
right  = (int)right
top    = (int)top
```

The PDFium source warns: *"may introduce rounding errors that are visible to
users as off-by-one pixels/lines."*

#### 2.5.4 ToRoundedFxRect() — Round (legacy)

```
left   = roundf(left)
bottom = roundf(bottom)
right  = roundf(right)
top    = roundf(top)
```

Same warning as `ToFxRect()`. Both legacy methods can produce rects where the
integer width/height differs from `ceil(float_width)`, causing off-by-one
rendering.

#### 2.5.5 MatchFloatRange() — Span Preservation

A helper that converts a float range `[start, start+length]` to integers while
preserving the span:

1. Integer length = `ceil(float_length)`
2. Integer start = `floor(start)` or `ceil(start)`, whichever produces the
   smaller rounding error

This is used for image destination rects and avoids the common problem where
independently rounding start and end produces an integer span that is 1 pixel
too wide or too narrow.

### 2.6 No Snap-to-Device

PDFium has **no equivalent** of Adobe's `snap_to_device`, `PixelOverlap`, or
`setstrokeadjust`. Coordinates are transformed through the matrix as floats
and converted to integers at the rasterization boundary using one of the four
methods above.

There is no tolerance-based snapping, no overlap compensation for adjacent
fills, and no automatic stroke widening.

### 2.7 Dirty Rect Strategy

PDFium converts dirty regions from page space to screen space (never the
reverse). The source documents that inverting the transform (screen→page→screen
round-trip) would leave dirty pixels or shift text by a pixel at certain zoom
levels.

### 2.8 Image Scaling

Image destination dimensions use `ceil()` on the matrix scale components:

```
dest_width  = ceil(hypotf(matrix.a, matrix.b))
dest_height = ceil(hypotf(matrix.c, matrix.d))
```

Images always round **up** in size. This prevents clipping at the cost of a
potential 1-pixel overshoot.

### 2.9 Text Rendering (FreeType)

PDFium uses FreeType for glyph rasterization with standard autohinting (not
DirectWrite or ClearType).

#### Glyph Position Rounding

| AA Mode | X position | Y position |
|---|---|---|
| Normal (gray-scale) | `round()` | `round()` |
| LCD sub-pixel | `floor()` | `round()` |

In LCD mode, X positions are floored (not rounded) to align with the sub-pixel
grid.

#### Glyph Spacing Drift Compensation

When rendering a run of multiple glyphs, accumulated rounding error in
individual glyph positions causes the last glyph to drift from its intended
position. An `AdjustGlyphSpace` pass runs before bounding-box computation (when
AA is below LCD level and there are multiple glyphs) to redistribute the error
across inter-glyph gaps.

This is a post-hoc correction, not a proactive alignment like Adobe's
TextGridSize.

#### Anti-Aliasing

Standard gray-scale anti-aliasing via FreeType. No dynamic AA selection, no
anisotropic AA, no contrast adjustment, no glyph darkening. LCD sub-pixel
rendering is available but the code noted it "looked ugly" and historically
fell back to normal AA on Windows.

### 2.10 Summary of PDFium Rounding Behavior

```
PDF coordinate
  │
  ├─ CTM transform → device-space float
  │
  ├─ (no snapping, no tolerance check)
  │
  ├─ if clip/dirty rect:
  │     GetOuterRect(): floor-left, ceil-right (expand)
  │
  ├─ if content bounds:
  │     GetInnerRect(): ceil-left, floor-right (contract)
  │
  ├─ if image:
  │     MatchFloatRange() for span preservation
  │     dest dimensions = ceil(scaled size)
  │
  ├─ if text:
  │     glyph X = round() [normal] or floor() [LCD]
  │     glyph Y = round()
  │     AdjustGlyphSpace for multi-glyph drift
  │
  ├─ if legacy code path:
  │     ToFxRect() truncation or ToRoundedFxRect() rounding
  │     (known off-by-one bugs)
  │
  └─ AGG software rasterization
```

---

## 3. Side-by-Side Comparison

| Aspect | Adobe Acrobat | Chromium / PDFium |
|---|---|---|
| **Internal precision** | Dual: Int16 + IntCoord (resolution-selected) + fixed-point for glyphs | Single `float` throughout |
| **Resolution model** | Discrete table (`DeviceResolutionList`) with threshold-based feature switching | Continuous `zoom × device_scale_factor` |
| **Per-monitor DPI** | Unknown (not observable from static analysis) | Not supported in PDF path |
| **Device snapping** | `snap_to_device` with SnapToPureTolerance = 0.0001 | None |
| **Gap prevention** | PixelOverlap = 0.01 px overlap between adjacent fills | None |
| **Stroke adjustment** | PostScript `setstrokeadjust` — widens thin strokes to pixel boundaries | Not implemented |
| **Text grid** | Separate TextGridSize for text positioning | Same rounding as graphics |
| **Rect rounding (expand)** | Not directly observable | `GetOuterRect()`: floor/ceil |
| **Rect rounding (contract)** | Not directly observable | `GetInnerRect()`: ceil/floor |
| **Rect rounding (legacy)** | N/A | `ToFxRect()` truncate / `ToRoundedFxRect()` round — both have off-by-one bugs |
| **Span preservation** | Not directly observable | `MatchFloatRange()`: ceil length, best-fit start |
| **Image dest size** | Not directly observable | `ceil()` — always rounds up |
| **Glyph X rounding** | Hinted stem placement on sub-pixel grid | `round()` normal / `floor()` LCD |
| **Glyph Y rounding** | Hinted via BlueValues/zones | `round()` |
| **Glyph advance rounding** | `round()` with zero-guard (never rounds to 0) | `round()` with `AdjustGlyphSpace` drift compensation |
| **AA strategy** | Dynamic per-glyph selection, anisotropic, contrast-adjusted, edge-darkened | Fixed gray-scale FreeType AA |
| **Hinting engine** | Proprietary CFF hinter with histogram stem detection | FreeType autohinter |
| **GPU compositing** | DirectX 9 shaders (CMYK↔XYZ↔RGB, blend modes) | None (AGG CPU-only) |
| **GDI interop** | Full hybrid path (`WinGDIPort` + DX9 compositor) | None |

---

## 4. Known Artifacts Per Viewer

### 4.1 Adobe Acrobat

- **Line weight jumping**: `setstrokeadjust` causes thin lines to jump between
  1px and 2px at certain zoom levels. A 0.5pt line renders as 1px when its
  center is near a pixel boundary, then 2px when it straddles two pixels.

- **Inconsistent snapping**: `SnapToPureTolerance = 0.0001` is so tight that
  most coordinates don't qualify for snapping. Elements that happen to land
  within tolerance snap crisply; neighboring elements that don't land within
  tolerance stay at fractional positions. The inconsistency is most visible at
  intermediate zoom levels.

- **Resolution threshold jumps**: when zooming crosses a `DevicePixelSize`
  threshold (ATCThreshold, CIDThreshold, TrueTypeThreshold), rendering behavior
  changes abruptly — text may reflow slightly, dash patterns may change, or
  font rendering mode may switch.

- **Compositing seams**: the DX9↔GDI handoff in hybrid compositing can produce
  visible seams where GPU-composited regions meet GDI-rendered regions, if
  their pixel grids don't align exactly.

- **Overlap darkening**: `PixelOverlap = 0.01` prevents gaps but produces
  faintly visible dark seams at high zoom where the overlapping fill regions
  are both composited.

### 4.2 Chromium / PDFium

- **Hairline gaps**: no `PixelOverlap` compensation. Adjacent fills that share
  an edge in PDF space can produce a 1-pixel gap when their shared edge rounds
  to different integer positions from each side.

- **Off-by-one rects**: the `ToFxRect()` and `ToRoundedFxRect()` legacy paths
  produce integer rects that are 1 pixel too wide or too narrow at specific
  coordinate values.

- **Glyph drift**: accumulated rounding error across a text run shifts the last
  characters. `AdjustGlyphSpace` compensates but distributes the error across
  inter-glyph gaps rather than eliminating it, producing slightly uneven
  spacing.

- **No stroke adjustment**: thin lines render at whatever fractional width the
  matrix produces. A 0.25pt line can render as a faint anti-aliased smear
  rather than a crisp 1px line.

- **Consistent but imprecise**: because there's no snapping or threshold
  switching, artifacts are gradual and predictable. No sudden jumps during
  zoom.

---

## 5. Emulation Parameters

To emulate each viewer's rendering, the following parameters are needed:

### 5.1 Adobe Acrobat Emulation

```
snap_to_device_tolerance: 0.0001
pixel_overlap: 0.01
stroke_adjust: true
text_grid_enabled: true
resolution_model: discrete
coordinate_precision: dual (int16 below threshold, int32 above)
hinting: full_cff (BlueValues, StemSnap, histogram stem detection)
aa_mode: dynamic_per_glyph
glyph_advance_rounding: round_with_zero_guard
gpu_compositing: dx9_shaders
gdi_fallback: true
```

### 5.2 Chromium/PDFium Emulation

```
snap_to_device_tolerance: none
pixel_overlap: none
stroke_adjust: false
text_grid_enabled: false
resolution_model: continuous (zoom × device_scale_factor)
coordinate_precision: float32
hinting: freetype_autohint
aa_mode: fixed_grayscale
glyph_x_rounding: round (normal) | floor (lcd)
glyph_y_rounding: round
glyph_advance_rounding: round_with_drift_compensation
rect_expansion: floor_left_ceil_right (GetOuterRect)
rect_contraction: ceil_left_floor_right (GetInnerRect)
image_dest_size: ceil
span_preservation: match_float_range
gpu_compositing: none
gdi_fallback: none
```

---


