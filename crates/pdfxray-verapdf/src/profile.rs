//! Validation profiles and how they map onto the veraPDF command line.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A validation profile to run a document against.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Profile {
    /// PDF/UA-1 (ISO 14289-1), built-in flavour `ua1`.
    PdfUa1,
    /// PDF/UA-2 (ISO 14289-2) together with Tagged PDF rules (ISO 32005), flavour `ua2`.
    PdfUa2,
    /// WCAG 2.2 machine-checkable rules; profile file shipped with [`crate::install`].
    Wcag22Machine,
    /// Well-Tagged PDF 1.0 accessibility, flavour `wt1a`.
    Wtpdf10Accessibility,
    /// Tagged PDF structure relations for ISO 32000-1; profile file shipped with
    /// [`crate::install`].
    Iso32000_1Tagged,
    /// Tagged PDF structure relations for ISO 32000-2; profile file shipped with
    /// [`crate::install`].
    Iso32000_2Tagged,
    /// PDF/A-1a, flavour `1a`.
    PdfA1a,
    /// PDF/A-1b, flavour `1b`.
    PdfA1b,
    /// PDF/A-2a, flavour `2a`.
    PdfA2a,
    /// PDF/A-2b, flavour `2b`.
    PdfA2b,
    /// PDF/A-2u, flavour `2u`.
    PdfA2u,
    /// PDF/A-3a, flavour `3a`.
    PdfA3a,
    /// PDF/A-3b, flavour `3b`.
    PdfA3b,
    /// PDF/A-3u, flavour `3u`.
    PdfA3u,
    /// PDF/A-4, flavour `4`.
    PdfA4,
    /// PDF/A-4e, flavour `4e`.
    PdfA4e,
    /// PDF/A-4f, flavour `4f`.
    PdfA4f,
    /// A veraPDF validation profile XML file chosen by the user (`--profile`).
    Custom(PathBuf),
}

/// How a [`Profile`] is selected on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Selection {
    /// `--flavour <flag>`
    Flavour(&'static str),
    /// `--profile <profiles_dir>/<file>`
    ProfileFile(&'static str),
    /// `--profile <path>`
    Path(PathBuf),
}

impl Profile {
    pub(crate) fn selection(&self) -> Selection {
        match self {
            Self::PdfUa1 => Selection::Flavour("ua1"),
            Self::PdfUa2 => Selection::Flavour("ua2"),
            Self::Wtpdf10Accessibility => Selection::Flavour("wt1a"),
            Self::PdfA1a => Selection::Flavour("1a"),
            Self::PdfA1b => Selection::Flavour("1b"),
            Self::PdfA2a => Selection::Flavour("2a"),
            Self::PdfA2b => Selection::Flavour("2b"),
            Self::PdfA2u => Selection::Flavour("2u"),
            Self::PdfA3a => Selection::Flavour("3a"),
            Self::PdfA3b => Selection::Flavour("3b"),
            Self::PdfA3u => Selection::Flavour("3u"),
            Self::PdfA4 => Selection::Flavour("4"),
            Self::PdfA4e => Selection::Flavour("4e"),
            Self::PdfA4f => Selection::Flavour("4f"),
            Self::Wcag22Machine => Selection::ProfileFile("WCAG-2-2-Machine.xml"),
            Self::Iso32000_1Tagged => Selection::ProfileFile("ISO-32000-1-Tagged.xml"),
            Self::Iso32000_2Tagged => Selection::ProfileFile("ISO-32000-2-Tagged.xml"),
            Self::Custom(path) => Selection::Path(path.clone()),
        }
    }

    /// The built-in flavour flag (`--flavour`), if the profile is a built-in flavour.
    #[must_use]
    pub fn flavour_flag(&self) -> Option<&'static str> {
        match self.selection() {
            Selection::Flavour(flag) => Some(flag),
            _ => None,
        }
    }

    /// Short human readable label for pickers.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::PdfUa1 => "PDF/UA-1".to_owned(),
            Self::PdfUa2 => "PDF/UA-2".to_owned(),
            Self::Wcag22Machine => "WCAG 2.2 (machine)".to_owned(),
            Self::Wtpdf10Accessibility => "WTPDF 1.0 Accessibility".to_owned(),
            Self::Iso32000_1Tagged => "Tagged PDF (ISO 32000-1)".to_owned(),
            Self::Iso32000_2Tagged => "Tagged PDF (ISO 32000-2)".to_owned(),
            Self::PdfA1a => "PDF/A-1a".to_owned(),
            Self::PdfA1b => "PDF/A-1b".to_owned(),
            Self::PdfA2a => "PDF/A-2a".to_owned(),
            Self::PdfA2b => "PDF/A-2b".to_owned(),
            Self::PdfA2u => "PDF/A-2u".to_owned(),
            Self::PdfA3a => "PDF/A-3a".to_owned(),
            Self::PdfA3b => "PDF/A-3b".to_owned(),
            Self::PdfA3u => "PDF/A-3u".to_owned(),
            Self::PdfA4 => "PDF/A-4".to_owned(),
            Self::PdfA4e => "PDF/A-4e".to_owned(),
            Self::PdfA4f => "PDF/A-4f".to_owned(),
            Self::Custom(path) => format!("Custom ({})", path.display()),
        }
    }

    /// Every non-custom profile, in picker order.
    #[must_use]
    pub fn builtin() -> &'static [Profile] {
        &[
            Self::PdfUa1,
            Self::PdfUa2,
            Self::Wcag22Machine,
            Self::Wtpdf10Accessibility,
            Self::Iso32000_1Tagged,
            Self::Iso32000_2Tagged,
            Self::PdfA1a,
            Self::PdfA1b,
            Self::PdfA2a,
            Self::PdfA2b,
            Self::PdfA2u,
            Self::PdfA3a,
            Self::PdfA3b,
            Self::PdfA3u,
            Self::PdfA4,
            Self::PdfA4e,
            Self::PdfA4f,
        ]
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn selections_match_the_cli() {
        let got: Vec<(String, Selection)> = Profile::builtin()
            .iter()
            .map(|p| (p.label(), p.selection()))
            .collect();
        let expected = vec![
            ("PDF/UA-1".to_owned(), Selection::Flavour("ua1")),
            ("PDF/UA-2".to_owned(), Selection::Flavour("ua2")),
            (
                "WCAG 2.2 (machine)".to_owned(),
                Selection::ProfileFile("WCAG-2-2-Machine.xml"),
            ),
            ("WTPDF 1.0 Accessibility".to_owned(), Selection::Flavour("wt1a")),
            (
                "Tagged PDF (ISO 32000-1)".to_owned(),
                Selection::ProfileFile("ISO-32000-1-Tagged.xml"),
            ),
            (
                "Tagged PDF (ISO 32000-2)".to_owned(),
                Selection::ProfileFile("ISO-32000-2-Tagged.xml"),
            ),
            ("PDF/A-1a".to_owned(), Selection::Flavour("1a")),
            ("PDF/A-1b".to_owned(), Selection::Flavour("1b")),
            ("PDF/A-2a".to_owned(), Selection::Flavour("2a")),
            ("PDF/A-2b".to_owned(), Selection::Flavour("2b")),
            ("PDF/A-2u".to_owned(), Selection::Flavour("2u")),
            ("PDF/A-3a".to_owned(), Selection::Flavour("3a")),
            ("PDF/A-3b".to_owned(), Selection::Flavour("3b")),
            ("PDF/A-3u".to_owned(), Selection::Flavour("3u")),
            ("PDF/A-4".to_owned(), Selection::Flavour("4")),
            ("PDF/A-4e".to_owned(), Selection::Flavour("4e")),
            ("PDF/A-4f".to_owned(), Selection::Flavour("4f")),
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn custom_profile_uses_the_given_path() {
        let p = Profile::Custom(PathBuf::from("/tmp/my profile.xml"));
        assert_eq!(
            p.selection(),
            Selection::Path(PathBuf::from("/tmp/my profile.xml"))
        );
        assert_eq!(p.flavour_flag(), None);
        assert_eq!(Profile::PdfUa1.flavour_flag(), Some("ua1"));
    }
}
