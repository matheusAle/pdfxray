use serde::{Deserialize, Serialize};

/// Which kinds of nodes a canvas click may select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerMode {
    /// Everything under the cursor.
    #[default]
    All,
    /// Structure elements and their marked content.
    Tags,
    /// Content operators and glyphs.
    Content,
    /// Annotations.
    Annotations,
    /// Page boxes.
    Boxes,
    /// Artifact sequences.
    Artifacts,
}

impl LayerMode {
    /// Every mode, in menu order.
    pub const ALL: [LayerMode; 6] = [
        Self::All,
        Self::Tags,
        Self::Content,
        Self::Annotations,
        Self::Boxes,
        Self::Artifacts,
    ];

    /// Menu title.
    pub fn title(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Tags => "Tags",
            Self::Content => "Content",
            Self::Annotations => "Annotations",
            Self::Boxes => "Boxes",
            Self::Artifacts => "Artifacts",
        }
    }
}
