//! A document as a graph of *labelled* objects, written under any numbering and any byte order.
//!
//! [`PdfWriter`] works in object numbers. The diff pairs need one logical document written twice
//! with different numbers, so their objects refer to each other by label instead: a body template
//! names another object as `@label`, and [`ObjectGraph::write`] replaces every such token with
//! `N 0 R` under the numbering it is given. Renumbering a whole file, including every reference,
//! the trailer `/Root` and the xref table, is then a different map rather than a second document.
//!
//! Labels are `[a-z0-9._-]+`. Stream *data* is never templated, so content streams can hold any
//! bytes.

use std::collections::BTreeMap;

use crate::writer::{Output, PdfVersion, PdfWriter, Trailer, WriterError};

/// What an object holds.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Body {
    /// A direct object body, as a template.
    Object(String),
    /// A stream: dictionary entries (template, without `<< >>` and `/Length`) and raw data.
    Stream { dict: String, data: Vec<u8> },
}

/// One labelled object.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    label: String,
    body: Body,
}

/// Object numbers by label.
pub type Numbering = BTreeMap<String, u32>;

/// A set of labelled objects in declaration order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectGraph {
    nodes: Vec<Node>,
}

impl ObjectGraph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a direct object whose body is `template`.
    pub fn object(&mut self, label: impl Into<String>, template: impl Into<String>) {
        self.nodes.push(Node {
            label: label.into(),
            body: Body::Object(template.into()),
        });
    }

    /// Adds a stream object. `dict` is a template of the dictionary entries without `/Length`.
    pub fn stream(&mut self, label: impl Into<String>, dict: impl Into<String>, data: impl Into<Vec<u8>>) {
        self.nodes.push(Node {
            label: label.into(),
            body: Body::Stream {
                dict: dict.into(),
                data: data.into(),
            },
        });
    }

    /// Labels in declaration order.
    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().map(|n| n.label.as_str())
    }

    /// Numbers `1, 2, 3...` in declaration order.
    #[must_use]
    pub fn sequential(&self) -> Numbering {
        self.labels().zip(1..).map(|(l, n)| (l.to_owned(), n)).collect()
    }

    /// Writes every object under `numbers`, in the byte order given by `order` (labels; every
    /// label exactly once), closes the file with a dense classic xref table and a trailer whose
    /// `/Root` is `root`.
    ///
    /// # Errors
    /// [`WriterError::UnknownLabel`] when a template, `order` or `root` names a label the graph
    /// or the numbering does not have, [`WriterError::DuplicateLabel`] when `order` is not a
    /// permutation of the labels, and any error of the underlying [`PdfWriter`].
    pub fn write(
        &self,
        version: PdfVersion,
        numbers: &Numbering,
        order: &[&str],
        root: &str,
    ) -> Result<Output, WriterError> {
        let by_label: BTreeMap<&str, &Node> = self.nodes.iter().map(|n| (n.label.as_str(), n)).collect();
        if by_label.len() != self.nodes.len() {
            let dup = self
                .labels()
                .find(|l| self.labels().filter(|m| m == l).count() > 1)
                .unwrap_or_default();
            return Err(WriterError::DuplicateLabel(dup.to_owned()));
        }
        let mut seen = Vec::new();
        for label in order {
            if seen.contains(label) {
                return Err(WriterError::DuplicateLabel((*label).to_owned()));
            }
            seen.push(label);
        }
        if let Some(missing) = self.labels().find(|l| !order.contains(l)) {
            return Err(WriterError::UnknownLabel(missing.to_owned()));
        }

        let number = |label: &str| {
            numbers
                .get(label)
                .copied()
                .ok_or_else(|| WriterError::UnknownLabel(label.to_owned()))
        };
        let mut w = PdfWriter::new(version).free_gaps();
        for label in order {
            let node = by_label
                .get(label)
                .ok_or_else(|| WriterError::UnknownLabel((*label).to_owned()))?;
            let num = number(label)?;
            match &node.body {
                Body::Object(template) => {
                    w.object(num, resolve(template, numbers)?);
                }
                Body::Stream { dict, data } => {
                    w.stream(num, &resolve(dict, numbers)?, data);
                }
            }
        }
        w.finish_table(&Trailer::new(number(root)?))?;
        w.into_output()
    }
}

/// Replaces every `@label` in `template` with `N 0 R`.
///
/// # Errors
/// [`WriterError::UnknownLabel`] for a label `numbers` does not hold.
pub fn resolve(template: &str, numbers: &Numbering) -> Result<String, WriterError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')))
            .unwrap_or(tail.len());
        let label = &tail[..end];
        let num = numbers
            .get(label)
            .ok_or_else(|| WriterError::UnknownLabel(label.to_owned()))?;
        out.push_str(&num.to_string());
        out.push_str(" 0 R");
        rest = &tail[end..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn tiny() -> ObjectGraph {
        let mut g = ObjectGraph::new();
        g.object("catalog", "<< /Type /Catalog /Pages @pages >>");
        g.object("pages", "<< /Type /Pages /Kids [@page] /Count 1 >>");
        g.object("page", "<< /Type /Page /Parent @pages /Contents @content >>");
        g.stream("content", "", b"0 0 m @not-a-ref".to_vec());
        g
    }

    #[test]
    fn resolve_replaces_every_label_with_a_reference() {
        let numbers: Numbering = [("a".to_owned(), 7), ("b.c_d-1".to_owned(), 12)].into();

        let text = resolve("[@a @b.c_d-1]/K @a>>", &numbers).unwrap();

        assert_eq!(text, "[7 0 R 12 0 R]/K 7 0 R>>");
        // Smoke: an unknown label is an error, not a silent pass-through.
        assert_eq!(
            resolve("@zz", &numbers),
            Err(WriterError::UnknownLabel("zz".to_owned()))
        );
    }

    #[test]
    fn write_follows_numbering_and_order_and_leaves_stream_data_alone() {
        let g = tiny();
        let numbers: Numbering = [("catalog", 9), ("pages", 3), ("page", 20), ("content", 1)]
            .map(|(l, n)| (l.to_owned(), n))
            .into();

        let out = g
            .write(
                PdfVersion::V1_7,
                &numbers,
                &["content", "page", "catalog", "pages"],
                "catalog",
            )
            .unwrap();

        let text: String = out.bytes.iter().map(|&b| char::from(b)).collect();
        let at = |needle: &str| text.find(needle).unwrap();
        assert!(
            at("1 0 obj") < at("20 0 obj") && at("20 0 obj") < at("9 0 obj") && at("9 0 obj") < at("3 0 obj")
        );
        assert!(text.contains("<< /Type /Page /Parent 3 0 R /Contents 1 0 R >>"));
        assert!(text.contains("0 0 m @not-a-ref"));
        assert!(text.contains("/Root 9 0 R"));
        assert!(text.contains("xref\n0 21\n"));
        // Smoke: exactly the four objects are in use.
        assert_eq!(out.revisions[0].objects(), [1, 3, 9, 20]);
    }

    #[test]
    fn write_rejects_an_order_that_is_not_a_permutation() {
        let g = tiny();
        let numbers = g.sequential();

        let missing = g.write(
            PdfVersion::V1_7,
            &numbers,
            &["catalog", "pages", "page"],
            "catalog",
        );
        let doubled = g.write(
            PdfVersion::V1_7,
            &numbers,
            &["catalog", "pages", "page", "page"],
            "catalog",
        );

        assert_eq!(missing, Err(WriterError::UnknownLabel("content".to_owned())));
        assert_eq!(doubled, Err(WriterError::DuplicateLabel("page".to_owned())));
        // Smoke: the sequential numbering in declaration order writes fine.
        let order: Vec<&str> = g.labels().collect();
        assert!(g.write(PdfVersion::V1_7, &numbers, &order, "catalog").is_ok());
    }
}
