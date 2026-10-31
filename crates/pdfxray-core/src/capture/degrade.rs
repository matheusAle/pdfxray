//! Why a page's operator-to-paint attribution cannot be trusted.

use std::fmt;

use crate::Point;

/// The first problem that made a page [`Alignment::Degraded`](super::Alignment::Degraded).
///
/// The walker keeps the reason typed; the wording users read (diagnostics, CLI JSON, the app)
/// is its [`Display`](fmt::Display) text, which becomes `Alignment::Degraded::reason`.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum DegradeReason {
    /// The engine panicked outside the run over one content stream.
    PagePanicked,
    /// The tokenizer's operator spans could not be matched to the stream.
    BoundariesLost,
    /// The tokenizer gave up at byte `from` of a content stream.
    Untokenizable {
        /// Byte offset where tokenizing stopped.
        from: usize,
    },
    /// The engine panicked while running one content stream.
    StreamPanicked {
        /// Number of operators the engine reached before the panic.
        after: usize,
    },
    /// The engine drew something before the first operator marker.
    StrayContent,
    /// The engine stopped before a painting operator.
    EngineStopped {
        /// Index of the first operator the engine did not reach.
        at: usize,
        /// Number of operators in the stream.
        count: usize,
        /// The first painting operator left unexecuted.
        operator: String,
    },
    /// A text operator drew a different number of glyphs than its string has codes.
    GlyphCountMismatch {
        /// The text operator.
        op: u32,
        /// Glyphs the engine drew.
        glyphs: usize,
        /// Character codes in the string.
        codes: usize,
    },
    /// The engine's first glyph is not where the tracked text matrix puts it.
    FirstGlyphMisplaced {
        /// The text operator.
        op: u32,
        /// Origin of the engine's first glyph, page space.
        engine: Point,
        /// Origin the walker's text state predicts, page space.
        tracked: Point,
    },
    /// A form shows text in a font inherited from its invoker that its own resources lack.
    InheritedFontUndefined {
        /// The `Do` that entered the form.
        op: u32,
        /// The form's resource name.
        form: String,
        /// The inherited font's resource name.
        font: String,
    },
}

impl fmt::Display for DegradeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PagePanicked => f.write_str("the engine panicked while capturing the page"),
            Self::BoundariesLost => f.write_str("could not locate operator boundaries in a content stream"),
            Self::Untokenizable { from } => write!(
                f,
                "the content stream could not be tokenized from byte {from}; operators after it are neither executed nor captured"
            ),
            Self::StreamPanicked { after } => {
                write!(
                    f,
                    "the engine panicked after operator {after} of a content stream"
                )
            }
            Self::StrayContent => f.write_str("the engine reported content outside any operator"),
            Self::EngineStopped { at, count, operator } => write!(
                f,
                "the engine stopped at operator {at} of {count} (before `{operator}`); the rest was not painted"
            ),
            Self::GlyphCountMismatch { op, glyphs, codes } => write!(
                f,
                "operator {op}: the engine drew {glyphs} glyphs but the string holds {codes} character codes"
            ),
            Self::FirstGlyphMisplaced { op, engine, tracked } => write!(
                f,
                "operator {op}: the engine placed the first glyph at ({:.3}, {:.3}) but the tracked text position is ({:.3}, {:.3})",
                engine.x, engine.y, tracked.x, tracked.y
            ),
            Self::InheritedFontUndefined { op, form, font } => write!(
                f,
                "operator {op}: form {form} shows text in the inherited font {font}, which its resources do not define; its glyph geometry is unreliable"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn every_reason_keeps_the_wording_users_read() {
        let texts: Vec<String> = [
            DegradeReason::PagePanicked,
            DegradeReason::BoundariesLost,
            DegradeReason::Untokenizable { from: 15 },
            DegradeReason::StreamPanicked { after: 3 },
            DegradeReason::StrayContent,
            DegradeReason::EngineStopped {
                at: 1,
                count: 4,
                operator: "re".to_owned(),
            },
            DegradeReason::GlyphCountMismatch {
                op: 7,
                glyphs: 2,
                codes: 3,
            },
            DegradeReason::FirstGlyphMisplaced {
                op: 9,
                engine: Point::new(1.0, 2.25),
                tracked: Point::new(-0.5, 1234.56789),
            },
            DegradeReason::InheritedFontUndefined {
                op: 12,
                form: "Fm0".to_owned(),
                font: "F1".to_owned(),
            },
        ]
        .iter()
        .map(ToString::to_string)
        .collect();

        assert_eq!(
            texts,
            [
                "the engine panicked while capturing the page",
                "could not locate operator boundaries in a content stream",
                "the content stream could not be tokenized from byte 15; operators after it are neither executed nor captured",
                "the engine panicked after operator 3 of a content stream",
                "the engine reported content outside any operator",
                "the engine stopped at operator 1 of 4 (before `re`); the rest was not painted",
                "operator 7: the engine drew 2 glyphs but the string holds 3 character codes",
                "operator 9: the engine placed the first glyph at (1.000, 2.250) but the tracked text position is (-0.500, 1234.568)",
                "operator 12: form Fm0 shows text in the inherited font F1, which its resources do not define; its glyph geometry is unreliable",
            ]
        );
        // smoke: no reason renders empty
        assert!(texts.iter().all(|t| !t.is_empty()));
    }
}
