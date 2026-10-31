//! `pdf2-namespaces`: a PDF 2.0 structure tree with three namespaces (PDF 2.0, `MathML` and a custom
//! one that maps back to the PDF 2.0 types through `/RoleMapNS`), `/Ref`, `/Phoneme` and an
//! associated file.

use std::collections::BTreeMap;

use super::common::{HELVETICA, built};
use crate::expected::{
    CatalogFacts, EmbeddedFileFacts, Expected, NamespaceFacts, NodeFacts, OutlineFacts, PageFacts,
    RoleTarget, Sourced,
};
use crate::writer::{PdfVersion, PdfWriter, Trailer};
use crate::{Built, WriterError};

/// Namespace URI of the PDF 2.0 structure types.
const PDF2_NS: &str = "http://iso.org/pdf2/ssn";
/// Namespace URI of `MathML`.
const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";
/// A namespace that exists only in this fixture.
const CUSTOM_NS: &str = "urn:pdfxray:test:chapters";
/// The embedded file's bytes, 12 of them.
const EMBEDDED: &str = "pdfxray data";

const CONTENT: &str = "\
/H1 << /MCID 0 >> BDC BT /F1 24 Tf 20 160 Td (Namespaces) Tj ET EMC
/P << /MCID 1 >> BDC BT /F1 12 Tf 20 130 Td (cat) Tj ET EMC
/mi << /MCID 2 >> BDC BT /F1 12 Tf 20 100 Td (x) Tj ET EMC
/mo << /MCID 3 >> BDC BT /F1 12 Tf 35 100 Td (+) Tj ET EMC
/mn << /MCID 4 >> BDC BT /F1 12 Tf 50 100 Td (2) Tj ET EMC
/Figure << /MCID 5 >> BDC 0.9 0.8 0.8 rg 20 40 60 40 re f EMC
/Aside << /MCID 6 >> BDC BT /F1 10 Tf 100 40 Td (An aside) Tj ET EMC";

pub(super) fn build() -> Result<Built, WriterError> {
    let mut w = PdfWriter::new(PdfVersion::V2_0);
    w.object(
        1,
        "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 6 0 R /MarkInfo << /Marked true >> /Lang (en-US) >>",
    );
    w.object(2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    w.object(
        3,
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /StructParents 0 \
         /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
    );
    w.stream(4, "", CONTENT.as_bytes());
    w.object(5, HELVETICA);
    w.object(
        6,
        "<< /Type /StructTreeRoot /K 10 0 R /Namespaces [7 0 R 8 0 R 9 0 R] \
         /ParentTree 22 0 R /ParentTreeNextKey 1 >>",
    );
    w.object(7, format!("<< /Type /Namespace /NS ({PDF2_NS}) >>"));
    w.object(8, format!("<< /Type /Namespace /NS ({MATHML_NS}) >>"));
    w.object(
        9,
        format!(
            "<< /Type /Namespace /NS ({CUSTOM_NS}) \
             /RoleMapNS << /Chapter [/Sect 7 0 R] /Aside [/Div 7 0 R] >> >>"
        ),
    );
    w.object(
        10,
        "<< /Type /StructElem /S /Document /NS 7 0 R /P 6 0 R /K [11 0 R] >>",
    );
    w.object(
        11,
        "<< /Type /StructElem /S /Chapter /NS 9 0 R /P 10 0 R /K [12 0 R 13 0 R 14 0 R 18 0 R 19 0 R] >>",
    );
    w.object(
        12,
        "<< /Type /StructElem /S /H1 /NS 7 0 R /P 11 0 R /Pg 3 0 R /K 0 >>",
    );
    w.object(
        13,
        "<< /Type /StructElem /S /P /NS 7 0 R /P 11 0 R /Pg 3 0 R /Phoneme (kat) /PhoneticAlphabet /ipa \
         /Ref [19 0 R] /K 1 >>",
    );
    w.object(
        14,
        "<< /Type /StructElem /S /math /NS 8 0 R /P 11 0 R /K [15 0 R 16 0 R 17 0 R] >>",
    );
    w.object(
        15,
        "<< /Type /StructElem /S /mi /NS 8 0 R /P 14 0 R /Pg 3 0 R /K 2 >>",
    );
    w.object(
        16,
        "<< /Type /StructElem /S /mo /NS 8 0 R /P 14 0 R /Pg 3 0 R /K 3 >>",
    );
    w.object(
        17,
        "<< /Type /StructElem /S /mn /NS 8 0 R /P 14 0 R /Pg 3 0 R /K 4 >>",
    );
    w.object(
        18,
        "<< /Type /StructElem /S /Figure /NS 7 0 R /P 11 0 R /Pg 3 0 R /Alt (Pale red box) /AF [20 0 R] /K 5 >>",
    );
    w.object(
        19,
        "<< /Type /StructElem /S /Aside /NS 9 0 R /P 11 0 R /Pg 3 0 R /K 6 >>",
    );
    w.object(
        20,
        "<< /Type /Filespec /F (data.txt) /UF (data.txt) /AFRelationship /Data /EF << /F 21 0 R >> >>",
    );
    w.stream(
        21,
        &format!(
            "/Type /EmbeddedFile /Subtype /text#2Fplain /Params << /Size {} >>",
            EMBEDDED.len()
        ),
        EMBEDDED.as_bytes(),
    );
    w.object(
        22,
        "<< /Nums [0 [12 0 R 13 0 R 15 0 R 16 0 R 17 0 R 18 0 R 19 0 R]] >>",
    );
    w.finish_table(&Trailer::new(1))?;
    let out = w.into_output()?;

    let in_pdf2 = |node: NodeFacts| node.namespaces(PDF2_NS, PDF2_NS);
    let in_mathml = |node: NodeFacts| node.namespaces(MATHML_NS, MATHML_NS);
    let math = in_mathml(NodeFacts::new(14, "math", Some("math")))
        .elem(in_mathml(NodeFacts::new(15, "mi", Some("mi")).page(0).mcr(0, 2)))
        .elem(in_mathml(NodeFacts::new(16, "mo", Some("mo")).page(0).mcr(0, 3)))
        .elem(in_mathml(NodeFacts::new(17, "mn", Some("mn")).page(0).mcr(0, 4)));
    let mut paragraph = in_pdf2(NodeFacts::new(13, "P", Some("P")).page(0).mcr(0, 1));
    paragraph.phoneme = Some("kat".to_owned());
    paragraph.phonetic_alphabet = Some("ipa".to_owned());
    paragraph.refs = vec![19];
    let mut figure = in_pdf2(
        NodeFacts::new(18, "Figure", Some("Figure"))
            .page(0)
            .alt("Pale red box")
            .mcr(0, 5),
    );
    figure.associated_files = vec![20];
    // `Chapter` and `Aside` only exist in the custom namespace; RoleMapNS sends them to PDF 2.0.
    let chapter = NodeFacts::new(11, "Chapter", Some("Sect"))
        .namespaces(CUSTOM_NS, PDF2_NS)
        .elem(in_pdf2(NodeFacts::new(12, "H1", Some("H1")).page(0).mcr(0, 0)))
        .elem(paragraph)
        .elem(math)
        .elem(figure)
        .elem(
            NodeFacts::new(19, "Aside", Some("Div"))
                .namespaces(CUSTOM_NS, PDF2_NS)
                .page(0)
                .mcr(0, 6),
        );

    let mut outline = OutlineFacts::new(6);
    outline.namespaces = vec![
        NamespaceFacts {
            object: 7,
            uri: PDF2_NS.to_owned(),
            role_map: BTreeMap::new(),
        },
        NamespaceFacts {
            object: 8,
            uri: MATHML_NS.to_owned(),
            role_map: BTreeMap::new(),
        },
        NamespaceFacts {
            object: 9,
            uri: CUSTOM_NS.to_owned(),
            role_map: [("Chapter", "Sect"), ("Aside", "Div")]
                .iter()
                .map(|(from, to)| {
                    (
                        (*from).to_owned(),
                        RoleTarget {
                            role: (*to).to_owned(),
                            namespace: PDF2_NS.to_owned(),
                        },
                    )
                })
                .collect(),
        },
    ];
    outline.children = vec![in_pdf2(NodeFacts::new(10, "Document", Some("Document"))).elem(chapter)];
    outline.page_keys = vec![crate::expected::PageKeyFacts { page: 0, key: 0 }];
    outline.parent_tree = vec![crate::expected::ParentTreeEntry {
        key: 0,
        parents: [12, 13, 15, 16, 17, 18, 19].map(Some).to_vec(),
    }];
    outline.parent_tree_nodes = vec![22];
    outline.parent_tree_next_key = Some(1);

    let expected = Expected::new(
        PdfVersion::V2_0,
        &out.revisions,
        CatalogFacts::new(1).marked(true).lang("en-US"),
    )
    .with_page(PageFacts::new(0, 3, Sourced::own([0.0, 0.0, 200.0, 200.0], 3)).mcids(&[0, 1, 2, 3, 4, 5, 6]))
    .with_struct_outline(outline)
    .with_embedded_file(EmbeddedFileFacts {
        filespec: 20,
        name: "data.txt".to_owned(),
        relationship: "Data".to_owned(),
        stream: 21,
        size: 12,
    });
    Ok(built(out, expected))
}
