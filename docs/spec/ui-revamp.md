# pdfxray UI revamp: implementation handoff

Source of truth: `design/pdfxray-revamp.pen` (open in pen.dev). The PNG exports in
`design/exports/` show the same frames. Read `00-spec-navigation-interactions.png` first.

All screens are **states of one window**, not separate pages:

```
┌ Title bar (44) ─ file · chips · layer mode · rev chip · Validate ───────────────┐
│ Rail │ Navigator (300) │ Center: canvas OR object view      │ Inspector (340)  │
│ (52) │                 ├────────────────────────────────────┤                  │
│      │                 │ Drawer (196, resizable, tabs)      │                  │
└ Status bar (26-28) ─ cursor · counts · open time · diagnostics · UNITS · veraPDF ┘
```

## Screens (exports → frames)

| PNG | Frame | What it shows |
|---|---|---|
| 01-tags | Inspector · Tags | Structure tree, X-ray on, Figure selected, hit stack |
| 02-content-glyph | Content · Glyph | Content-stream tree, glyph boxes, untagged "DRAFT", operators drawer |
| 03-objects-stream | Objects · Stream | Object view (dictionary + decoded with plain-language Explain), References drawer |
| 04-measure-page-boxes | Measure · Page boxes | Box overlays + legend, margin / gap measurement, page thumbnails |
| 05-validation | Validation · PDF/UA | Findings by rule, fail badges on canvas, Why it matters / How to fix |
| 06-paint-order | Debug · Paint order | Ghosted unpainted ops, current op, stepper drawer, covered-by stack |
| 07–10 | State · Empty / Opening / Encrypted / Broken file | App states |
| 11-components | Components | TreeRow, PropRow, ToolButton(+active), Chip, Crumb |

Navigation and every micro-interaction are in the spec frame **"Spec · Navigation & interactions"**
(export `00-…png`): a screen map, a micro-interaction table, keyboard shortcuts and tokens.

## Design decisions the user made (keep them)

- **No ⌘K search box** in the title bar. The navigator filter (⌘F) is the only search.
- **Copy icons only on row hover**, and the hovered row tints `surface-2`.
- **Units switch lives in the status bar** and applies globally (pt · mm · in · px), not per section.
- **Property labels → hover hint** (no click popovers): hovering a property name shows what it means in plain words, e.g. "CTM · current transformation matrix: [a b c d e f] = scale x, skew y, skew x, scale y, translate x, translate y". *(Changed 2026-10-30: the CTM click popover was removed by the user.)*
- **Clip label → hover hint** explaining W / W* / n (same pattern as every label).
- **Colour swatches show hex** + colour space + the operator that set it.
- **Objects view is approachable by default**: Decoded is the default, Raw bytes is a tab. A diagnostic's "Show bytes" jumps there.
  The compression row collapses into a header chip. An **Explain** switch (default on) shows a plain-language note per operator.
- **Revisions are not a drawer**: a title-bar chip "rev N of N ▾" opens a timeline popover. Only objects changed in later revisions get a badge.
- Inspector diagnostic cards explain the problem in plain language ("Acrobat silently fixes this; other readers may drop the last command").

## Data the UI needs that core may not expose yet

Check each against `pdfxray-core` before building the UI. Where it's missing, add it in core with tests first.

1. Plain-language description per operator (static table keyed by operator + operand shape).
2. CTM decomposition (scale, skew, rotation, uniform) + unit conversion (reuse `units.rs`).
3. Glyph metrics: char code, Unicode via ToUnicode, ActualText, width/advance, ascent/descent/bbox, font embedding/subset.
4. Content coverage counts per page (tagged / artifact / untagged).
5. Object "used by" (reverse refs) and "resources it uses" (name → ref → kind), plus per-object last-changed revision.
6. Paint order: step index, "covered later by" and the visibility % for the current op (an approximation via bbox overlap is fine; label it so).
7. Validation: a per-rule "why it matters" / "how to fix" text table (our own words, keyed by veraPDF rule id).
8. Page boxes: provenance (own / inherited from obj / default), the box-outside-mediabox diagnostic and print figures (bleed amount, trim size).

## Process (repo golden rules)

- Plan first, and include the refactors the revamp needs (e.g. splitting `panels.rs` / `inspector.rs` into components) in the plan. Don't bolt new UI around the old structure.
- Check for dead code after each refactor (`cargo machete`, clippy `dead_code`, unused modules).
- Tests: kittest UI tests with fluent, behavioural expectations (what is selected or shown after an interaction), not just counts.
  Back them with a smoke expectation and update the snapshot PNGs intentionally.
- Keep the full gate green: fmt, clippy `-D warnings`, tests, `cargo deny`, `cargo machete`.
- **Never commit without the user's explicit permission. Never add a co-author trailer.**
- Logo: still being iterated in the "Brand · Logo" board. It is out of scope here, so keep the existing icon.
