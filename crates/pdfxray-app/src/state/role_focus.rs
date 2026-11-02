//! Tag roles focus: the roles the Tag roles card picks out on the canvas.
//!
//! A click on a role row focuses that role; a click on a focused row releases the focus;
//! Shift-click adds or removes one role, and removing the last releases. A table's rows, header
//! cells and data cells are roles of their own ([`TagRole`]). While roles are focused the canvas
//! draws them at full strength with their labels, and keeps the other roles as faint boxes
//! without labels. Focus is a view, not a filter: a click on the canvas still picks a faint
//! element.
//!
//! The focus belongs to the document it was set on and lasts for the session: opening another
//! file starts with no focus, and opening the first one again brings its focus back. It acts
//! while the tag boxes or the tag labels are on.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use super::AppState;
use crate::roles::TagRole;

/// The roles in focus after a click on the row of `class`, from `focus`. `add` is Shift.
pub fn clicked(focus: &BTreeSet<TagRole>, class: TagRole, add: bool) -> BTreeSet<TagRole> {
    let mut next = focus.clone();
    if add {
        if !next.remove(&class) {
            next.insert(class);
        }
    } else if focus.contains(&class) {
        next.clear();
    } else {
        next = BTreeSet::from([class]);
    }
    next
}

/// The focused roles of every document opened in this session.
#[derive(Debug, Default)]
pub struct RoleFocus {
    by_file: HashMap<PathBuf, BTreeSet<TagRole>>,
}

impl AppState {
    /// The roles the user focused on in the open document; empty when none is.
    pub fn focused_roles(&self) -> BTreeSet<TagRole> {
        self.document
            .as_ref()
            .and_then(|loaded| self.role_focus.by_file.get(&loaded.path))
            .cloned()
            .unwrap_or_default()
    }

    /// True while the Tag roles card picks roles: the tag boxes or the tag labels are on.
    pub const fn role_focus_active(&self) -> bool {
        let show = self.canvas.view.show;
        show.all_boxes || show.labels
    }

    /// The roles the canvas draws at full strength, or `None` when it draws them all: nothing is
    /// focused, or the tag boxes and labels are off.
    pub fn shown_roles(&self) -> Option<BTreeSet<TagRole>> {
        let focus = self.focused_roles();
        (self.role_focus_active() && !focus.is_empty()).then_some(focus)
    }

    /// A click on the row of `class` in the Tag roles card; `add` is Shift. Nothing happens while
    /// the tag boxes and labels are off.
    pub fn click_role(&mut self, class: TagRole, add: bool) {
        if !self.role_focus_active() {
            return;
        }
        let next = clicked(&self.focused_roles(), class, add);
        self.set_focus(next);
    }

    /// Releases the focus: every role shows again (Esc, or *Show all*).
    pub fn show_all_roles(&mut self) {
        self.set_focus(BTreeSet::new());
    }

    fn set_focus(&mut self, roles: BTreeSet<TagRole>) {
        let Some(path) = self.document.as_ref().map(|loaded| loaded.path.clone()) else {
            return;
        };
        if roles.is_empty() {
            self.role_focus.by_file.remove(&path);
        } else {
            self.role_focus.by_file.insert(path, roles);
        }
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::RoleClass;
    use pretty_assertions::assert_eq;

    use super::*;

    const HEADING: TagRole = TagRole::Class(RoleClass::Heading);
    const FIGURE: TagRole = TagRole::Class(RoleClass::Figure);
    const PARAGRAPH: TagRole = TagRole::Class(RoleClass::Paragraph);

    fn set<const N: usize>(roles: [TagRole; N]) -> BTreeSet<TagRole> {
        BTreeSet::from(roles)
    }

    #[test]
    fn a_click_focuses_one_role_and_a_click_on_a_focused_row_releases() {
        assert_eq!(clicked(&set([]), HEADING, false), set([HEADING]));
        // Another row moves the focus there.
        assert_eq!(clicked(&set([HEADING]), FIGURE, false), set([FIGURE]));
        // The focused row again: every role shows.
        assert_eq!(clicked(&set([HEADING]), HEADING, false), set([]));
        assert_eq!(clicked(&set([HEADING, FIGURE]), FIGURE, false), set([]));
    }

    #[test]
    fn shift_adds_or_removes_one_role_and_removing_the_last_releases() {
        assert_eq!(clicked(&set([HEADING]), FIGURE, true), set([HEADING, FIGURE]));
        assert_eq!(clicked(&set([HEADING, FIGURE]), HEADING, true), set([FIGURE]));
        assert_eq!(clicked(&set([FIGURE]), FIGURE, true), set([]));
        // Smoke: Shift on nothing focused starts a focus of one.
        assert_eq!(clicked(&set([]), PARAGRAPH, true), set([PARAGRAPH]));
    }

    #[test]
    fn the_parts_of_a_table_are_picked_one_by_one() {
        let header = TagRole::TableHeader;
        let cell = TagRole::TableCell;

        assert_eq!(clicked(&set([]), header, false), set([header]));
        assert_eq!(clicked(&set([header]), cell, true), set([header, cell]));
        // Smoke: the table itself is not one of its parts.
        let table = TagRole::Class(RoleClass::Table);
        assert_eq!(clicked(&set([header, cell]), table, false), set([table]));
    }
}
