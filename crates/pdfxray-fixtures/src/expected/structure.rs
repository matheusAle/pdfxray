//! Facts about rich logical structure: attributes, classes, namespaces, annotations and
//! associated files.
//!
//! Attribute values are rendered in PDF syntax so a fact can be compared with the file as is:
//! names keep their slash (`/Column`), strings are parenthesized (`(text)`), arrays are
//! `[a b c]` with single spaces, dictionaries are `<< /K v /K2 v2 >>` in file order and indirect
//! references are `N 0 R`. Numbers use the shortest form (`10`, `0.5`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ParentTreeEntry, Rect};

/// Where an attribute object came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttributeSource {
    /// Written in the element's own `/A`.
    Direct,
    /// Looked up through a class name in the root's `/ClassMap`.
    Class {
        /// The class name.
        name: String,
    },
}

/// One attribute object: its owner (`/O`) and every other entry rendered in PDF syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeSetFacts {
    /// The `/O` name, without slash (`Layout`, `Table`, ...).
    pub owner: String,
    /// All entries except `/O`, key without slash to value in PDF syntax.
    pub entries: BTreeMap<String, String>,
}

impl AttributeSetFacts {
    /// An attribute object with the given owner and `(key, value)` entries.
    #[must_use]
    pub fn new(owner: &str, entries: &[(&str, &str)]) -> Self {
        Self {
            owner: owner.to_owned(),
            entries: entries
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }
}

/// An attribute object on an element together with its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeFacts {
    /// Direct or class-provided.
    pub source: AttributeSource,
    /// The attribute object.
    pub set: AttributeSetFacts,
}

/// A structure type reached through a `/RoleMapNS` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleTarget {
    /// Target structure type name.
    pub role: String,
    /// URI of the target namespace.
    pub namespace: String,
}

/// A `/Namespaces` entry of the structure tree root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceFacts {
    /// Object number of the namespace dictionary.
    pub object: u32,
    /// The `/NS` URI.
    pub uri: String,
    /// The `/RoleMapNS` entries.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub role_map: BTreeMap<String, RoleTarget>,
}

/// One child of a structure element, in `/K` order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KidFacts {
    /// A child structure element.
    Elem(Box<NodeFacts>),
    /// Marked content, either a bare MCID in `/K` or a marked-content reference dictionary.
    Mcr {
        /// Zero-based page index of the content stream.
        page: u32,
        /// The marked-content identifier.
        mcid: i32,
        /// Form `XObject` holding the content, when `/Stm` is present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stream: Option<u32>,
    },
    /// An object reference to an annotation or other PDF object.
    Objr {
        /// Zero-based page index named by `/Pg`, when present.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<u32>,
        /// Object number of the referenced object.
        object: u32,
    },
}

/// A structure element in the rich outline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeFacts {
    /// Object number of the element.
    pub object: u32,
    /// The `/S` name as written.
    pub role: String,
    /// The role after following `/RoleMap` or `/RoleMapNS` to a standard type; `null` when it
    /// never reaches one.
    pub standard_role: Option<String>,
    /// Namespace URI from `/NS`, when the element has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Namespace of the resolved standard type, when namespaces are in play.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_namespace: Option<String>,
    /// Zero-based page index named by `/Pg`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// The `/ID` byte string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The `/Lang` text string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// The `/Alt` text string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    /// The `/ActualText` text string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_text: Option<String>,
    /// The `/Phoneme` text string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phoneme: Option<String>,
    /// The `/PhoneticAlphabet` name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phonetic_alphabet: Option<String>,
    /// Attribute objects: the element's direct `/A` first, then the classes' in `/C` order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<AttributeFacts>,
    /// Class names from `/C`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    /// Object numbers listed in `/Ref`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<u32>,
    /// File specification objects listed in `/AF`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub associated_files: Vec<u32>,
    /// Children in `/K` order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kids: Vec<KidFacts>,
}

impl NodeFacts {
    /// An element with a role and its resolved standard role, everything else empty.
    #[must_use]
    pub fn new(object: u32, role: &str, standard_role: Option<&str>) -> Self {
        Self {
            object,
            role: role.to_owned(),
            standard_role: standard_role.map(str::to_owned),
            namespace: None,
            standard_namespace: None,
            page: None,
            id: None,
            lang: None,
            alt: None,
            actual_text: None,
            phoneme: None,
            phonetic_alphabet: None,
            attributes: Vec::new(),
            classes: Vec::new(),
            refs: Vec::new(),
            associated_files: Vec::new(),
            kids: Vec::new(),
        }
    }

    /// Sets `/Pg` as a page index.
    #[must_use]
    pub const fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    /// Sets `/ID`.
    #[must_use]
    pub fn id(mut self, id: &str) -> Self {
        self.id = Some(id.to_owned());
        self
    }

    /// Sets `/Lang`.
    #[must_use]
    pub fn lang(mut self, lang: &str) -> Self {
        self.lang = Some(lang.to_owned());
        self
    }

    /// Sets `/Alt`.
    #[must_use]
    pub fn alt(mut self, alt: &str) -> Self {
        self.alt = Some(alt.to_owned());
        self
    }

    /// Sets `/ActualText`.
    #[must_use]
    pub fn actual_text(mut self, text: &str) -> Self {
        self.actual_text = Some(text.to_owned());
        self
    }

    /// Sets the namespace of the element and of its resolved standard type.
    #[must_use]
    pub fn namespaces(mut self, namespace: &str, standard_namespace: &str) -> Self {
        self.namespace = Some(namespace.to_owned());
        self.standard_namespace = Some(standard_namespace.to_owned());
        self
    }

    /// Appends a direct attribute object.
    #[must_use]
    pub fn direct(mut self, set: AttributeSetFacts) -> Self {
        self.attributes.push(AttributeFacts {
            source: AttributeSource::Direct,
            set,
        });
        self
    }

    /// Appends one attribute object provided by a class; a class with several attribute objects
    /// is added once per object and listed in [`NodeFacts::classes`] only once.
    #[must_use]
    pub fn class(mut self, name: &str, set: AttributeSetFacts) -> Self {
        if !self.classes.iter().any(|c| c == name) {
            self.classes.push(name.to_owned());
        }
        self.attributes.push(AttributeFacts {
            source: AttributeSource::Class {
                name: name.to_owned(),
            },
            set,
        });
        self
    }

    /// Appends a child element.
    #[must_use]
    pub fn elem(mut self, child: Self) -> Self {
        self.kids.push(KidFacts::Elem(Box::new(child)));
        self
    }

    /// Appends marked content on `page`.
    #[must_use]
    pub fn mcr(mut self, page: u32, mcid: i32) -> Self {
        self.kids.push(KidFacts::Mcr {
            page,
            mcid,
            stream: None,
        });
        self
    }

    /// Appends an object reference.
    #[must_use]
    pub fn objr(mut self, page: Option<u32>, object: u32) -> Self {
        self.kids.push(KidFacts::Objr { page, object });
        self
    }
}

/// A parent tree entry whose value is a single struct element (annotations, `XObject`s).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectParentFacts {
    /// The `/StructParent` key.
    pub key: i32,
    /// Object number of the owning struct element.
    pub parent: u32,
}

/// The `/StructParents` key of a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageKeyFacts {
    /// Zero-based page index.
    pub page: u32,
    /// Value of the page's `/StructParents`.
    pub key: i32,
}

/// The rich logical structure outline of a fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineFacts {
    /// Object number of the `/StructTreeRoot`.
    pub root: u32,
    /// The `/RoleMap` entries.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub role_map: BTreeMap<String, String>,
    /// The `/ClassMap` entries: class name to the attribute objects it provides.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub class_map: BTreeMap<String, Vec<AttributeSetFacts>>,
    /// The `/Namespaces` entries in file order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<NamespaceFacts>,
    /// The `/IDTree` leaves: element `/ID` to object number.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub id_tree: BTreeMap<String, u32>,
    /// Children of the root, in `/K` order.
    pub children: Vec<NodeFacts>,
    /// `/StructParents` of every page that has one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub page_keys: Vec<PageKeyFacts>,
    /// Parent tree entries whose value is an array (page and form streams), in key order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_tree: Vec<ParentTreeEntry>,
    /// Parent tree entries whose value is one struct element, in key order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_tree_objects: Vec<ObjectParentFacts>,
    /// Object numbers of every number tree node visited from the root, root first (depth first).
    pub parent_tree_nodes: Vec<u32>,
    /// `/ParentTreeNextKey`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_tree_next_key: Option<i32>,
}

impl OutlineFacts {
    /// An outline rooted at object `root`; everything else empty.
    #[must_use]
    pub const fn new(root: u32) -> Self {
        Self {
            root,
            role_map: BTreeMap::new(),
            class_map: BTreeMap::new(),
            namespaces: Vec::new(),
            id_tree: BTreeMap::new(),
            children: Vec::new(),
            page_keys: Vec::new(),
            parent_tree: Vec::new(),
            parent_tree_objects: Vec::new(),
            parent_tree_nodes: Vec::new(),
            parent_tree_next_key: None,
        }
    }
}

/// An annotation on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationFacts {
    /// Zero-based page index.
    pub page: u32,
    /// Object number of the annotation.
    pub object: u32,
    /// The `/Subtype` name.
    pub subtype: String,
    /// The `/Rect`.
    pub rect: Rect,
    /// The `/F` flag word (4 print, 2 hidden, 32 no view), 0 when absent.
    pub flags: u32,
    /// The `/StructParent` key, when the annotation is tagged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_parent: Option<i32>,
    /// URI of a `/URI` action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Zero-based page index of an explicit `/Dest` or `/GoTo` destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goto_page: Option<u32>,
    /// True when the annotation has a normal appearance stream (`/AP /N`).
    pub has_appearance: bool,
    /// The `/FT` field type of a widget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
    /// The `/T` field name of a widget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
}

/// An embedded file reachable through a file specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddedFileFacts {
    /// Object number of the file specification dictionary.
    pub filespec: u32,
    /// The `/UF` (or `/F`) file name.
    pub name: String,
    /// The `/AFRelationship` name.
    pub relationship: String,
    /// Object number of the `/EmbeddedFile` stream.
    pub stream: u32,
    /// Length of the (unfiltered) stream data in bytes.
    pub size: u32,
}
