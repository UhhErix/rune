//! LIFO dismissable layer stack matching Radix `DismissableLayer` semantics.
//!
//! Nested popovers, menus, dialogs, and drawers push onto a [`DismissStack`].
//! Pressing Escape dismisses **only** the topmost dismissable layer. An outside pointer press
//! dismisses every dismissable layer above the deepest layer that received the press.

/// Dismissal rules for a single overlay layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DismissPolicy {
    pub escape_key: bool,
    pub outside_pointer: bool,
    pub modal: bool,
}

impl Default for DismissPolicy {
    fn default() -> Self {
        Self {
            escape_key: true,
            outside_pointer: true,
            modal: true,
        }
    }
}

impl DismissPolicy {
    /// Non-dismissable alert dialog (must be confirmed via explicit actions).
    pub fn alert() -> Self {
        Self {
            escape_key: false,
            outside_pointer: false,
            modal: true,
        }
    }

    /// Light popover or tooltip (Escape & outside pointer dismiss, non-modal background).
    pub fn non_modal() -> Self {
        Self {
            escape_key: true,
            outside_pointer: true,
            modal: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DismissEntry {
    pub id: String,
    pub policy: DismissPolicy,
}

/// Stack of active overlay layers, bottom-most first and topmost last.
#[derive(Clone, Debug, Default)]
pub struct DismissStack {
    layers: Vec<DismissEntry>,
}

impl DismissStack {
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn top(&self) -> Option<&DismissEntry> {
        self.layers.last()
    }

    /// Pushes `id` to the top of the stack (moving it to the top if already present).
    pub fn push(&mut self, id: impl Into<String>, policy: DismissPolicy) {
        let id = id.into();
        self.layers.retain(|e| e.id != id);
        self.layers.push(DismissEntry { id, policy });
    }

    /// Removes `id` from the stack wherever it sits.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.layers.len();
        self.layers.retain(|e| e.id != id);
        self.layers.len() != before
    }

    /// Handles an Escape key press: dismisses and returns **only** the topmost layer if its
    /// policy allows Escape. If the topmost layer blocks Escape (e.g., an AlertDialog), lower
    /// layers are NOT dismissed.
    pub fn handle_escape(&mut self) -> Option<String> {
        let top = self.layers.last()?;
        if !top.policy.escape_key {
            return None;
        }
        self.layers.pop().map(|e| e.id)
    }

    /// Handles a pointer-down event whose hit path intersects `inside_ids`.
    /// Dismisses layers from top down until reaching a hit layer or a layer blocking outside
    /// pointer dismissal. Returns dismissed layer ids in top-down order.
    pub fn handle_outside_pointer(&mut self, inside_ids: &[&str]) -> Vec<String> {
        let mut dismissed = Vec::new();
        while let Some(top) = self.layers.last() {
            if inside_ids.iter().any(|id| *id == top.id) {
                break;
            }
            if !top.policy.outside_pointer {
                break;
            }
            if let Some(entry) = self.layers.pop() {
                dismissed.push(entry.id);
            }
        }
        dismissed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_dismisses_only_topmost_layer() {
        let mut stack = DismissStack::default();
        stack.push("dialog", DismissPolicy::default());
        stack.push("select-menu", DismissPolicy::default());
        assert_eq!(stack.handle_escape().as_deref(), Some("select-menu"));
        assert_eq!(stack.top().map(|e| e.id.as_str()), Some("dialog"));
        assert_eq!(stack.handle_escape().as_deref(), Some("dialog"));
        assert!(stack.is_empty());
    }

    #[test]
    fn alert_on_top_blocks_escape_and_outside_click() {
        let mut stack = DismissStack::default();
        stack.push("dialog", DismissPolicy::default());
        stack.push("confirm-alert", DismissPolicy::alert());
        assert_eq!(stack.handle_escape(), None);
        assert!(stack.handle_outside_pointer(&[]).is_empty());
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn outside_click_in_parent_dismisses_child_only() {
        let mut stack = DismissStack::default();
        stack.push("dialog", DismissPolicy::default());
        stack.push("popover", DismissPolicy::default());
        stack.push("submenu", DismissPolicy::default());
        let closed = stack.handle_outside_pointer(&["dialog"]);
        assert_eq!(closed, vec!["submenu".to_string(), "popover".to_string()]);
        assert_eq!(stack.top().map(|e| e.id.as_str()), Some("dialog"));
    }
}
