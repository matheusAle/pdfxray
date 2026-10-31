//! Standard structure types and role mapping.
//!
//! A custom structure type must reach a standard one through `/RoleMap` (ISO 32000-2 §14.7.3)
//! or, for elements in a namespace, through that namespace's `/RoleMapNS` (§14.7.4). Both the
//! PDF 1.7 and the PDF 2.0 type sets count as standard.

use std::collections::{BTreeMap, HashMap};

use serde::{Serialize, Serializer};

use super::parse::Builder;
use super::{Namespace, RoleResolution};
use crate::Severity;

/// Namespace of the PDF 2.0 structure types.
pub(super) const PDF2_NS: &str = "http://iso.org/pdf2/ssn";
/// Namespace of the PDF 1.7 structure types.
pub(super) const PDF17_NS: &str = "http://iso.org/pdf/ssn";
/// Namespace of `MathML` (a standard namespace of PDF 2.0).
pub(super) const MATHML_NS: &str = "http://www.w3.org/1998/Math/MathML";

/// Standard structure types of PDF 1.7 (ISO 32000-1 §14.8.4), without `H1`..`H6`.
const STANDARD_1_7: &[&str] = &[
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "P",
    "H",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Span",
    "Quote",
    "Note",
    "Reference",
    "BibEntry",
    "Code",
    "Link",
    "Annot",
    "Ruby",
    "RB",
    "RT",
    "RP",
    "Warichu",
    "WT",
    "WP",
    "Figure",
    "Formula",
    "Form",
];

/// Standard structure types that PDF 2.0 (ISO 32000-2 §14.8.4) adds.
const STANDARD_2_0_ADDED: &[&str] = &[
    "DocumentFragment",
    "Aside",
    "Title",
    "FENote",
    "Sub",
    "Em",
    "Strong",
    "Artifact",
];

/// `MathML` element names usable as structure types in the `MathML` namespace.
const MATHML: &[&str] = &[
    "math",
    "mi",
    "mn",
    "mo",
    "mtext",
    "mspace",
    "ms",
    "mrow",
    "mfrac",
    "msqrt",
    "mroot",
    "mstyle",
    "merror",
    "mpadded",
    "mphantom",
    "mfenced",
    "menclose",
    "msub",
    "msup",
    "msubsup",
    "munder",
    "mover",
    "munderover",
    "mmultiscripts",
    "mprescripts",
    "mtable",
    "mtr",
    "mtd",
    "mlabeledtr",
    "maction",
    "semantics",
    "annotation",
    "annotation-xml",
    "mglyph",
    "malignmark",
    "maligngroup",
    "none",
];

/// A structure type name together with its namespace.
type RoleKey = (String, Option<usize>);

/// True for a standard type of PDF 1.7 or PDF 2.0, including `Hn` for any `n`.
pub(super) fn is_standard(role: &str) -> bool {
    STANDARD_1_7.contains(&role) || STANDARD_2_0_ADDED.contains(&role) || is_heading_n(role)
}

fn is_heading_n(role: &str) -> bool {
    role.strip_prefix('H')
        .is_some_and(|n| !n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()))
}

/// True when `role` is a standard type of the namespace `ns_uri` (`None` = no namespace).
fn is_standard_in(role: &str, ns_uri: Option<&str>) -> bool {
    match ns_uri {
        None | Some(PDF2_NS | PDF17_NS) => is_standard(role),
        Some(MATHML_NS) => MATHML.contains(&role),
        Some(_) => false,
    }
}

/// Rotates `cycle` so it starts at its lexicographically smallest role.
fn normalise(mut cycle: Vec<String>) -> Vec<String> {
    if let Some(min) = cycle
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.cmp(b.1))
        .map(|(i, _)| i)
    {
        cycle.rotate_left(min);
    }
    cycle
}

/// Follows the role mapping from `role` (in namespace `ns`) until a standard type, a dead end
/// or a cycle.
pub(super) fn resolve(
    role: &str,
    ns: Option<usize>,
    role_map: &BTreeMap<String, String>,
    namespaces: &[Namespace],
) -> RoleResolution {
    let uri_of = |ns: Option<usize>| ns.and_then(|i| namespaces.get(i)).map(|n| n.uri.as_str());
    let mut chain = vec![role.to_owned()];
    let mut seen: Vec<(String, Option<usize>)> = vec![(role.to_owned(), ns)];
    let mut current = (role.to_owned(), ns);
    loop {
        if is_standard_in(&current.0, uri_of(current.1)) {
            return RoleResolution {
                chain,
                standard: Some(current.0),
                standard_ns: uri_of(current.1).map(str::to_owned),
                cycle: None,
            };
        }
        let next = match current.1 {
            None => role_map.get(&current.0).map(|target| (target.clone(), None)),
            Some(i) => namespaces
                .get(i)
                .and_then(|n| n.role_map.iter().find(|(k, _)| *k == current.0))
                .map(|(_, t)| (t.role.clone(), t.ns)),
        };
        let Some(next) = next else {
            return RoleResolution {
                chain,
                ..RoleResolution::default()
            };
        };
        chain.push(next.0.clone());
        if let Some(at) = seen.iter().position(|s| *s == next) {
            let cycle = seen[at..].iter().map(|(name, _)| name.clone()).collect();
            return RoleResolution {
                chain,
                cycle: Some(normalise(cycle)),
                ..RoleResolution::default()
            };
        }
        seen.push(next.clone());
        current = next;
    }
}

/// The family a structure element belongs to, for colouring and grouping. It comes from the
/// element's *standard* type, so a custom role takes the class of the standard type its
/// `/RoleMap` (or `/RoleMapNS`) reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoleClass {
    /// `H`, `H1`..`Hn` and `Title`.
    Heading,
    /// `P`.
    Paragraph,
    /// The illustration elements: `Figure`, `Formula` and `Form`.
    Figure,
    /// `Caption`.
    Caption,
    /// `Table`, `TR`, `TH`, `TD`, `THead`, `TBody` and `TFoot`.
    Table,
    /// `L`, `LI`, `Lbl` and `LBody`.
    List,
    /// `Link`, `Annot` and `Reference`.
    Link,
    /// Inline-level elements: `Span`, `Em`, `Strong`, `Code`, `Quote`, `Note`, `FENote`, `Sub`,
    /// `BibEntry` and the ruby and warichu parts.
    Inline,
    /// Grouping elements: `Document`, `DocumentFragment`, `Part`, `Art`, `Sect`, `Div`, `Aside`,
    /// `BlockQuote`, `TOC`, `TOCI`, `Index`, `NonStruct` and `Private`.
    Group,
    /// The PDF 2.0 `Artifact` structure type.
    Artifact,
    /// A type that never reaches a standard one (unmapped, cyclic or in an unknown namespace)
    /// and the `MathML` elements.
    Other,
}

impl RoleClass {
    /// Every class, in display order.
    pub const ALL: [Self; 11] = [
        Self::Heading,
        Self::Paragraph,
        Self::Figure,
        Self::Caption,
        Self::Table,
        Self::List,
        Self::Link,
        Self::Inline,
        Self::Group,
        Self::Artifact,
        Self::Other,
    ];

    /// A short stable key: `heading`, `p`, `figure`, `caption`, `table`, `list`, `link`,
    /// `inline`, `group`, `artifact` or `other`. The paragraph key matches the `role-p` colour
    /// token.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Paragraph => "p",
            Self::Figure => "figure",
            Self::Caption => "caption",
            Self::Table => "table",
            Self::List => "list",
            Self::Link => "link",
            Self::Inline => "inline",
            Self::Group => "group",
            Self::Artifact => "artifact",
            Self::Other => "other",
        }
    }

    /// The class of the standard type `standard`, which lives in the namespace `ns_uri`
    /// (`None` = no namespace). `MathML` and unknown namespaces give [`RoleClass::Other`].
    pub(super) fn of_standard(standard: &str, ns_uri: Option<&str>) -> Self {
        if !matches!(ns_uri, None | Some(PDF2_NS | PDF17_NS)) {
            return Self::Other;
        }
        match standard {
            "H" | "Title" => Self::Heading,
            "P" => Self::Paragraph,
            "Figure" | "Formula" | "Form" => Self::Figure,
            "Caption" => Self::Caption,
            "Table" | "TR" | "TH" | "TD" | "THead" | "TBody" | "TFoot" => Self::Table,
            "L" | "LI" | "Lbl" | "LBody" => Self::List,
            "Link" | "Annot" | "Reference" => Self::Link,
            "Span" | "Em" | "Strong" | "Code" | "Quote" | "Note" | "FENote" | "Sub" | "BibEntry" | "Ruby"
            | "RB" | "RT" | "RP" | "Warichu" | "WT" | "WP" => Self::Inline,
            "Document" | "DocumentFragment" | "Part" | "Art" | "Sect" | "Div" | "Aside" | "BlockQuote"
            | "TOC" | "TOCI" | "Index" | "NonStruct" | "Private" => Self::Group,
            "Artifact" => Self::Artifact,
            other if is_heading_n(other) => Self::Heading,
            _ => Self::Other,
        }
    }
}

impl Serialize for RoleClass {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.key())
    }
}

impl Builder<'_> {
    /// Resolves every element's type and reports cycles and unmapped custom types.
    pub(super) fn resolve_roles(&mut self) {
        self.scan_role_map_cycles();
        let mut cache: HashMap<RoleKey, RoleResolution> = HashMap::new();
        // Unmapped types in order of first use, each with the elements that use it.
        let mut unmapped: Vec<(RoleKey, Vec<usize>)> = Vec::new();
        for ei in 0..self.tree.elems.len() {
            let key = (self.tree.elems[ei].role.clone(), self.tree.elems[ei].ns);
            let resolution = cache
                .entry(key.clone())
                .or_insert_with(|| resolve(&key.0, key.1, &self.tree.role_map, &self.tree.namespaces))
                .clone();
            if let Some(cycle) = &resolution.cycle {
                let finding = self.cycle_finding(cycle);
                self.attach(finding, ei);
            } else if resolution.standard.is_none() {
                match unmapped.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, users)) => users.push(ei),
                    None => unmapped.push((key, vec![ei])),
                }
            }
            self.tree.elems[ei].resolved = resolution;
        }
        for ((role, ns), users) in unmapped {
            let first = users[0];
            let chain = self.tree.elems[first].resolved.chain.join(" → ");
            let what = if self.tree.elems[first].resolved.chain.len() > 1 {
                format!("/{role} maps only as far as {chain}, which is not")
            } else {
                format!("/{role} is not")
            };
            let namespace = ns
                .and_then(|i| self.tree.namespaces.get(i))
                .map_or_else(String::new, |n| format!(" in namespace {}", n.uri));
            let finding = self.find(
                "non-standard-type",
                Severity::Warning,
                format!(
                    "structure type {what} a standard type{namespace} and the role map does not reach one ({} element{})",
                    users.len(),
                    if users.len() == 1 { "" } else { "s" },
                ),
                Some(self.tree.elems[first].obj),
            );
            for ei in users {
                self.attach(finding, ei);
            }
        }
    }

    /// Reports each cycle of the plain `/RoleMap` once, whether or not an element uses it.
    fn scan_role_map_cycles(&mut self) {
        let keys: Vec<String> = self.tree.role_map.keys().cloned().collect();
        for key in keys {
            let mut path = vec![key];
            while let Some(current) = path.last() {
                if is_standard(current) {
                    break;
                }
                let Some(next) = self.tree.role_map.get(current) else {
                    break;
                };
                if let Some(at) = path.iter().position(|p| p == next) {
                    let cycle = normalise(path[at..].to_vec());
                    self.cycle_finding(&cycle);
                    break;
                }
                path.push(next.clone());
            }
        }
    }

    /// The finding for `cycle`, created on first use and attached to the tree root.
    fn cycle_finding(&mut self, cycle: &[String]) -> usize {
        if let Some(&f) = self.cycles.get(cycle) {
            return f;
        }
        let mut shown = cycle.join(" → ");
        shown.push_str(" → ");
        shown.push_str(&cycle[0]);
        let root = self.tree.root.as_ref().and_then(|r| r.obj);
        let finding = self.find(
            "rolemap-cycle",
            Severity::Error,
            format!("the role map contains a cycle: {shown}"),
            root,
        );
        self.attach_root(finding);
        self.cycles.insert(cycle.to_vec(), finding);
        finding
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::tags::RoleTarget;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn standard_types_cover_both_editions() {
        for role in ["P", "H1", "H12", "Art", "Aside", "FENote", "Strong", "Document"] {
            assert!(is_standard(role), "{role}");
        }
        for role in ["H", "Heading1", "H0", "H1x", "", "p", "MyPara"] {
            assert_eq!(is_standard(role), role == "H", "{role}");
        }
    }

    #[test]
    fn follows_the_chain_and_records_every_step() {
        let m = map(&[("Callout", "Remark"), ("Remark", "Span")]);

        let r = resolve("Callout", None, &m, &[]);

        assert_eq!(r.chain, ["Callout", "Remark", "Span"]);
        assert_eq!(r.standard.as_deref(), Some("Span"));
        assert_eq!(r.cycle, None);
        assert_eq!(resolve("P", None, &m, &[]).chain, ["P"]);
    }

    #[test]
    fn detects_cycles_and_dead_ends() {
        let m = map(&[
            ("LoopB", "LoopA"),
            ("LoopA", "LoopB"),
            ("Self", "Self"),
            ("A", "B"),
        ]);

        let a = resolve("LoopB", None, &m, &[]);
        assert_eq!(a.cycle, Some(vec!["LoopA".to_owned(), "LoopB".to_owned()]));
        assert_eq!(a.chain, ["LoopB", "LoopA", "LoopB"]);
        assert_eq!(a.standard, None);
        assert_eq!(
            resolve("Self", None, &m, &[]).cycle,
            Some(vec!["Self".to_owned()])
        );
        let dead = resolve("A", None, &m, &[]);
        assert_eq!(
            (dead.chain, dead.standard, dead.cycle),
            (vec!["A".into(), "B".into()], None, None)
        );
    }

    #[test]
    fn namespaced_types_use_their_namespaces_role_map() {
        let namespaces = vec![
            Namespace {
                obj: None,
                uri: PDF2_NS.to_owned(),
                role_map: vec![],
            },
            Namespace {
                obj: None,
                uri: "urn:custom".to_owned(),
                role_map: vec![(
                    "Chapter".to_owned(),
                    RoleTarget {
                        role: "Sect".to_owned(),
                        ns: Some(0),
                    },
                )],
            },
        ];

        let r = resolve("Chapter", Some(1), &BTreeMap::new(), &namespaces);
        assert_eq!(r.standard.as_deref(), Some("Sect"));
        assert_eq!(r.standard_ns.as_deref(), Some(PDF2_NS));
        assert_eq!(
            resolve("Nope", Some(1), &BTreeMap::new(), &namespaces).standard,
            None
        );
        let ml = Namespace {
            obj: None,
            uri: MATHML_NS.to_owned(),
            role_map: vec![],
        };
        assert_eq!(
            resolve("mi", Some(0), &BTreeMap::new(), &[ml])
                .standard
                .as_deref(),
            Some("mi")
        );
    }

    #[test]
    fn standard_types_fall_into_their_classes() {
        let class = |t: &str| RoleClass::of_standard(t, None);

        assert_eq!(
            ["H", "H1", "H6", "H12", "Title"].map(class),
            [RoleClass::Heading; 5]
        );
        assert_eq!(["P"].map(class), [RoleClass::Paragraph]);
        assert_eq!(["Figure", "Formula", "Form"].map(class), [RoleClass::Figure; 3]);
        assert_eq!(["Caption"].map(class), [RoleClass::Caption]);
        assert_eq!(
            ["Table", "TR", "TH", "TD", "THead", "TBody", "TFoot"].map(class),
            [RoleClass::Table; 7]
        );
        assert_eq!(["L", "LI", "Lbl", "LBody"].map(class), [RoleClass::List; 4]);
        assert_eq!(["Link", "Annot", "Reference"].map(class), [RoleClass::Link; 3]);
        assert_eq!(
            [
                "Span", "Em", "Strong", "Code", "Quote", "Note", "FENote", "Sub", "BibEntry", "Ruby", "WT"
            ]
            .map(class),
            [RoleClass::Inline; 11]
        );
        assert_eq!(
            [
                "Document",
                "DocumentFragment",
                "Part",
                "Art",
                "Sect",
                "Div",
                "Aside",
                "BlockQuote",
                "TOC",
                "TOCI",
                "Index",
                "NonStruct",
                "Private"
            ]
            .map(class),
            [RoleClass::Group; 13]
        );
        assert_eq!(["Artifact"].map(class), [RoleClass::Artifact]);
        // Smoke: names that are not standard types.
        assert_eq!(["Heading1", "H0", "p", ""].map(class), [RoleClass::Other; 4]);
    }

    #[test]
    fn the_class_depends_on_the_namespace_of_the_standard_type() {
        assert_eq!(RoleClass::of_standard("P", Some(PDF2_NS)), RoleClass::Paragraph);
        assert_eq!(RoleClass::of_standard("P", Some(PDF17_NS)), RoleClass::Paragraph);
        assert_eq!(RoleClass::of_standard("mi", Some(MATHML_NS)), RoleClass::Other);
        assert_eq!(RoleClass::of_standard("P", Some("urn:custom")), RoleClass::Other);
        // Smoke: every class has a unique key.
        let mut keys: Vec<&str> = RoleClass::ALL.iter().map(|c| c.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), RoleClass::ALL.len());
    }
}
