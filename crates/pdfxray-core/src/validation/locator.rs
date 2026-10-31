//! Where a validation check failed, in terms pdfxray can address.
//!
//! The validator adapter (`pdfxray-verapdf`) derives a [`Locator`] from veraPDF's context path;
//! the document resolves it to a [`NodeId`](crate::NodeId).

use serde::{Deserialize, Serialize};

/// Where in the document a veraPDF check failed, in terms pdfxray can address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Locator {
    /// The document as a whole (catalog, trailer, document level rules).
    Document,
    /// A page, by zero-based index in page order.
    Page {
        /// Zero-based page index.
        index: u32,
    },
    /// A raw COS object.
    Object {
        /// Object number.
        num: u32,
        /// Generation number.
        generation: u16,
    },
    /// A structure element, addressed by its object (when indirect) and the `K` index path from
    /// the structure tree root.
    StructElem {
        /// Object number and generation of the element, `None` for direct elements.
        obj: Option<(u32, u16)>,
        /// Child indices (`K[i]`) leading from the `StructTreeRoot` to the element.
        path: Vec<u32>,
    },
    /// A marked-content sequence on a page.
    MarkedContent {
        /// Zero-based page index.
        page: u32,
        /// Marked content identifier.
        mcid: i32,
    },
    /// A content stream operator.
    ContentOperator {
        /// Zero-based page index.
        page: u32,
        /// Zero-based index of the operator in veraPDF's operator list.
        operator_index: u32,
    },
    /// An annotation of a page.
    Annotation {
        /// Zero-based page index.
        page: u32,
        /// Zero-based index in the page's `/Annots` array.
        index: u32,
    },
    /// A font, by the name veraPDF reports.
    Font {
        /// Zero-based page index when the font was reached through a page.
        page: Option<u32>,
        /// Font name (`BaseFont`) as reported by veraPDF.
        name: String,
    },
    /// The context string could not be interpreted.
    Unknown,
}
