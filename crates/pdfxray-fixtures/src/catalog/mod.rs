//! The fixture catalog. One module per fixture, each exposing a `build` function.

mod annotations;
mod artifacts;
mod box_matrix;
mod broken_xref;
mod common;
mod encrypted;
mod form_nesting;
mod graphics_state;
mod hybrid_xref;
mod images;
mod incremental;
mod long_doc;
mod minimal;
mod orphan_mcid;
mod page_boxes;
mod pdf2_namespaces;
mod reading_order;
mod reading_order_columns;
mod reading_order_footnote;
mod resources_inheritance;
mod rolemap;
mod rolemap_two_hops;
mod speech_alt_hides;
mod speech_artifacts;
mod speech_footnote;
mod speech_lang;
mod speech_link_objr;
mod startxref_broken;
mod struct_rich;
mod table_headers_ids;
mod table_scope_pdf2;
mod table_simple_pdf17;
mod tables;
mod tagged_simple;
mod text_state;
mod type3_font;
mod untagged_content;
mod xref_stream;

use crate::Fixture;

static ALL: [Fixture; 36] = [
    Fixture::new(
        "minimal-one-page",
        "One page showing Helvetica text; classic xref, no tags.",
        minimal::build,
    ),
    Fixture::new(
        "tagged-simple",
        "StructTreeRoot > Document > H1 + P with a ParentTree, MarkInfo and Lang.",
        tagged_simple::build,
    ),
    Fixture::new(
        "artifacts",
        "Pagination header and footer, a Layout artifact and a bare artifact around one tagged P.",
        artifacts::build,
    ),
    Fixture::new(
        "page-boxes",
        "Three pages covering every page box, Rotate, an inherited MediaBox, UserUnit and a TrimBox outside the MediaBox.",
        page_boxes::build,
    ),
    Fixture::new(
        "rolemap-custom",
        "Custom roles mapped to H1 and P through RoleMap, plus a LoopA/LoopB cycle.",
        rolemap::build,
    ),
    Fixture::new(
        "broken-xref-offsets",
        "Classic xref where the entries of objects 3 and 4 are off by +7 and -3 bytes.",
        broken_xref::build,
    ),
    Fixture::new(
        "incremental-update",
        "Two revisions: the second replaces the page content stream and adds an Info dictionary.",
        incremental::build,
    ),
    Fixture::new(
        "xref-stream-objstm",
        "PDF 1.5 file indexed by an xref stream with four objects inside an uncompressed object stream.",
        xref_stream::build,
    ),
    Fixture::new(
        "form-xobject-nesting",
        "Page paints Form A (with Matrix) which paints Form B; tagged marked content in all three streams.",
        form_nesting::build,
    ),
    Fixture::new(
        "orphan-mcid",
        "Content MCID without a struct element, struct element with a missing MCID, and a ParentTree entry that disagrees with K.",
        orphan_mcid::build,
    ),
    Fixture::new(
        "encrypted-stub",
        "Declares a Standard security handler in /Encrypt; stream data is not actually encrypted.",
        encrypted::build,
    ),
    Fixture::new(
        "text-state",
        "Tc, Tw, Tz, TL, Ts, Tr 0-3, TJ kerning, ' and \", two fonts and sizes via Tf and via Tm; Flate-compressed content.",
        text_state::build,
    ),
    Fixture::new(
        "graphics-state",
        "q/Q nesting, cm, line parameters, Gray/RGB/CMYK colours, ExtGState alpha and Multiply, rectangular clips and an axial shading.",
        graphics_state::build,
    ),
    Fixture::new(
        "images",
        "A 2x2 image XObject, a rotated inline image and an image painted from inside a Form XObject.",
        images::build,
    ),
    Fixture::new(
        "type3-font",
        "A Type 3 font with a filled square and a stroked triangle glyph procedure, shown at two sizes.",
        type3_font::build,
    ),
    Fixture::new(
        "struct-rich",
        "RoleMap, ClassMap, attributes, Alt/ActualText/Lang, IDTree, table, list, OBJR link and a paragraph continued over two pages with a Kids number tree.",
        struct_rich::build,
    ),
    Fixture::new(
        "pdf2-namespaces",
        "PDF 2.0 structure with PDF 2.0, MathML and custom namespaces, RoleMapNS, Ref, Phoneme and an associated file.",
        pdf2_namespaces::build,
    ),
    Fixture::new(
        "annotations",
        "Two links, a text field widget with an appearance stream, a text note and a hidden annotation, tagged with OBJR.",
        annotations::build,
    ),
    Fixture::new(
        "untagged-content",
        "Content outside any marked-content sequence, a duplicated MCID and tagged content nested inside an artifact.",
        untagged_content::build,
    ),
    Fixture::new(
        "box-matrix",
        "Non-zero media box origin, every page box, Rotate 180 and 270, UserUnit 3 and a CropBox inherited from Pages.",
        box_matrix::build,
    ),
    Fixture::new(
        "resources-inheritance",
        "Resources inherited from Pages, overridden by a page, a shared font, forms with and without resources and unused entries.",
        resources_inheritance::build,
    ),
    Fixture::new(
        "startxref-broken",
        "A valid document whose startxref points 12 bytes into the xref table.",
        startxref_broken::build,
    ),
    Fixture::new(
        "hybrid-xref",
        "Classic xref table plus an XRefStm that restores the font and Info dictionary held in an object stream.",
        hybrid_xref::build,
    ),
    Fixture::new(
        "long-doc",
        "40 tagged pages, each with a heading and three paragraphs, 201 structure elements.",
        long_doc::build,
    ),
    Fixture::new(
        "reading-order",
        "H1 (custom role Heading1), P, Caption, Figure and P in structure order, where the Caption sits below the Figure on the page.",
        reading_order::build,
    ),
    Fixture::new(
        "reading-order-columns",
        "A heading, two columns tagged column by column and a closing paragraph, so the climb from the first column to the second is a column break and not a backtrack.",
        reading_order_columns::build,
    ),
    Fixture::new(
        "reading-order-footnote",
        "Three paragraphs with a Note (label and text) tagged after the first but drawn at the page bottom, so the jump down and back up is a detour and not a backtrack.",
        reading_order_footnote::build,
    ),
    Fixture::new(
        "speech-alt-hides",
        "A Sect with /Alt over two paragraphs, and two figures with /Alt of which only one has the BBox pdf.js needs.",
        speech_alt_hides::build,
    ),
    Fixture::new(
        "speech-artifacts",
        "A pagination header and footer and a layout rule around a heading and a paragraph.",
        speech_artifacts::build,
    ),
    Fixture::new(
        "speech-footnote",
        "A paragraph with a footnote Reference whose Note is tagged after the next paragraph.",
        speech_footnote::build,
    ),
    Fixture::new(
        "table-simple-pdf17",
        "PDF 1.7 table of three rows and columns with a TH row and no Scope.",
        table_simple_pdf17::build,
    ),
    Fixture::new(
        "table-scope-pdf2",
        "PDF 2.0 table in the PDF 2.0 namespace with Scope Column on the first row and Scope Row on the first column.",
        table_scope_pdf2::build,
    ),
    Fixture::new(
        "table-headers-ids",
        "PDF 1.7 table with RowSpan and ColSpan header cells, Headers lists of IDs (one recursive, one dangling) on the data cells.",
        table_headers_ids::build,
    ),
    Fixture::new(
        "speech-lang",
        "A document in en-US with a de-DE Span and a fr-FR paragraph whose language is in the marked-content property list.",
        speech_lang::build,
    ),
    Fixture::new(
        "speech-link-objr",
        "Two Link elements and a Form holding object references to link and widget annotations.",
        speech_link_objr::build,
    ),
    Fixture::new(
        "rolemap-two-hops",
        "A heading role reaching H1 through two RoleMap steps beside a paragraph role that needs one.",
        rolemap_two_hops::build,
    ),
];

/// All fixtures in catalog order.
#[must_use]
pub fn all() -> &'static [Fixture] {
    &ALL
}
