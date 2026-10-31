//! Back and forward through the objects the user has visited (⌘[ and ⌘]).

use pdfxray_core::NodeId;

use super::{AppState, RailItem};

/// The objects visited, oldest first, and where the user is in that list.
#[derive(Debug, Default)]
pub struct ObjectHistory {
    stack: Vec<NodeId>,
    cursor: usize,
}

impl ObjectHistory {
    /// Records a visit to `id`. Only objects count; going to a new one drops what was "forward".
    pub fn visit(&mut self, id: &NodeId) {
        if !matches!(id, NodeId::Object(_)) {
            return;
        }
        if self.current() == Some(id) {
            return;
        }
        if !self.stack.is_empty() {
            self.stack.truncate(self.cursor + 1);
        }
        self.stack.push(id.clone());
        self.cursor = self.stack.len() - 1;
    }

    /// Every object visited, oldest first.
    pub fn visited(&self) -> &[NodeId] {
        &self.stack
    }

    /// The object the user is at.
    pub fn current(&self) -> Option<&NodeId> {
        self.stack.get(self.cursor)
    }

    /// True when there is an older object to go back to.
    pub const fn can_go_back(&self) -> bool {
        self.cursor > 0
    }

    /// True when there is a newer object to go forward to.
    pub const fn can_go_forward(&self) -> bool {
        self.cursor + 1 < self.stack.len()
    }

    /// Moves one object back and returns it.
    pub fn back(&mut self) -> Option<NodeId> {
        if !self.can_go_back() {
            return None;
        }
        self.cursor -= 1;
        self.current().cloned()
    }

    /// Moves one object forward and returns it.
    pub fn forward(&mut self) -> Option<NodeId> {
        if !self.can_go_forward() {
            return None;
        }
        self.cursor += 1;
        self.current().cloned()
    }

    /// Forgets everything, e.g. when another file is opened.
    pub fn clear(&mut self) {
        self.stack.clear();
        self.cursor = 0;
    }
}

impl AppState {
    /// ⌘[: selects the object visited before this one. Only the Objects entry has a history.
    pub fn history_back(&mut self) {
        if self.nav.rail == RailItem::Objects
            && let Some(id) = self.history.back()
        {
            self.select(id);
        }
    }

    /// ⌘]: selects the object visited after this one.
    pub fn history_forward(&mut self) {
        if self.nav.rail == RailItem::Objects
            && let Some(id) = self.history.forward()
        {
            self.select(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use pdfxray_core::ObjRef;
    use pretty_assertions::assert_eq;

    use super::*;

    fn obj(n: u32) -> NodeId {
        NodeId::Object(ObjRef::new(n, 0))
    }

    #[test]
    fn back_and_forward_walk_the_visited_objects() {
        let mut history = ObjectHistory::default();
        for n in [1, 2, 3] {
            history.visit(&obj(n));
        }

        assert_eq!(history.back(), Some(obj(2)));
        assert_eq!(history.back(), Some(obj(1)));
        assert_eq!(history.back(), None);
        assert_eq!(history.forward(), Some(obj(2)));
        assert!(history.can_go_forward());
        // Smoke: the position is where the walk left it.
        assert_eq!(history.current(), Some(&obj(2)));
    }

    #[test]
    fn visiting_drops_the_forward_part_and_ignores_other_nodes() {
        let mut history = ObjectHistory::default();
        for n in [1, 2, 3] {
            history.visit(&obj(n));
        }
        history.back();
        history.back();

        history.visit(&NodeId::Page(0));
        history.visit(&obj(9));

        assert!(!history.can_go_forward());
        assert_eq!(history.back(), Some(obj(1)));
        assert_eq!(history.forward(), Some(obj(9)));
        // Visiting the current object again changes nothing.
        history.visit(&obj(9));
        assert_eq!(history.back(), Some(obj(1)));
    }
}
