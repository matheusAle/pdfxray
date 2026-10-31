# App icon and logo

The official pdfxray logo, "X-ray document": a page split by a scan line, solid on the left and
the x-ray glass in the design accent #3DD6F5 on the right, on a dark tile (#1F242C → #08090C,
corner radius 22.5 %). At 48 px and below it uses a simplified mark: a light page, a solid accent
half, three thick lines and a white scan line. It is the project's own artwork, drawn in
`design/pdfxray-revamp.pen` (board "5.0 Brand · Official logo (X-ray document)", export
`design/exports/15-brand-official-logo.png`), and is MIT-licensed with the rest of the
repository. Nothing in it is taken from elsewhere.

Copied unchanged from the design export `design/exports/brand/`:

| File here | From | Use |
|---|---|---|
| `pdfxray.iconset/*` | `brand/pdfxray.iconset/` | macOS iconset: Big Sur layout (artwork 824/1024, centred, soft shadow); sizes up to 32 px use the simplified mark. `cargo xtask icon` folds it into `assets/pdfxray.icns` with `iconutil`. |
| `window-icon-256.png` | `brand/pdfxray.iconset/icon_256x256.png` | window / Dock / taskbar icon at runtime, on every platform |
| `logo-master.png` (1030 px) | `brand/icon-master-full.png` | the in-app logo above 48 px (empty state 72 px, About panel) |
| `logo-small.png` (258 px) | `brand/icon-small-full.png` | the in-app logo at 48 px and below (rail 28 px, opening state 48 px) |

Usage rules are on the brand board (export `15-brand-official-logo.png`).
