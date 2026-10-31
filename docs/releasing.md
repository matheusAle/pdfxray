# Releasing pdfxray

pdfxray ships two macOS assets, on a GitHub release and on `https://dl.pdfxray.app`:

* `pdfxray-<version>-macos-<arch>.dmg`, the installer: `pdfxray.app` next to a link to
  `/Applications`. This is what the site's download button points to.
* `pdfxray-<version>-macos-<arch>.zip`: `pdfxray.app`, `pdfxray` (the command-line binary),
  `README.md`, `LICENSE` and `THIRD-PARTY.txt` (licenses of every crate in the binaries), for
  people who want the bare CLI.

The app also carries the CLI as `pdfxray.app/Contents/MacOS/pdfxray`, so it is updated with the
app; link it into the `PATH` with
`ln -s /Applications/pdfxray.app/Contents/MacOS/pdfxray /usr/local/bin/pdfxray`.

Releases are for Apple Silicon only (Intel Macs are being phased out, so a universal build is
not worth twice the build time and size), require macOS 11 or later, and are built by
`.github/workflows/release.yml` on a tag. The plan behind this setup, with what has to be done
by hand once, is `docs/spec/distribution-plan.md`.

## Cut a release

1. Set `[workspace.package].version` in `Cargo.toml` (and commit `Cargo.lock`). The binaries report
   this version; `cargo xtask bundle` refuses a `--version` whose `MAJOR.MINOR.PATCH` differs.
2. Merge to `main` and wait for CI to pass.
3. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`. A tag with a suffix (`v0.2.0-rc.1`)
   becomes a GitHub *pre-release*.
4. The workflow builds both architectures, merges them with `lipo`, signs (and notarizes if
   configured), builds the DMG, verifies everything, and attaches the DMG, the zip and
   `SHA256SUMS` to the release.
5. A notarized build is uploaded to the R2 bucket behind `dl.pdfxray.app`:
   `releases/<version>/` gets the DMG, the zip and `SHA256SUMS`. A stable release (no suffix)
   also replaces `latest/pdfxray-macos.dmg`, the site's download link (a name without the
   architecture, so the link survives build changes); a release
   candidate never touches `latest/`.

Nothing in the repository creates tags or releases except that workflow. The site (`site/`) is
deployed separately by `.github/workflows/site.yml` whenever it changes on `main`.

## Secrets and variables

Signing and notarization switch on when the three certificate secrets and one set of
notarization credentials exist. Otherwise the workflow still succeeds, with an ad-hoc signed
build, a warning, and no Cloudflare upload (only notarized builds are published there).

| Secret | Content |
|--------|---------|
| `MACOS_CERTIFICATE` | Base64 of the "Developer ID Application" certificate with its private key, exported as `.p12` (`base64 -i cert.p12 \| pbcopy`) |
| `MACOS_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12` |
| `MACOS_SIGN_IDENTITY` | The identity, e.g. `Developer ID Application: Your Name (ABCDE12345)` (`security find-identity -v -p codesigning`) |
| `APPLE_API_KEY` | Base64 of the App Store Connect API key `AuthKey_<KEYID>.p8` (`base64 -i AuthKey_X.p8 \| pbcopy`); access *Developer* is enough |
| `APPLE_API_KEY_ID` | The key's ID |
| `APPLE_API_ISSUER` | The issuer ID (a UUID, on the same App Store Connect page) |
| `CLOUDFLARE_API_TOKEN` | Token with Account › Workers R2 Storage › Edit, Account › Workers Scripts › Edit and, for the site's custom domain, Zone › Workers Routes › Edit on the site's zone |
| `CLOUDFLARE_ACCOUNT_ID` | The Cloudflare account ID |

Instead of the API key, notarization also works with an Apple ID: `APPLE_ID`, `APPLE_TEAM_ID`
(10 characters) and `APPLE_APP_PASSWORD` (an app-specific password from appleid.apple.com). The
API key wins when both are set; it does not depend on a person's password or two-factor login,
which is why CI should use it.

Variable (not secret), with its default: `R2_BUCKET` (`pdfxray-releases`).

The certificate goes into a temporary keychain and the API key into a temporary file, both
deleted at the end of the job. `notarytool` gets the key as a file path and an app-specific
password as `@env:APPLE_APP_PASSWORD`, never a secret as an argument, and xtask error messages do
not repeat command lines.

## Cloudflare, set up once

* R2 bucket `pdfxray-releases`, with the custom domain `dl.pdfxray.app` (R2 › the bucket ›
  Settings › Custom Domains). Objects under `releases/` are served with a one-year immutable
  cache; `latest/` with five minutes.
* The site is the Worker `pdfxray`, which serves `site/` as static assets (what Cloudflare
  Pages has become). Its configuration, custom domain included, is `wrangler.jsonc` at the
  root; `npx wrangler deploy` from the root deploys it by hand.

* Protection and analytics: `packaging/cloudflare/protect-downloads.sh` sets the zone's
  firewall rules (`dl.pdfxray.app` answers only GET/HEAD, only for `/releases/`, `/latest/` and
  `/appcast.xml`, never to an empty user agent), a rate limit (30 requests per 10 s per IP, then
  a 10 s block; the Free plan's limits), edge caching that follows the objects' `Cache-Control`,
  and the Web Analytics site of `pdfxray.app`. It replaces those rule sets on every run, so edit
  the script rather than the dashboard. Because files under `releases/` are cached as immutable,
  never upload different bytes under a published version (for example by re-running a release
  for the same tag): the edge would keep serving the old file while the update feed describes
  the new one. Tag the next `-rc.N` instead. Bot Fight Mode stays off: on the Free plan it covers the
  whole zone with no exceptions, and its challenges would break Sparkle's update checks and
  `curl` downloads.

R2 must be enabled once in the dashboard (it asks to accept its terms). Then, with wrangler
logged in (`npx wrangler login`): `npx wrangler r2 bucket create pdfxray-releases`, and attach
the custom domain in the dashboard.

## Building locally

```sh
cargo xtask bundle                                  # host architecture, ad-hoc signed
cargo xtask bundle --target universal               # not used for releases; needs: rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo xtask bundle --target x86_64-apple-darwin --version 0.2.0-rc.1

# Developer ID signed
cargo xtask bundle --sign "Developer ID Application: Your Name (ABCDE12345)"

# with the installer disk image
cargo xtask bundle --dmg

# signed and notarized, credentials from the environment: an API key ...
APPLE_API_KEY=~/keys/AuthKey_ABC123.p8 APPLE_API_KEY_ID=ABC123 APPLE_API_ISSUER=69a6de7e-... \
  cargo xtask bundle --target aarch64-apple-darwin --sign "Developer ID Application: ..." --notarize --dmg
# ... or an Apple ID
APPLE_ID=you@example.com APPLE_TEAM_ID=ABCDE12345 APPLE_APP_PASSWORD=xxxx-xxxx-xxxx-xxxx \
  cargo xtask bundle --target aarch64-apple-darwin --sign "Developer ID Application: ..." --notarize
# ... or from a notarytool keychain profile (`xcrun notarytool store-credentials pdfxray-notary`)
cargo xtask bundle --sign "Developer ID Application: ..." --notarize --keychain-profile pdfxray-notary
```

Output is in `target/bundle/`:

```
pdfxray.app                                   the finished app (also inside the zip)
pdfxray-<version>-macos-<arch>.zip            the release asset; <arch> is aarch64, x86_64 or universal
pdfxray-<version>-macos-<arch>.dmg            with --dmg: the installer
pdfxray-<version>-macos-<arch>/               what the zip was made from
bin/                                          the (merged) binaries before bundling
THIRD-PARTY.txt, entitlements.plist           generated inputs
```

What `bundle` does, in order: build `pdfxray-app` and `pdfxray-cli` in release mode (separate
`cargo build` runs so the CLI's `pdfium` feature does not leak into the app;
`MACOSX_DEPLOYMENT_TARGET=11.0`), `lipo` for universal, generate `THIRD-PARTY.txt` from
`cargo metadata` (normal dependencies of both binaries, with license texts), sign the CLI
(`dev.pdfxray.cli`) with the hardened runtime, assemble the `.app` from `assets/pdfxray.icns` with
a copy of the signed CLI in `Contents/MacOS`, sign the app (`dev.pdfxray.app`), zip with `ditto`.
With `--notarize` it submits a zip to `xcrun notarytool submit --wait`, requires status
`Accepted` (printing Apple's log otherwise), staples the app with `xcrun stapler staple`, then
zips again so the zip contains the stapled app. With `--dmg` it then puts the (stapled) app and
an `/Applications` link into a compressed HFS+ image (`hdiutil`), signs the image with the same
identity, and with `--notarize` notarizes and staples the image too: the image's ticket covers
its own signature, the app's ticket keeps the app valid once copied out. Ad-hoc builds get an
unsigned image.

Missing notarization credentials are reported before anything is built.

### The icon

The official logo is drawn in the design file (`design/pdfxray-revamp.pen`, sheet
`design/exports/15-brand-official-logo.png`) and exported finished into `assets/icon/`
(provenance in `assets/icon/SOURCES.md`): `pdfxray.iconset/` (the ten macOS iconset members,
Big Sur layout), `window-icon-256.png` (the window, Dock and taskbar icon at runtime on every
platform), `logo-master.png` and `logo-small.png` (the logo inside the app, above and up to
48 pt). The app shows these PNGs as they are; nothing redraws the logo in code.

The `.icns` in the bundle is `assets/pdfxray.icns`, committed so bundling never needs
`iconutil`. After a new export run `cargo xtask icon`: it checks that every iconset member is
there at its pixel size and rebuilds the `.icns` with `iconutil` (macOS only). A test fails when
`assets/pdfxray.icns` no longer matches the iconset, or the window icon its 256 px entry.

### Opening files and several windows

`Info.plist` declares PDF (`com.adobe.pdf`) as a document type the app views
(`CFBundleTypeRole` Viewer) with `LSHandlerRank` **Alternate**: pdfxray appears in Finder's
"Open With" menu and accepts PDFs on its Dock icon, but never becomes the default PDF viewer by
itself (users can choose it in Get Info › Open with › Change All). The app receives the files
through its application delegate (`crates/pdfxray-macos`). `LSMultipleInstancesProhibited` is
false because every window is its own process, and `NSSupportsAutomaticGraphicsSwitching` lets a
Mac with two GPUs stay on the integrated one.

After installing a new build, Launch Services picks up the document type when the app is first
opened or moved into `/Applications`; to check it by hand:

```sh
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f pdfxray.app
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -dump | grep -A3 -i 'dev.pdfxray.app'
open -a pdfxray.app file.pdf
```

### Linux and Windows

There is no Linux or Windows release yet; the app builds and runs there from source, with the
window icon, the window title and one process per window (Ctrl+N, Ctrl+O, Ctrl+W).

On Linux, `packaging/linux/pdfxray.desktop` adds pdfxray to the application menu and to "Open
With" for PDFs. Install it with the 256 px icon for the current user:

```sh
cargo install --path crates/pdfxray-app            # puts pdfxray-app in ~/.cargo/bin
install -Dm644 packaging/linux/pdfxray.desktop ~/.local/share/applications/pdfxray.desktop
install -Dm644 assets/icon/window-icon-256.png ~/.local/share/icons/hicolor/256x256/apps/pdfxray.png
update-desktop-database ~/.local/share/applications
gtk-update-icon-cache ~/.local/share/icons/hicolor 2>/dev/null || true
```

`Exec=pdfxray-app %F` needs `pdfxray-app` on the `PATH`; edit the line for another location.
`StartupWMClass=pdfxray` matches the app id the window sets, so the desktop groups its windows
under the icon.

On Windows, "Open with" and the file association need an installer that writes the registry
(`HKCR\.pdf\OpenWithProgids` and a ProgID). That installer is not done; run
`pdfxray-app.exe file.pdf` or drop files on the window.

## Auto-update

Developer ID builds update themselves with [Sparkle 2](https://sparkle-project.org) (MIT) from
`https://dl.pdfxray.app/appcast.xml`. `cargo xtask bundle --sign …` fetches the pinned Sparkle
(`xtask/src/sparkle/manifest.rs`, hash-checked, cached in `target/sparkle/<version>/`;
`cargo xtask sparkle fetch` does the same by hand), copies `Sparkle.framework` into
`Contents/Frameworks`, removes its XPC services (they are for sandboxed apps only), and signs
`Autoupdate`, `Updater.app` and then the framework before the app is signed. The app loads the
framework at run time; builds without it (ad-hoc bundles, `cargo run`, Linux, Windows) simply
have no updater and no "Check for Updates…" item.

`Info.plist` holds `SUFeedURL`, `SUPublicEDKey`, a daily automatic check
(`SUEnableAutomaticChecks`; with it set Sparkle checks without asking first) and
`SUVerifyUpdateBeforeExtraction`. `CFBundleVersion` is what Sparkle compares:
`MAJOR.MINOR.<PATCH><stage>`, with stage `1NN`/`2NN`/`3NN` for alpha/beta/rc `.NN` and `999` for
the release, so `0.2.0-rc.1` (`0.2.301`) < `0.2.0` (`0.2.999`) < `0.2.1-rc.1` (`0.2.1301`).
Other pre-release names are rejected before the build.

Every window is a process, and only one runs the updater: the one holding an advisory lock on
`updater.lock` in the app data directory. "Check for Updates…" (app menu) is enabled in that
window and disabled in the others; when it closes, another open window takes over.

**Signing key.** `cargo xtask sparkle keygen` prints a new Ed25519 key pair and saves nothing.
Put the public key in `PUBLIC_ED_KEY` (`xtask/src/sparkle/keys.rs`), store the private key as
the `SPARKLE_ED_PRIVATE_KEY` secret and in a password manager. Losing it means installed copies
cannot verify updates.

**Feed.** The release workflow writes and uploads the feed for stable tags, after the DMG is on
the download host. By hand:

```sh
SPARKLE_ED_PRIVATE_KEY=… cargo xtask sparkle appcast --version 0.2.0 \
  --archive target/bundle/pdfxray-0.2.0-macos-aarch64.dmg \
  --url https://dl.pdfxray.app/releases/0.2.0/pdfxray-0.2.0-macos-aarch64.dmg \
  --out target/bundle/appcast.xml
```

It signs the DMG, refuses a key that does not match `PUBLIC_ED_KEY`, and writes a feed with that
one release. Release candidates are not published to this feed.

**Testing updates locally.** With library validation on, an ad-hoc signed app cannot load an
ad-hoc signed framework ("different Team IDs"), which is why ad-hoc bundles leave Sparkle out.
To test the updater, sign with an Apple Development certificate (it has a team ID), or re-sign a
throwaway copy with `com.apple.security.cs.disable-library-validation` (never the release
entitlements), and point it at a local feed with
`defaults write dev.pdfxray.app SUFeedURL http://127.0.0.1:<port>/appcast.xml`. Delete that key
afterwards.

**Known gaps.** Sparkle quits only the window that runs it; other pdfxray windows keep running
the old build until they are closed. An idle window takes the updater over only after it next
repaints.

## Verify a build

```sh
codesign --verify --strict --verbose=2 pdfxray.app
codesign -dv --verbose=4 pdfxray.app          # Authority=Developer ID ..., TeamIdentifier, flags=...(runtime)
lipo -archs pdfxray.app/Contents/MacOS/pdfxray-app     # arm64 for releases
spctl -a -vv --type exec pdfxray.app          # "accepted  source=Notarized Developer ID" when notarized
xcrun stapler validate pdfxray.app            # the ticket is attached (works offline)
shasum -a 256 -c SHA256SUMS                   # the downloaded zip and DMG
codesign --verify --strict --verbose=2 pdfxray-<version>-macos-aarch64.dmg
spctl -a -vv -t open --context context:primary-signature pdfxray-<version>-macos-aarch64.dmg
xcrun stapler validate pdfxray-<version>-macos-aarch64.dmg
```

For an ad-hoc build `codesign -dv` shows `Signature=adhoc` and `spctl` says `rejected`; that is
expected.

## Gatekeeper and ad-hoc builds

Without Developer ID credentials the build is signed ad-hoc (`codesign -s -`). That is enough to run
on Apple Silicon, which refuses completely unsigned code, but it carries no identity and is not
notarized:

* A build you compiled yourself runs normally: nothing marks it as downloaded.
* A zip downloaded with a browser carries the quarantine flag, and Gatekeeper refuses to open the
  app ("cannot be opened because the developer cannot be verified", or "is damaged"). Remove the
  flag after unpacking: `xattr -dr com.apple.quarantine pdfxray.app`. On macOS 15 and later the
  alternative is to try opening it once, then allow it under System Settings, Privacy & Security,
  "Open Anyway".
* The `pdfxray` command-line binary is not quarantined when it is downloaded with `curl`, but one
  from a browser download is; the same `xattr` command applies.

Developer ID signed and notarized builds open without any of this.

## Hardened runtime, entitlements and PDFium

Both executables are signed with `--options runtime` and an entitlements file in which every
exception is explicitly off: no JIT, no unsigned executable memory, no `DYLD_*` environment
variables, and library validation enabled. pdfxray needs none of them.

The consequence: with library validation on, the hardened runtime only loads dynamic libraries
signed by Apple or by the same team as the executable. The optional PDFium reference renderer is
loaded with `dlopen` from a build that the user downloads (`cargo xtask pdfium fetch` in a source
checkout), which is not signed with this team's identity, so **the PDFium comparison does not work
inside a signed release build**. It is a developer feature: use a build from source for it. We
chose this over adding `com.apple.security.cs.disable-library-validation`, which would let any
dylib be injected into a signed, notarized app. If the PDFium view in releases becomes a
requirement, the options are shipping a PDFium that is re-signed with the release identity inside the
bundle, or enabling that entitlement for the CLI only (the CLI is the binary that links the
`pdfium` feature); both are a decision to take then, not now.

veraPDF runs as a separate Java process and is not affected.

## Known limits

* The title-bar proxy icon (the file icon you can drag from a document window's title) is not
  set: pdfxray draws its own title bar over a hidden native one, where the proxy icon would not
  show anyway.
* Only the `.app` can be stapled. The bare `pdfxray` binary is covered by the notarization of the
  zip (Gatekeeper looks the ticket up online), so on a machine that is offline its first launch
  from a quarantined download can fail the check.
* `THIRD-PARTY.txt` covers Rust crates. PDFium and veraPDF are not distributed with the release.
