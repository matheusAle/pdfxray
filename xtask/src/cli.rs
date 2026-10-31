//! Command-line definition for `cargo xtask`.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::bundle::target::TargetArg;

/// pdfxray developer tasks.
#[derive(Debug, Parser)]
#[command(name = "xtask", version, about = "pdfxray developer tasks")]
pub struct Cli {
    /// Task group to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level task groups.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Tier 3 corpus: externally hosted PDFs pinned in `fixtures/corpus.toml`.
    #[command(subcommand)]
    Corpus(CorpusCommand),
    /// Tier 2 corpus: PDFs vendored under `fixtures/third_party`.
    #[command(subcommand)]
    Vendored(VendoredCommand),
    /// Optional `PDFium` reference renderer: a pinned, hash-verified build fetched on demand.
    #[command(subcommand)]
    Pdfium(PdfiumCommand),
    /// Build, sign, notarize and zip the macOS release (`target/bundle/`).
    Bundle(BundleArgs),
    /// Sparkle, the macOS auto-updater: the pinned framework, the update signing keys, the feed.
    #[command(subcommand)]
    Sparkle(SparkleCommand),
    /// Build `assets/pdfxray.icns` from `assets/icon/pdfxray.iconset`, after checking every
    /// member is there at its pixel size (needs macOS `iconutil`).
    Icon {
        /// Write the icon here instead of `assets/pdfxray.icns`.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
}

/// Options of `bundle`.
#[derive(Debug, Args)]
pub struct BundleArgs {
    /// Architecture to build; defaults to this machine's. `universal` builds both and merges with `lipo`.
    #[arg(long, value_enum)]
    pub target: Option<TargetArg>,
    /// Release version (a leading `v` is accepted); defaults to the workspace version. Its
    /// MAJOR.MINOR.PATCH must match the workspace version.
    #[arg(long, value_name = "X.Y.Z[-PRE]")]
    pub version: Option<String>,
    /// Sign with this "Developer ID Application" identity (hardened runtime, secure timestamp).
    /// Without it the build is ad-hoc signed.
    #[arg(long, value_name = "IDENTITY")]
    pub sign: Option<String>,
    /// Notarize with Apple and staple the ticket. Needs `--sign` and one of: `--keychain-profile`;
    /// an App Store Connect API key in `APPLE_API_KEY` (path of the `.p8`), `APPLE_API_KEY_ID`
    /// and `APPLE_API_ISSUER`; or `APPLE_ID`, `APPLE_TEAM_ID` and `APPLE_APP_PASSWORD`.
    #[arg(long, requires = "sign")]
    pub notarize: bool,
    /// `notarytool` keychain profile to use instead of the `APPLE_*` environment variables.
    #[arg(long, value_name = "NAME", requires = "notarize")]
    pub keychain_profile: Option<String>,
    /// Also build the installer disk image (`.dmg`), signed and notarized like the app.
    #[arg(long)]
    pub dmg: bool,
}

/// Tasks on the fetched (tier 3) corpus.
#[derive(Debug, Subcommand)]
pub enum CorpusCommand {
    /// List manifest entries and whether each is present and valid in the cache.
    List,
    /// Download entries into `fixtures/cache/`, verifying SHA-256. Skips verified files.
    Fetch {
        /// Fetch only the entry with this id.
        #[arg(long)]
        id: Option<String>,
    },
    /// Check every cached file against the manifest; exits non-zero on any problem.
    Verify,
}

/// Tasks on the vendored (tier 2) corpus.
#[derive(Debug, Subcommand)]
pub enum VendoredCommand {
    /// Check the veraPDF tree against its `manifest.toml`.
    Verify,
}

/// Tasks on Sparkle, the auto-updater of the macOS app.
#[derive(Debug, Subcommand)]
pub enum SparkleCommand {
    /// Download the pinned Sparkle release into `target/sparkle/<version>/` (`bundle` does this by
    /// itself when it is missing).
    Fetch,
    /// Generate an Ed25519 key pair for signing updates and print both halves; nothing is saved.
    Keygen,
    /// Sign the update archive with the private key in `SPARKLE_ED_PRIVATE_KEY` and write a feed
    /// with that one release.
    Appcast {
        /// Release version (a leading `v` is accepted).
        #[arg(long, value_name = "X.Y.Z[-PRE]")]
        version: String,
        /// The update archive (the DMG).
        #[arg(long, value_name = "FILE")]
        archive: PathBuf,
        /// Public URL of the archive.
        #[arg(long, value_name = "URL")]
        url: String,
        /// Where to write the feed.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
    },
}

/// Tasks on the `PDFium` reference renderer.
#[derive(Debug, Subcommand)]
pub enum PdfiumCommand {
    /// Download the pinned `PDFium` build for this machine into `<DIR>/<version>/lib/`.
    Fetch {
        /// Install below this directory instead of `target/pdfium`.
        #[arg(long, value_name = "DIR")]
        dest: Option<PathBuf>,
    },
    /// Check the installed library against its pinned hash; exits non-zero on any problem.
    Verify {
        /// Directory that holds the install, `target/pdfium` by default.
        #[arg(long, value_name = "DIR")]
        dest: Option<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_consistent() {
        Cli::command().debug_assert();
        // smoke: the definition exposes every task group
        let names: Vec<_> = Cli::command()
            .get_subcommands()
            .map(|c| c.get_name().to_owned())
            .collect();
        assert_eq!(
            names,
            ["corpus", "vendored", "pdfium", "bundle", "sparkle", "icon"]
        );
    }

    #[test]
    fn parses_sparkle_commands() {
        let cli = Cli::parse_from([
            "xtask",
            "sparkle",
            "appcast",
            "--version",
            "v0.2.0",
            "--archive",
            "/b/x.dmg",
            "--url",
            "https://dl.pdfxray.app/releases/0.2.0/x.dmg",
            "--out",
            "/b/appcast.xml",
        ]);
        assert!(
            matches!(
                &cli.command,
                Command::Sparkle(SparkleCommand::Appcast { version, archive, url, out })
                    if version == "v0.2.0"
                        && archive.as_path() == Path::new("/b/x.dmg")
                        && url == "https://dl.pdfxray.app/releases/0.2.0/x.dmg"
                        && out.as_path() == Path::new("/b/appcast.xml")
            ),
            "expected `sparkle appcast` with every option, parsed {:?}",
            cli.command
        );
        assert!(matches!(
            Cli::parse_from(["xtask", "sparkle", "fetch"]).command,
            Command::Sparkle(SparkleCommand::Fetch)
        ));
        assert!(matches!(
            Cli::parse_from(["xtask", "sparkle", "keygen"]).command,
            Command::Sparkle(SparkleCommand::Keygen)
        ));
        // smoke: the private key has no command-line option, only the environment variable
        let err = Cli::try_parse_from(["xtask", "sparkle", "appcast", "--version", "1.0.0"])
            .expect_err("missing options");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        assert!(Cli::try_parse_from(["xtask", "sparkle", "keygen", "--private-key", "k"]).is_err());
    }

    #[test]
    fn parses_fetch_with_id() {
        let cli = Cli::parse_from(["xtask", "corpus", "fetch", "--id", "pdfjs-issue6782"]);
        assert!(
            matches!(
                &cli.command,
                Command::Corpus(CorpusCommand::Fetch { id: Some(id) }) if id == "pdfjs-issue6782"
            ),
            "expected `corpus fetch` with the id, parsed {:?}",
            cli.command
        );
    }

    #[test]
    fn parses_pdfium_fetch_and_verify() {
        let cli = Cli::parse_from(["xtask", "pdfium", "fetch", "--dest", "/tmp/p"]);
        assert!(
            matches!(
                &cli.command,
                Command::Pdfium(PdfiumCommand::Fetch { dest: Some(dest) }) if dest.as_path() == Path::new("/tmp/p")
            ),
            "expected `pdfium fetch` into /tmp/p, parsed {:?}",
            cli.command
        );

        let cli = Cli::parse_from(["xtask", "pdfium", "verify"]);
        assert!(
            matches!(cli.command, Command::Pdfium(PdfiumCommand::Verify { dest: None })),
            "expected `pdfium verify` without a directory, parsed {:?}",
            cli.command
        );
    }

    #[test]
    fn parses_bundle_options() {
        let cli = Cli::parse_from([
            "xtask",
            "bundle",
            "--target",
            "universal",
            "--version",
            "v1.0.0",
            "--sign",
            "Dev ID",
            "--notarize",
            "--keychain-profile",
            "prof",
            "--dmg",
        ]);
        assert!(
            matches!(
                &cli.command,
                Command::Bundle(BundleArgs {
                    target: Some(TargetArg::Universal),
                    version: Some(version),
                    sign: Some(sign),
                    notarize: true,
                    keychain_profile: Some(profile),
                    dmg: true,
                }) if version == "v1.0.0" && sign == "Dev ID" && profile == "prof"
            ),
            "expected `bundle` with every option, parsed {:?}",
            cli.command
        );

        let cli = Cli::parse_from(["xtask", "bundle", "--target", "x86_64-apple-darwin"]);
        assert!(
            matches!(
                cli.command,
                Command::Bundle(BundleArgs {
                    target: Some(TargetArg::X86_64),
                    sign: None,
                    notarize: false,
                    dmg: false,
                    ..
                })
            ),
            "expected an unsigned x86_64 `bundle`, parsed {:?}",
            cli.command
        );
    }

    #[test]
    fn notarize_requires_a_signing_identity() {
        let err = Cli::try_parse_from(["xtask", "bundle", "--notarize"]).expect_err("needs --sign");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        let err = Cli::try_parse_from(["xtask", "bundle", "--keychain-profile", "p"])
            .expect_err("needs --notarize");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        let err = Cli::try_parse_from(["xtask", "bundle", "--target", "riscv"]).expect_err("bad target");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn parses_icon_output_override() {
        let cli = Cli::parse_from(["xtask", "icon", "--out", "/tmp/x.icns"]);
        assert!(
            matches!(&cli.command, Command::Icon { out: Some(out) } if out.as_path() == Path::new("/tmp/x.icns")),
            "expected `icon` writing to /tmp/x.icns, parsed {:?}",
            cli.command
        );
        // smoke: there is nothing to import any more; the design exports the finished iconset
        assert!(Cli::try_parse_from(["xtask", "icon", "import", "/tmp/e"]).is_err());
    }
}
