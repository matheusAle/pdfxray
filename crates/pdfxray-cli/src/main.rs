//! `pdfxray` binary entry point; all logic lives in the library.
use std::io::Write;
use std::process::ExitCode;

use clap::Parser;
use pdfxray_cli::{Cli, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli, &mut std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // Nothing sensible is left to do if stderr itself fails.
            let _ = writeln!(std::io::stderr(), "error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
