//! Ordered portal z-layer host (`PortalHost`) built on [`rune_kit::root`].
//!
//! Overlays push into semantic [`LayerSlot`] buckets (`Sticky`, `Dropdown`, `Popover`, `Modal`,
//! `Toast`, `Tooltip`) so tooltips always paint above dialogs and menus escape clipping ancestors
//! regardless of the order components were built in the view tree.

use rune::El;

/// Semantic z-order bucket for portaled overlay layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum LayerSlot {
    Sticky = 10,
    Dropdown = 20,
    Popover = 30,
    Modal = 40,
    Toast = 50,
    Tooltip = 60,
}

/// Collects portaled overlays and mounts them in deterministic [`LayerSlot`] order above `base`.
#[derive(Default)]
pub struct PortalHost {
    entries: Vec<(LayerSlot, El)>,
}

impl PortalHost {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, slot: LayerSlot, layer: El) {
        self.entries.push((slot, layer));
    }

    pub fn extend(&mut self, slot: LayerSlot, layers: impl IntoIterator<Item = El>) {
        for layer in layers {
            self.entries.push((slot, layer));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Consumes the collected overlays, sorts stably by [`LayerSlot`], and mounts them above `base`.
    pub fn mount(mut self, base: El) -> El {
        self.entries.sort_by_key(|(slot, _)| *slot);
        rune_kit::root(base, self.entries.into_iter().map(|(_, el)| el))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::el;

    #[test]
    fn portal_host_orders_slots_stably() {
        let mut host = PortalHost::new();
        host.push(LayerSlot::Tooltip, el().key("tip"));
        host.push(LayerSlot::Modal, el().key("dlg"));
        host.push(LayerSlot::Dropdown, el().key("menu"));
        host.push(LayerSlot::Toast, el().key("toast"));
        assert_eq!(host.len(), 4);
        host.entries.sort_by_key(|(slot, _)| *slot);
        let slots: Vec<LayerSlot> = host.entries.iter().map(|(s, _)| *s).collect();
        assert_eq!(
            slots,
            vec![
                LayerSlot::Dropdown,
                LayerSlot::Modal,
                LayerSlot::Toast,
                LayerSlot::Tooltip
            ]
        );
    }
}
