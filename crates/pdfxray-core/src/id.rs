use std::fmt;
use std::str::FromStr;

use pdfxray_raw::ObjRef;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Zero-based page index.
pub type PageIx = u32;

/// The page boxes of ISO 32000-2 §14.11.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BoxKind {
    /// `/MediaBox`.
    Media,
    /// `/CropBox`.
    Crop,
    /// `/BleedBox`.
    Bleed,
    /// `/TrimBox`.
    Trim,
    /// `/ArtBox`.
    Art,
}

impl BoxKind {
    /// All boxes, outermost first.
    pub const ALL: [BoxKind; 5] = [Self::Media, Self::Crop, Self::Bleed, Self::Trim, Self::Art];

    /// The PDF key without the slash, e.g. `MediaBox`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Media => "MediaBox",
            Self::Crop => "CropBox",
            Self::Bleed => "BleedBox",
            Self::Trim => "TrimBox",
            Self::Art => "ArtBox",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Crop => "crop",
            Self::Bleed => "bleed",
            Self::Trim => "trim",
            Self::Art => "art",
        }
    }

    fn from_token(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.token() == s)
    }
}

impl Serialize for BoxKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

/// Resource categories of a `/Resources` dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ResKind {
    /// `/Font`.
    Font,
    /// `/XObject`.
    XObject,
    /// `/ColorSpace`.
    ColorSpace,
    /// `/Pattern`.
    Pattern,
    /// `/Shading`.
    Shading,
    /// `/ExtGState`.
    ExtGState,
    /// `/Properties`.
    Properties,
}

impl ResKind {
    /// All categories in dictionary order.
    pub const ALL: [ResKind; 7] = [
        Self::Font,
        Self::XObject,
        Self::ColorSpace,
        Self::Pattern,
        Self::Shading,
        Self::ExtGState,
        Self::Properties,
    ];

    /// The PDF key without the slash, e.g. `XObject`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Font => "Font",
            Self::XObject => "XObject",
            Self::ColorSpace => "ColorSpace",
            Self::Pattern => "Pattern",
            Self::Shading => "Shading",
            Self::ExtGState => "ExtGState",
            Self::Properties => "Properties",
        }
    }

    fn from_key(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == s)
    }
}

/// Stable identity of every inspectable thing. The UI, the CLI and the tests all address
/// nodes by `NodeId`; its string form (see [`fmt::Display`]) is the CLI syntax.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeId {
    /// The document as a whole (`doc`).
    Document,
    /// The newest trailer dictionary (`trailer`).
    Trailer,
    /// A grouping row in a navigation tree, numbered per tree (`group:2`); it has no props.
    Group(u32),
    /// A raw indirect object in the newest revision (`obj:12:0`).
    Object(ObjRef),
    /// An incremental-update revision, oldest = 0 (`rev:1`).
    Revision(u32),
    /// A page (`page:0`).
    Page(PageIx),
    /// A page box (`box:0:crop`).
    PageBox {
        /// Page.
        page: PageIx,
        /// Which box.
        kind: BoxKind,
    },
    /// A structure element (`struct:20:0`).
    StructElem(ObjRef),
    /// A marked-content sequence with an MCID (`mc:0:3`).
    MarkedContent {
        /// Page whose content holds the sequence.
        page: PageIx,
        /// Marked-content identifier.
        mcid: i32,
    },
    /// An artifact sequence, numbered in paint order per page (`artifact:0:2`).
    Artifact {
        /// Page.
        page: PageIx,
        /// Sequence number on the page.
        seq: u32,
    },
    /// A content-stream operator, numbered in execution order per page including nested
    /// Form `XObject`s (`op:0:57`).
    Op {
        /// Page.
        page: PageIx,
        /// Execution index.
        ix: u32,
    },
    /// One glyph drawn by a text operator (`glyph:0:57:3`).
    Glyph {
        /// Page.
        page: PageIx,
        /// Execution index of the text-showing operator.
        op: u32,
        /// Glyph index within that operator.
        glyph: u32,
    },
    /// A named resource (`res:5:0:Font:F1`).
    Resource {
        /// Object holding the `/Resources` dictionary (page, form or pattern).
        owner: ObjRef,
        /// Category.
        kind: ResKind,
        /// Resource name without the slash.
        name: String,
    },
    /// A diagnostic, by index into the document's list (`diag:4`).
    Diagnostic(u32),
    /// A validation finding, by index into the last report (`val:9`).
    Validation(u32),
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document => write!(f, "doc"),
            Self::Trailer => write!(f, "trailer"),
            Self::Group(n) => write!(f, "group:{n}"),
            Self::Object(r) => write!(f, "obj:{}:{}", r.num, r.generation),
            Self::Revision(n) => write!(f, "rev:{n}"),
            Self::Page(p) => write!(f, "page:{p}"),
            Self::PageBox { page, kind } => write!(f, "box:{page}:{}", kind.token()),
            Self::StructElem(r) => write!(f, "struct:{}:{}", r.num, r.generation),
            Self::MarkedContent { page, mcid } => write!(f, "mc:{page}:{mcid}"),
            Self::Artifact { page, seq } => write!(f, "artifact:{page}:{seq}"),
            Self::Op { page, ix } => write!(f, "op:{page}:{ix}"),
            Self::Glyph { page, op, glyph } => write!(f, "glyph:{page}:{op}:{glyph}"),
            Self::Resource { owner, kind, name } => {
                write!(f, "res:{}:{}:{}:{name}", owner.num, owner.generation, kind.key())
            }
            Self::Diagnostic(n) => write!(f, "diag:{n}"),
            Self::Validation(n) => write!(f, "val:{n}"),
        }
    }
}

/// Error for an unparseable [`NodeId`] string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid node id `{0}`")]
pub struct ParseNodeIdError(pub String);

impl FromStr for NodeId {
    type Err = ParseNodeIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseNodeIdError(s.to_owned());
        let parts: Vec<&str> = s.splitn(5, ':').collect();
        let num = |i: usize| parts.get(i).and_then(|p| p.parse::<u32>().ok()).ok_or_else(err);
        let generation = |i: usize| parts.get(i).and_then(|p| p.parse::<u16>().ok()).ok_or_else(err);
        let arity = |n: usize| if parts.len() == n { Ok(()) } else { Err(err()) };
        let id = match parts.first().copied().unwrap_or_default() {
            "doc" => {
                arity(1)?;
                Self::Document
            }
            "trailer" => {
                arity(1)?;
                Self::Trailer
            }
            "group" => {
                arity(2)?;
                Self::Group(num(1)?)
            }
            "obj" => {
                arity(3)?;
                Self::Object(ObjRef::new(num(1)?, generation(2)?))
            }
            "rev" => {
                arity(2)?;
                Self::Revision(num(1)?)
            }
            "page" => {
                arity(2)?;
                Self::Page(num(1)?)
            }
            "box" => {
                arity(3)?;
                let kind = parts
                    .get(2)
                    .and_then(|t| BoxKind::from_token(t))
                    .ok_or_else(err)?;
                Self::PageBox { page: num(1)?, kind }
            }
            "struct" => {
                arity(3)?;
                Self::StructElem(ObjRef::new(num(1)?, generation(2)?))
            }
            "mc" => {
                arity(3)?;
                let mcid = parts.get(2).and_then(|p| p.parse::<i32>().ok()).ok_or_else(err)?;
                Self::MarkedContent { page: num(1)?, mcid }
            }
            "artifact" => {
                arity(3)?;
                Self::Artifact {
                    page: num(1)?,
                    seq: num(2)?,
                }
            }
            "op" => {
                arity(3)?;
                Self::Op {
                    page: num(1)?,
                    ix: num(2)?,
                }
            }
            "glyph" => {
                arity(4)?;
                Self::Glyph {
                    page: num(1)?,
                    op: num(2)?,
                    glyph: num(3)?,
                }
            }
            "res" => {
                arity(5)?;
                let kind = parts.get(3).and_then(|k| ResKind::from_key(k)).ok_or_else(err)?;
                let name = parts.get(4).filter(|n| !n.is_empty()).ok_or_else(err)?;
                Self::Resource {
                    owner: ObjRef::new(num(1)?, generation(2)?),
                    kind,
                    name: (*name).to_owned(),
                }
            }
            "diag" => {
                arity(2)?;
                Self::Diagnostic(num(1)?)
            }
            "val" => {
                arity(2)?;
                Self::Validation(num(1)?)
            }
            _ => return Err(err()),
        };
        Ok(id)
    }
}

impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn samples() -> Vec<(NodeId, &'static str)> {
        vec![
            (NodeId::Document, "doc"),
            (NodeId::Trailer, "trailer"),
            (NodeId::Group(2), "group:2"),
            (NodeId::Object(ObjRef::new(12, 0)), "obj:12:0"),
            (NodeId::Revision(1), "rev:1"),
            (NodeId::Page(0), "page:0"),
            (
                NodeId::PageBox {
                    page: 2,
                    kind: BoxKind::Trim,
                },
                "box:2:trim",
            ),
            (NodeId::StructElem(ObjRef::new(20, 1)), "struct:20:1"),
            (NodeId::MarkedContent { page: 0, mcid: 3 }, "mc:0:3"),
            (NodeId::Artifact { page: 1, seq: 2 }, "artifact:1:2"),
            (NodeId::Op { page: 0, ix: 57 }, "op:0:57"),
            (
                NodeId::Glyph {
                    page: 0,
                    op: 57,
                    glyph: 3,
                },
                "glyph:0:57:3",
            ),
            (
                NodeId::Resource {
                    owner: ObjRef::new(5, 0),
                    kind: ResKind::Font,
                    name: "F1:a".into(),
                },
                "res:5:0:Font:F1:a",
            ),
            (NodeId::Diagnostic(4), "diag:4"),
            (NodeId::Validation(9), "val:9"),
        ]
    }

    #[test]
    fn display_and_parse_round_trip() {
        for (id, text) in samples() {
            assert_eq!(id.to_string(), text);
            assert_eq!(text.parse::<NodeId>(), Ok(id));
        }
    }

    #[test]
    fn serde_uses_string_form() {
        let id = NodeId::MarkedContent { page: 0, mcid: -1 };

        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"mc:0:-1\"");
        assert_eq!(serde_json::from_str::<NodeId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<NodeId>("\"nope\"").is_err());
    }

    #[test]
    fn rejects_malformed_ids() {
        for bad in [
            "",
            "obj:1",
            "obj:1:0:9",
            "page:x",
            "box:0:sideways",
            "res:1:0:Nope:F1",
            "res:1:0:Font:",
            "zzz:1",
        ] {
            assert_eq!(
                bad.parse::<NodeId>(),
                Err(ParseNodeIdError(bad.to_owned())),
                "{bad}"
            );
        }
    }
}
