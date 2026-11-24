//! Radix-style Select with groups, disabled items, check indicator, roving + typeahead,
//! and anchored popover viewport with flip & shift.

use std::rc::Rc;

use rune::{Color, El, Icon, Key, KeyEvent, Tx, col, el, icon, row, spacer, text};
use rune_kit::{Placement, popover};

use crate::primitives::focus::{RovingOrientation, RovingState};
use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::primitives::typeahead::TypeaheadBuffer;
use crate::theme::UiTheme;

#[derive(Clone, Debug)]
pub struct UiSelectOption {
    pub value: String,
    pub label: String,
    pub group: Option<String>,
    pub icon: Option<&'static Icon>,
    pub disabled: bool,
}

impl UiSelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            group: None,
            icon: None,
            disabled: false,
        }
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
    pub fn icon(mut self, glyph: &'static Icon) -> Self {
        self.icon = Some(glyph);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// Owned Select state supporting open/close, keyboard roving, and Radix typeahead.
#[derive(Clone, Debug, Default)]
pub struct UiSelectState {
    pub open: bool,
    pub roving: RovingState,
    pub typeahead: TypeaheadBuffer,
}

impl UiSelectState {
    pub fn new() -> Self {
        Self {
            open: false,
            roving: RovingState::new(RovingOrientation::Vertical),
            typeahead: TypeaheadBuffer::default(),
        }
    }

    pub fn toggle(&mut self, options: &[UiSelectOption], selected: &str) {
        self.open = !self.open;
        self.typeahead.clear();
        if self.open {
            self.roving.active = options
                .iter()
                .position(|o| !o.disabled && o.value == selected)
                .unwrap_or(0);
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.typeahead.clear();
    }

    /// Handles keyboard navigation & typeahead; returns `Some(value)` when confirmed.
    pub fn handle_key(
        &mut self,
        ev: &KeyEvent,
        now: f64,
        options: &[UiSelectOption],
    ) -> Option<String> {
        if !self.open {
            return None;
        }
        let disabled: Vec<bool> = options.iter().map(|o| o.disabled).collect();
        match &ev.key {
            Key::Escape => {
                self.close();
                None
            }
            Key::Enter => {
                let opt = options.get(self.roving.active)?;
                if opt.disabled {
                    return None;
                }
                let val = opt.value.clone();
                self.close();
                Some(val)
            }
            Key::Text(t) if !ev.mods.ctrl && !ev.mods.alt && !ev.mods.cmd => {
                let items = options.iter().map(|o| (o.label.as_str(), o.disabled));
                if let Some(hit) =
                    self.typeahead
                        .push_and_match(t, now, Some(self.roving.active), items)
                {
                    self.roving.active = hit;
                }
                None
            }
            _ => {
                let _ = self.roving.handle_key(ev, &disabled);
                None
            }
        }
    }
}

type PickFn = Rc<dyn Fn(El, String) -> El>;
type BindFn = Box<dyn FnOnce(El) -> El>;

pub struct UiSelect {
    key: String,
    placeholder: String,
    selected: Option<String>,
    options: Vec<UiSelectOption>,
    open: bool,
    active_index: usize,
    width: f32,
    disabled: bool,
    on_toggle: Option<BindFn>,
    on_close: Option<BindFn>,
    on_pick: Option<PickFn>,
}

impl UiSelect {
    pub fn new(key: impl Into<String>, selected: Option<String>) -> Self {
        Self {
            key: key.into(),
            placeholder: "Select an option...".into(),
            selected,
            options: Vec::new(),
            open: false,
            active_index: 0,
            width: 220.0,
            disabled: false,
            on_toggle: None,
            on_close: None,
            on_pick: None,
        }
    }

    pub fn placeholder(mut self, ph: impl Into<String>) -> Self {
        self.placeholder = ph.into();
        self
    }

    pub fn options(mut self, opts: Vec<UiSelectOption>) -> Self {
        self.options = opts;
        self
    }

    pub fn open(mut self, state: &UiSelectState) -> Self {
        self.open = state.open;
        self.active_index = state.roving.active;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(120.0);
        self
    }

    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }

    pub fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(move |e: El| e.modal(true, f)));
        self
    }

    pub fn on_select<S: 'static>(mut self, f: impl Fn(&mut S, String) + Clone + 'static) -> Self {
        self.on_pick = Some(Rc::new(move |e: El, val: String| {
            let f_c = f.clone();
            e.on_press(move |s: &mut S| f_c(s, val.clone()))
        }));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let trigger_key = format!("{}-trigger", self.key);
        let chosen_label = self
            .selected
            .as_deref()
            .and_then(|sel| self.options.iter().find(|o| o.value == sel))
            .map(|o| o.label.clone());

        let (display, display_color) = match chosen_label {
            Some(l) => (l, c.text_base),
            None => (self.placeholder, c.text_muted),
        };

        let mut trigger = row()
            .key(trigger_key.clone())
            .w(self.width)
            .h(36.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .role("combobox")
            .label(display.clone())
            .disabled(self.disabled)
            .focusable(!self.disabled)
            .child(
                text(display)
                    .font(k.ty.ui_medium)
                    .color(display_color)
                    .truncate(),
            )
            .child(spacer())
            .child(icon(k.glyph.chevron_down, 13.0).color(c.icon_muted));

        trigger = theme.control_surface(trigger, theme.radii.control);
        if self.disabled {
            trigger = trigger.opacity(0.45);
        } else {
            trigger = theme.apply_focus(trigger.hover(theme.hover_fill));
            if let Some(cb) = self.on_toggle {
                trigger = cb(trigger);
            }
        }

        if self.open && !self.disabled {
            let mut list = col().w(self.width).padding(4.0).gap(2.0).role("listbox");
            let mut last_group: Option<String> = None;

            for (i, opt) in self.options.into_iter().enumerate() {
                if opt.group != last_group {
                    if let Some(g) = &opt.group {
                        list =
                            list.child(el().px(8.0).pt(6.0).pb(2.0).child(
                                text(g.clone()).font(k.ty.small_medium).color(c.text_muted),
                            ));
                    }
                    last_group = opt.group.clone();
                }

                let is_selected = self.selected.as_deref() == Some(opt.value.as_str());
                let is_active = i == self.active_index;
                let mut item = row()
                    .key(format!("{}-opt-{i}", self.key))
                    .w_full()
                    .h(32.0)
                    .px(8.0)
                    .gap(8.0)
                    .center_y()
                    .radius(theme.radii.control - 2.0)
                    .role("option")
                    .label(opt.label.clone())
                    .disabled(opt.disabled)
                    .fill(if is_active {
                        theme.selected_fill
                    } else {
                        Color::TRANSPARENT
                    })
                    .hover(theme.hover_fill);

                if let Some(glyph) = opt.icon {
                    item = item.child(icon(glyph, 14.0).color(c.icon_base));
                }
                item = item
                    .child(text(opt.label).font(k.ty.ui_compact).color(c.text_base))
                    .child(spacer());
                if is_selected {
                    item = item.child(icon(k.glyph.check, 13.0).color(c.text_accent));
                }
                if opt.disabled {
                    item = item.opacity(0.45);
                } else if let Some(pick) = &self.on_pick {
                    item = pick(item, opt.value);
                }
                list = list.child(item);
            }

            let mut surface = theme.overlay_surface(list, theme.radii.menu);
            if let Some(close) = self.on_close {
                surface = close(surface);
            }
            let anchored = popover::anchored(k, surface, &trigger_key, Placement::BottomStart, 6.0);
            portals.push(LayerSlot::Dropdown, anchored);
        }

        let _ = Tx::default;
        trigger
    }
}
