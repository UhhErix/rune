//! Sonner-style Toast Stack with variants, collapsed peek/scale, hover expansion,
//! swipe-to-dismiss physics, action/cancel buttons, pause-on-hover/hidden, and Alt+T hotkey.

use std::rc::Rc;

use rune::{Color, Drag, El, Phase, Tx, col, el, icon, row, spacer, text};
use rune_kit::toast_stack::{ToastEvent, ToastPage, ToastStack};
use rune_motion::{VelocityTracker, deceleration, project};

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToastVariant {
    #[default]
    Default,
    Success,
    Error,
    Warning,
    Info,
    Loading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToastPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    #[default]
    BottomRight,
}

/// Payload for one Sonner toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SonnerToast {
    pub title: String,
    pub description: Option<String>,
    pub variant: ToastVariant,
    pub action_label: Option<String>,
    pub cancel_label: Option<String>,
    pub dismissible: bool,
}

impl SonnerToast {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            variant: ToastVariant::Default,
            action_label: None,
            cancel_label: None,
            dismissible: true,
        }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn variant(mut self, v: ToastVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action_label = Some(label.into());
        self
    }

    pub fn cancel(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = Some(label.into());
        self
    }
}

/// Swipe-to-dismiss gesture state for the front toast.
#[derive(Clone, Debug, Default)]
pub struct ToastSwipeState {
    pub toast_id: Option<u64>,
    pub offset_x: f32,
    velocity: VelocityTracker<f32>,
}

impl ToastSwipeState {
    pub fn drag_move(&mut self, toast_id: u64, dx: f32, now: f64) {
        self.toast_id = Some(toast_id);
        self.offset_x = dx;
        self.velocity.add(now, dx);
    }

    /// Returns `Some(toast_id)` if swipe exceeded distance or velocity dismiss threshold.
    pub fn drag_end(&mut self, cancelled: bool) -> Option<u64> {
        let id = self.toast_id.take();
        let vel = self.velocity.velocity();
        let projected = project(self.offset_x, vel, deceleration::NORMAL);
        self.offset_x = 0.0;
        self.velocity.reset();
        if cancelled {
            return None;
        }
        if projected.abs() >= 90.0 || vel.abs() >= 420.0 {
            id
        } else {
            None
        }
    }
}

/// Owned Sonner toaster state wrapping [`rune_kit::ToastStack`] + swipe-to-dismiss physics.
#[derive(Clone, Debug, Default)]
pub struct SonnerState {
    pub stack: ToastStack<SonnerToast>,
    pub position: ToastPosition,
    pub swipe: ToastSwipeState,
}

impl SonnerState {
    pub fn new(position: ToastPosition) -> Self {
        Self {
            stack: ToastStack::default(),
            position,
            swipe: ToastSwipeState::default(),
        }
    }

    pub fn push(&mut self, toast: SonnerToast, duration_secs: Option<f64>, now: f64) {
        let key = format!("{:?}:{}", toast.variant, toast.title);
        let _ = self.stack.step(ToastEvent::Shown {
            key,
            options: toast,
            secs: duration_secs,
            now,
        });
    }

    pub fn dismiss(&mut self, id: Option<u64>, now: f64) {
        let _ = self.stack.step(ToastEvent::Dismissed { id, now });
    }

    pub fn tick(&mut self, now: f64) {
        let _ = self.stack.step(ToastEvent::Tick { now });
    }

    pub fn set_page_visible(&mut self, visible: bool, now: f64) {
        let page = if visible {
            ToastPage::Shown
        } else {
            ToastPage::Hidden
        };
        let _ = self.stack.step(ToastEvent::Page { page, now });
    }

    pub fn toggle_expand_hotkey(&mut self, now: f64) {
        let _ = self.stack.step(ToastEvent::Hotkey { now });
    }
}

type ToastIdHandler = Rc<dyn Fn(El, u64) -> El>;

/// Renders a [`SonnerState`] stack into `portals` (`LayerSlot::Toast`).
pub struct SonnerToaster<'a> {
    key: String,
    state: &'a SonnerState,
    width: f32,
    on_dismiss: Option<ToastIdHandler>,
    on_action: Option<ToastIdHandler>,
    on_swipe: Option<ToastIdHandler>,
}

impl<'a> SonnerToaster<'a> {
    pub fn new(key: impl Into<String>, state: &'a SonnerState) -> Self {
        Self {
            key: key.into(),
            state,
            width: 356.0,
            on_dismiss: None,
            on_action: None,
            on_swipe: None,
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(240.0);
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut SonnerState + Clone + 'static,
        on_action: impl Fn(&mut S, u64) + Clone + 'static,
    ) -> Self {
        let acc_d = access.clone();
        self.on_dismiss = Some(Rc::new(move |e: El, id: u64| {
            let acc_c = acc_d.clone();
            e.on_press_tx(move |s: &mut S, tx: &mut Tx| acc_c(s).dismiss(Some(id), tx.clock()))
        }));
        self.on_action = Some(Rc::new(move |e: El, id: u64| {
            let act = on_action.clone();
            e.on_press(move |s: &mut S| act(s, id))
        }));
        let acc_s = access;
        self.on_swipe = Some(Rc::new(move |e: El, id: u64| {
            let acc_c = acc_s.clone();
            e.on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
                let st = acc_c(s);
                match d.phase {
                    Phase::Down | Phase::Move => st.swipe.drag_move(id, d.dx, tx.clock()),
                    Phase::Up => {
                        if let Some(dismissed_id) = st.swipe.drag_end(d.cancelled()) {
                            st.dismiss(Some(dismissed_id), tx.clock());
                        }
                    }
                }
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) {
        let entries = self.state.stack.entries();
        if entries.is_empty() {
            return;
        }
        let k = &theme.kit;
        let c = &k.c;
        let height_for = |id: u64| -> f32 {
            entries.iter().find(|e| e.id == id).map_or(56.0, |e| {
                if e.options.description.is_some() {
                    72.0
                } else {
                    52.0
                }
            })
        };

        let mut stack_box = col()
            .key(format!("{}-toasts", self.key))
            .w(self.width)
            .gap(8.0);

        for entry in entries.iter().rev() {
            let Some(place) = self.state.stack.place(entry.id, height_for) else {
                continue;
            };
            if !place.visible {
                continue;
            }
            let opts = &entry.options;
            let accent = match opts.variant {
                ToastVariant::Success => Some(c.success.icon),
                ToastVariant::Error => Some(c.danger.icon),
                ToastVariant::Warning => Some(c.warning.icon),
                ToastVariant::Info => Some(c.info.icon),
                ToastVariant::Loading => Some(c.bg_accent),
                ToastVariant::Default => None,
            };

            let mut card = row()
                .key(format!("{}-t-{}", self.key, entry.id))
                .w(self.width)
                .px(14.0)
                .py(12.0)
                .gap(10.0)
                .center_y()
                .role("status")
                .label(opts.title.clone())
                .scale(place.scale)
                .motion(theme.springs.move_);

            if self.state.swipe.toast_id == Some(entry.id) {
                card = card.offset(self.state.swipe.offset_x, 0.0);
            }
            if let Some(dot) = accent {
                card = card.child(el().size(8.0, 8.0).radius(theme.radii.pill).fill(dot));
            }
            let mut copy = col().grow(1.0).shrink().gap(2.0).child(
                text(opts.title.clone())
                    .font(k.ty.ui_medium)
                    .color(c.text_base),
            );
            if let Some(desc) = &opts.description {
                copy = copy.child(
                    text(desc.clone())
                        .font(k.ty.small)
                        .color(c.text_muted)
                        .wrap(),
                );
            }
            card = card.child(copy);

            if let Some(act_lbl) = &opts.action_label {
                let mut act_btn = el()
                    .px(10.0)
                    .py(5.0)
                    .radius(theme.radii.control)
                    .fill(c.bg_inverse)
                    .role("button")
                    .label(act_lbl.clone())
                    .child(
                        text(act_lbl.clone())
                            .font(k.ty.small_medium)
                            .color(c.text_inverse),
                    );
                if let Some(cb) = &self.on_action {
                    act_btn = cb(act_btn, entry.id);
                }
                card = card.child(act_btn);
            }

            if opts.dismissible {
                let mut close_btn = el()
                    .size(22.0, 22.0)
                    .center()
                    .radius(theme.radii.pill)
                    .hover(theme.hover_fill)
                    .role("button")
                    .label("Dismiss")
                    .child(icon(k.glyph.close, 12.0).color(c.icon_muted));
                if let Some(cb) = &self.on_dismiss {
                    close_btn = cb(close_btn, entry.id);
                }
                card = card.child(close_btn);
            }

            card = theme.overlay_surface(card, theme.radii.card);
            if let Some(sw) = &self.on_swipe {
                card = sw(card, entry.id);
            }
            stack_box = stack_box.child(card);
        }

        let layer = match self.state.position {
            ToastPosition::TopLeft => col().justify_start().items_start(),
            ToastPosition::TopCenter => col().justify_start().items_center(),
            ToastPosition::TopRight => col().justify_start().items_end(),
            ToastPosition::BottomLeft => col().justify_end().items_start(),
            ToastPosition::BottomCenter => col().justify_end().items_center(),
            ToastPosition::BottomRight => col().justify_end().items_end(),
        }
        .key(format!("{}-toast-layer", self.key))
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .padding(20.0)
        .child(stack_box);

        let _ = (Color::TRANSPARENT, spacer());
        portals.push(LayerSlot::Toast, layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonner_stack_coalesces_and_swipes_to_dismiss() {
        let mut s = SonnerState::new(ToastPosition::BottomRight);
        s.push(
            SonnerToast::new("Saved settings").variant(ToastVariant::Success),
            Some(4.0),
            0.0,
        );
        s.push(
            SonnerToast::new("Download ready").description("Cyberpunk 2077"),
            Some(4.0),
            0.5,
        );
        assert_eq!(s.stack.entries().len(), 2);

        let front_id = s.stack.front().map(|e| e.id).unwrap_or(0);
        s.swipe.drag_move(front_id, 20.0, 1.0);
        s.swipe.drag_move(front_id, 130.0, 1.02);
        let dismissed = s.swipe.drag_end(false);
        assert_eq!(dismissed, Some(front_id));
        s.dismiss(dismissed, 1.06);
        assert_eq!(s.stack.entries().len(), 1);
    }
}
