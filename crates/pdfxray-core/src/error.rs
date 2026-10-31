use std::path::PathBuf;

/// Errors that prevent a document from being opened or queried.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The file could not be read.
    #[error("cannot read {path}")]
    Io {
        /// File that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The bytes are not recognisable as PDF.
    #[error("not a PDF")]
    NotPdf(#[from] pdfxray_raw::RawError),
    /// The caller stopped opening the document (see
    /// [`Document::open_with_progress`](crate::Document::open_with_progress)).
    #[error("opening was cancelled")]
    Cancelled,
}
