//! Parsing of veraPDF *context strings* into [`Locator`]s (the type lives in `pdfxray-core`).
//!
//! veraPDF reports where a check failed as a slash separated path through its validation
//! model, for example
//!
//! ```text
//! root/document[0]/pages[0](12 0 obj PDPage)/contentStream[0](13 0 obj PDSemanticContentStream)/operators[17]
//! root/document[0]/StructTreeRoot[0](20 0 obj PDStructTreeRoot)/K[0](21 0 obj SEDocument Document)/K[2](..)
//! ```
//!
//! # Grammar
//!
//! ```text
//! context  := "root" ( "/" segment )*
//! segment  := name ( "[" index "]" )? ( "(" text ")" )? ( "{mcid:" int "}" )?
//! ```
//!
//! `text` may itself contain `/` and nested parentheses (XMP property names such as
//! `Properties[0](http://purl.org/dc/elements/1.1/ - dc:creator)`), so splitting is
//! parenthesis aware. Inside `text`, an object reference appears as `<num> <gen> obj <Type...>`
//! (or `outline <num> <gen>` for outline items).
//!
//! # Mapping
//!
//! The most specific locator that pdfxray can address wins:
//!
//! | context contains                                   | locator                                  |
//! |----------------------------------------------------|------------------------------------------|
//! | only `root` / `document` / `trailer`               | [`Locator::Document`]                    |
//! | `StructTreeRoot` only                              | [`Locator::Object`] of the root          |
//! | `StructTreeRoot/K[i]/K[j]...`                      | [`Locator::StructElem`] (`K` indices)    |
//! | `pages[p]` only (also `Group`, `Resources`, ...)   | [`Locator::Page`]                        |
//! | `pages[p]/annots[a]/...` (also appearance streams) | [`Locator::Annotation`]                  |
//! | `...{mcid:m}` below a page                         | [`Locator::MarkedContent`]               |
//! | `operators[n]/font[k](Name)`                       | [`Locator::Font`]                        |
//! | `operators[n]/xObject[k](N G obj ..)`              | [`Locator::Object`] of the `XObject`      |
//! | `operators[n]/...`                                 | [`Locator::ContentOperator`]             |
//! | `Outlines[i](outline N G)`, `metadata[i](N G obj)` | [`Locator::Object`]                      |
//!
//! Operators inside a Form `XObject` (`operators[n]/xObject[k]/contentStream[0]/operators[m]`)
//! are reported with the *outermost* operator index `n` (the `Do` on the page), because
//! [`Locator::ContentOperator`] has no way to name the nested stream.
//! Un-marked content items (`content[n]/contentItem[m]` without an MCID) fall back to
//! [`Locator::Page`]. The raw string is always preserved in `Finding::context`.

use pdfxray_core::validation::Locator;

/// One `/`-separated piece of a context string.
#[derive(Debug, Default)]
struct Segment<'a> {
    name: &'a str,
    index: Option<u32>,
    paren: Option<&'a str>,
    mcid: Option<i32>,
}

impl Segment<'_> {
    /// `(N G obj Type)` or `(outline N G)` as an object reference.
    fn object_ref(&self) -> Option<(u32, u16)> {
        let mut words = self.paren?.split_whitespace();
        let first = words.next()?;
        if first == "outline" {
            return Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?));
        }
        let generation = words.next()?;
        if words.next()? != "obj" {
            return None;
        }
        Some((first.parse().ok()?, generation.parse().ok()?))
    }
}

/// Parses a veraPDF context string. Never fails: unrecognised input yields [`Locator::Unknown`].
#[must_use]
pub fn parse_context(context: &str) -> Locator {
    let Some(segments) = split_segments(context.trim()) else {
        return Locator::Unknown;
    };
    let mut iter = segments.iter();
    if iter.next().map(|s| s.name) != Some("root") {
        return Locator::Unknown;
    }
    let rest = &segments[1..];
    match rest.first().map(|s| s.name) {
        None | Some("trailer") => Locator::Document,
        Some("document") => locate_in_document(&rest[1..]),
        Some(_) => Locator::Unknown,
    }
}

fn locate_in_document(segments: &[Segment<'_>]) -> Locator {
    if let Some(at) = segments.iter().position(|s| s.name == "StructTreeRoot") {
        return locate_in_struct_tree(&segments[at..]);
    }
    if let Some(at) = segments.iter().position(|s| s.name == "pages") {
        return locate_in_page(&segments[at..]);
    }
    // Document level objects (outlines, metadata, actions, fields ...): the deepest object
    // reference is the most specific thing we can name.
    if let Some(font) = segments.iter().rev().find_map(font_of) {
        return Locator::Font {
            page: None,
            name: font,
        };
    }
    segments
        .iter()
        .rev()
        .find_map(Segment::object_ref)
        .map_or(Locator::Document, |(num, generation)| Locator::Object {
            num,
            generation,
        })
}

fn font_of(segment: &Segment<'_>) -> Option<String> {
    (segment.name == "font")
        .then_some(segment.paren)
        .flatten()
        .map(str::to_owned)
}

fn locate_in_struct_tree(segments: &[Segment<'_>]) -> Locator {
    let mut obj = None;
    let mut path = Vec::new();
    for segment in &segments[1..] {
        // Property segments (`Lang`, `Alt`, `A`, ...) below an element stay on that element.
        if segment.name != "K" {
            break;
        }
        let Some(index) = segment.index else { break };
        path.push(index);
        obj = segment.object_ref();
    }
    if path.is_empty() {
        return segments[0]
            .object_ref()
            .map_or(Locator::Document, |(num, generation)| Locator::Object {
                num,
                generation,
            });
    }
    Locator::StructElem { obj, path }
}

fn locate_in_page(segments: &[Segment<'_>]) -> Locator {
    let Some(page) = segments[0].index else {
        return Locator::Unknown;
    };
    let below = &segments[1..];
    match below.first() {
        Some(first) if first.name == "annots" => {
            first
                .index
                .map_or(Locator::Page { index: page }, |index| Locator::Annotation {
                    page,
                    index,
                })
        }
        Some(first) if first.name == "contentStream" => locate_in_content_stream(page, below),
        _ => Locator::Page { index: page },
    }
}

fn locate_in_content_stream(page: u32, segments: &[Segment<'_>]) -> Locator {
    let mut mcid = None;
    let mut first_operator = None;
    let mut font = None;
    let mut xobject = None;
    for segment in segments {
        mcid = mcid.or(segment.mcid);
        match segment.name {
            "operators" => first_operator = first_operator.or(segment.index),
            "font" => font = font.or_else(|| font_of(segment)),
            // Deepest XObject that names an object (e.g. an image inside a form).
            "xObject" => xobject = segment.object_ref().or(xobject),
            _ => {}
        }
    }
    if let Some(mcid) = mcid {
        return Locator::MarkedContent { page, mcid };
    }
    if let Some(name) = font {
        return Locator::Font {
            page: Some(page),
            name,
        };
    }
    if let Some((num, generation)) = xobject {
        return Locator::Object { num, generation };
    }
    first_operator.map_or(Locator::Page { index: page }, |operator_index| {
        Locator::ContentOperator { page, operator_index }
    })
}

/// Splits on `/` outside parentheses and parses every piece. `None` on malformed input.
fn split_segments(context: &str) -> Option<Vec<Segment<'_>>> {
    if context.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    let mut depth = 0u32;
    let mut start = 0;
    for (at, ch) in context.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            '/' if depth == 0 => {
                segments.push(parse_segment(&context[start..at])?);
                start = at + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    segments.push(parse_segment(&context[start..])?);
    Some(segments)
}

fn parse_segment(raw: &str) -> Option<Segment<'_>> {
    let name_end = raw.find(['[', '(', '{']).unwrap_or(raw.len());
    if name_end == 0 {
        return None;
    }
    let mut segment = Segment {
        name: &raw[..name_end],
        ..Segment::default()
    };
    let mut rest = &raw[name_end..];
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('[') {
            let (digits, tail) = tail.split_once(']')?;
            segment.index = Some(digits.parse().ok()?);
            rest = tail;
        } else if rest.starts_with('(') {
            let close = matching_paren(rest)?;
            segment.paren = Some(&rest[1..close]);
            rest = &rest[close + 1..];
        } else {
            let (digits, tail) = rest.strip_prefix("{mcid:")?.split_once('}')?;
            segment.mcid = Some(digits.parse().ok()?);
            rest = tail;
        }
    }
    Some(segment)
}

/// Byte offset of the `)` matching the leading `(` of `s`.
fn matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0u32;
    for (at, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn page_stream(page: u32, page_obj: u32, stream_obj: u32) -> String {
        format!(
            "root/document[0]/pages[{page}]({page_obj} 0 obj PDPage)/contentStream[0]({stream_obj} 0 obj PDSemanticContentStream)"
        )
    }

    #[test]
    fn document_level_contexts() {
        let cases = [
            ("root", Locator::Document),
            ("root/document[0]", Locator::Document),
            ("root/trailer[0]/Info[0]", Locator::Document),
            ("root/document[0]/OpenActionDestination[0]", Locator::Document),
        ];
        for (context, expected) in cases {
            assert_eq!(parse_context(context), expected, "{context}");
        }
    }

    #[test]
    fn page_level_contexts() {
        assert_eq!(
            parse_context("root/document[0]/pages[3](12 0 obj PDPage)"),
            Locator::Page { index: 3 }
        );
        assert_eq!(
            parse_context("root/document[0]/pages[0](10 0 obj PDPage)/Group[0]/colorSpace[0]"),
            Locator::Page { index: 0 }
        );
    }

    #[test]
    fn operator_contexts() {
        let ctx = format!("{}/operators[17]", page_stream(0, 12, 13));
        assert_eq!(
            parse_context(&ctx),
            Locator::ContentOperator {
                page: 0,
                operator_index: 17
            }
        );
        let ctx = format!("{}/operators[5]/fillCS[0]", page_stream(2, 9, 10));
        assert_eq!(
            parse_context(&ctx),
            Locator::ContentOperator {
                page: 2,
                operator_index: 5
            }
        );
        // Plain (non-semantic) content streams use the same shape.
        let ctx = "root/document[0]/pages[1](12 0 obj PDPage)/contentStream[0](20 0 obj PDContentStream)/operators[23]/xObject[0]";
        assert_eq!(
            parse_context(ctx),
            Locator::ContentOperator {
                page: 1,
                operator_index: 23
            }
        );
    }

    #[test]
    fn marked_content_contexts() {
        let ctx = format!(
            "{}/content[1]{{mcid:1}}/contentItem[0]{{mcid:1}}",
            page_stream(0, 3, 4)
        );
        assert_eq!(parse_context(&ctx), Locator::MarkedContent { page: 0, mcid: 1 });
        // Content that is not marked at all has no MCID to point at.
        let ctx = format!("{}/content[2]/contentItem[0]", page_stream(1, 12, 20));
        assert_eq!(parse_context(&ctx), Locator::Page { index: 1 });
    }

    #[test]
    fn struct_element_chain() {
        let ctx = "root/document[0]/StructTreeRoot[0](20 0 obj PDStructTreeRoot)/K[0](21 0 obj SEDocument Document)/K[2](24 0 obj SEP P)";
        assert_eq!(
            parse_context(ctx),
            Locator::StructElem {
                obj: Some((24, 0)),
                path: vec![0, 2]
            }
        );
        let ctx = "root/document[0]/StructTreeRoot[0](5 0 obj PDStructTreeRoot)/K[0](11 0 obj SEDocument Document)/K[1](21 0 obj SETable Table)/K[1](42 0 obj SETR TR)/K[1](28 0 obj SETD TD)";
        assert_eq!(
            parse_context(ctx),
            Locator::StructElem {
                obj: Some((28, 0)),
                path: vec![0, 1, 1, 1]
            }
        );
    }

    #[test]
    fn struct_element_property_and_root() {
        let lang = "root/document[0]/StructTreeRoot[0](6 0 obj PDStructTreeRoot)/K[0](13 0 obj SEDocument Document)/K[0](21 0 obj SEP P)/Lang[0]";
        assert_eq!(
            parse_context(lang),
            Locator::StructElem {
                obj: Some((21, 0)),
                path: vec![0, 0]
            }
        );
        let root = "root/document[0]/StructTreeRoot[0](6 0 obj PDStructTreeRoot)";
        assert_eq!(
            parse_context(root),
            Locator::Object {
                num: 6,
                generation: 0
            }
        );
        let direct = "root/document[0]/StructTreeRoot[0](6 0 obj PDStructTreeRoot)/K[0]/K[3]";
        assert_eq!(
            parse_context(direct),
            Locator::StructElem {
                obj: None,
                path: vec![0, 3]
            }
        );
    }

    #[test]
    fn annotation_contexts() {
        assert_eq!(
            parse_context("root/document[0]/pages[0]/annots[1](30 0 obj PDAnnot)"),
            Locator::Annotation { page: 0, index: 1 }
        );
        assert_eq!(
            parse_context(
                "root/document[0]/pages[6](10 0 obj PDPage)/annots[0](22 0 obj PDLinkAnnot)/A[0](31 0 obj PDGoToAction)"
            ),
            Locator::Annotation { page: 6, index: 0 }
        );
        let appearance = "root/document[0]/pages[0](10 0 obj PDPage)/annots[2](16 0 obj PDMarkupAnnot)/appearance[0]/contentStream[0](40 0 obj PDContentStream)/operators[3]/extGState[0]";
        assert_eq!(
            parse_context(appearance),
            Locator::Annotation { page: 0, index: 2 }
        );
    }

    #[test]
    fn font_contexts() {
        let ctx = format!("{}/operators[4]/font[0](Helvetica)", page_stream(0, 3, 4));
        assert_eq!(
            parse_context(&ctx),
            Locator::Font {
                page: Some(0),
                name: "Helvetica".to_owned()
            }
        );
        let nested = "root/document[0]/pages[1](3 0 obj PDPage)/contentStream[0](4 0 obj PDContentStream)/operators[2]/xObject[0]/contentStream[0](9 0 obj PDContentStream)/operators[6]/font[0](ABCDEF+Times-Roman)";
        assert_eq!(
            parse_context(nested),
            Locator::Font {
                page: Some(1),
                name: "ABCDEF+Times-Roman".to_owned()
            }
        );
    }

    #[test]
    fn xobject_contexts() {
        let image = "root/document[0]/pages[0](3 0 obj PDPage)/contentStream[0](4 0 obj PDContentStream)/operators[8]/xObject[0](9 0 obj PDXImage)/imageCS[0]";
        assert_eq!(
            parse_context(image),
            Locator::Object {
                num: 9,
                generation: 0
            }
        );
        let form = "root/document[0]/pages[0](3 0 obj PDPage)/contentStream[0](4 0 obj PDContentStream)/operators[8]/xObject[0]/contentStream[0](9 0 obj PDContentStream)/operators[2]/fillCS[0]";
        assert_eq!(
            parse_context(form),
            Locator::ContentOperator {
                page: 0,
                operator_index: 8
            }
        );
        let nested_mcid = "root/document[0]/pages[0](3 0 obj PDPage)/contentStream[0](4 0 obj PDSemanticContentStream)/operators[8]/xObject[0]/contentStream[0](9 0 obj PDSemanticContentStream)/content[0]{mcid:7}/contentItem[0]{mcid:7}";
        assert_eq!(
            parse_context(nested_mcid),
            Locator::MarkedContent { page: 0, mcid: 7 }
        );
    }

    #[test]
    fn document_object_contexts() {
        assert_eq!(
            parse_context("root/document[0]/Outlines[2](outline 20 0)"),
            Locator::Object {
                num: 20,
                generation: 0
            }
        );
        assert_eq!(
            parse_context("root/document[0]/Outlines[0](outline 11 0)/A[0](14 0 obj PDGoToAction)"),
            Locator::Object {
                num: 14,
                generation: 0
            }
        );
        assert_eq!(
            parse_context("root/document[0]/metadata[0](8 0 obj PDMetadata)/XMPPackage[0]"),
            Locator::Object {
                num: 8,
                generation: 0
            }
        );
        // `/` inside parentheses must not split the segment.
        assert_eq!(
            parse_context(
                "root/document[0]/metadata[0](8 0 obj PDMetadata)/XMPPackage[0]/Properties[3](http://purl.org/dc/elements/1.1/ - dc:creator)"
            ),
            Locator::Object {
                num: 8,
                generation: 0
            }
        );
    }

    #[test]
    fn malformed_input_is_unknown() {
        for context in [
            "",
            "   ",
            "nothing",
            "root/document[0]/pages(",
            "root/document[x]",
            "root//x",
            "root/document[0]/pages[0](3 0 obj PDPage",
            "other/document[0]",
        ] {
            assert_eq!(parse_context(context), Locator::Unknown, "{context:?}");
        }
    }
}
