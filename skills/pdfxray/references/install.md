# Installing pdfxray

Install only when `command -v pdfxray` finds nothing, and ask the user first.

The prebuilt downloads are for **Apple Silicon Macs, macOS 11 or later**. There is no Intel,
Linux or Windows build.

## 1. The app, which carries the CLI (recommended)

The DMG is notarized and the app updates itself, and the CLI inside it with it.

1. Download `https://dl.pdfxray.app/latest/pdfxray-macos.dmg` and drag `pdfxray.app` into
   `/Applications`. From a terminal:

   ```sh
   curl -L -o /tmp/pdfxray.dmg https://dl.pdfxray.app/latest/pdfxray-macos.dmg
   hdiutil attach /tmp/pdfxray.dmg -nobrowse -mountpoint /tmp/pdfxray-dmg
   cp -R /tmp/pdfxray-dmg/pdfxray.app /Applications/
   hdiutil detach /tmp/pdfxray-dmg
   ```

2. Put the CLI on the `PATH`:

   ```sh
   ln -s /Applications/pdfxray.app/Contents/MacOS/pdfxray /usr/local/bin/pdfxray
   ```

   If `/usr/local/bin` does not exist or is not writable, this needs `sudo` (ask the user), or
   link into another directory on the `PATH`, such as `~/.local/bin`.

## 2. The CLI alone

Every release on `https://github.com/matheusAle/pdfxray/releases` has
`pdfxray-<version>-macos-aarch64.zip`. Inside, `pdfxray-<version>-macos-aarch64/pdfxray` is the
bare command-line binary, next to `pdfxray.app`, `README.md`, `LICENSE` and `THIRD-PARTY.txt`.
The same zip is at `https://dl.pdfxray.app/releases/<version>/pdfxray-<version>-macos-aarch64.zip`.
Copy `pdfxray` into a directory on the `PATH`. It does not update itself.

## 3. From source

With Rust (the repository pins its toolchain in `rust-toolchain.toml`):

```sh
cargo install --locked --git https://github.com/matheusAle/pdfxray pdfxray-cli
```

This builds the `pdfxray` binary from the latest source on `main` and puts it in
`~/.cargo/bin`. It has been checked on Apple Silicon macOS only.

## Optional extras

* **veraPDF**, for `validate`: needs Java 11 or later. `pdfxray validate --install` downloads the
  pinned veraPDF release when none is found.
* **PDFium**, for `render --reference pdfium` only: a PDFium library found through `--pdfium
  <file or dir>`, the `PDFXRAY_PDFIUM` variable, or the pdfxray data directory. Without it,
  compare against another file (`--against`) instead.

## Check

```sh
pdfxray --version
pdfxray info some.pdf --pretty
```
