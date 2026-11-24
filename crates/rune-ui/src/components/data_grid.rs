//! Mantine / shadcn / TanStack-style DataGrid with column sorting, search filter,
//! pagination, row selection + select-all tri-state checkbox, density, and skeleton loading.

use std::collections::HashSet;
use std::rc::Rc;

use rune::{Align, Color, El, col, el, icon, row, spacer, text};
use rune_kit::SortDir;

use crate::controls::toggle::{CheckState, UiCheckbox};
use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridDensity {
    Compact,
    #[default]
    Comfortable,
}

impl GridDensity {
    pub const fn row_height(self) -> f32 {
        match self {
            Self::Compact => 34.0,
            Self::Comfortable => 44.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DataGridColumn {
    pub id: String,
    pub title: String,
    pub width: Option<f32>,
    pub align: Align,
    pub sortable: bool,
}

impl DataGridColumn {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: None,
            align: Align::Start,
            sortable: false,
        }
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w.max(40.0));
        self
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }
}

#[derive(Clone, Debug)]
pub struct DataGridRow {
    pub id: String,
    pub cells: Vec<String>,
}

impl DataGridRow {
    pub fn new(id: impl Into<String>, cells: Vec<String>) -> Self {
        Self {
            id: id.into(),
            cells,
        }
    }
}

/// Owned DataGrid state: sort column/direction, filter text, page index, page size, selection.
#[derive(Clone, Debug)]
pub struct DataGridState {
    pub sort: Option<(String, SortDir)>,
    pub filter: String,
    pub page: usize,
    pub page_size: usize,
    pub selected: HashSet<String>,
    pub density: GridDensity,
}

impl Default for DataGridState {
    fn default() -> Self {
        Self {
            sort: None,
            filter: String::new(),
            page: 0,
            page_size: 10,
            selected: HashSet::new(),
            density: GridDensity::Comfortable,
        }
    }
}

impl DataGridState {
    pub fn toggle_sort(&mut self, col_id: &str) {
        self.sort = match &self.sort {
            Some((id, dir)) if id == col_id => Some((col_id.to_owned(), dir.flip())),
            _ => Some((col_id.to_owned(), SortDir::Ascending)),
        };
        self.page = 0;
    }

    pub fn toggle_row(&mut self, row_id: &str) {
        if !self.selected.remove(row_id) {
            self.selected.insert(row_id.to_owned());
        }
    }

    pub fn toggle_all(&mut self, row_ids: &[String]) {
        let all_selected =
            !row_ids.is_empty() && row_ids.iter().all(|id| self.selected.contains(id));
        if all_selected {
            for id in row_ids {
                self.selected.remove(id);
            }
        } else {
            for id in row_ids {
                self.selected.insert(id.clone());
            }
        }
    }

    pub fn select_all_state(&self, visible_ids: &[String]) -> CheckState {
        if visible_ids.is_empty() {
            return CheckState::Unchecked;
        }
        let count = visible_ids
            .iter()
            .filter(|id| self.selected.contains(*id))
            .count();
        if count == 0 {
            CheckState::Unchecked
        } else if count == visible_ids.len() {
            CheckState::Checked
        } else {
            CheckState::Indeterminate
        }
    }

    /// Applies filter, sort, and pagination slice to `rows`. Returns `(total_filtered, page_rows)`.
    pub fn query<'a>(
        &self,
        columns: &[DataGridColumn],
        rows: &'a [DataGridRow],
    ) -> (usize, Vec<&'a DataGridRow>) {
        let q = self.filter.trim().to_lowercase();
        let mut list: Vec<&DataGridRow> = rows
            .iter()
            .filter(|r| q.is_empty() || r.cells.iter().any(|c| c.to_lowercase().contains(&q)))
            .collect();

        if let Some((col_id, dir)) = &self.sort
            && let Some(ci) = columns.iter().position(|c| &c.id == col_id)
        {
            list.sort_by(|a, b| {
                let va = a.cells.get(ci).map_or("", String::as_str);
                let vb = b.cells.get(ci).map_or("", String::as_str);
                let ord = match (va.parse::<f64>(), vb.parse::<f64>()) {
                    (Ok(na), Ok(nb)) => na.total_cmp(&nb),
                    _ => va.to_lowercase().cmp(&vb.to_lowercase()),
                };
                dir.apply(ord)
            });
        }

        let total = list.len();
        let size = self.page_size.max(1);
        let max_page = total.saturating_sub(1) / size;
        let page = self.page.min(max_page);
        let start = page * size;
        let end = (start + size).min(total);
        (total, list[start..end].to_vec())
    }
}

type IdHandler = Rc<dyn Fn(El, String) -> El>;
type ToggleAllHandler = Rc<dyn Fn(El, Vec<String>) -> El>;

pub struct DataGrid<'a> {
    key: String,
    state: &'a DataGridState,
    columns: Vec<DataGridColumn>,
    rows: Vec<DataGridRow>,
    selectable: bool,
    striped: bool,
    loading: bool,
    empty_label: String,
    on_sort: Option<IdHandler>,
    on_toggle_row: Option<IdHandler>,
    on_toggle_all: Option<ToggleAllHandler>,
}

impl<'a> DataGrid<'a> {
    pub fn new(key: impl Into<String>, state: &'a DataGridState) -> Self {
        Self {
            key: key.into(),
            state,
            columns: Vec::new(),
            rows: Vec::new(),
            selectable: false,
            striped: false,
            loading: false,
            empty_label: "No rows to display.".into(),
            on_sort: None,
            on_toggle_row: None,
            on_toggle_all: None,
        }
    }

    pub fn columns(mut self, cols: Vec<DataGridColumn>) -> Self {
        self.columns = cols;
        self
    }

    pub fn rows(mut self, rows: Vec<DataGridRow>) -> Self {
        self.rows = rows;
        self
    }

    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = on;
        self
    }

    pub fn striped(mut self, on: bool) -> Self {
        self.striped = on;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn empty_label(mut self, lbl: impl Into<String>) -> Self {
        self.empty_label = lbl.into();
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut DataGridState + Clone + 'static,
    ) -> Self {
        let a1 = access.clone();
        self.on_sort = Some(Rc::new(move |e: El, col_id: String| {
            let ac = a1.clone();
            e.on_press(move |s: &mut S| ac(s).toggle_sort(&col_id))
        }));
        let a2 = access.clone();
        self.on_toggle_row = Some(Rc::new(move |e: El, row_id: String| {
            let ac = a2.clone();
            e.on_press(move |s: &mut S| ac(s).toggle_row(&row_id))
        }));
        self.on_toggle_all = Some(Rc::new(move |e: El, ids: Vec<String>| {
            let ac = access.clone();
            e.on_press(move |s: &mut S| ac(s).toggle_all(&ids))
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let row_h = self.state.density.row_height();
        let (_total, page_rows) = self.state.query(&self.columns, &self.rows);
        let visible_ids: Vec<String> = page_rows.iter().map(|r| r.id.clone()).collect();

        let mut hdr = row()
            .w_full()
            .h(38.0)
            .px(12.0)
            .gap(12.0)
            .center_y()
            .fill(c.bg_deep);

        if self.selectable {
            let check_state = self.state.select_all_state(&visible_ids);
            let mut cb = UiCheckbox::new(format!("{}-all", self.key), check_state).build(theme);
            if let Some(tog) = &self.on_toggle_all {
                cb = tog(cb, visible_ids);
            }
            hdr = hdr.child(el().w(28.0).center().child(cb));
        }

        for col_def in &self.columns {
            let active_sort = self
                .state
                .sort
                .as_ref()
                .filter(|(id, _)| id == &col_def.id)
                .map(|(_, d)| *d);

            let mut cell = row().gap(4.0).center_y().child(
                text(col_def.title.clone())
                    .font(k.ty.small_medium)
                    .color(c.text_muted),
            );

            if let Some(dir) = active_sort {
                let g = match dir {
                    SortDir::Ascending => k.glyph.chevron_up,
                    SortDir::Descending => k.glyph.chevron_down,
                };
                cell = cell.child(icon(g, 11.0).color(c.text_accent));
            }

            cell = match col_def.width {
                Some(w) => cell.w(w),
                None => cell.grow(1.0).shrink(),
            };
            if col_def.align == Align::End {
                cell = cell.justify_end();
            }
            if col_def.sortable {
                cell = cell.hover(theme.hover_fill).radius(4.0).px(4.0).py(2.0);
                if let Some(cb) = &self.on_sort {
                    cell = cb(cell, col_def.id.clone());
                }
            }
            hdr = hdr.child(cell);
        }

        let mut body = col().w_full();
        if self.loading {
            for i in 0..4 {
                body = body.child(
                    row()
                        .key(format!("{}-skel-{i}", self.key))
                        .w_full()
                        .h(row_h)
                        .px(12.0)
                        .gap(12.0)
                        .center_y()
                        .child(el().grow(1.0).h(12.0).radius(4.0).fill(c.border_muted)),
                );
            }
        } else if page_rows.is_empty() {
            body = body.child(
                el().w_full()
                    .py(28.0)
                    .center()
                    .child(text(self.empty_label).font(k.ty.ui).color(c.text_muted)),
            );
        } else {
            for (ri, r) in page_rows.into_iter().enumerate() {
                let selected = self.state.selected.contains(&r.id);
                let bg = if selected {
                    theme.selected_fill
                } else if self.striped && ri % 2 == 1 {
                    c.bg_deep
                } else {
                    Color::TRANSPARENT
                };

                let mut row_el = row()
                    .key(format!("{}-r-{}", self.key, r.id))
                    .w_full()
                    .h(row_h)
                    .px(12.0)
                    .gap(12.0)
                    .center_y()
                    .fill(bg)
                    .hover(theme.hover_fill);

                if self.selectable {
                    let cb = UiCheckbox::new(
                        format!("{}-cb-{}", self.key, r.id),
                        CheckState::from_bool(selected),
                    )
                    .build(theme);
                    row_el = row_el.child(el().w(28.0).center().child(cb));
                }

                for (ci, col_def) in self.columns.iter().enumerate() {
                    let val = r.cells.get(ci).cloned().unwrap_or_default();
                    let mut cell = row().center_y().child(
                        text(val)
                            .font(k.ty.ui_compact)
                            .color(c.text_base)
                            .truncate(),
                    );
                    cell = match col_def.width {
                        Some(w) => cell.w(w),
                        None => cell.grow(1.0).shrink(),
                    };
                    if col_def.align == Align::End {
                        cell = cell.justify_end();
                    }
                    row_el = row_el.child(cell);
                }

                if self.selectable
                    && let Some(cb) = &self.on_toggle_row
                {
                    row_el = cb(row_el, r.id.clone());
                }
                body = body
                    .child(row_el)
                    .child(el().w_full().h(1.0).fill(theme.hairline));
            }
        }

        let _ = spacer();
        col()
            .key(self.key)
            .w_full()
            .role("grid")
            .radius(theme.radii.card)
            .border(1.0, theme.float_line)
            .child(hdr)
            .child(el().w_full().h(1.0).fill(theme.hairline))
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_grid_filters_sorts_numerically_and_paginates() {
        let cols = vec![
            DataGridColumn::new("name", "Name").sortable(),
            DataGridColumn::new("hours", "Hours").sortable(),
        ];
        let rows = vec![
            DataGridRow::new("1", vec!["Hades".into(), "42".into()]),
            DataGridRow::new("2", vec!["Elden Ring".into(), "120".into()]),
            DataGridRow::new("3", vec!["Hollow Knight".into(), "85".into()]),
        ];
        let mut st = DataGridState {
            page_size: 2,
            ..DataGridState::default()
        };
        st.toggle_sort("hours");
        let (total, page) = st.query(&cols, &rows);
        assert_eq!(total, 3);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].id, "1");
        assert_eq!(page[1].id, "3");

        st.toggle_sort("hours");
        let (_, desc) = st.query(&cols, &rows);
        assert_eq!(desc[0].id, "2");
    }
}
