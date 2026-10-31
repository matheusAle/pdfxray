//! `cargo xtask`: pdfxray developer tasks. All logic lives in the `xtask` library.

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match xtask::run(std::env::args_os(), &mut io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // `{err:#}` prints the whole context chain on one line.
            let _ = writeln!(io::stderr(), "error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
