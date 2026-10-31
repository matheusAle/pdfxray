//! The speech order: a linear, document-wide list of what a screen reader announces, built from
//! the structure tree with the documented substitution and skip rules, for one reader preset.
//!
//! The list is not a text-to-speech string. Every item keeps the identity of the node it came
//! from, so a front end can draw it on the page and a test can assert on it.
//!
//! # What an item is
//!
//! The walk is a depth-first pre-order over `/K` (ISO 32000-1 §14.7.2, §14.8.2.3.1). It yields:
//!
//! - one item per run of consecutive marked-content references of an element, carrying the text
//!   they paint (an element with child elements in the middle of its `/K` yields several runs);
//! - one item for an element whose `/Alt`, `/ActualText` or `/E` replaces it (ISO 32000-1
//!   Table 323, §14.9.3–14.9.5), after which everything below it is listed as skipped, covered by
//!   that element;
//! - one item for each object reference, at its position among the `/K` entries
//!   (ISO 32000-1 §14.8.2.3.2). The content of its element is the annotation's name;
//! - one item for an empty table cell, which must still be conveyed;
//! - the artifacts of each page, as skipped items next to the content they were painted beside.
//!
//! An item that is not spoken carries a [`SkipReason`]; a front end that only wants the
//! transcript filters with [`SpeechOrder::spoken`].
//!
//! The reader presets are approximations of what real viewers do, documented rule by rule in
//! [`SpeechPreset::rules`]. Nothing here was recorded from a screen reader.
//!
//! # Where the code is
//!
//! This module holds the result types and [`Document::speech_order`]. The reader presets and their
//! rules are in [`preset`]; the depth-first walk that turns elements into items (text runs,
//! substitutions, object references) is in [`walk`]; the passes over the finished list (table
//! cells, artifacts, language changes, paint order) are in [`passes`]; footnote links are in
//! [`notes`].

mod notes;
mod passes;
mod preset;
mod walk;

use std::collections::HashMap;

use serde::Serialize;

pub use self::preset::{PresetRule, SpeechPreset};
use super::table::HeaderRef;
use super::{RoleClass, TagTree};
use crate::{Document, NodeId, ObjRef, PageIx};

/// Characters of painted text kept as the painted text of a substituted element.
const PAINTED_MAX: usize = 2000;

/// What replaced the text an element paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Substitution {
    /// `/Alt`.
    Alt,
    /// `/ActualText`.
    ActualText,
    /// `/E`, the expansion of an abbreviation.
    Expansion,
}

/// Why an item is not spoken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// An artifact: pagination, layout or background content, or tagged content inside one.
    Artifact,
    /// Inside a `Private` element, which readers do not interpret.
    Private,
    /// Below an element whose `/Alt`, `/ActualText` or `/E` replaces it.
    CoveredBy(NodeId),
    /// An element with nothing to say.
    Empty,
}

/// A change of language between two spoken items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LangChange {
    /// The language of the previous spoken item (the document's to begin with).
    pub from: Option<String>,
    /// The language of this item.
    pub to: Option<String>,
}

/// The table cell an item is, with its header cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpeechCell {
    /// The `Table` element.
    pub table: NodeId,
    /// First row, from 0.
    pub row: u32,
    /// First column, from 0.
    pub col: u32,
    /// Rows covered.
    pub row_span: u32,
    /// Columns covered.
    pub col_span: u32,
    /// True for a header cell (`TH`).
    pub header: bool,
    /// The header cells announced with the cell, nearest first, with the rule that found each.
    pub headers: Vec<HeaderRef>,
}

/// Which role an item has in a note link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteRole {
    /// The item is the reference to a note.
    Reference,
    /// The item is the start of the note.
    Note,
}

/// How a reference was tied to its note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteLinkSource {
    /// A `/Ref` entry (PDF 2.0), on the reference or on the note.
    Ref,
    /// The label of the reference equals the text of the `Lbl` of the next note after it.
    LabelMatch,
}

/// The link between a footnote reference and its note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoteFlag {
    /// Whether this item is the reference or the note.
    pub role: NoteRole,
    /// The other end.
    pub other: NodeId,
    /// How the two were tied.
    pub via: NoteLinkSource,
    /// Spoken items between the reference and the note: how far the listener travels.
    pub items_apart: usize,
}

/// Where a link goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    /// An external address.
    Uri(String),
    /// A page of the document, from 0.
    Page(PageIx),
    /// Anything else: a named destination, or another action, described.
    Other(String),
}

/// The annotation an item stands for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkFlag {
    /// The annotation's `/Subtype`.
    pub subtype: String,
    /// Where it goes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<LinkTarget>,
    /// Its `/Contents`. Readers do not announce it as a link's name (`[NVDA#15229]`), so it is
    /// shown for reference only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<String>,
}

/// One thing a screen reader announces. See [`Document::speech_order`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeechItem {
    /// Position in [`SpeechOrder::items`], from 1, skipped items included.
    pub index: usize,
    /// The node the item came from: the structure element, the annotation of an object
    /// reference, an artifact, or (Chromium) the content itself.
    pub node: NodeId,
    /// The marked content the item's text was painted by, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marked_content: Vec<NodeId>,
    /// The page of the item, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageIx>,
    /// The structure type as written in `/S`; empty when there is no element.
    pub declared_role: String,
    /// The structure type the reader announces: the standard type the role resolves to, as far as
    /// the preset follows the mapping. `None` when the reader announces no role.
    pub role: Option<String>,
    /// The family of the announced role.
    pub class: RoleClass,
    /// The level of a numbered heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// What is announced: the substitute text, else the painted text, whitespace collapsed.
    pub text: String,
    /// The text that was painted, when `text` replaces it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub painted: Option<String>,
    /// What replaced the painted text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub substituted_by: Option<Substitution>,
    /// The element's `/ActualText` when `/Alt` won and the preset keeps the other as well.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub also_actual_text: Option<String>,
    /// The language in effect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Set when the language differs from the previous spoken item's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang_change: Option<LangChange>,
    /// The table cell the item is, with its header cells.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<SpeechCell>,
    /// The footnote reference or note this item is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<NoteFlag>,
    /// The annotation this item stands for, with its target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<LinkFlag>,
    /// True when all of the text was painted invisibly (`3 Tr`). It is spoken all the same.
    pub invisible: bool,
    /// Why the item is not spoken; `None` for an item that is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<SkipReason>,
}

/// The announcements of a document for one preset. See [`Document::speech_order`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpeechOrder {
    /// The reader the order models.
    pub preset: SpeechPreset,
    /// True when `preset` is an approximation of a particular viewer.
    pub approximation: bool,
    /// The items in order, those that are not spoken included.
    pub items: Vec<SpeechItem>,
}

impl SpeechOrder {
    /// The items that are spoken, in order.
    pub fn spoken(&self) -> impl Iterator<Item = &SpeechItem> {
        self.items.iter().filter(|i| i.skipped.is_none())
    }
}

/// A run of marked content: page, MCID, form stream.
type McRef = (PageIx, i32, Option<ObjRef>);

/// A language code, or `None` for the empty string (language unknown).
fn language(lang: &str) -> Option<String> {
    let lang = lang.trim();
    (!lang.is_empty()).then(|| lang.to_owned())
}

/// The walk's state.
struct Speaker<'a> {
    tree: &'a TagTree,
    doc: &'a Document,
    preset: SpeechPreset,
    items: Vec<SpeechItem>,
    /// Per item, the execution index of the first operator of its content, to place artifacts.
    first_ops: Vec<Option<u32>>,
    /// Per element, the first item at or below it.
    first_item: HashMap<usize, usize>,
    /// Per cell element, how it is announced.
    cells: HashMap<usize, SpeechCell>,
}

impl<'a> Speaker<'a> {
    fn new(tree: &'a TagTree, doc: &'a Document, preset: SpeechPreset) -> Self {
        Self {
            tree,
            doc,
            preset,
            items: Vec::new(),
            first_ops: Vec::new(),
            first_item: HashMap::new(),
            cells: HashMap::new(),
        }
    }

    fn catalog_lang(&self) -> Option<String> {
        self.tree.catalog_lang.as_deref().and_then(language)
    }

    fn emit(&mut self, item: SpeechItem, first_op: Option<u32>) {
        self.items.push(item);
        self.first_ops.push(first_op);
    }
}

impl Document {
    /// What a screen reader announces for the whole document under `preset`, in order: the
    /// structure tree read depth-first with the preset's substitution and skip rules (artifacts
    /// and `Private` content skipped, `/Alt`, `/ActualText` and `/E` replacing an element and its
    /// subtree, object references at their position, table cells with their header cells,
    /// language changes and footnote links flagged); for
    /// [`SpeechPreset::ChromiumDefault`], the page text in paint order with the tags ignored.
    ///
    /// Items that are not spoken stay in the list with a [`SkipReason`]. See
    /// [`SpeechPreset::rules`] for how the presets differ.
    #[must_use]
    pub fn speech_order(&self, preset: SpeechPreset) -> SpeechOrder {
        let mut speaker = Speaker::new(&self.tags, self, preset);
        if preset.follows_tags() {
            speaker.follow_tags();
        } else {
            speaker.paint_order();
        }
        let mut items = speaker.items;
        for (i, item) in items.iter_mut().enumerate() {
            item.index = i + 1;
        }
        SpeechOrder {
            preset,
            approximation: preset.is_approximation(),
            items,
        }
    }
}
