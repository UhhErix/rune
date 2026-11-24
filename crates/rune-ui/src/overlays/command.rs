//! cmdk-inspired Command Palette with fuzzy `command-score` ranking, dynamic group visibility,
//! nested pages with Backspace pop, shortcut keycaps, and full keyboard/controller navigation.

use std::rc::Rc;

use rune::{
    Color, El, Icon, Key, KeyEvent, TextEdit, col, el, icon, row, spacer, text, text_input,
};
use rune_kit::Keybind;

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

/// Scores `candidate` (and optional `keywords`) against `query` following cmdk's `command-score`
/// weighting: exact prefix, contiguous runs, word-boundary/camelCase hits, and acronym matches.
/// Returns `None` when `query` is not a subsequence of the candidate or its keywords.
pub fn command_score(query: &str, candidate: &str, keywords: &[String]) -> Option<f32> {
    let q = query.trim();
    if q.is_empty() {
        return Some(1.0);
    }
    let mut best = score_single(q, candidate);
    for kw in keywords {
        if let Some(s) = score_single(q, kw) {
            let weighted = s * 0.85;
            best = Some(best.map_or(weighted, |b| b.max(weighted)));
        }
    }
    best
}

fn score_single(query: &str, target: &str) -> Option<f32> {
    let q_chars: Vec<char> = query.to_lowercase().chars().collect();
    let t_orig: Vec<char> = target.chars().collect();
    let t_lower: Vec<char> = target.to_lowercase().chars().collect();
    if q_chars.is_empty() {
        return Some(1.0);
    }
    if t_lower.is_empty() || q_chars.len() > t_lower.len() {
        return None;
    }

    let mut qi = 0;
    let mut score = 0.0f32;
    let mut prev_match: Option<usize> = None;
    let mut first_match: Option<usize> = None;

    for (ti, &tc) in t_lower.iter().enumerate() {
        if qi < q_chars.len() && tc == q_chars[qi] {
            if first_match.is_none() {
                first_match = Some(ti);
            }
            let contiguous = prev_match.is_some_and(|p| p + 1 == ti);
            let boundary = ti == 0
                || !t_orig[ti - 1].is_alphanumeric()
                || (t_orig[ti].is_uppercase() && t_orig[ti - 1].is_lowercase());
            score += 10.0;
            if contiguous {
                score += 14.0;
            }
            if boundary {
                score += 12.0;
            }
            if ti == 0 && qi == 0 {
                score += 8.0;
            }
            prev_match = Some(ti);
            qi += 1;
        }
    }
    if qi < q_chars.len() {
        return None;
    }
    let span = prev_match
        .unwrap_or(0)
        .saturating_sub(first_match.unwrap_or(0))
        + 1;
    let tightness = (q_chars.len() as f32 / span as f32).clamp(0.0, 1.0);
    let coverage = (q_chars.len() as f32 / t_lower.len() as f32).clamp(0.0, 1.0);
    Some(score + tightness * 15.0 + coverage * 10.0)
}

/// One selectable command item inside a [`CommandGroup`].
#[derive(Clone, Debug)]
pub struct CommandItem {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub shortcut: Option<String>,
    pub icon: Option<&'static Icon>,
    pub disabled: bool,
    pub pushes_page: Option<String>,
}

impl CommandItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            keywords: Vec::new(),
            shortcut: None,
            icon: None,
            disabled: false,
            pushes_page: None,
        }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    pub fn keywords(mut self, kws: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.keywords = kws.into_iter().map(Into::into).collect();
        self
    }

    pub fn shortcut(mut self, sc: impl Into<String>) -> Self {
        self.shortcut = Some(sc.into());
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

    pub fn pushes_page(mut self, page_id: impl Into<String>) -> Self {
        self.pushes_page = Some(page_id.into());
        self
    }
}

/// Logical group of command items (hidden automatically when none of its items match).
#[derive(Clone, Debug)]
pub struct CommandGroup {
    pub heading: String,
    pub items: Vec<CommandItem>,
}

impl CommandGroup {
    pub fn new(heading: impl Into<String>, items: Vec<CommandItem>) -> Self {
        Self {
            heading: heading.into(),
            items,
        }
    }
}

/// Owned cmdk command palette state: open/closed, search input, active row, and page stack.
#[derive(Clone, Debug, Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub search: TextEdit,
    pub pages: Vec<String>,
    pub active_index: usize,
}

impl CommandPaletteState {
    pub fn open_root(&mut self) {
        self.open = true;
        self.pages.clear();
        self.search.clear();
        self.search.focused = true;
        self.active_index = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.search.focused = false;
    }

    pub fn current_page(&self) -> Option<&str> {
        self.pages.last().map(String::as_str)
    }

    pub fn push_page(&mut self, page: impl Into<String>) {
        self.pages.push(page.into());
        self.search.clear();
        self.active_index = 0;
    }

    pub fn pop_page(&mut self) -> bool {
        if self.pages.pop().is_some() {
            self.search.clear();
            self.active_index = 0;
            true
        } else {
            false
        }
    }

    /// Filters and ranks `groups` against the current search text, dropping empty groups.
    pub fn filter_groups<'a>(
        &self,
        groups: &'a [CommandGroup],
    ) -> Vec<(&'a str, Vec<&'a CommandItem>)> {
        let query = self.search.text.as_str();
        let mut out = Vec::new();
        for g in groups {
            let mut scored: Vec<(f32, &CommandItem)> = g
                .items
                .iter()
                .filter_map(|item| {
                    command_score(query, &item.label, &item.keywords).map(|s| (s, item))
                })
                .collect();
            if !query.trim().is_empty() {
                scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            }
            if !scored.is_empty() {
                out.push((
                    g.heading.as_str(),
                    scored.into_iter().map(|(_, i)| i).collect(),
                ));
            }
        }
        out
    }

    /// Handles cmdk keyboard navigation (`Up`/`Down`/`Ctrl+N`/`Ctrl+P`/`Home`/`End`/`Enter`/
    /// `Escape`/empty-`Backspace` page pop). Returns `Some(selected_id)` on Enter.
    pub fn handle_key(&mut self, ev: &KeyEvent, flat_items: &[&CommandItem]) -> Option<String> {
        let len = flat_items.len();
        match &ev.key {
            Key::Escape => {
                if !self.pop_page() {
                    self.close();
                }
                None
            }
            Key::Backspace if self.search.text.is_empty() && !self.pages.is_empty() => {
                self.pop_page();
                None
            }
            Key::Down => {
                if len > 0 {
                    self.active_index = (self.active_index + 1) % len;
                }
                None
            }
            Key::Up => {
                if len > 0 {
                    self.active_index = (self.active_index + len - 1) % len;
                }
                None
            }
            Key::Text(t) if ev.mods.ctrl && t == "n" => {
                if len > 0 {
                    self.active_index = (self.active_index + 1) % len;
                }
                None
            }
            Key::Text(t) if ev.mods.ctrl && t == "p" => {
                if len > 0 {
                    self.active_index = (self.active_index + len - 1) % len;
                }
                None
            }
            Key::Home if self.search.text.is_empty() => {
                self.active_index = 0;
                None
            }
            Key::End if self.search.text.is_empty() && len > 0 => {
                self.active_index = len - 1;
                None
            }
            Key::Enter => {
                let item = flat_items.get(self.active_index)?;
                if item.disabled {
                    return None;
                }
                if let Some(page) = &item.pushes_page {
                    self.push_page(page.clone());
                    None
                } else {
                    let id = item.id.clone();
                    self.close();
                    Some(id)
                }
            }
            _ => None,
        }
    }
}

type SelectHandler = Rc<dyn Fn(El, String) -> El>;
type BindFn = Box<dyn FnOnce(El) -> El>;

/// cmdk-style Command Palette modal overlay builder.
pub struct CommandPalette<'a> {
    key: String,
    state: &'a CommandPaletteState,
    placeholder: String,
    empty_label: String,
    groups: Vec<CommandGroup>,
    width: f32,
    on_edit: Option<BindFn>,
    on_close: Option<BindFn>,
    on_select: Option<SelectHandler>,
}

impl<'a> CommandPalette<'a> {
    pub fn new(key: impl Into<String>, state: &'a CommandPaletteState) -> Self {
        Self {
            key: key.into(),
            state,
            placeholder: "Type a command or search...".into(),
            empty_label: "No results found.".into(),
            groups: Vec::new(),
            width: 560.0,
            on_edit: None,
            on_close: None,
            on_select: None,
        }
    }

    pub fn placeholder(mut self, ph: impl Into<String>) -> Self {
        self.placeholder = ph.into();
        self
    }

    pub fn empty_label(mut self, label: impl Into<String>) -> Self {
        self.empty_label = label.into();
        self
    }

    pub fn groups(mut self, groups: Vec<CommandGroup>) -> Self {
        self.groups = groups;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(320.0);
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut CommandPaletteState + Clone + 'static,
        on_select: impl Fn(&mut S, String) + Clone + 'static,
    ) -> Self {
        let acc_edit = access.clone();
        self.on_edit = Some(Box::new(move |e: El| {
            e.on_edit(move |s: &mut S| &mut acc_edit(s).search)
        }));
        let acc_close = access.clone();
        self.on_close = Some(Box::new(move |e: El| {
            e.modal(true, move |s: &mut S| acc_close(s).close())
        }));
        let acc_sel = access;
        self.on_select = Some(Rc::new(move |e: El, id: String| {
            let acc_c = acc_sel.clone();
            let sel_c = on_select.clone();
            e.on_press(move |s: &mut S| {
                acc_c(s).close();
                sel_c(s, id.clone());
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) {
        if !self.state.open {
            return;
        }
        let k = &theme.kit;
        let c = &k.c;
        let filtered = self.state.filter_groups(&self.groups);

        let mut search_field = text_input(&self.state.search, k.ty.ui)
            .key(format!("{}-search", self.key))
            .single_line()
            .placeholder(self.placeholder, c.text_muted)
            .color(c.text_base)
            .grow(1.0)
            .shrink();
        if let Some(bind) = self.on_edit {
            search_field = bind(search_field);
        }

        let mut search_bar = row()
            .w_full()
            .h(46.0)
            .px(14.0)
            .gap(10.0)
            .center_y()
            .child(icon(k.glyph.search, 16.0).color(c.icon_muted));

        for page in &self.state.pages {
            search_bar = search_bar.child(
                el().px(8.0)
                    .py(3.0)
                    .radius(theme.radii.control)
                    .fill(theme.selected_fill)
                    .child(
                        text(page.clone())
                            .font(k.ty.small_medium)
                            .color(c.text_base),
                    ),
            );
        }
        search_bar = search_bar.child(search_field);

        let mut list = col().w_full().padding(8.0).gap(4.0).max_h(340.0);
        if filtered.is_empty() {
            list = list.child(
                el().w_full()
                    .py(28.0)
                    .center()
                    .child(text(self.empty_label).font(k.ty.ui).color(c.text_muted)),
            );
        } else {
            let mut flat_idx = 0usize;
            for (heading, items) in filtered {
                if !heading.is_empty() {
                    list = list.child(
                        el().px(10.0).pt(6.0).pb(2.0).child(
                            text(heading.to_owned())
                                .font(k.ty.small_medium)
                                .color(c.text_muted),
                        ),
                    );
                }
                for item in items {
                    let is_active = flat_idx == self.state.active_index;
                    let mut item_row = row()
                        .key(format!("{}-item-{}", self.key, item.id))
                        .w_full()
                        .h(36.0)
                        .px(10.0)
                        .gap(10.0)
                        .center_y()
                        .radius(theme.radii.control)
                        .role("option")
                        .label(item.label.clone())
                        .disabled(item.disabled)
                        .fill(if is_active {
                            theme.selected_fill
                        } else {
                            Color::TRANSPARENT
                        })
                        .hover(theme.hover_fill);

                    if let Some(glyph) = item.icon {
                        item_row = item_row.child(icon(glyph, 15.0).color(c.icon_base));
                    }
                    let mut copy = col().gap(1.0).child(
                        text(item.label.clone())
                            .font(k.ty.ui_medium)
                            .color(c.text_base),
                    );
                    if let Some(desc) = &item.description {
                        copy = copy.child(text(desc.clone()).font(k.ty.small).color(c.text_muted));
                    }
                    item_row = item_row.child(copy).child(spacer());
                    if let Some(sc) = &item.shortcut {
                        item_row = item_row.child(Keybind::new(&[sc.as_str()]).build(k));
                    }
                    if item.disabled {
                        item_row = item_row.opacity(0.45);
                    } else if let Some(sel) = &self.on_select {
                        item_row = sel(item_row, item.id.clone());
                    }
                    list = list.child(item_row);
                    flat_idx += 1;
                }
            }
        }

        let footer = row()
            .w_full()
            .h(32.0)
            .px(14.0)
            .gap(12.0)
            .center_y()
            .child(text("↑↓ Navigate").font(k.ty.small).color(c.text_muted))
            .child(text("↵ Open").font(k.ty.small).color(c.text_muted))
            .child(text("Esc Close").font(k.ty.small).color(c.text_muted));

        let mut panel = col()
            .key(format!("{}-cmdk", self.key))
            .w(self.width)
            .role("dialog")
            .label("Command palette")
            .child(search_bar)
            .child(el().w_full().h(1.0).fill(theme.hairline))
            .child(list)
            .child(el().w_full().h(1.0).fill(theme.hairline))
            .child(footer);

        panel = theme.overlay_surface(panel, theme.radii.panel);
        if let Some(close) = self.on_close {
            panel = close(panel);
        } else {
            panel = panel.modal_trap();
        }

        let scrim = el()
            .key(format!("{}-scrim", self.key))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(Color::BLACK.alpha(0.5))
            .a11y_hidden();

        let wrapper = col()
            .key(format!("{}-layer", self.key))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .pt(96.0)
            .items_center()
            .child(panel);

        portals.push(LayerSlot::Modal, scrim);
        portals.push(LayerSlot::Modal, wrapper);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_score_ranks_prefix_and_word_boundaries_highest() {
        let exact = command_score("set", "Settings", &[]).unwrap_or(0.0);
        let loose = command_score("set", "Share External Theme", &[]).unwrap_or(0.0);
        assert!(exact > loose);
        assert!(command_score("xyz", "Settings", &[]).is_none());
        assert!(command_score("gpu", "Graphics", &["gpu tuning".into()]).is_some());
    }

    #[test]
    fn command_palette_hides_empty_groups_and_pops_nested_pages() {
        let mut st = CommandPaletteState::default();
        st.open_root();
        let groups = vec![
            CommandGroup::new("Games", vec![CommandItem::new("play", "Launch Balatro")]),
            CommandGroup::new("System", vec![CommandItem::new("hdr", "Toggle HDR")]),
        ];
        st.search.text = "hdr".into();
        let filtered = st.filter_groups(&groups);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "System");

        st.push_page("Display");
        assert_eq!(st.current_page(), Some("Display"));
        assert!(st.pop_page());
        assert_eq!(st.current_page(), None);
    }
}
