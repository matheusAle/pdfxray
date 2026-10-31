//! Developer tasks for the pdfxray workspace: test-corpus fetching and verification, the optional `PDFium` reference renderer, and the macOS release (`bundle`, `icon`, `sparkle`).
//!
//! The binary in `main.rs` only forwards to [`run`]; everything else is testable library code.

pub mod bundle;
pub mod cli;
pub mod corpus;
pub mod hash;
pub mod icon;
pub mod paths;
pub mod pdfium;
pub mod sparkle;
mod staging;
pub mod vendored;

#[cfg(test)]
pub(crate) mod test_util;

use std::ffi::OsString;
use std::io::Write;

use anyhow::{Context, Result};
use clap::Parser;

use crate::cli::{Cli, Command, CorpusCommand, PdfiumCommand, SparkleCommand, VendoredCommand};
use crate::corpus::fetch::UreqDownloader;

/// Parses `args` and runs the selected task, writing human-readable output to `out`.
pub fn run<I, T>(args: I, out: &mut dyn Write) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    let root = paths::workspace_root()?;
    match cli.command {
        Command::Corpus(CorpusCommand::List) => corpus::list(&root, out),
        Command::Corpus(CorpusCommand::Fetch { id }) => {
            corpus::fetch_cmd(&root, id.as_deref(), &UreqDownloader::new()?, out)
        }
        Command::Corpus(CorpusCommand::Verify) => corpus::verify_cmd(&root, out),
        Command::Vendored(VendoredCommand::Verify) => vendored::verify_cmd(&root, out),
        Command::Pdfium(PdfiumCommand::Fetch { dest }) => {
            let dest = dest.unwrap_or_else(|| pdfium::default_dest(&root));
            pdfium::fetch_cmd(&dest, pdfium::manifest::host()?, &UreqDownloader::new()?, out)
        }
        Command::Pdfium(PdfiumCommand::Verify { dest }) => {
            let dest = dest.unwrap_or_else(|| pdfium::default_dest(&root));
            pdfium::verify_cmd(&dest, pdfium::manifest::host()?, out)
        }
        Command::Bundle(args) => bundle::bundle_cmd(&root, &args, out),
        Command::Sparkle(SparkleCommand::Fetch) => sparkle::fetch_cmd(
            &sparkle::default_dest(&root),
            &sparkle::manifest::PINNED,
            &UreqDownloader::new()?,
            out,
        ),
        Command::Sparkle(SparkleCommand::Keygen) => sparkle::keygen_cmd(out),
        Command::Sparkle(SparkleCommand::Appcast {
            version,
            archive,
            url,
            out: feed,
        }) => sparkle::appcast_cmd(
            &sparkle::AppcastRequest {
                version: &version,
                archive: &archive,
                url: &url,
                out: &feed,
            },
            sparkle::keys::PUBLIC_ED_KEY,
            |name| std::env::var(name).ok(),
            sparkle::unix_now(),
            out,
        ),
        Command::Icon { out: icns } => {
            let icns = icns.unwrap_or_else(|| icon::default_icns(&root));
            icon::icon_cmd(&paths::iconset_dir(&root), &icns, out)
        }
    }
    .context("xtask failed")
}
