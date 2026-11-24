//! Switch, tri-state Checkbox (`Unchecked`/`Checked`/`Indeterminate`), and RadioGroup with
//! Radix / React Aria semantics, spring motion, and AccessKit roles.

use rune::{Color, El, KeyEvent, Tx, col, el, icon, row, text};

use crate::primitives::focus::{RovingOrientation, RovingState, is_activation_key};
use crate::theme::UiTheme;

type ActFn = Box<dyn FnOnce(El) -> El>;

/// Spring-animated Switch matching Radix Switch semantics.
pub struct UiSwitch {
    key: String,
    checked: bool,
    label: Option<String>,
    description: Option<String>,
    disabled: bool,
    actions: Vec<ActFn>,
}

impl UiSwitch {
    pub fn new(key: impl Into<String>, checked: bool) -> Self {
        Self {
            key: key.into(),
            checked,
            label: None,
            description: None,
            disabled: false,
            actions: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_toggle<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        let f_key = f.clone();
        self.actions.push(Box::new(move |e: El| e.on_press(f)));
        self.actions.push(Box::new(move |e: El| {
            e.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                if is_activation_key(ev) {
                    f_key(s);
                    true
                } else {
                    false
                }
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let track_w = 38.0;
        let track_h = 22.0;
        let knob = 16.0;
        let pad = 3.0;
        let knob_x = if self.checked {
            track_w - knob - pad
        } else {
            pad
        };

        let track_fill = if self.checked {
            c.bg_accent
        } else {
            c.border_base
        };
        let thumb_fill = if self.checked { c.knob_on } else { c.knob };

        let thumb = el()
            .key(format!("{}-knob", self.key))
            .absolute(knob_x, pad)
            .size(knob, knob)
            .radius(theme.radii.pill)
            .fill(thumb_fill)
            .motion(theme.springs.quick);

        let track = el()
            .key(format!("{}-track", self.key))
            .size(track_w, track_h)
            .radius(theme.radii.pill)
            .fill(track_fill)
            .motion(theme.springs.quick)
            .child(thumb);

        let a11y_label = self.label.clone().unwrap_or_else(|| self.key.clone());
        let mut root = row()
            .key(self.key)
            .gap(10.0)
            .center_y()
            .role("switch")
            .label(a11y_label)
            .checked(self.checked)
            .disabled(self.disabled)
            .focusable(!self.disabled)
            .child(track);

        if self.label.is_some() || self.description.is_some() {
            let mut copy = col().gap(2.0);
            if let Some(lbl) = self.label {
                copy = copy.child(text(lbl).font(k.ty.ui_medium).color(c.text_base));
            }
            if let Some(desc) = self.description {
                copy = copy.child(text(desc).font(k.ty.small).color(c.text_muted));
            }
            root = root.child(copy);
        }

        if self.disabled {
            root = root.opacity(0.45);
        } else {
            root = theme.apply_focus(root);
            for act in self.actions {
                root = act(root);
            }
        }
        root
    }
}

/// Tri-state checkbox state matching Radix Checkbox (`checked` | `unchecked` | `indeterminate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CheckState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

impl CheckState {
    pub fn from_bool(checked: bool) -> Self {
        if checked {
            Self::Checked
        } else {
            Self::Unchecked
        }
    }
    pub fn is_checked(self) -> bool {
        self == Self::Checked
    }
    pub fn toggle(self) -> Self {
        match self {
            Self::Checked => Self::Unchecked,
            Self::Unchecked | Self::Indeterminate => Self::Checked,
        }
    }
}

/// Tri-state Checkbox with AccessKit `checked` / `checked_mixed`.
pub struct UiCheckbox {
    key: String,
    state: CheckState,
    label: Option<String>,
    description: Option<String>,
    disabled: bool,
    actions: Vec<ActFn>,
}

impl UiCheckbox {
    pub fn new(key: impl Into<String>, state: CheckState) -> Self {
        Self {
            key: key.into(),
            state,
            label: None,
            description: None,
            disabled: false,
            actions: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change<S: 'static>(
        mut self,
        f: impl Fn(&mut S, CheckState) + Clone + 'static,
    ) -> Self {
        let next = self.state.toggle();
        let f_key = f.clone();
        self.actions.push(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| f(s, next))
        }));
        self.actions.push(Box::new(move |e: El| {
            e.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                if is_activation_key(ev) {
                    f_key(s, next);
                    true
                } else {
                    false
                }
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let active = self.state != CheckState::Unchecked;
        let box_fill = if active {
            c.bg_accent
        } else {
            Color::TRANSPARENT
        };
        let box_border = if active { c.bg_accent } else { c.border_strong };
        let glyph = match self.state {
            CheckState::Indeterminate => k.glyph.dash,
            CheckState::Checked | CheckState::Unchecked => k.glyph.check,
        };

        let box_el = el()
            .key(format!("{}-box", self.key))
            .size(18.0, 18.0)
            .center()
            .radius(5.0)
            .fill(box_fill)
            .border(1.5, box_border)
            .motion(theme.springs.quick)
            .child(
                icon(glyph, 11.0)
                    .color(c.submit_fg)
                    .opacity(if active { 1.0 } else { 0.0 }),
            );

        let a11y_label = self.label.clone().unwrap_or_else(|| self.key.clone());
        let mut root = row()
            .key(self.key)
            .gap(10.0)
            .center_y()
            .role("checkbox")
            .label(a11y_label)
            .disabled(self.disabled)
            .focusable(!self.disabled)
            .child(box_el);

        root = match self.state {
            CheckState::Indeterminate => root.checked_mixed(),
            CheckState::Checked => root.checked(true),
            CheckState::Unchecked => root.checked(false),
        };

        if self.label.is_some() || self.description.is_some() {
            let mut copy = col().gap(2.0);
            if let Some(lbl) = self.label {
                copy = copy.child(text(lbl).font(k.ty.ui_medium).color(c.text_base));
            }
            if let Some(desc) = self.description {
                copy = copy.child(text(desc).font(k.ty.small).color(c.text_muted));
            }
            root = root.child(copy);
        }

        if self.disabled {
            root = root.opacity(0.45);
        } else {
            root = theme.apply_focus(root);
            for act in self.actions {
                root = act(root);
            }
        }
        root
    }
}

/// One option inside [`UiRadioGroup`].
#[derive(Clone, Debug)]
pub struct UiRadioOption {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    pub disabled: bool,
}

impl UiRadioOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
            disabled: false,
        }
    }
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

type RadioSelectFn = std::rc::Rc<dyn Fn(El, String) -> El>;

/// Radio group with roving arrow navigation, classic or card-tile layout, and AccessKit roles.
pub struct UiRadioGroup {
    key: String,
    label: Option<String>,
    options: Vec<UiRadioOption>,
    selected: String,
    orientation: RovingOrientation,
    card_style: bool,
    on_select_el: Option<RadioSelectFn>,
    on_group_key: Option<ActFn>,
}

impl UiRadioGroup {
    pub fn new(key: impl Into<String>, selected: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: None,
            options: Vec::new(),
            selected: selected.into(),
            orientation: RovingOrientation::Vertical,
            card_style: false,
            on_select_el: None,
            on_group_key: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn option(mut self, opt: UiRadioOption) -> Self {
        self.options.push(opt);
        self
    }

    pub fn options(mut self, opts: impl IntoIterator<Item = UiRadioOption>) -> Self {
        self.options.extend(opts);
        self
    }

    pub fn horizontal(mut self) -> Self {
        self.orientation = RovingOrientation::Horizontal;
        self
    }

    pub fn card_style(mut self, cards: bool) -> Self {
        self.card_style = cards;
        self
    }

    pub fn on_change<S: 'static>(mut self, f: impl Fn(&mut S, String) + Clone + 'static) -> Self {
        let f_item = f.clone();
        self.on_select_el = Some(std::rc::Rc::new(move |e: El, val: String| {
            let f_click = f_item.clone();
            e.on_press(move |s: &mut S| f_click(s, val.clone()))
        }));

        let opts_values: Vec<String> = self.options.iter().map(|o| o.value.clone()).collect();
        let disabled: Vec<bool> = self.options.iter().map(|o| o.disabled).collect();
        let active = opts_values
            .iter()
            .position(|v| v == &self.selected)
            .unwrap_or(0);
        let orientation = self.orientation;

        self.on_group_key = Some(Box::new(move |e: El| {
            e.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                let mut roving = RovingState {
                    active,
                    orientation,
                    loop_nav: true,
                };
                if let Some(next_idx) = roving.handle_key(ev, &disabled)
                    && let Some(val) = opts_values.get(next_idx)
                {
                    f(s, val.clone());
                    return true;
                }
                false
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let mut built_items = Vec::with_capacity(self.options.len());

        for (i, opt) in self.options.into_iter().enumerate() {
            let is_selected = opt.value == self.selected;
            let ring_border = if is_selected {
                c.bg_accent
            } else {
                c.border_strong
            };
            let dot = el()
                .size(8.0, 8.0)
                .radius(theme.radii.pill)
                .fill(c.bg_accent)
                .opacity(if is_selected { 1.0 } else { 0.0 })
                .motion(theme.springs.quick);
            let circle = el()
                .size(18.0, 18.0)
                .center()
                .radius(theme.radii.pill)
                .border(1.5, ring_border)
                .child(dot);

            let mut text_col = col().gap(2.0).child(
                text(opt.label.clone())
                    .font(k.ty.ui_medium)
                    .color(c.text_base),
            );
            if let Some(desc) = opt.description {
                text_col = text_col.child(text(desc).font(k.ty.small).color(c.text_muted));
            }

            let mut item = row()
                .key(format!("{}-opt-{i}", self.key))
                .gap(10.0)
                .center_y()
                .role("radio")
                .label(opt.label)
                .checked(is_selected)
                .disabled(opt.disabled)
                .focusable(!opt.disabled)
                .child(circle)
                .child(text_col);

            if self.card_style {
                item = item
                    .padding(12.0)
                    .radius(theme.radii.card)
                    .fill(if is_selected {
                        theme.selected_fill
                    } else {
                        c.surface_raised
                    })
                    .border(
                        1.0,
                        if is_selected {
                            c.bg_accent
                        } else {
                            theme.float_line
                        },
                    );
            }
            if opt.disabled {
                item = item.opacity(0.45);
            } else {
                item = theme.apply_focus(item);
                if let Some(cb) = &self.on_select_el {
                    item = cb(item, opt.value);
                }
            }
            built_items.push(item);
        }

        let mut list = match self.orientation {
            RovingOrientation::Horizontal => row().gap(10.0),
            RovingOrientation::Vertical | RovingOrientation::Both => col().gap(8.0),
        };
        list = list.role("radiogroup").focus_group().children(built_items);
        if let Some(lbl) = &self.label {
            list = list.label(lbl.clone());
        }
        if let Some(gk) = self.on_group_key {
            list = gk(list);
        }

        let mut root = col().key(self.key).gap(6.0);
        if let Some(lbl) = self.label {
            root = root.child(text(lbl).font(k.ty.small_medium).color(c.text_base));
        }
        root.child(list)
    }
}
