//! Thin native entry point; all logic lives in the `pdfxray_app` library.
//!
//! Usage: `pdfxray-app [file.pdf [--compare other.pdf]]`.

fn main() -> eframe::Result {
    pdfxray_app::run(pdfxray_app::Launch::parse(std::env::args_os().skip(1)))
}
