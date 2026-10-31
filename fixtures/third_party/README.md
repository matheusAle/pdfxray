# Third-party fixtures

Files in this tree are **not** covered by the repository's MIT license. Each subdirectory keeps
its own license and attribution next to the files; those terms apply to them.

| Directory | License | Notes |
|-----------|---------|-------|
| `verapdf/` | CC BY 4.0 | Unmodified subset of the veraPDF test corpus. See `verapdf/LICENSE` and `verapdf/ATTRIBUTION.md`. |

Check the vendored tree with `cargo xtask vendored verify`.

Files fetched by `cargo xtask corpus fetch` (tier 3, listed in `fixtures/corpus.toml`) live in
`fixtures/cache/`, are never committed, and keep whatever license their source has.
