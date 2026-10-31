//! Thin native entry point; all logic lives in the `pdfxray_app` library.
//!
//! Usage: `pdfxray-app [file.pdf]`.

fn main() -> eframe::Result {
    pdfxray_app::run(std::env::args_os().nth(1).map(Into::into))
}
