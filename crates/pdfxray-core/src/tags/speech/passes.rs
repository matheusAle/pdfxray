//! The passes that run around the walk: table cells before it; artifacts and language changes
//! over the finished list; and, for viewers that ignore the tags, the page text in paint order.

use super::super::table::TableRules;
use super::super::{Kid, RoleClass};
use super::{LangChange, SkipReason, Speaker, SpeechCell, SpeechItem, SpeechPreset};
use crate::content::RunKind;
use crate::{NodeId, PageIx};

impl Speaker<'_> {
    /// Records how every table cell of the document is announced.
    fn collect_cells(&mut self, rules: TableRules) {
        let tree = self.tree;
        for ei in 0..tree.elems.len() {
            let Some(model) = tree.table_model(ei, rules) else {
                continue;
            };
            for cell in &model.cells {
                let NodeId::StructElem(r) = &cell.node else {
                    continue;
                };
                let Some(&cei) = tree.index.get(r) else { continue };
                self.cells.insert(
                    cei,
                    SpeechCell {
                        table: model.table.clone(),
                        row: cell.row,
                        col: cell.col,
                        row_span: cell.row_span,
                        col_span: cell.col_span,
                        header: cell.header,
                        headers: cell.headers.clone(),
                    },
                );
            }
        }
    }

    // ---- passes over the finished list ------------------------------------------------------

    /// Marks where the language changes between spoken items.
    fn mark_language_changes(&mut self) {
        let mut previous = self.catalog_lang();
        for item in self.items.iter_mut().filter(|i| i.skipped.is_none()) {
            if item.lang != previous {
                item.lang_change = Some(LangChange {
                    from: previous.clone(),
                    to: item.lang.clone(),
                });
                previous.clone_from(&item.lang);
            }
        }
    }

    /// Lists the text-painting artifacts of each page as skipped items, each before the first
    /// item of the page painted after it, or after the page's last item.
    fn insert_artifacts(&mut self) {
        let mut inserts: Vec<(usize, SpeechItem)> = Vec::new();
        for page in 0..PageIx::try_from(self.doc.pages.len()).unwrap_or(PageIx::MAX) {
            for art in self.doc.artifact_runs(page) {
                let on_page = |i: &usize| self.items[*i].page == Some(page);
                let slot = (0..self.items.len())
                    .filter(&on_page)
                    .find(|&i| self.first_ops[i].is_some_and(|op| op > art.begin_op))
                    .or_else(|| (0..self.items.len()).rfind(&on_page).map(|i| i + 1))
                    .unwrap_or(self.items.len());
                inserts.push((
                    slot,
                    SpeechItem {
                        index: 0,
                        node: NodeId::Artifact { page, seq: art.seq },
                        marked_content: Vec::new(),
                        page: Some(page),
                        declared_role: "Artifact".to_owned(),
                        role: None,
                        class: RoleClass::Artifact,
                        level: None,
                        text: art.text,
                        painted: None,
                        substituted_by: None,
                        also_actual_text: None,
                        lang: self.catalog_lang(),
                        lang_change: None,
                        cell: None,
                        note: None,
                        link: None,
                        invisible: false,
                        skipped: Some(SkipReason::Artifact),
                    },
                ));
            }
        }
        inserts.sort_by_key(|(slot, _)| *slot);
        let mut merged = Vec::with_capacity(self.items.len() + inserts.len());
        let mut pending = inserts.into_iter().peekable();
        for (i, item) in std::mem::take(&mut self.items).into_iter().enumerate() {
            while let Some((_, art)) = pending.next_if(|(slot, _)| *slot <= i) {
                merged.push(art);
            }
            merged.push(item);
        }
        merged.extend(pending.map(|(_, art)| art));
        self.items = merged;
    }

    /// The speech order of a tag-following preset.
    pub(super) fn follow_tags(&mut self) {
        let rules = match self.preset {
            SpeechPreset::FirefoxPdfJs => TableRules::Pdf17,
            _ => self.tree.table_rules,
        };
        self.collect_cells(rules);
        let tree = self.tree;
        for kid in tree.root.iter().flat_map(|r| &r.kids) {
            if let Kid::Elem(ei) = kid {
                self.visit(*ei, None);
            }
        }
        self.flag_notes();
        self.insert_artifacts();
        self.mark_language_changes();
    }

    /// The speech order of a viewer that ignores the tags: the text of each page in paint order.
    pub(super) fn paint_order(&mut self) {
        let lang = self.catalog_lang();
        for page in 0..PageIx::try_from(self.doc.pages.len()).unwrap_or(PageIx::MAX) {
            for run in self.doc.text_runs(page) {
                let (node, declared_role, marked_content) = match &run.kind {
                    RunKind::Tagged(mcid, _) => {
                        let mc = NodeId::MarkedContent { page, mcid: *mcid };
                        match self.tree.owners.get(&(page, *mcid)) {
                            Some(&ei) => (self.tree.id_of(ei), self.tree.elems[ei].role.clone(), vec![mc]),
                            None => (mc.clone(), String::new(), vec![mc]),
                        }
                    }
                    RunKind::Artifact(seq) => (
                        NodeId::Artifact { page, seq: *seq },
                        "Artifact".to_owned(),
                        Vec::new(),
                    ),
                    RunKind::Untagged => (
                        NodeId::Op {
                            page,
                            ix: run.first_op,
                        },
                        String::new(),
                        Vec::new(),
                    ),
                };
                self.items.push(SpeechItem {
                    index: 0,
                    node,
                    marked_content,
                    page: Some(page),
                    declared_role,
                    role: None,
                    class: RoleClass::Other,
                    level: None,
                    text: run.text,
                    painted: None,
                    substituted_by: None,
                    also_actual_text: None,
                    lang: lang.clone(),
                    lang_change: None,
                    cell: None,
                    note: None,
                    link: None,
                    invisible: run.invisible,
                    skipped: None,
                });
            }
        }
    }
}
