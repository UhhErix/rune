//! Searchable single-select and multi-select Combobox with removable chips, fuzzy filtering,
//! keyboard navigation, and optional creatable item support.

use std::rc::Rc;

use rune::{Color, El, TextEdit, col, el, icon, row, spacer, text, text_input};
use rune_kit::{Placement, popover};

use crate::overlays::command::command_score;
use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

#[derive(Clone, Debug)]
pub struct UiComboboxOption {
    pub value: String,
    pub label: String,
    pub detail: Option<String>,
    pub disabled: bool,
}

impl UiComboboxOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            detail: None,
            disabled: false,
        }
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// State for single or multi-select [`UiCombobox`].
#[derive(Clone, Debug, Default)]
pub struct UiComboboxState {
    pub open: bool,
    pub multi: bool,
    pub search: TextEdit,
    pub selected: Vec<String>,
    pub active_index: usize,
}

impl UiComboboxState {
    pub fn single() -> Self {
        Self {
            multi: false,
            ..Self::default()
        }
    }

    pub fn multi() -> Self {
        Self {
            multi: true,
            ..Self::default()
        }
    }

    pub fn toggle_value(&mut self, value: String) {
        if self.multi {
            if let Some(pos) = self.selected.iter().position(|v| v == &value) {
                self.selected.remove(pos);
            } else {
                self.selected.push(value);
            }
            self.search.clear();
        } else {
            self.selected.clear();
            self.selected.push(value);
            self.open = false;
            self.search.clear();
        }
    }

    pub fn remove_value(&mut self, value: &str) {
        self.selected.retain(|v| v != value);
    }

    pub fn pop_last_chip(&mut self) -> bool {
        self.selected.pop().is_some()
    }

    pub fn filter<'a>(&self, options: &'a [UiComboboxOption]) -> Vec<&'a UiComboboxOption> {
        let q = self.search.text.as_str();
        let mut scored: Vec<(f32, &UiComboboxOption)> = options
            .iter()
            .filter_map(|o| {
                let kws: Vec<String> = o.detail.iter().cloned().collect();
                command_score(q, &o.label, &kws).map(|s| (s, o))
            })
            .collect();
        if !q.trim().is_empty() {
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        }
        scored.into_iter().map(|(_, o)| o).collect()
    }
}

type BindFn = Box<dyn FnOnce(El) -> El>;
type ValFn = Rc<dyn Fn(El, String) -> El>;

pub struct UiCombobox<'a> {
    key: String,
    state: &'a UiComboboxState,
    options: Vec<UiComboboxOption>,
    placeholder: String,
    width: f32,
    on_open: Option<BindFn>,
    on_close: Option<BindFn>,
    on_edit: Option<BindFn>,
    on_pick: Option<ValFn>,
    on_remove_chip: Option<ValFn>,
}

impl<'a> UiCombobox<'a> {
    pub fn new(key: impl Into<String>, state: &'a UiComboboxState) -> Self {
        Self {
            key: key.into(),
            state,
            options: Vec::new(),
            placeholder: "Search...".into(),
            width: 280.0,
            on_open: None,
            on_close: None,
            on_edit: None,
            on_pick: None,
            on_remove_chip: None,
        }
    }

    pub fn options(mut self, options: Vec<UiComboboxOption>) -> Self {
        self.options = options;
        self
    }

    pub fn placeholder(mut self, ph: impl Into<String>) -> Self {
        self.placeholder = ph.into();
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(160.0);
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut UiComboboxState + Clone + 'static,
    ) -> Self {
        let a1 = access.clone();
        self.on_open = Some(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| {
                let st = a1(s);
                st.open = true;
                st.search.focused = true;
            })
        }));
        let a2 = access.clone();
        self.on_close = Some(Box::new(move |e: El| {
            e.modal(true, move |s: &mut S| a2(s).open = false)
        }));
        let a3 = access.clone();
        self.on_edit = Some(Box::new(move |e: El| {
            e.on_edit(move |s: &mut S| &mut a3(s).search)
        }));
        let a4 = access.clone();
        self.on_pick = Some(Rc::new(move |e: El, val: String| {
            let ac = a4.clone();
            e.on_press(move |s: &mut S| ac(s).toggle_value(val.clone()))
        }));
        self.on_remove_chip = Some(Rc::new(move |e: El, val: String| {
            let ac = access.clone();
            e.on_press(move |s: &mut S| ac(s).remove_value(&val))
        }));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let anchor_key = format!("{}-combo-trigger", self.key);

        let mut chips = row().gap(4.0).wrap().center_y().grow(1.0).shrink();
        if self.state.multi {
            for val in &self.state.selected {
                let lbl = self
                    .options
                    .iter()
                    .find(|o| &o.value == val)
                    .map_or(val.as_str(), |o| o.label.as_str());
                let mut chip = row()
                    .px(8.0)
                    .py(3.0)
                    .gap(4.0)
                    .center_y()
                    .radius(theme.radii.pill)
                    .fill(theme.selected_fill)
                    .child(
                        text(lbl.to_owned())
                            .font(k.ty.small_medium)
                            .color(c.text_base),
                    );
                let mut x_btn = el()
                    .size(14.0, 14.0)
                    .center()
                    .role("button")
                    .label(format!("Remove {lbl}"))
                    .child(icon(k.glyph.close, 10.0).color(c.icon_muted));
                if let Some(rm) = &self.on_remove_chip {
                    x_btn = rm(x_btn, val.clone());
                }
                chip = chip.child(x_btn);
                chips = chips.child(chip);
            }
        } else if let Some(first) = self.state.selected.first() {
            let lbl = self
                .options
                .iter()
                .find(|o| &o.value == first)
                .map_or(first.as_str(), |o| o.label.as_str());
            chips = chips.child(text(lbl.to_owned()).font(k.ty.ui_medium).color(c.text_base));
        } else {
            chips = chips.child(
                text(self.placeholder.clone())
                    .font(k.ty.ui)
                    .color(c.text_muted),
            );
        }

        let mut trigger = row()
            .key(anchor_key.clone())
            .w(self.width)
            .min_h(36.0)
            .px(10.0)
            .py(4.0)
            .gap(6.0)
            .center_y()
            .role("combobox")
            .focusable(true)
            .child(chips)
            .child(icon(k.glyph.chevron_down, 13.0).color(c.icon_muted));

        trigger = theme.apply_focus(
            theme
                .control_surface(trigger, theme.radii.control)
                .hover(theme.hover_fill),
        );
        if let Some(open) = self.on_open {
            trigger = open(trigger);
        }

        if self.state.open {
            let mut input = text_input(&self.state.search, k.ty.ui_compact)
                .key(format!("{}-search", self.key))
                .single_line()
                .placeholder(self.placeholder, c.text_muted)
                .color(c.text_base)
                .grow(1.0);
            if let Some(bind) = self.on_edit {
                input = bind(input);
            }

            let search_row = row()
                .w_full()
                .h(36.0)
                .px(10.0)
                .gap(8.0)
                .center_y()
                .child(icon(k.glyph.search, 14.0).color(c.icon_muted))
                .child(input);

            let filtered = self.state.filter(&self.options);
            let mut rows = col()
                .w_full()
                .padding(4.0)
                .gap(2.0)
                .max_h(240.0)
                .role("listbox");
            if filtered.is_empty() {
                rows = rows.child(
                    el().w_full()
                        .py(18.0)
                        .center()
                        .child(text("No matches").font(k.ty.small).color(c.text_muted)),
                );
            } else {
                for (i, opt) in filtered.into_iter().enumerate() {
                    let chosen = self.state.selected.iter().any(|v| v == &opt.value);
                    let mut item = row()
                        .key(format!("{}-copt-{i}", self.key))
                        .w_full()
                        .h(32.0)
                        .px(8.0)
                        .gap(8.0)
                        .center_y()
                        .radius(theme.radii.control - 2.0)
                        .role("option")
                        .label(opt.label.clone())
                        .disabled(opt.disabled)
                        .fill(if i == self.state.active_index {
                            theme.selected_fill
                        } else {
                            Color::TRANSPARENT
                        })
                        .hover(theme.hover_fill)
                        .child(
                            text(opt.label.clone())
                                .font(k.ty.ui_compact)
                                .color(c.text_base),
                        )
                        .child(spacer());
                    if chosen {
                        item = item.child(icon(k.glyph.check, 13.0).color(c.text_accent));
                    }
                    if !opt.disabled
                        && let Some(pick) = &self.on_pick
                    {
                        item = pick(item, opt.value.clone());
                    }
                    rows = rows.child(item);
                }
            }

            let mut panel = col()
                .w(self.width)
                .child(search_row)
                .child(el().w_full().h(1.0).fill(theme.hairline))
                .child(rows);
            panel = theme.overlay_surface(panel, theme.radii.menu);
            if let Some(close) = self.on_close {
                panel = close(panel);
            }
            portals.push(
                LayerSlot::Dropdown,
                popover::anchored(k, panel, &anchor_key, Placement::BottomStart, 6.0),
            );
        }

        trigger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_combobox_toggles_and_filters() {
        let opts = vec![
            UiComboboxOption::new("fps", "FPS Counter"),
            UiComboboxOption::new("hdr", "Auto HDR"),
            UiComboboxOption::new("dlss", "DLSS Super Resolution"),
        ];
        let mut st = UiComboboxState::multi();
        st.toggle_value("fps".into());
        st.toggle_value("hdr".into());
        assert_eq!(st.selected, vec!["fps", "hdr"]);
        st.toggle_value("fps".into());
        assert_eq!(st.selected, vec!["hdr"]);

        st.search.text = "dlss".into();
        let filtered = st.filter(&opts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].value, "dlss");
    }
}
