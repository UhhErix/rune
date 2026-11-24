//! Radix-style Accordion supporting `Single { collapsible }` and `Multiple` modes with
//! smooth spring transitions and keyboard navigation.

use std::collections::HashSet;
use std::rc::Rc;

use rune::{El, col, el, icon, row, spacer, text};

use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccordionMode {
    Single { collapsible: bool },
    Multiple,
}

impl Default for AccordionMode {
    fn default() -> Self {
        Self::Single { collapsible: true }
    }
}

/// Owned accordion state supporting Single (with optional collapsible) and Multiple modes.
#[derive(Clone, Debug, Default)]
pub struct AccordionState {
    pub mode: AccordionMode,
    pub expanded: HashSet<String>,
}

impl AccordionState {
    pub fn new(mode: AccordionMode) -> Self {
        Self {
            mode,
            expanded: HashSet::new(),
        }
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.expanded.contains(id)
    }

    pub fn toggle(&mut self, id: &str) {
        let was_open = self.expanded.contains(id);
        match self.mode {
            AccordionMode::Single { collapsible } => {
                if was_open {
                    if collapsible {
                        self.expanded.remove(id);
                    }
                } else {
                    self.expanded.clear();
                    self.expanded.insert(id.to_owned());
                }
            }
            AccordionMode::Multiple => {
                if was_open {
                    self.expanded.remove(id);
                } else {
                    self.expanded.insert(id.to_owned());
                }
            }
        }
    }
}

pub struct UiAccordionItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub content: El,
    pub disabled: bool,
}

impl UiAccordionItem {
    pub fn new(id: impl Into<String>, title: impl Into<String>, content: El) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            content,
            disabled: false,
        }
    }
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
}

type ToggleFn = Rc<dyn Fn(El, String) -> El>;

pub struct UiAccordion<'a> {
    key: String,
    state: &'a AccordionState,
    items: Vec<UiAccordionItem>,
    on_toggle: Option<ToggleFn>,
}

impl<'a> UiAccordion<'a> {
    pub fn new(key: impl Into<String>, state: &'a AccordionState) -> Self {
        Self {
            key: key.into(),
            state,
            items: Vec::new(),
            on_toggle: None,
        }
    }

    pub fn item(mut self, item: UiAccordionItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn items(mut self, items: impl IntoIterator<Item = UiAccordionItem>) -> Self {
        self.items.extend(items);
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut AccordionState + Clone + 'static,
    ) -> Self {
        self.on_toggle = Some(Rc::new(move |e: El, id: String| {
            let acc = access.clone();
            e.on_press(move |s: &mut S| acc(s).toggle(&id))
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let mut list = col().key(self.key.clone()).w_full().focus_group();

        for (i, item) in self.items.into_iter().enumerate() {
            let open = self.state.is_open(&item.id);
            let chev = if open {
                k.glyph.chevron_up
            } else {
                k.glyph.chevron_down
            };

            let mut head_copy = col().gap(2.0).child(
                text(item.title.clone())
                    .font(k.ty.ui_medium)
                    .color(c.text_base),
            );
            if let Some(sub) = item.subtitle {
                head_copy = head_copy.child(text(sub).font(k.ty.small).color(c.text_muted));
            }

            let mut header = row()
                .key(format!("{}-acc-h-{i}", self.key))
                .w_full()
                .py(12.0)
                .px(8.0)
                .gap(10.0)
                .center_y()
                .radius(theme.radii.control)
                .role("button")
                .label(item.title)
                .disabled(item.disabled)
                .focusable(!item.disabled)
                .hover(theme.hover_fill)
                .child(head_copy)
                .child(spacer())
                .child(icon(chev, 13.0).color(c.icon_muted));

            if item.disabled {
                header = header.opacity(0.45);
            } else {
                header = theme.apply_focus(header);
                if let Some(tog) = &self.on_toggle {
                    header = tog(header, item.id.clone());
                }
            }

            let mut section = col().w_full().child(header);
            if open {
                section = section.child(
                    el().key(format!("{}-acc-b-{i}", self.key))
                        .w_full()
                        .px(8.0)
                        .pb(12.0)
                        .motion(theme.springs.base)
                        .child(item.content),
                );
            }
            section = section.child(el().w_full().h(1.0).fill(theme.hairline));
            list = list.child(section);
        }

        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accordion_single_collapsible_and_multiple_modes() {
        let mut s = AccordionState::new(AccordionMode::Single { collapsible: false });
        s.toggle("a");
        assert!(s.is_open("a"));
        s.toggle("a");
        assert!(s.is_open("a"));
        s.toggle("b");
        assert!(!s.is_open("a") && s.is_open("b"));

        let mut m = AccordionState::new(AccordionMode::Multiple);
        m.toggle("a");
        m.toggle("b");
        assert!(m.is_open("a") && m.is_open("b"));
    }
}
