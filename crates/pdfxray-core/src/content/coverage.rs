//! How much of a page's painted content is tagged, artifact or neither.

use serde::Serialize;

use crate::capture::PaintKind;
use crate::{Document, PageIx};

/// Painted items of a page by how they are marked. See [`Document::coverage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Coverage {
    /// Items inside a marked-content sequence that carries an MCID (real content tied to the
    /// structure tree), and not inside an artifact.
    pub tagged: u32,
    /// Items inside an `/Artifact` sequence, wherever an MCID sequence also encloses them.
    pub artifact: u32,
    /// Items inside neither an MCID sequence nor an artifact.
    pub untagged: u32,
    /// All painted items: `tagged + artifact + untagged`.
    pub total: u32,
    /// Execution indices of the operators that painted untagged items, ascending, once each.
    pub untagged_ops: Vec<u32>,
}

impl Document {
    /// Counts the items painted on `page` by how they are marked.
    ///
    /// An item is one painted thing: a glyph (a string of five characters is five items), a path
    /// fill or stroke, an image, a shading. Clips paint nothing and are not counted. Each item
    /// falls in exactly one class:
    ///
    /// * **artifact**: some enclosing marked-content sequence is an `/Artifact`. This wins over
    ///   the next rule, because content inside an artifact is not part of the structure even when
    ///   it carries an MCID (the tag checks report that case separately);
    /// * **tagged**: some enclosing sequence has an MCID;
    /// * **untagged**: neither. This is the class PDF/UA clause 7.1 forbids.
    ///
    /// Sequences opened in an invoking stream count for items inside Form `XObject`s. `None`
    /// when the page does not exist or its content was not captured.
    pub fn coverage(&self, page: PageIx) -> Option<Coverage> {
        let data = self.content.data(page)?;
        let mut coverage = Coverage {
            tagged: 0,
            artifact: 0,
            untagged: 0,
            total: 0,
            untagged_ops: Vec::new(),
        };
        for item in data
            .cap
            .items
            .iter()
            .filter(|i| !matches!(i.kind, PaintKind::Clip))
        {
            let around = || item.mc_stack.iter().filter_map(|&s| data.seq(s));
            if around().any(|s| s.artifact.is_some()) {
                coverage.artifact += 1;
            } else if around().any(|s| s.mcid.is_some()) {
                coverage.tagged += 1;
            } else {
                coverage.untagged += 1;
                // Items of one operator are contiguous, so a repeat is always the last entry.
                if coverage.untagged_ops.last() != Some(&item.op) {
                    coverage.untagged_ops.push(item.op);
                }
            }
            coverage.total += 1;
        }
        Some(coverage)
    }
}
