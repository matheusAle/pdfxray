//! Regenerates `fixtures/generated/` from the fixture catalog.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut stderr = std::io::stderr().lock();
            // Nothing sensible to do if stderr itself is closed.
            let _ = writeln!(stderr, "gen: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let dir = pdfxray_fixtures::fixtures_dir();
    let written = pdfxray_fixtures::generate(&dir)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "wrote {} files to {}", written.len(), dir.display())?;
    Ok(())
}
