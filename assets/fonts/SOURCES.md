# Embedded fonts

Both families are licensed under the SIL Open Font License 1.1 (licence texts next to the fonts).
They are compiled into the app with `include_bytes!` in `crates/pdfxray-app/src/theme/fonts.rs`.
Downloaded from the official GitHub release assets over HTTPS into a clean directory, then only the
files below were copied.

## Inter 4.1

- Source: https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip
- Archive sha256: `9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e`
- Licence: `Inter-OFL.txt` (copy of `LICENSE.txt` from the archive)

| File | Archive path | sha256 |
|---|---|---|
| `Inter-Regular.ttf` | `extras/ttf/Inter-Regular.ttf` | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| `Inter-Medium.ttf` | `extras/ttf/Inter-Medium.ttf` | `97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872` |
| `Inter-SemiBold.ttf` | `extras/ttf/Inter-SemiBold.ttf` | `78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3` |
| `Inter-Bold.ttf` | `extras/ttf/Inter-Bold.ttf` | `288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f` |

## JetBrains Mono 2.304

- Source: https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
- Archive sha256: `6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf`
- Licence: `JetBrainsMono-OFL.txt` (copy of `OFL.txt` from the archive)

| File | Archive path | sha256 |
|---|---|---|
| `JetBrainsMono-Regular.ttf` | `fonts/ttf/JetBrainsMono-Regular.ttf` | `a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f` |
| `JetBrainsMono-Medium.ttf` | `fonts/ttf/JetBrainsMono-Medium.ttf` | `31c92d01a8a08528b718a43addf0ad3df0af2ca4b7b3290a452f70f358e14d3d` |
| `JetBrainsMono-SemiBold.ttf` | `fonts/ttf/JetBrainsMono-SemiBold.ttf` | `1b3bfa1ed5665a4ce3f9feb68d2d4e40e70bf8b4b7d9a3edd418f321b4e166a0` |
| `JetBrainsMono-Bold.ttf` | `fonts/ttf/JetBrainsMono-Bold.ttf` | `5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb` |
