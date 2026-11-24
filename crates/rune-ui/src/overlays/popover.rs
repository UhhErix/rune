//! Radix-style Popover and Tooltip with hover open delay and skip-delay group across sibling
//! tooltips.

use rune::{El, col, el, text};
use rune_kit::{Placement, popover};

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

type ActFn = Box<dyn FnOnce(El) -> El>;

/// Anchored popover surface mounted in [`LayerSlot::Popover`].
pub struct UiPopover {
    key: String,
    trigger: El,
    content: El,
    open: bool,
    placement: Placement,
    gap: f32,
    on_close: Option<ActFn>,
}

impl UiPopover {
    pub fn new(key: impl Into<String>, trigger: El, content: El) -> Self {
        Self {
            key: key.into(),
            trigger,
            content,
            open: false,
            placement: Placement::BottomStart,
            gap: 6.0,
            on_close: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn placement(mut self, p: Placement) -> Self {
        self.placement = p;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(move |e: El| e.modal(true, f)));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        let anchor_key = format!("{}-pop-trigger", self.key);
        let trigger = el().key(anchor_key.clone()).child(self.trigger);
        if self.open {
            let mut surface =
                theme.overlay_surface(col().padding(12.0).child(self.content), theme.radii.panel);
            if let Some(close) = self.on_close {
                surface = close(surface);
            }
            portals.push(
                LayerSlot::Popover,
                popover::anchored(&theme.kit, surface, &anchor_key, self.placement, self.gap),
            );
        }
        trigger
    }
}

/// Radix TooltipProvider state: hover open delay + skip-delay window when moving between sibling
/// tooltip triggers.
#[derive(Clone, Debug)]
pub struct TooltipGroupState {
    pub active_id: Option<String>,
    pub opened_at: f64,
    pub last_closed_at: f64,
    pub open_delay_secs: f64,
    pub skip_delay_secs: f64,
}

impl Default for TooltipGroupState {
    fn default() -> Self {
        Self {
            active_id: None,
            opened_at: f64::NEG_INFINITY,
            last_closed_at: f64::NEG_INFINITY,
            open_delay_secs: 0.35,
            skip_delay_secs: 0.30,
        }
    }
}

impl TooltipGroupState {
    /// Returns true if moving immediately from another tooltip within `skip_delay_secs`.
    pub fn should_skip_delay(&self, now: f64) -> bool {
        (now - self.last_closed_at) <= self.skip_delay_secs
    }

    pub fn pointer_enter(&mut self, id: impl Into<String>, now: f64) {
        let id = id.into();
        let instant = self.active_id.is_some() || self.should_skip_delay(now);
        self.active_id = Some(id);
        self.opened_at = if instant {
            now
        } else {
            now + self.open_delay_secs
        };
    }

    pub fn pointer_leave(&mut self, id: &str, now: f64) {
        if self.active_id.as_deref() == Some(id) {
            self.active_id = None;
            self.last_closed_at = now;
        }
    }

    pub fn is_visible(&self, id: &str, now: f64) -> bool {
        self.active_id.as_deref() == Some(id) && now >= self.opened_at
    }
}

/// Anchored Tooltip mounted in [`LayerSlot::Tooltip`].
pub struct UiTooltip {
    key: String,
    trigger: El,
    label: String,
    open: bool,
    placement: Placement,
}

impl UiTooltip {
    pub fn new(key: impl Into<String>, trigger: El, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            trigger,
            label: label.into(),
            open: false,
            placement: Placement::Top,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn placement(mut self, p: Placement) -> Self {
        self.placement = p;
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        let anchor_key = format!("{}-tip-trigger", self.key);
        let trigger = el().key(anchor_key.clone()).child(self.trigger);
        if self.open {
            let tip = theme.overlay_surface(
                el().px(10.0)
                    .py(6.0)
                    .role("tooltip")
                    .label(self.label.clone())
                    .child(
                        text(self.label)
                            .font(theme.kit.ty.small_medium)
                            .color(theme.kit.c.text_base),
                    ),
                theme.radii.control,
            );
            portals.push(
                LayerSlot::Tooltip,
                popover::anchored(&theme.kit, tip, &anchor_key, self.placement, 6.0),
            );
        }
        trigger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_group_applies_open_delay_and_skip_delay_window() {
        let mut g = TooltipGroupState::default();
        g.pointer_enter("a", 1.0);
        assert!(!g.is_visible("a", 1.1));
        assert!(g.is_visible("a", 1.4));
        g.pointer_leave("a", 1.5);
        g.pointer_enter("b", 1.6);
        assert!(g.is_visible("b", 1.6));
    }
}
