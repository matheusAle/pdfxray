//! The reader presets: what each modelled viewer changes against the spec-neutral reading, rule by
//! rule.

use serde::Serialize;

/// A reader the speech order can model. See [`SpeechPreset::rules`] for what each one changes
/// against the spec-neutral [`SpeechPreset::Structure`].
///
/// Every preset but `Structure` is an approximation built from public documentation and the
/// public source of the viewers, as `docs/research/screen-reader-reading-order.md` describes it;
/// see [`SpeechPreset::is_approximation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechPreset {
    /// The structure tree read the way ISO 32000 and the Matterhorn Protocol describe, with no
    /// viewer's quirks. The default.
    Structure,
    /// Acrobat and Reader on Windows, as NVDA and JAWS consume it through MSAA and the Acrobat
    /// DOM.
    AcrobatWindows,
    /// Firefox's built-in viewer (pdf.js), through its struct-tree layer.
    FirefoxPdfJs,
    /// The default Chrome and Edge viewer (PDFium): tags ignored, content in paint order,
    /// artifacts read.
    ChromiumDefault,
}

impl SpeechPreset {
    /// Every preset, the neutral one first.
    pub const ALL: [Self; 4] = [
        Self::Structure,
        Self::AcrobatWindows,
        Self::FirefoxPdfJs,
        Self::ChromiumDefault,
    ];

    /// A short name for menus and headings.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Structure => "Structure (spec)",
            Self::AcrobatWindows => "Acrobat on Windows (NVDA, JAWS)",
            Self::FirefoxPdfJs => "Firefox (pdf.js)",
            Self::ChromiumDefault => "Chrome and Edge (default)",
        }
    }

    /// True when the preset models a particular viewer and so can be wrong about it. Only
    /// [`SpeechPreset::Structure`] is not an approximation.
    #[must_use]
    pub const fn is_approximation(self) -> bool {
        !matches!(self, Self::Structure)
    }

    /// The rules in which the preset differs from [`SpeechPreset::Structure`], each with its
    /// evidence. Citation keys are those of section 8 of the research note.
    ///
    /// Only differences the research rates as reliable are modelled. Left out on purpose, because
    /// the evidence is a single source or an inference:
    ///
    /// - which viewers pass a phrase-level `/Lang` on to the voice (pdf.js and Chromium
    ///   reportedly do not; `[PM]`, M);
    /// - whether pdf.js and Chromium read a `Private` element (they have no mapping for it;
    ///   `[PDFJS-STL]`, `[CR-ROLES]`, M);
    /// - whether anything speaks text painted invisibly (`3 Tr`); it is flagged on the item and
    ///   spoken in every preset (ISO 32000-1 §14.8.2.2.3; Acrobat undocumented, L);
    /// - `/E` expansions: applied by every preset that follows the tree (Acrobat's DOM exposes
    ///   them, `[A-SDK-MSAA]`; NVDA has no handling, L);
    /// - image `/Alt` by MCID and links placed next to the nearest text in Chromium
    ///   (`[CR-HEUR]`, M), and marked-content `/ActualText`, which `PDFium` applies (`[CR-STRUCT]`,
    ///   M);
    /// - `VoiceOver` and `PDFKit`, `TalkBack`, Edge on the Adobe reader, JAWS beyond its release notes:
    ///   no reliable model exists (research §4.4, §4.9).
    #[must_use]
    pub const fn rules(self) -> &'static [PresetRule] {
        match self {
            Self::Structure => &[],
            Self::AcrobatWindows => &ACROBAT_RULES,
            Self::FirefoxPdfJs => &PDFJS_RULES,
            Self::ChromiumDefault => &CHROMIUM_RULES,
        }
    }

    /// True when the preset follows the structure tree at all.
    pub(super) const fn follows_tags(self) -> bool {
        !matches!(self, Self::ChromiumDefault)
    }
}

/// One way a preset differs from [`SpeechPreset::Structure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PresetRule {
    /// What the rule is about.
    pub topic: &'static str,
    /// What the viewer does, compared with the spec-neutral reading.
    pub behaviour: &'static str,
    /// The sources, as keys of section 8 of `docs/research/screen-reader-reading-order.md`, with
    /// the research's confidence (H high, M medium).
    pub evidence: &'static str,
}

const ACROBAT_RULES: [PresetRule; 1] = [PresetRule {
    topic: "Alt and ActualText",
    behaviour: "On an element with both, only /Alt is said; /ActualText is dropped, not attached",
    evidence: "[A-SDK-MSAA] [NVDA-SRC] [NVDA#2174], H for the Acrobat DOM, M for what NVDA and JAWS then say",
}];

const PDFJS_RULES: [PresetRule; 3] = [
    PresetRule {
        topic: "Alt on containers",
        behaviour: "/Alt (or /ActualText when there is no /Alt) replaces an element only on the Figure \
                    path: a Figure with /Alt, a /BBox attribute and content as its first child. Anywhere \
                    else the label goes on the element and its children are read",
        evidence: "[PDFJS-STL] [PDFJS-CORE], H for the code at a pinned version",
    },
    PresetRule {
        topic: "Role mapping",
        behaviour: "Follows one /RoleMap hop only, so a role that needs two reaches the reader as a \
                    generic element",
        evidence: "[PDFJS-CORE] [PDFIUM-ST], M in section 2.6, H for the code at a pinned version in section 4.4",
    },
    PresetRule {
        topic: "Table headers",
        behaviour: "A cell's /Headers list is used; otherwise a TH is a row or column header by its \
                    Scope, else by position (the PDF 1.7 rules, whatever the PDF version)",
        evidence: "[PDFJS-STL] [PDFJS#21702], H",
    },
];

const CHROMIUM_RULES: [PresetRule; 4] = [
    PresetRule {
        topic: "Order",
        behaviour: "The tags are ignored: text is read in content-stream (paint) order",
        evidence: "[CR-HEUR] [CR-FEAT] [IAG] [PM], H",
    },
    PresetRule {
        topic: "Artifacts",
        behaviour: "Artifacts are read like any other text",
        evidence: "[CR-HEUR] [PM], H",
    },
    PresetRule {
        topic: "Roles",
        behaviour: "No headings, tables, lists or links are announced on a tagged file; text only",
        evidence: "[CR-HEUR] [IAG] [PM], H",
    },
    PresetRule {
        topic: "Untagged content",
        behaviour: "Text outside any marked content is read where it was painted",
        evidence: "[CR-HEUR], M",
    },
];
