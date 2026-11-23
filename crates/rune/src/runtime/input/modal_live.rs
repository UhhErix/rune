//! Nodes that stay live through an open modal (`El::modal_live`): the pointer reaches them, as
//! with no modal open, while focus, keys and assistive technology stay inside the modal.
use crate::runtime::types::Node;

/// The nodes that a modal's keys name: a node with one of the keys, and every node inside it.
fn named(nodes: &[Node], keys: &[String]) -> Vec<bool> {
    let mut live: Vec<bool> = nodes
        .iter()
        .map(|n| {
            n.key
                .as_deref()
                .is_some_and(|k| keys.iter().any(|x| x == k))
        })
        .collect();
    // A parent can come after its children in the list, so repeat until nothing new is live.
    loop {
        let mut changed = false;
        for (i, n) in nodes.iter().enumerate() {
            if !live[i] && n.parent.is_some_and(|p| live[p]) {
                live[i] = true;
                changed = true;
            }
        }
        if !changed {
            return live;
        }
    }
}

/// The nodes live through the open modals `stack` (indices in `nodes`, innermost last), empty
/// when none is live. A node is live only when every one of them names it, so a dialog open over
/// a menu leaves the menu's bar inert, and a submenu keeps the bar live only if it names it too.
/// Without keys, which is every modal's default, this costs nothing.
pub(super) fn through(nodes: &[Node], stack: &[usize]) -> Vec<bool> {
    let keys = |m: usize| -> &[String] {
        nodes[m]
            .handlers
            .get()
            .and_then(|h| h.modal.as_ref())
            .map_or(&[], |spec| spec.live.as_slice())
    };
    if stack.iter().any(|&m| keys(m).is_empty()) {
        return Vec::new();
    }
    let mut live = vec![true; nodes.len()];
    for &m in stack {
        for (l, own) in live.iter_mut().zip(named(nodes, keys(m))) {
            *l &= own;
        }
    }
    live
}

#[cfg(test)]
#[path = "modal_live_tests.rs"]
mod tests;
