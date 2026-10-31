//! Plain-language explanations of every diagnostic code pdfxray can produce: the syntax layer's
//! (`pdfxray-raw`), the page-tree checks and the tagged-PDF checks.

/// `(code, text)` for every diagnostic code, in the order raw, page tree, structure.
pub(super) const TABLE: &[(&str, &str)] = &[
    // ---- file structure (pdfxray-raw) ----
    (
        "header-not-at-start",
        "The file has extra bytes before the %PDF- header. Most readers skip them, but stricter tools may refuse the file.",
    ),
    (
        "startxref-missing",
        "The end of the file does not say where the object index starts, so readers must search for it. Most do; some give up.",
    ),
    (
        "startxref-wrong",
        "The file's final pointer (startxref) does not lead to the object index. Readers that trust it fail; many silently search for the index instead.",
    ),
    (
        "xref-rebuilt",
        "The object index was damaged, so pdfxray rebuilt it by scanning the whole file. Readers that do not repair files cannot open it.",
    ),
    (
        "xref-section-offset-wrong",
        "An index section sits a few bytes away from where the file says. Tolerant readers skip the gap and find it.",
    ),
    (
        "xref-section-invalid",
        "A pointer to an earlier index section leads nowhere, so older revisions of the file cannot be followed.",
    ),
    (
        "xref-table-malformed",
        "The table that lists where each object lives is cut short or garbled. Objects after the damage may not be found.",
    ),
    (
        "xref-entry-malformed",
        "One line of the object table is not in the strict 20-byte layout. Readers usually understand it anyway.",
    ),
    (
        "xref-subsection-short",
        "A block of the object table promises more entries than it holds, so some objects have no recorded location.",
    ),
    (
        "xref-duplicate-entry",
        "The same object is listed twice in one table. Readers use the first live entry.",
    ),
    (
        "xref-stream-malformed",
        "The compressed object index (an xref stream) is damaged, so object locations cannot be read from it.",
    ),
    (
        "xref-stream-bad-entry-type",
        "An entry in the compressed object index has a type the format does not define. Readers ignore it.",
    ),
    (
        "prev-cycle",
        "The chain of earlier revisions loops back on itself. pdfxray stops at the loop; other readers may hang or give up.",
    ),
    (
        "trailer-missing",
        "There is no trailer dictionary, the place that points at the document root. Readers have to guess the root.",
    ),
    (
        "trailer-key-missing",
        "The trailer lacks a required entry such as /Root or /Size. Readers may have to guess it or refuse the file.",
    ),
    (
        "trailer-key-invalid",
        "A trailer entry holds the wrong kind of value, so readers cannot use it.",
    ),
    (
        "trailer-size-inconsistent",
        "The trailer's /Size is smaller than the highest object number in use. Strict readers may ignore the objects above it.",
    ),
    (
        "xref-offset-wrong",
        "The index points at the wrong byte for an object. Acrobat and most viewers search for it; a strict reader cannot find the object.",
    ),
    (
        "object-number-mismatch",
        "The index points at an object that carries a different number than the index lists. Readers may show the wrong object or ignore it.",
    ),
    (
        "stream-length-wrong",
        "The stream's declared length does not match its real length. Acrobat silently fixes this; other readers may drop the last command. pdfxray shows it as-is.",
    ),
    (
        "stream-keyword-eol",
        "The word 'stream' is not followed by CRLF or LF as required. Almost every reader copes; a strict one may read one byte too many.",
    ),
    (
        "missing-endstream",
        "A stream never ends with 'endstream', so where its data stops is a guess. Readers may drop data or keep extra bytes.",
    ),
    (
        "missing-endobj",
        "An object has no 'endobj' marker. Readers normally carry on, but the objects after it can be misread.",
    ),
    (
        "empty-object-body",
        "An object is defined but holds nothing. Readers treat it as null.",
    ),
    (
        "duplicate-dict-key",
        "A dictionary repeats a key. Readers disagree on whether the first or the last value wins, so the page can differ between viewers.",
    ),
    (
        "dict-key-not-name",
        "A dictionary has something other than a name where a key belongs, so the entries after it may be misread.",
    ),
    (
        "dict-value-missing",
        "A dictionary key has no value. The entry is dropped and the entries after it may shift.",
    ),
    (
        "unterminated-container",
        "An array or dictionary is never closed, so everything after it may be swallowed into it.",
    ),
    (
        "nesting-too-deep",
        "Arrays or dictionaries are nested far deeper than any real file. pdfxray stops there; other readers set their own limits.",
    ),
    (
        "unexpected-keyword",
        "A word that cannot appear here was found inside an object. It usually means data is corrupt or misplaced.",
    ),
    (
        "unterminated-string",
        "A text string is never closed with a ')', so it swallows what follows.",
    ),
    (
        "hex-string-odd-digits",
        "A hex string has an odd number of digits. The standard says to add a trailing 0, which every reader does.",
    ),
    (
        "hex-string-invalid-char",
        "A hex string contains a character that is not a hex digit. Readers drop it or reject the string.",
    ),
    (
        "invalid-name-escape",
        "A name has a '#' that is not followed by two hex digits, so it cannot be decoded exactly.",
    ),
    (
        "integer-overflow",
        "A whole number is too large for 64 bits and was read as a decimal, so some precision is lost.",
    ),
    (
        "trailing-data",
        "Bytes follow the final %%EOF marker. Readers ignore them; they sometimes hide leftover data or a signature.",
    ),
    (
        "missing-eof",
        "The final %%EOF marker is missing. Most readers do not mind, but strict checks and some appenders do.",
    ),
    (
        "unreferenced-object",
        "An object is in the file but no index lists it, so readers never see it. It is often leftover data from an earlier edit.",
    ),
    (
        "free-list-broken",
        "The list of deleted objects is inconsistent. It does not change what you see, but an editor may reuse object numbers wrongly.",
    ),
    (
        "objstm-malformed",
        "A container of compressed objects is damaged, so the objects inside cannot be read.",
    ),
    (
        "objstm-encrypted",
        "An object container is encrypted and pdfxray does not decrypt, so the objects inside are not listed.",
    ),
    (
        "objstm-entry-mismatch",
        "The index and a compressed-object container disagree about which object sits at a position, so the wrong object may be read.",
    ),
    (
        "objstm-member-malformed",
        "One object inside a compressed container cannot be parsed and is skipped.",
    ),
    (
        "objstm-container-missing",
        "The index says an object lives inside a container that does not exist, so the object is lost.",
    ),
    // ---- page tree and boxes ----
    (
        "page-tree-missing",
        "The document has no page tree, so there are no pages to show.",
    ),
    (
        "page-tree-bad-kid",
        "A page-tree node lists a child that is not a page or that loops back up the tree. That branch is skipped.",
    ),
    (
        "page-tree-too-deep",
        "Page nodes are nested so deeply that pdfxray stopped following them. Pages beyond that point are not shown.",
    ),
    (
        "page-count-mismatch",
        "The page tree lists a different number of pages than the renderer found, so viewers may disagree on the page count.",
    ),
    (
        "box-outside-mediabox",
        "A print box (crop, bleed, trim or art) sticks out of the page's paper (MediaBox). Viewers clip it to the paper, so the intended edges may be cut off.",
    ),
    // ---- tagged structure ----
    (
        "duplicate-element-id",
        "Two structure elements share the same ID, so a link to either of them by ID is ambiguous.",
    ),
    (
        "id-tree-mismatch",
        "The ID lookup table does not match the IDs the elements really carry, so a tool following an ID can land on the wrong element.",
    ),
    (
        "parent-tree-missing",
        "Pages have tagged content but the structure tree has no ParentTree, so tools cannot go from content back to its element.",
    ),
    (
        "struct-parent-missing",
        "Content or an annotation points at a ParentTree entry that does not exist, so tools cannot tell which element owns it.",
    ),
    (
        "parent-tree-mismatch",
        "The ParentTree says a piece of content belongs to one element while the structure tree says another.",
    ),
    (
        "objr-target-missing",
        "A structure element points at an annotation or object that is missing, so a screen reader cannot reach it.",
    ),
    (
        "mcid-not-in-content",
        "A structure element refers to marked content that is not on the page, so the element reads as empty.",
    ),
    (
        "mcid-not-in-structure",
        "Some page content is tagged but no structure element claims it, so a screen reader skips it.",
    ),
    (
        "mcid-duplicate-in-content",
        "Two marked-content sequences on one page use the same ID, so the structure cannot tell them apart.",
    ),
    (
        "marked-content-in-artifact",
        "Tagged content is nested inside an artifact (decoration). Assistive technology ignores artifacts, so that content is lost to it.",
    ),
    (
        "untagged-content",
        "Some content is neither tagged nor marked as an artifact, so a screen reader may skip it or read it out of order.",
    ),
    (
        "marked-true-no-struct-tree",
        "The file says it is tagged but has no structure tree, so there is nothing for assistive technology to read.",
    ),
    (
        "marked-false-but-tagged",
        "The file has a structure tree but does not declare itself tagged (/Marked true). Some readers ignore the tags.",
    ),
    (
        "pg-missing",
        "A structure element with page content does not say which page it is on, so its content cannot be found.",
    ),
    (
        "mcid-referenced-twice",
        "The same piece of content is claimed by two structure elements, so it would be read twice.",
    ),
    (
        "struct-kid-invalid",
        "A structure element lists a child that is not an element, a content ID or an object reference. It is ignored.",
    ),
    (
        "pg-not-a-page",
        "A structure element's /Pg does not point at a page, so its content cannot be placed.",
    ),
    (
        "struct-elem-direct",
        "A structure element is written inline instead of as its own object, so nothing else can refer to it.",
    ),
    (
        "struct-tree-too-deep",
        "The structure tree is nested so deeply that pdfxray stopped. Elements below that point are not shown.",
    ),
    (
        "struct-elem-cycle",
        "A structure element is its own ancestor. Readers that follow the tree can loop forever.",
    ),
    (
        "struct-elem-multiple-parents",
        "An element is listed under more than one parent, so its place in the reading order is ambiguous.",
    ),
    (
        "class-not-found",
        "An element uses an attribute class that the ClassMap does not define. Those attributes are ignored.",
    ),
    (
        "struct-elem-no-type",
        "A structure element has no type (/S), so a reader does not know whether it is a heading, a paragraph or something else.",
    ),
    (
        "struct-elem-parent-mismatch",
        "An element's /P parent link disagrees with the element that lists it as a child, so walking upward leads somewhere else.",
    ),
    (
        "non-standard-type",
        "A custom element type is not mapped to a standard one, so assistive technology does not know how to treat it.",
    ),
    (
        "rolemap-cycle",
        "The role map sends a type around in a circle, so it never resolves to a standard type.",
    ),
    (
        "reading-order-backtrack",
        "The structure tree makes a screen reader jump back up the page, for example reading a caption before the figure above it. Listeners hear the content out of order.",
    ),
    (
        "alt-hides-content",
        "An element with alternate text (/Alt) or actual text (/ActualText) still has content below it. Acrobat, and so NVDA and JAWS in it, say only that text and skip everything below, so a section described this way goes silent. A figure's description belongs on the figure. Related: Matterhorn 13-004 context, PDF/UA-1 section 7.3, PDF/UA-2 section 8.2.5.28, WCAG 1.3.1 and 1.1.1.",
    ),
    (
        "figure-alt-missing",
        "A figure has no alternate text (/Alt) or actual text (/ActualText) of its own or from an ancestor, so a screen reader says only \"graphic\". Decorative images should be artifacts instead. Matterhorn 13-004, PDF/UA-1 section 7.3, PDF/UA-2 section 8.2.5.28, WCAG technique PDF1 (success criterion 1.1.1).",
    ),
    (
        "alt-and-actualtext",
        "An element has both alternate text (/Alt) and actual text (/ActualText). Acrobat-based readers say only the /Alt and drop the /ActualText, while other tools expect both to be available. Keep the one that fits: /Alt describes, /ActualText replaces text. Tagged PDF Best Practice Guide section 5.5.2, PDF/UA-1 section 7.3, WCAG 1.1.1.",
    ),
    (
        "actualtext-on-structure",
        "Actual text (/ActualText) sits on a heading, table cell, list, figure or grouping element. It replaces the whole element with plain text, so a heading stops being a heading and a cell stops being a cell. Put /ActualText on a Span around the characters it replaces. Tagged PDF Best Practice Guide section 5.5.3, Matterhorn 13-005 for figures, WCAG 1.3.1.",
    ),
    (
        "rolemap-multi-hop",
        "A custom structure type reaches a standard one only through two or more role map steps. Acrobat follows the whole chain, but pdf.js and PDFium follow one step, so those viewers see a generic element with no heading or list meaning. Map each custom type straight to a standard type. Matterhorn 02-001 context, PDF/UA-1 section 7.1, WCAG 1.3.1.",
    ),
    (
        "private-hides-content",
        "A Private element holds content. Readers that follow the standard do not interpret a Private element or anything below it, so that content is never read. Private is for application data, not for page content. ISO 32000-1 Table 333, PDF/UA-1 section 7.1, WCAG 1.3.2.",
    ),
    (
        "table-header-missing",
        "A table with several rows and columns has no header cells (TH). A screen reader user hears every cell without knowing which column or row it belongs to. Matterhorn 15-001 and 15-002, PDF/UA-1 section 7.5, WCAG technique PDF6 (success criterion 1.3.1).",
    ),
    (
        "table-header-unresolved",
        "A data cell has no header cell: no /Headers entry applies and no header cell is found by scope or position, so it is read without context. Matterhorn 15-005, PDF/UA-1 section 7.5, PDF/UA-2 section 8.2.5.26, WCAG 1.3.1.",
    ),
    (
        "table-headers-id-missing",
        "A cell's /Headers list names an ID that no cell of its table has, so that header is silently dropped. Give the header cell the /ID, or correct the list. Matterhorn 15-005, PDF/UA-2 section 8.2.5.26, WCAG 1.3.1.",
    ),
    (
        "table-irregular",
        "The rows of a table are not all as wide as the widest one once row and column spans are counted, so cells do not line up in a grid. Screen readers navigate such tables unreliably. Matterhorn 09-004 context, PDF/UA-2 section 8.2.5.26, WCAG 1.3.1.",
    ),
    (
        "unused-resource",
        "A named resource is defined but no operator selects it. It only adds weight to the file.",
    ),
];

/// The generic explanation of `code`.
pub(super) fn generic(code: &str) -> Option<&'static str> {
    TABLE.iter().find(|(c, _)| *c == code).map(|(_, text)| *text)
}

/// The explanation of `code` with the figures from the diagnostic's `message` written in where
/// the code has any; otherwise the generic text.
pub(super) fn with_figures(code: &str, message: &str) -> Option<String> {
    let nums = integers(message);
    let figures = match code {
        "stream-length-wrong" => stream_length(message, &nums),
        "xref-offset-wrong" => xref_offset(message, &nums),
        "startxref-wrong" => startxref(message, &nums),
        "header-not-at-start" => nums.first().map(|at| {
            format!(
                "The %PDF- header starts at byte {} instead of byte 0. Most readers skip the extra bytes; stricter tools may refuse the file.",
                thousands(*at)
            )
        }),
        "trailing-data" => nums.first().map(|n| {
            format!(
                "{} byte{} follow the final %%EOF marker. Readers ignore them; they sometimes hide leftover data or a signature.",
                thousands(*n),
                if *n == 1 { "" } else { "s" }
            )
        }),
        "trailer-size-inconsistent" => match nums.as_slice() {
            [size, required] => Some(format!(
                "The trailer says /Size is {} but objects go up to number {}. Strict readers may ignore the objects above it.",
                thousands(*size),
                thousands(required - 1)
            )),
            _ => None,
        },
        _ => None,
    };
    figures.or_else(|| generic(code).map(str::to_owned))
}

fn stream_length(message: &str, nums: &[u64]) -> Option<String> {
    let actual = *nums.last()?;
    if message.contains("no usable /Length") {
        return Some(format!(
            "The stream has no usable length but really is {} bytes long. Acrobat silently fixes this; other readers may drop the last command. pdfxray shows it as-is.",
            thousands(actual)
        ));
    }
    let declared = *nums.get(nums.len().checked_sub(2)?)?;
    Some(format!(
        "The stream says it is {} bytes long but really is {}. Acrobat silently fixes this; other readers may drop the last command. pdfxray shows it as-is.",
        thousands(declared),
        thousands(actual)
    ))
}

fn xref_offset(message: &str, nums: &[u64]) -> Option<String> {
    if message.contains("nowhere in the file") {
        let recorded = *nums.last()?;
        return Some(format!(
            "The index says an object is at byte {}, but it is not in the file at all. Every reader loses that object.",
            thousands(recorded)
        ));
    }
    let [.., recorded, actual] = nums else {
        return None;
    };
    Some(format!(
        "The index says an object is at byte {} but it really starts at byte {}. Acrobat and most viewers search for it; a strict reader cannot find the object.",
        thousands(*recorded),
        thousands(*actual)
    ))
}

fn startxref(message: &str, nums: &[u64]) -> Option<String> {
    if message.contains("no xref section") {
        return None;
    }
    let [recorded, actual] = nums else {
        return None;
    };
    Some(format!(
        "The file's final pointer (startxref) says the index is at byte {} but it starts at byte {}. Readers that trust the pointer fail; many silently search for the index instead.",
        thousands(*recorded),
        thousands(*actual)
    ))
}

/// The unsigned integers in `text`, in order.
fn integers(text: &str) -> Vec<u64> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// `1204` as `1,204`.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn thousands_groups_digits() {
        let shown: Vec<String> = [0, 7, 999, 1_000, 1_204, 1_234_567]
            .into_iter()
            .map(thousands)
            .collect();

        assert_eq!(shown, ["0", "7", "999", "1,000", "1,204", "1,234,567"]);
    }

    #[test]
    fn codes_are_unique_and_kebab_case() {
        let mut seen = std::collections::BTreeSet::new();
        for (code, text) in TABLE {
            assert!(seen.insert(*code), "{code} appears twice");
            assert!(
                code.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'),
                "{code}"
            );
            assert!(text.ends_with('.'), "{code}: {text}");
        }
    }

    #[test]
    fn a_missing_length_is_explained_without_a_declared_figure() {
        let text = with_figures(
            "stream-length-wrong",
            "stream 5 0 R has no usable /Length; 43 data bytes before endstream",
        );

        assert_eq!(
            text.as_deref(),
            Some(
                "The stream has no usable length but really is 43 bytes long. Acrobat silently fixes this; other readers may drop the last command. pdfxray shows it as-is."
            )
        );
    }
}
