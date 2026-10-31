//! Helpers shared by the xref tests.

use crate::RawDiagnostic;

pub(super) fn codes(diags: &[RawDiagnostic]) -> Vec<&'static str> {
    diags.iter().map(RawDiagnostic::code).collect()
}
