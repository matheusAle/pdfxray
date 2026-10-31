//! A tiny, deliberately independent PDF scanner.
//!
//! Nothing here uses `PdfWriter`, so a bug in the writer cannot hide behind the same bug in
//! the checks. It only understands the constructs the fixtures use.

use std::collections::BTreeMap;
use std::io::Read;

use pdfxray_fixtures::{CompressedObject, RevisionFacts, XrefKind};

/// One byte per char, so offsets into the `String` equal offsets into the file for ASCII text.
pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// `(object number, byte offset of "N 0 obj")` for every object header, in file order.
pub(crate) fn scan_objects(pdf: &[u8]) -> Vec<(u32, usize)> {
    let mut found = Vec::new();
    let mut start = 0;
    for line in pdf.split(|&b| b == b'\n') {
        let number = std::str::from_utf8(line)
            .ok()
            .and_then(|s| s.strip_suffix(" 0 obj"))
            .and_then(|n| n.parse::<u32>().ok());
        if let Some(number) = number {
            found.push((number, start));
        }
        start += line.len() + 1;
    }
    found
}

/// Offset of the *last* definition of `obj` (the live one after incremental updates).
pub(crate) fn object_offset(pdf: &[u8], obj: u32) -> Option<usize> {
    scan_objects(pdf)
        .into_iter()
        .rev()
        .find_map(|(n, offset)| (n == obj).then_some(offset))
}

/// Text of the last definition of `obj`, from its header to `endobj`.
pub(crate) fn object_text(pdf: &[u8], obj: u32) -> Option<String> {
    let offset = object_offset(pdf, obj)?;
    let rest = &pdf[offset..];
    let end = find(rest, b"endobj")? + b"endobj".len();
    Some(latin1(&rest[..end]))
}

/// Same as [`object_text`] but panics with a helpful message; for tests that know it exists.
pub(crate) fn text_of(pdf: &[u8], obj: u32) -> String {
    object_text(pdf, obj).unwrap_or_else(|| panic!("object {obj} not found"))
}

pub(crate) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub(crate) fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

/// The first `%PDF-x.y` version.
pub(crate) fn header_version(pdf: &[u8]) -> String {
    let line = pdf.split(|&b| b == b'\n').next().unwrap_or_default();
    latin1(line).trim_start_matches("%PDF-").to_owned()
}

/// Value after the last `startxref`.
pub(crate) fn last_startxref(pdf: &[u8]) -> usize {
    let at = pdf
        .windows(b"startxref\n".len())
        .rposition(|w| w == b"startxref\n")
        .expect("startxref present");
    let rest = latin1(&pdf[at + b"startxref\n".len()..]);
    rest.lines()
        .next()
        .expect("offset line")
        .parse()
        .expect("numeric offset")
}

/// Number following `key` (for example `"/Prev "`).
pub(crate) fn int_after(text: &str, key: &str) -> Option<i64> {
    let rest = &text[text.find(key)? + key.len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Float following `key`.
pub(crate) fn float_after(text: &str, key: &str) -> Option<f64> {
    let rest = &text[text.find(key)? + key.len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Numbers inside the `[...]` that follows `key` (for example `"/MediaBox "`).
pub(crate) fn numbers_after(text: &str, key: &str) -> Option<Vec<f64>> {
    let rest = &text[text.find(key)? + key.len()..];
    let open = rest.find('[')?;
    let close = rest.find(']')?;
    rest[open + 1..close]
        .split_whitespace()
        .map(|n| n.parse().ok())
        .collect()
}

/// Name following `key` (for example `"/Subtype "` gives `Header`).
pub(crate) fn name_after(text: &str, key: &str) -> Option<String> {
    let rest = text[text.find(key)? + key.len()..].strip_prefix('/')?;
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '/' | '>' | '<' | '[' | ']'))
        .unwrap_or(rest.len());
    Some(rest[..end].to_owned())
}

/// Object number of `key N 0 R`.
pub(crate) fn ref_after(text: &str, key: &str) -> Option<u32> {
    let rest = &text[text.find(key)? + key.len()..];
    let mut tokens = rest.split_whitespace();
    let n = tokens.next()?.parse().ok()?;
    (tokens.next()? == "0" && tokens.next()? == "R").then_some(n)
}

/// Every integer that follows `/MCID `, in order.
pub(crate) fn mcids_in(text: &str) -> Vec<i32> {
    text.match_indices("/MCID ")
        .filter_map(|(at, key)| {
            let rest = &text[at + key.len()..];
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[..end].parse().ok()
        })
        .collect()
}

/// Object numbers of all `N 0 R` references in `text`.
pub(crate) fn refs_in(text: &str) -> Vec<u32> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    tokens
        .windows(3)
        .filter(|w| w[1] == "0" && w[2].trim_end_matches([']', '>']) == "R")
        .filter_map(|w| w[0].trim_start_matches(['[', '<']).parse().ok())
        .collect()
}

/// Content stream bytes: the `/Length` bytes after `stream\n`, inflated when the dictionary names
/// `/FlateDecode` (the only filter the fixtures use).
pub(crate) fn stream_data(pdf: &[u8], obj: u32) -> Vec<u8> {
    let raw = raw_stream_data(pdf, obj);
    if latin1(&dict_text(pdf, obj)).contains("/FlateDecode") {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .expect("valid zlib stream");
        out
    } else {
        raw
    }
}

/// The stream's bytes exactly as stored, without decoding.
pub(crate) fn raw_stream_data(pdf: &[u8], obj: u32) -> Vec<u8> {
    let offset = object_offset(pdf, obj).expect("stream object");
    let rest = &pdf[offset..];
    let dict = latin1(&rest[..find(rest, b"stream\n").expect("stream keyword")]);
    let length = usize::try_from(int_after(&dict, "/Length ").expect("/Length")).expect("length");
    let start = find(rest, b"stream\n").expect("stream keyword") + b"stream\n".len();
    rest[start..start + length].to_vec()
}

/// The object text up to the `stream` keyword.
pub(crate) fn dict_text(pdf: &[u8], obj: u32) -> Vec<u8> {
    let offset = object_offset(pdf, obj).expect("object");
    let rest = &pdf[offset..];
    let end = find(rest, b"stream\n").unwrap_or(rest.len());
    rest[..end].to_vec()
}

/// One entry of a classic xref table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TableEntry {
    pub(crate) obj: u32,
    pub(crate) offset: usize,
    pub(crate) in_use: bool,
}

/// A parsed classic xref section.
pub(crate) struct ParsedTable {
    pub(crate) entries: Vec<TableEntry>,
    pub(crate) prev: Option<usize>,
    pub(crate) xref_stm: Option<usize>,
}

/// Entries and `/Prev` of the classic xref section at `at`.
pub(crate) fn parse_table(pdf: &[u8], at: usize) -> (Vec<TableEntry>, Option<usize>) {
    let parsed = parse_table_full(pdf, at);
    (parsed.entries, parsed.prev)
}

/// Parses the classic xref section at `at`, asserting the 20-byte entry format on the way.
pub(crate) fn parse_table_full(pdf: &[u8], at: usize) -> ParsedTable {
    let text = latin1(&pdf[at..]);
    let mut lines = text.split_inclusive('\n');
    assert_eq!(lines.next(), Some("xref\n"), "xref keyword at {at}");

    let mut entries = Vec::new();
    let mut trailer = String::new();
    let (mut next_obj, mut remaining) = (0u32, 0u32);
    while let Some(line) = lines.next() {
        if remaining > 0 {
            assert_eq!(line.len(), 20, "xref entry must be 20 bytes: {line:?}");
            assert!(line.ends_with(" \n"), "xref entry EOL: {line:?}");
            entries.push(TableEntry {
                obj: next_obj,
                offset: line[..10].parse().expect("offset digits"),
                in_use: &line[17..18] == "n",
            });
            next_obj += 1;
            remaining -= 1;
        } else if line == "trailer\n" {
            for rest in lines.by_ref().take_while(|l| *l != "startxref\n") {
                trailer.push_str(rest);
            }
            break;
        } else {
            let mut parts = line.split_whitespace();
            next_obj = parts.next().expect("subsection start").parse().expect("number");
            remaining = parts.next().expect("subsection count").parse().expect("number");
        }
    }
    let prev = int_after(&trailer, "/Prev ").map(|p| usize::try_from(p).expect("prev offset"));
    let xref_stm = int_after(&trailer, "/XRefStm ").map(|p| usize::try_from(p).expect("XRefStm offset"));
    ParsedTable {
        entries,
        prev,
        xref_stm,
    }
}

/// Walks the `startxref` / `/Prev` chain and describes every revision, oldest first.
pub(crate) fn observe_revisions(pdf: &[u8]) -> Vec<RevisionFacts> {
    observe_revisions_from(pdf, last_startxref(pdf))
}

/// Like [`observe_revisions`] but starting at an explicit xref offset, for files whose
/// `startxref` is wrong on purpose. A hybrid-reference section merges the table with its
/// `/XRefStm` stream.
pub(crate) fn observe_revisions_from(pdf: &[u8], start: usize) -> Vec<RevisionFacts> {
    let mut chain = Vec::new();
    let mut at = Some(start);
    while let Some(offset) = at {
        let facts = if pdf[offset..].starts_with(b"xref\n") {
            let parsed = parse_table_full(pdf, offset);
            at = parsed.prev;
            let mut objects: Vec<u32> = parsed
                .entries
                .iter()
                .filter(|e| e.in_use)
                .map(|e| e.obj)
                .collect();
            let mut compressed = Vec::new();
            if let Some(stm) = parsed.xref_stm {
                let (stream_facts, _) = decode_xref_stream(pdf, stm);
                objects.extend(stream_facts.objects);
                objects.sort_unstable();
                compressed = stream_facts.compressed;
            }
            RevisionFacts {
                number: 0,
                xref_kind: XrefKind::Table,
                xref_offset: offset as u64,
                prev: parsed.prev.map(|p| p as u64),
                objects,
                compressed,
                xref_stm: parsed.xref_stm.map(|p| p as u64),
            }
        } else {
            let (facts, prev) = decode_xref_stream(pdf, offset);
            at = prev;
            facts
        };
        chain.push(facts);
    }
    chain.reverse();
    for (i, rev) in chain.iter_mut().enumerate() {
        rev.number = u32::try_from(i + 1).expect("small");
    }
    chain
}

fn decode_xref_stream(pdf: &[u8], offset: usize) -> (RevisionFacts, Option<usize>) {
    let rest = &pdf[offset..];
    let header_end = find(rest, b"stream\n").expect("stream keyword");
    let dict = latin1(&rest[..header_end]);
    let widths = numbers_after(&dict, "/W ").expect("/W");
    assert_eq!(widths, [1.0, 4.0, 2.0], "fixtures use /W [1 4 2]");
    let index = numbers_after(&dict, "/Index ").expect("/Index");
    let length = usize::try_from(int_after(&dict, "/Length ").expect("/Length")).expect("length");
    let data = &rest[header_end + b"stream\n".len()..][..length];

    let mut objects = Vec::new();
    let mut compressed = Vec::new();
    let mut rows = data.chunks(7);
    for run in index.chunks(2) {
        for obj in (run[0] as u32)..(run[0] as u32 + run[1] as u32) {
            let row = rows.next().expect("row per index entry");
            let field2 = u32::from_be_bytes([row[1], row[2], row[3], row[4]]);
            let field3 = u32::from(u16::from_be_bytes([row[5], row[6]]));
            match row[0] {
                0 => {}
                1 => objects.push(obj),
                2 => {
                    objects.push(obj);
                    compressed.push(CompressedObject {
                        obj,
                        stream: field2,
                        index: field3,
                    });
                }
                other => panic!("unknown xref stream row type {other}"),
            }
        }
    }
    assert!(rows.next().is_none(), "no surplus rows");
    let prev = int_after(&dict, "/Prev ").map(|p| usize::try_from(p).expect("prev"));
    let facts = RevisionFacts {
        number: 0,
        xref_kind: XrefKind::Stream,
        xref_offset: offset as u64,
        prev: prev.map(|p| p as u64),
        objects,
        compressed,
        xref_stm: None,
    };
    (facts, prev)
}

/// Page objects (`/Type /Page`, not `/Pages`) in file order.
pub(crate) fn page_objects(pdf: &[u8]) -> Vec<u32> {
    let mut pages = Vec::new();
    for (obj, _) in scan_objects(pdf) {
        let is_page =
            object_text(pdf, obj).is_some_and(|t| t.contains("/Type /Page ") && !t.contains("/Type /ObjStm"));
        if is_page && !pages.contains(&obj) {
            pages.push(obj);
        }
    }
    pages
}

/// Nearest definition of `key` on `obj` or its `/Parent` chain: `(node, text of node)`.
pub(crate) fn inherited(pdf: &[u8], obj: u32, key: &str) -> Option<(u32, String)> {
    let text = object_text(pdf, obj)?;
    if text.contains(key) {
        return Some((obj, text));
    }
    let parent = ref_after(&text, "/Parent ")?;
    inherited(pdf, parent, key)
}

/// Resolves a role through `role_map` to a standard type, or `None` on a cycle or dead end.
pub(crate) fn resolve_role(role_map: &BTreeMap<String, String>, role: &str) -> Option<String> {
    const STANDARD: [&str; 12] = [
        "Document", "Part", "Sect", "Div", "H1", "H2", "P", "Span", "Caption", "Figure", "Note", "Lbl",
    ];
    let mut seen = vec![role.to_owned()];
    let mut current = role.to_owned();
    loop {
        if STANDARD.contains(&current.as_str()) {
            return Some(current);
        }
        let next = role_map.get(&current)?.clone();
        if seen.contains(&next) {
            return None;
        }
        seen.push(next.clone());
        current = next;
    }
}

/// `/K` of a structure element or the structure root: `(child object refs, MCIDs)`.
pub(crate) fn kids(text: &str) -> (Vec<u32>, Vec<i32>) {
    let Some(at) = text.find("/K ") else {
        return (Vec::new(), Vec::new());
    };
    let rest = text[at + 3..].trim_start();
    if rest.starts_with('[') {
        let end = rest.find(']').expect("closing bracket");
        (refs_in(&rest[..=end]), Vec::new())
    } else if rest.starts_with("<<") {
        let end = rest.find(">>").expect("closing dict");
        (Vec::new(), mcids_in(&rest[..end]))
    } else {
        let tokens: Vec<&str> = rest.split_whitespace().take(3).collect();
        let n: i64 = tokens[0].parse().expect("integer or reference");
        if tokens.get(1) == Some(&"0") && tokens.get(2).is_some_and(|t| t.starts_with('R')) {
            (vec![u32::try_from(n).expect("object number")], Vec::new())
        } else {
            (Vec::new(), vec![i32::try_from(n).expect("mcid")])
        }
    }
}

/// `/RoleMap << /A /B ... >>` pairs.
pub(crate) fn role_map(text: &str) -> BTreeMap<String, String> {
    let Some(at) = text.find("/RoleMap <<") else {
        return BTreeMap::new();
    };
    let rest = &text[at + "/RoleMap <<".len()..];
    let body = &rest[..rest.find(">>").expect("closing dict")];
    let names: Vec<&str> = body.split_whitespace().collect();
    names
        .chunks(2)
        .map(|pair| {
            (
                pair[0].trim_start_matches('/').to_owned(),
                pair[1].trim_start_matches('/').to_owned(),
            )
        })
        .collect()
}

/// `/Nums [key [refs...] ...]` as `(key, per-MCID owner)` pairs.
pub(crate) fn parent_tree_nums(text: &str) -> Vec<(i32, Vec<Option<u32>>)> {
    let Some(at) = text.find("/Nums [") else {
        return Vec::new();
    };
    let rest = &text[at + "/Nums [".len()..];
    let mut rest = rest[..rest.rfind(']').expect("closing bracket")].trim();
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (key, after) = rest.split_once(char::is_whitespace).expect("key then array");
        let after = after.trim_start();
        let end = after.find(']').expect("array end");
        let tokens: Vec<&str> = after[1..end].split_whitespace().collect();
        let mut owners = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if tokens[i] == "null" {
                owners.push(None);
                i += 1;
            } else {
                owners.push(Some(tokens[i].parse().expect("object number")));
                i += 3;
            }
        }
        out.push((key.parse().expect("key"), owners));
        rest = after[end + 1..].trim();
    }
    out
}

/// Object number of the trailer `/Root`, from the last trailer or xref stream dictionary.
pub(crate) fn root_object(pdf: &[u8]) -> u32 {
    let text = latin1(pdf);
    let at = text.rfind("/Root ").expect("/Root present");
    ref_after(&text[at..], "/Root ").expect("reference")
}
