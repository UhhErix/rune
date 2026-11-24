//! Radix-style DropdownMenu and ContextMenu supporting submenus, checkbox items,
//! radio items, shortcut keycaps, danger items, separators, and Liquid Glass surfaces.

use rune::{Color, El, Icon, PointerPress, col, el, icon, row, spacer, text};
use rune_kit::{Keybind, Placement, popover};

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

type ActFn = Box<dyn FnOnce(El) -> El>;

pub enum UiMenuEntry {
    Item(Box<UiMenuItem>),
    Separator,
    Label(String),
}

impl UiMenuEntry {
    pub fn item(item: UiMenuItem) -> Self {
        Self::Item(Box::new(item))
    }
}

pub struct UiMenuItem {
    pub label: String,
    pub icon: Option<&'static Icon>,
    pub shortcut: Option<String>,
    pub checked: Option<bool>,
    pub radio: Option<bool>,
    pub danger: bool,
    pub disabled: bool,
    pub submenu: Option<(Vec<UiMenuEntry>, bool)>,
    on_select: Option<ActFn>,
}

impl UiMenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            shortcut: None,
            checked: None,
            radio: None,
            danger: false,
            disabled: false,
            submenu: None,
            on_select: None,
        }
    }

    pub fn icon(mut self, glyph: &'static Icon) -> Self {
        self.icon = Some(glyph);
        self
    }

    pub fn shortcut(mut self, sc: impl Into<String>) -> Self {
        self.shortcut = Some(sc.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn radio(mut self, selected: bool) -> Self {
        self.radio = Some(selected);
        self
    }

    pub fn danger(mut self, danger: bool) -> Self {
        self.danger = danger;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn submenu(mut self, entries: Vec<UiMenuEntry>, open: bool) -> Self {
        self.submenu = Some((entries, open));
        self
    }

    pub fn on_select<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }
}

fn render_entries(key: &str, entries: Vec<UiMenuEntry>, theme: &UiTheme, min_w: f32) -> El {
    let k = &theme.kit;
    let c = &k.c;
    let mut list = col().w(min_w).padding(4.0).gap(2.0).role("menu");

    for (i, entry) in entries.into_iter().enumerate() {
        match entry {
            UiMenuEntry::Separator => {
                list = list.child(el().w_full().h(1.0).my(4.0).fill(theme.hairline));
            }
            UiMenuEntry::Label(lbl) => {
                list = list.child(
                    el().px(10.0)
                        .py(4.0)
                        .child(text(lbl).font(k.ty.small_medium).color(c.text_muted)),
                );
            }
            UiMenuEntry::Item(item) => {
                let fg = if item.danger {
                    c.danger.fg
                } else {
                    c.text_base
                };
                let role = if item.checked.is_some() {
                    "menuitemcheckbox"
                } else if item.radio.is_some() {
                    "menuitemradio"
                } else {
                    "menuitem"
                };

                let mut r = row()
                    .key(format!("{key}-mi-{i}"))
                    .w_full()
                    .h(32.0)
                    .px(10.0)
                    .gap(8.0)
                    .center_y()
                    .radius(theme.radii.control - 2.0)
                    .role(role)
                    .label(item.label.clone())
                    .disabled(item.disabled)
                    .focusable(!item.disabled)
                    .hover(theme.hover_fill);

                if let Some(chk) = item.checked {
                    r = r.checked(chk).child(
                        el().size(14.0, 14.0).center().child(
                            icon(k.glyph.check, 12.0)
                                .color(c.text_accent)
                                .opacity(if chk { 1.0 } else { 0.0 }),
                        ),
                    );
                } else if let Some(sel) = item.radio {
                    r = r.checked(sel).child(
                        el().size(14.0, 14.0).center().child(
                            el().size(6.0, 6.0)
                                .radius(theme.radii.pill)
                                .fill(c.text_accent)
                                .opacity(if sel { 1.0 } else { 0.0 }),
                        ),
                    );
                } else if let Some(glyph) = item.icon {
                    r = r.child(icon(glyph, 14.0).color(fg));
                }

                r = r
                    .child(text(item.label).font(k.ty.ui_compact).color(fg))
                    .child(spacer());

                if let Some(sc) = item.shortcut {
                    r = r.child(Keybind::new(&[sc.as_str()]).build(k));
                }
                if item.submenu.is_some() {
                    r = r.child(icon(k.glyph.caret_right, 11.0).color(c.icon_muted));
                }

                if item.disabled {
                    r = r.opacity(0.45);
                } else {
                    r = theme.apply_focus(r);
                    if let Some(cb) = item.on_select {
                        r = cb(r);
                    }
                }
                list = list.child(r);
            }
        }
    }

    theme.overlay_surface(list, theme.radii.menu)
}

/// Anchored dropdown menu triggered by a control.
pub struct DropdownMenu {
    key: String,
    trigger: El,
    entries: Vec<UiMenuEntry>,
    open: bool,
    placement: Placement,
    min_width: f32,
    on_close: Option<ActFn>,
}

impl DropdownMenu {
    pub fn new(key: impl Into<String>, trigger: El) -> Self {
        Self {
            key: key.into(),
            trigger,
            entries: Vec::new(),
            open: false,
            placement: Placement::BottomStart,
            min_width: 210.0,
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

    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w.max(140.0);
        self
    }

    pub fn entries(mut self, entries: Vec<UiMenuEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(move |e: El| e.modal(true, f)));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        let anchor_key = format!("{}-dd-trigger", self.key);
        let trigger = el().key(anchor_key.clone()).child(self.trigger);
        if self.open {
            let mut panel = render_entries(&self.key, self.entries, theme, self.min_width);
            if let Some(close) = self.on_close {
                panel = close(panel);
            }
            let anchored = popover::anchored(&theme.kit, panel, &anchor_key, self.placement, 6.0);
            portals.push(LayerSlot::Dropdown, anchored);
        }
        trigger
    }
}

/// Right-click context menu anchored at the secondary-press pointer location.
pub struct ContextMenu {
    key: String,
    target: El,
    at: Option<PointerPress>,
    entries: Vec<UiMenuEntry>,
    min_width: f32,
    on_close: Option<ActFn>,
}

impl ContextMenu {
    pub fn new(key: impl Into<String>, target: El, at: Option<PointerPress>) -> Self {
        Self {
            key: key.into(),
            target,
            at,
            entries: Vec::new(),
            min_width: 210.0,
            on_close: None,
        }
    }

    pub fn entries(mut self, entries: Vec<UiMenuEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w.max(140.0);
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Box::new(move |e: El| e.modal(true, f)));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        if let Some(pt) = self.at {
            let mut panel =
                render_entries(&self.key, self.entries, theme, self.min_width).absolute(pt.x, pt.y);
            if let Some(close) = self.on_close {
                panel = close(panel);
            }
            portals.push(LayerSlot::Dropdown, panel);
        }
        let _ = Color::TRANSPARENT;
        self.target
    }
}
