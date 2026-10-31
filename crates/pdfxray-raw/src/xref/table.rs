//! Classic `xref` tables: subsections of fixed-width entries, then the trailer.

use super::{EntryState, ParsedSection, XrefEntry, XrefKind, XrefSection, int_entry, report};
use crate::lexer::TokenKind;
use crate::object::{Object, Spanned};
use crate::parser::Parser;
use crate::{RawDiagnostic, RawDiagnosticKind, Span, XrefEntryProblem};

pub(super) fn parse_table(src: &[u8], start: usize, diags: &mut Vec<RawDiagnostic>) -> ParsedSection {
    let mut parser = Parser::new(src, start + 4);
    let mut entries = Vec::new();
    let mut end = start + 4;
    let mut trailer = None;
    loop {
        let before = parser.lex.pos();
        let token = parser.lex.next_significant();
        match token.kind {
            TokenKind::Keyword if parser.lex.text(token.span) == b"trailer" => {
                end = token.span.end;
                if let Ok(Spanned {
                    value: Object::Dict(dict),
                    span,
                }) = parser.value(0)
                {
                    end = span.end;
                    trailer = Some(dict);
                } else {
                    report(diags, RawDiagnosticKind::TrailerMissing, token.span);
                }
                break;
            }
            TokenKind::Int(first) => {
                let second = parser.lex.next_significant();
                let (Ok(first), TokenKind::Int(count)) = (u32::try_from(first), second.kind) else {
                    let span = Span::new(token.span.start, second.span.end);
                    report(
                        diags,
                        RawDiagnosticKind::XrefTableMalformed {
                            detail: "subsection header is not two integers",
                        },
                        span,
                    );
                    report(diags, RawDiagnosticKind::TrailerMissing, Span::new(end, end));
                    parser.lex.set_pos(before);
                    break;
                };
                let header = Span::new(token.span.start, second.span.end);
                end = second.span.end;
                read_subsection(&mut parser, first, count, header, &mut entries, &mut end, diags);
            }
            _ => {
                let span = Span::new(token.span.start, token.span.end);
                report(
                    diags,
                    RawDiagnosticKind::XrefTableMalformed {
                        detail: "expected a subsection header or `trailer`",
                    },
                    span,
                );
                report(diags, RawDiagnosticKind::TrailerMissing, Span::new(end, end));
                break;
            }
        }
    }
    diags.extend(parser.take_diagnostics());
    let prev = trailer.as_ref().and_then(|t| int_entry(t, b"Prev"));
    let xrefstm = trailer.as_ref().and_then(|t| int_entry(t, b"XRefStm"));
    ParsedSection {
        section: XrefSection {
            kind: XrefKind::Table,
            span: Span::new(start, end),
            entries,
        },
        trailer,
        prev,
        xrefstm,
    }
}

fn read_subsection(
    parser: &mut Parser<'_>,
    first: u32,
    declared: i64,
    header: Span,
    entries: &mut Vec<XrefEntry>,
    end: &mut usize,
    diags: &mut Vec<RawDiagnostic>,
) {
    let src = parser.lex.source();
    let Ok(declared) = u32::try_from(declared) else {
        report(
            diags,
            RawDiagnosticKind::XrefTableMalformed {
                detail: "subsection entry count is negative or too large",
            },
            header,
        );
        return;
    };
    let mut found = 0u32;
    for i in 0..declared {
        let before = parser.lex.pos();
        let a = parser.lex.next_significant();
        let b = parser.lex.next_significant();
        let c = parser.lex.next_significant();
        let layout = match (&a.kind, &b.kind, &c.kind) {
            (TokenKind::Int(offset), TokenKind::Int(generation), TokenKind::Keyword) => {
                match parser.lex.text(c.span) {
                    b"n" => Some((true, *offset, *generation)),
                    b"f" => Some((false, *offset, *generation)),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some((in_use, offset, generation)) = layout else {
            parser.lex.set_pos(before);
            break;
        };
        found += 1;
        let Some(num) = first.checked_add(i) else {
            break;
        };
        let span = Span::new(a.span.start, c.span.end);
        *end = c.span.end;
        let mut problem = entry_problem(src, a.span, b.span, c.span);
        let generation_u16 = u16::try_from(generation).unwrap_or_else(|_| {
            problem.get_or_insert(XrefEntryProblem::FieldWidth);
            u16::MAX
        });
        if let Some(problem) = problem {
            report(
                diags,
                RawDiagnosticKind::XrefEntryMalformed { num, problem },
                span,
            );
        }
        let state = if in_use {
            let Ok(offset) = u64::try_from(offset) else {
                report(
                    diags,
                    RawDiagnosticKind::XrefEntryMalformed {
                        num,
                        problem: XrefEntryProblem::FieldWidth,
                    },
                    span,
                );
                continue;
            };
            EntryState::InUse { offset }
        } else {
            EntryState::Free {
                next: u32::try_from(offset).unwrap_or(0),
            }
        };
        entries.push(XrefEntry {
            num,
            generation: generation_u16,
            state,
            span: Some(span),
        });
    }
    if found < declared {
        report(
            diags,
            RawDiagnosticKind::XrefSubsectionShort {
                first,
                declared,
                found,
            },
            header,
        );
    }
}

/// Checks the 20 byte layout of ISO 32000-2 §7.5.4: 10 digits, space, 5 digits, space, `n`/`f`,
/// two byte end-of-line.
fn entry_problem(src: &[u8], offset: Span, generation: Span, keyword: Span) -> Option<XrefEntryProblem> {
    if offset.len() != 10 || generation.len() != 5 {
        return Some(XrefEntryProblem::FieldWidth);
    }
    if src.get(offset.end..generation.start) != Some(b" ")
        || src.get(generation.end..keyword.start) != Some(b" ")
    {
        return Some(XrefEntryProblem::Separator);
    }
    match src.get(keyword.end..keyword.end + 2) {
        Some(b" \n" | b" \r" | b"\r\n") => None,
        _ => Some(XrefEntryProblem::Eol),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::super::parse_section_at;
    use super::super::test_util::codes;
    use super::*;

    fn table(src: &[u8]) -> (ParsedSection, Vec<RawDiagnostic>) {
        let mut diags = Vec::new();
        let parsed = parse_section_at(src, 0, &mut diags).unwrap();
        (parsed, diags)
    }

    const CLEAN: &[u8] = b"xref\n0 3\n0000000000 65535 f \n0000000015 00000 n \n0000000064 00002 n \ntrailer\n<< /Size 3 /Root 1 0 R /Prev 7 >>\n";

    #[test]
    fn clean_table_has_exact_entries_and_spans() {
        let (parsed, diags) = table(CLEAN);

        assert_eq!(diags, vec![]);
        assert_eq!(parsed.section.kind, XrefKind::Table);
        assert_eq!(parsed.section.span, Span::new(0, CLEAN.len() - 1));
        assert_eq!(
            parsed.section.entries,
            vec![
                XrefEntry {
                    num: 0,
                    generation: 65535,
                    state: EntryState::Free { next: 0 },
                    span: Some(Span::new(9, 27))
                },
                XrefEntry {
                    num: 1,
                    generation: 0,
                    state: EntryState::InUse { offset: 15 },
                    span: Some(Span::new(29, 47))
                },
                XrefEntry {
                    num: 2,
                    generation: 2,
                    state: EntryState::InUse { offset: 64 },
                    span: Some(Span::new(49, 67))
                },
            ]
        );
        assert_eq!(parsed.prev.map(|(v, _)| v), Some(7));
        assert_eq!(parsed.trailer.unwrap().get_int(b"Size"), Some(3));
    }

    #[test]
    fn multiple_subsections_use_their_first_numbers() {
        let src =
            b"xref\n0 1\n0000000000 65535 f \n5 2\n0000000100 00000 n \n0000000200 00000 n \ntrailer\n<< >>";
        let (parsed, diags) = table(src);

        assert_eq!(codes(&diags), Vec::<&str>::new());
        let nums: Vec<_> = parsed.section.entries.iter().map(|e| (e.num, e.state)).collect();
        assert_eq!(
            nums,
            vec![
                (0, EntryState::Free { next: 0 }),
                (5, EntryState::InUse { offset: 100 }),
                (6, EntryState::InUse { offset: 200 }),
            ]
        );
    }

    #[test]
    fn short_entries_are_reported_but_read() {
        // 19 byte entries (`\n` only) and a one-digit generation.
        let src = b"xref\n0 2\n0000000000 65535 f\n0000000015 0 n\ntrailer\n<< >>";
        let (parsed, diags) = table(src);

        assert_eq!(parsed.section.entries.len(), 2);
        assert_eq!(
            diags,
            vec![
                RawDiagnostic::new(
                    RawDiagnosticKind::XrefEntryMalformed {
                        num: 0,
                        problem: XrefEntryProblem::Eol
                    },
                    Span::new(9, 27)
                ),
                RawDiagnostic::new(
                    RawDiagnosticKind::XrefEntryMalformed {
                        num: 1,
                        problem: XrefEntryProblem::FieldWidth
                    },
                    Span::new(28, 42)
                ),
            ]
        );
    }

    #[test]
    fn subsection_with_too_few_entries_is_reported() {
        let src = b"xref\n0 3\n0000000000 65535 f \n0000000015 00000 n \ntrailer\n<< >>";
        let (parsed, diags) = table(src);

        assert_eq!(parsed.section.entries.len(), 2);
        assert_eq!(
            diags,
            vec![RawDiagnostic::new(
                RawDiagnosticKind::XrefSubsectionShort {
                    first: 0,
                    declared: 3,
                    found: 2
                },
                Span::new(5, 8)
            )]
        );
    }

    #[test]
    fn missing_trailer_is_reported() {
        let (parsed, diags) = table(b"xref\n0 1\n0000000000 65535 f \n");

        assert_eq!(parsed.trailer, None);
        assert_eq!(codes(&diags), vec!["xref-table-malformed", "trailer-missing"]);
    }
}
