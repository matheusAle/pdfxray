//! Facts about page geometry and resource dictionaries.
//!
//! # Effective render size
//!
//! The *visible box* of a page is its `/CropBox` (inherited like `/MediaBox`) intersected with its
//! `/MediaBox`; without a crop box it is the media box. The **render size at scale 1** is the
//! visible box' width and height in default user space units, with width and height swapped when
//! the effective `/Rotate` is 90 or 270. One unit renders to one pixel: `/UserUnit` is *not*
//! applied, because the renderers pdfxray builds on ignore it. The **physical size** is the render
//! size multiplied by `/UserUnit`, so it is in points (1/72 inch).

use serde::{Deserialize, Serialize};

use super::Rect;

/// Derived geometry of one page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageGeometryFacts {
    /// Zero-based page index.
    pub page: u32,
    /// Crop box (or media box) clipped to the media box, in default user space.
    pub visible_box: Rect,
    /// Effective `/Rotate` normalized to 0, 90, 180 or 270.
    pub rotate: i32,
    /// `/UserUnit`, 1.0 when absent.
    pub user_unit: f64,
    /// `[width, height]` in pixels at scale 1, see the module documentation.
    pub render_size: [f64; 2],
    /// `[width, height]` in points: the render size times the user unit.
    pub physical_size: [f64; 2],
}

/// One name in a resource category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEntryFacts {
    /// Category dictionary: `Font`, `XObject`, `ExtGState`, `ColorSpace`, `Shading`, ...
    pub category: String,
    /// Resource name without the slash.
    pub name: String,
    /// Object number the name refers to.
    pub target: u32,
    /// How many times the owner's content stream selects the name (`Tf`, `Do`, `gs`, `sh`, ...).
    pub uses: u32,
}

/// The effective resource dictionary of a page or form and where it comes from.
///
/// A form without `/Resources` uses the resources of whatever paints it; for such a form `node`
/// is the painter's resource node and `inherited` is true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceScopeFacts {
    /// Object number of the page or form `XObject` that owns the content stream.
    pub owner: u32,
    /// Object number of the page, `/Pages` node or form that carries the `/Resources` dictionary.
    pub node: u32,
    /// True when `node` is not the owner itself.
    pub inherited: bool,
    /// Every name of the effective dictionary, grouped by category in file order.
    pub entries: Vec<ResourceEntryFacts>,
}

/// An object that more than one scope refers to by resource name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedResourceFacts {
    /// Object number of the shared resource.
    pub object: u32,
    /// Owners (pages, forms) whose effective resources contain it, ascending.
    pub owners: Vec<u32>,
    /// Sum of `uses` over all owners.
    pub total_uses: u32,
}
