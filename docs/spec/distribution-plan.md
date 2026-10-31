# Distribution plan: notarized macOS releases on pdfxray.app

Goal: a tag `vX.Y.Z` on GitHub builds a universal, Developer ID signed and notarized app on a
GitHub-hosted macOS runner, publishes it to Cloudflare (R2 for the files, a static-assets Worker for the site), and
installed copies update themselves.

## Decisions (defaults; say so to change them)

| Topic | Decision | Why |
|---|---|---|
| Installer | Signed, notarized and stapled `.dmg` (app + `Applications` link); the existing `.zip` stays as a second asset | The usual macOS install; the zip keeps the bare CLI for `curl` users |
| CLI | Also shipped inside the app (`pdfxray.app/Contents/MacOS/pdfxray`) | It updates together with the app; users link it into `PATH` |
| Notarization auth in CI | App Store Connect API key (`.p8` + key ID + issuer ID) | Does not depend on a personal Apple ID password or 2FA; Apple ID + app password stays supported |
| Auto-update | [Sparkle 2](https://sparkle-project.org) (MIT), loaded at runtime from `Contents/Frameworks` | The standard macOS updater: EdDSA-signed appcast, handles relaunch, permissions and app translocation. Builds from source (no framework) simply have no updater |
| Update feed | `https://dl.pdfxray.app/appcast.xml`; release candidates are not put in the stable feed | |
| File hosting | R2 bucket `pdfxray-releases` on the custom domain `dl.pdfxray.app` | Static, cheap, no egress fees |
| Site | Cloudflare Worker `pdfxray` serving `site/` as static assets (Cloudflare's current form of Pages), configured in `wrangler.jsonc`, on `pdfxray.app` | Deployed by CI on every push to `main` that touches `site/`; its download button links to the stable `latest/` DMG, so releases need no site deploy |

R2 layout:

```
dl.pdfxray.app/
  releases/<version>/pdfxray-<version>-macos-universal.dmg
  releases/<version>/pdfxray-<version>-macos-universal.zip
  releases/<version>/SHA256SUMS
  latest/pdfxray-macos-universal.dmg     copy of the newest stable DMG (short cache)
  appcast.xml                            Sparkle feed (short cache)
```

## What only you can do

### Apple

1. **Apple Developer Program** membership (USD 99 a year), individual or organization. The
   name on it is what users see: "Developer ID Application: <name> (<TEAMID>)".
2. **Developer ID Application certificate.** In Keychain Access › Certificate Assistant ›
   Request a Certificate From a Certificate Authority (save to disk), then on
   developer.apple.com › Certificates › + › *Developer ID Application* (G2 Sub-CA), upload the
   CSR, download and double-click the `.cer`. Export it **with its private key** from Keychain
   Access as `.p12` with a password.
3. **App Store Connect API key** for notarization: App Store Connect › Users and Access ›
   Integrations › App Store Connect API › Team Keys › +, access *Developer*. Download the
   `AuthKey_<KEYID>.p8` (only downloadable once) and note the Key ID and the Issuer ID.

### GitHub

4. Repository: `github.com/matheusAle/pdfxray`, created private on 2026-10-31. Public gives
   free macOS runner minutes (private macOS minutes are billed at 10x) and makes the site's
   README and user-guide links work for visitors.

### Cloudflare

5. **Account ID** (dashboard, right sidebar of any zone).
6. **API token** (My Profile › API Tokens › Create › Custom) with:
   * Account › Workers R2 Storage › Edit
   * Account › Workers Scripts › Edit
   * Zone › Workers Routes › Edit, on the site's zone (for the custom domain)
   * scoped to your account only. Optionally a TTL.
7. One-time setup (or let me run it after `npx wrangler login`):
   * create the R2 bucket `pdfxray-releases` and connect the custom domain `dl.pdfxray.app`
     (R2 › bucket › Settings › Custom Domains);
   * the site Worker `pdfxray` exists (created 2026-10-31, live on
     `pdfxray.m-matheus-ale.workers.dev`); its custom domain goes into `wrangler.jsonc`.

### Update signing key

8. A Sparkle EdDSA key pair is generated once (`cargo xtask sparkle keygen`). The public key is compiled into `Info.plist`; the private key goes into a
   GitHub secret **and your password manager**. Losing it means installed copies cannot verify
   new updates.

## GitHub configuration

Secrets (Settings › Secrets and variables › Actions › Secrets):

| Secret | Content |
|---|---|
| `MACOS_CERTIFICATE` | `base64 -i DeveloperID.p12 \| pbcopy` |
| `MACOS_CERTIFICATE_PASSWORD` | password of the `.p12` |
| `MACOS_SIGN_IDENTITY` | `Developer ID Application: <name> (<TEAMID>)` |
| `APPLE_API_KEY` | `base64 -i AuthKey_<KEYID>.p8 \| pbcopy` |
| `APPLE_API_KEY_ID` | the Key ID |
| `APPLE_API_ISSUER` | the Issuer ID (UUID) |
| `CLOUDFLARE_API_TOKEN` | token from step 6 |
| `CLOUDFLARE_ACCOUNT_ID` | account ID from step 5 |
| `SPARKLE_ED_PRIVATE_KEY` | private key from step 8 |

Variable (same page, Variables tab), default in brackets: `R2_BUCKET` [`pdfxray-releases`].

Missing Apple secrets produce an ad-hoc signed build with a warning (as today); missing
Cloudflare secrets skip publishing with a warning, so the pipeline can be tried before
everything is in place.

## Work items

| # | Item | Who |
|---|---|---|
| W1 | xtask: notarytool API-key auth; CLI inside the app; signed, notarized, stapled DMG | done (ad-hoc path tested locally; the signed path needs the Apple secrets) |
| W2 | Release workflow: API key, DMG, verification, GitHub release, upload to R2; `site.yml`; site download button; `docs/releasing.md` | done (actionlint clean; runs for real after W4) |
| W3 | Sparkle auto-update: pinned framework fetch, embed + sign in the bundle, runtime loading, "Check for Updates…" menu item, one updater across the per-window processes, appcast generation + EdDSA signing in CI, key generation | done (ad-hoc e2e update A→B passed with a test-only re-sign; Developer ID path untested; production key pending) |
| W4 | GitHub repo and first push (done, private); `OWNER` placeholder (done); site on pdfxray.app (done); R2 bucket + `dl.pdfxray.app` (waits for R2 to be enabled); secrets | in progress |
| W5 | First release candidate `v0.1.0-rc.1` end to end | after W1-W4 |
