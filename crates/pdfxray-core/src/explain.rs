//! Plain-language explanations in pdfxray's own words: what a content-stream operator does and
//! what a diagnostic means for the reader of the file.
//!
//! * [`operator`] and [`operator_with_text`] describe one operator of ISO 32000-2 table A.1,
//!   filling in its operands ("font F1 at 26 pt").
//! * [`diagnostic`] explains a diagnostic code; [`DiagnosticInfo::explain`](crate::DiagnosticInfo)
//!   fills in the figures from the diagnostic itself.

mod diagnostics;
mod operators;

/// A note on one operator, in lowercase plain language of at most 60 characters.
///
/// `operands` are in PDF syntax, as in [`OpRecord::operands`](crate::capture::OpRecord). A missing
/// operand reads as `?`; an operator that is not in the specification gets a generic note.
///
/// ```
/// use pdfxray_core::explain;
///
/// assert_eq!(explain::operator("Td", &["40".into(), "676".into()]), "move to x 40, y 676");
/// assert_eq!(explain::operator("Tf", &["/F1".into(), "26".into()]), "font F1 at 26 pt");
/// ```
pub fn operator(op: &str, operands: &[String]) -> String {
    operators::note(op, operands, None)
}

/// Like [`operator`], for text-showing operators (`Tj`, `TJ`, `'`, `"`) whose decoded text is
/// known: "draw “Annual Report 2025”". Other operators ignore `text`.
///
/// ```
/// use pdfxray_core::explain;
///
/// let note = explain::operator_with_text("Tj", &["<0041>".into()], Some("Annual Report 2025"));
/// assert_eq!(note, "draw “Annual Report 2025”");
/// ```
pub fn operator_with_text(op: &str, operands: &[String], text: Option<&str>) -> String {
    operators::note(op, operands, text)
}

/// True when [`operator`] has a specific note for `op` (every operator of ISO 32000-2).
pub fn is_known_operator(op: &str) -> bool {
    operators::is_known(op)
}

/// Every operator [`operator`] has a specific note for.
pub fn known_operators() -> &'static [&'static str] {
    &operators::OPERATORS
}

/// The generic explanation of the diagnostic `code` (for example `stream-length-wrong`), without
/// any figures. `None` for a code pdfxray does not produce.
pub fn diagnostic(code: &str) -> Option<&'static str> {
    diagnostics::generic(code)
}

/// Every diagnostic code that has an explanation.
pub fn diagnostic_codes() -> impl Iterator<Item = &'static str> {
    diagnostics::TABLE.iter().map(|(code, _)| *code)
}

/// The explanation of `code` with the figures of `message` written in where the code has any.
pub(crate) fn diagnostic_with_message(code: &str, message: &str) -> Option<String> {
    diagnostics::with_figures(code, message)
}

#[cfg(test)]
mod tests;
