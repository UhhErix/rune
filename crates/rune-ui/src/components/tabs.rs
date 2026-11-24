//! Radix / HeroUI / Framer Motion Tabs with smooth shared-element (`El::matched`) active
//! indicator, horizontal/vertical orientation, automatic or manual activation, and roving arrows.

use std::rc::Rc;

use rune::{Color, El, Icon, Key, KeyEvent, Tx, col, el, icon, row, text};

use crate::primitives::focus::{RovingOrientation, RovingState};
use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TabVariant {
    #[default]
    Underline,
    Pill,
    Segmented,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TabActivation {
    #[default]
    Automatic,
    Manual,
}

#[derive(Clone, Debug)]
pub struct UiTabItem {
    pub id: String,
    pub label: String,
    pub icon: Option<&'static Icon>,
    pub badge: Option<String>,
    pub disabled: bool,
}

impl UiTabItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            badge: None,
            disabled: false,
        }
    }
    pub fn icon(mut self, glyph: &'static Icon) -> Self {
        self.icon = Some(glyph);
        self
    }
    pub fn badge(mut self, b: impl Into<String>) -> Self {
        self.badge = Some(b.into());
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

type TabPickFn = Rc<dyn Fn(El, String) -> El>;
type GroupKeyFn = Box<dyn FnOnce(El) -> El>;

pub struct UiTabs {
    key: String,
    items: Vec<UiTabItem>,
    selected: String,
    variant: TabVariant,
    orientation: RovingOrientation,
    activation: TabActivation,
    panel: Option<El>,
    on_select: Option<TabPickFn>,
    on_key: Option<GroupKeyFn>,
}

impl UiTabs {
    pub fn new(key: impl Into<String>, selected: impl Into<String>, items: Vec<UiTabItem>) -> Self {
        Self {
            key: key.into(),
            items,
            selected: selected.into(),
            variant: TabVariant::Underline,
            orientation: RovingOrientation::Horizontal,
            activation: TabActivation::Automatic,
            panel: None,
            on_select: None,
            on_key: None,
        }
    }

    pub fn variant(mut self, v: TabVariant) -> Self {
        self.variant = v;
        self
    }

    pub fn vertical(mut self) -> Self {
        self.orientation = RovingOrientation::Vertical;
        self
    }

    pub fn activation(mut self, mode: TabActivation) -> Self {
        self.activation = mode;
        self
    }

    pub fn panel(mut self, panel: El) -> Self {
        self.panel = Some(panel);
        self
    }

    pub fn on_change<S: 'static>(mut self, f: impl Fn(&mut S, String) + Clone + 'static) -> Self {
        let f_click = f.clone();
        self.on_select = Some(Rc::new(move |e: El, id: String| {
            let fc = f_click.clone();
            e.on_press(move |s: &mut S| fc(s, id.clone()))
        }));

        let ids: Vec<String> = self.items.iter().map(|i| i.id.clone()).collect();
        let disabled: Vec<bool> = self.items.iter().map(|i| i.disabled).collect();
        let active = ids.iter().position(|id| id == &self.selected).unwrap_or(0);
        let orientation = self.orientation;
        let auto = self.activation == TabActivation::Automatic;

        self.on_key = Some(Box::new(move |e: El| {
            e.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                let mut roving = RovingState {
                    active,
                    orientation,
                    loop_nav: true,
                };
                if let Some(next) = roving.handle_key(ev, &disabled) {
                    if auto && let Some(id) = ids.get(next) {
                        f(s, id.clone());
                    }
                    return true;
                }
                if matches!(ev.key, Key::Enter)
                    && !auto
                    && let Some(id) = ids.get(active)
                {
                    f(s, id.clone());
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
        let indicator_id = format!("{}-tab-indicator", self.key);
        let mut triggers = Vec::with_capacity(self.items.len());

        for item in self.items {
            let is_active = item.id == self.selected;
            let fg = if is_active { c.text_base } else { c.text_muted };

            let mut label_row = row().center().gap(6.0);
            if let Some(glyph) = item.icon {
                label_row = label_row.child(icon(glyph, 14.0).color(fg));
            }
            label_row = label_row.child(text(item.label.clone()).font(k.ty.ui_medium).color(fg));
            if let Some(b) = item.badge {
                label_row = label_row.child(
                    el().px(6.0)
                        .py(1.0)
                        .radius(theme.radii.pill)
                        .fill(theme.selected_fill)
                        .child(text(b).font(k.ty.small).color(c.text_muted)),
                );
            }

            let mut tab_el = col()
                .key(format!("{}-tab-{}", self.key, item.id))
                .px(12.0)
                .h(34.0)
                .center()
                .role("tab")
                .label(item.label)
                .focus_selected(is_active)
                .disabled(item.disabled)
                .focusable(!item.disabled);

            match self.variant {
                TabVariant::Underline => {
                    tab_el = tab_el.child(label_row);
                    if is_active {
                        tab_el = tab_el.child(
                            el().key(format!("{}-u-{}", self.key, item.id))
                                .matched(indicator_id.clone())
                                .w_full()
                                .h(2.0)
                                .radius(theme.radii.pill)
                                .fill(c.bg_accent)
                                .motion(theme.springs.move_),
                        );
                    }
                }
                TabVariant::Pill | TabVariant::Segmented => {
                    let rad = if self.variant == TabVariant::Pill {
                        theme.radii.pill
                    } else {
                        theme.radii.control
                    };
                    if is_active {
                        tab_el = tab_el
                            .radius(rad)
                            .child(
                                el().key(format!("{}-bg-{}", self.key, item.id))
                                    .matched(indicator_id.clone())
                                    .absolute(0.0, 0.0)
                                    .w_full()
                                    .h_full()
                                    .radius(rad)
                                    .fill(theme.selected_fill)
                                    .motion(theme.springs.move_),
                            )
                            .child(label_row);
                    } else {
                        tab_el = tab_el.radius(rad).hover(theme.hover_fill).child(label_row);
                    }
                }
            }

            if item.disabled {
                tab_el = tab_el.opacity(0.45);
            } else {
                tab_el = theme.apply_focus(tab_el);
                if let Some(pick) = &self.on_select {
                    tab_el = pick(tab_el, item.id);
                }
            }
            triggers.push(tab_el);
        }

        let mut list = match self.orientation {
            RovingOrientation::Horizontal | RovingOrientation::Both => row().gap(4.0).center_y(),
            RovingOrientation::Vertical => col().gap(4.0),
        }
        .role("tablist")
        .focus_group()
        .children(triggers);

        if self.variant == TabVariant::Segmented {
            list = list
                .padding(3.0)
                .radius(theme.radii.control + 2.0)
                .fill(c.bg_deep)
                .border(1.0, theme.hairline);
        }
        if let Some(gk) = self.on_key {
            list = gk(list);
        }

        let mut root = match self.orientation {
            RovingOrientation::Horizontal | RovingOrientation::Both => col().gap(12.0),
            RovingOrientation::Vertical => row().gap(16.0).items_start(),
        }
        .key(self.key)
        .child(list);

        if self.variant == TabVariant::Underline
            && self.orientation == RovingOrientation::Horizontal
        {
            root = root.child(el().w_full().h(1.0).fill(theme.hairline));
        }
        if let Some(panel) = self.panel {
            root = root.child(el().role("tabpanel").w_full().child(panel));
        }
        let _ = Color::TRANSPARENT;
        root
    }
}
