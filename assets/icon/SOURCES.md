# App icon and logo

The official pdfxray logo, "P zoom ×3.2 · hatch": a blueprint-blue tile (#0E3A6E) zoomed into
a white-outlined P with a pale-cyan crosshatch fill, guide lines, cyan markers and corner
crosshairs. At 48 px and below it uses a simplified version: the same P filled flat with the
master's pale blue #ACDAF0 (the hatch's average colour), a heavier white outline, and no grid,
guides or crosshairs. It is the project's own artwork, drawn in `design/pdfxray-revamp.pen` (sheet
`design/exports/15-brand-official-logo.png`), and is MIT-licensed with the rest of the
repository. Its textures are drawn or generated in the design file; nothing is taken from
elsewhere.

Copied unchanged from the design export `design/exports/brand/`:

| File here | From | Use |
|---|---|---|
| `pdfxray.iconset/*` | `brand/pdfxray.iconset/` | macOS iconset: Big Sur layout (artwork 824/1024, centred, soft shadow); 16 and 32 px use the small version. `cargo xtask icon` folds it into `assets/pdfxray.icns` with `iconutil`. |
| `window-icon-256.png` | `brand/pdfxray.iconset/icon_256x256.png` | window / Dock / taskbar icon at runtime, on every platform |
| `logo-master.png` (288 px) | `brand/web/mark-144@2x.png` | the in-app logo above 32 px (empty state 72 px) |
| `logo-small.png` (256 px) | `brand/icon-small-full.png` | the in-app logo at 48 px and below (rail 28 px, opening state 48 px) |

Rules from the sheet: never recolour the letters, never drop the blueprint tile, keep the P cut
at the left edge.
