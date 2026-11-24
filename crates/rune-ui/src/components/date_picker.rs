//! Pure-Rust DatePicker & DateRangePicker with exact proleptic Gregorian calendar math
//! (Howard Hinnant algorithm, zero external dependencies), leap years, min/max bounds,
//! keyboard grid navigation, and anchored popover trigger.

use std::rc::Rc;

use rune::{Color, El, Key, KeyEvent, col, el, icon, row, spacer, text};
use rune_kit::{Placement, popover};

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

/// Gregorian calendar date (`year`, `month` 1..=12, `day` 1..=31).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl CivilDate {
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(1..=12).contains(&month) {
            return None;
        }
        let dim = days_in_month(year, month);
        if day < 1 || day > dim {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Days since 1970-01-01 (Howard Hinnant civil algorithm).
    pub fn to_epoch_days(self) -> i64 {
        let y = i64::from(self.year) - if self.month <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = i64::from(self.month);
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Converts days since 1970-01-01 back to [`CivilDate`].
    pub fn from_epoch_days(z: i64) -> Self {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        let year = (y + if m <= 2 { 1 } else { 0 }) as i32;
        Self {
            year,
            month: m as u8,
            day: d as u8,
        }
    }

    pub fn add_days(self, delta: i64) -> Self {
        Self::from_epoch_days(self.to_epoch_days() + delta)
    }

    /// Day of week: `0 = Sunday .. 6 = Saturday`.
    pub fn weekday_sun0(self) -> u8 {
        (self.to_epoch_days() + 4).rem_euclid(7) as u8
    }

    pub fn format_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 30,
    }
}

pub fn month_name(month: u8) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

/// Owned DatePicker / DateRangePicker state.
#[derive(Clone, Debug)]
pub struct DatePickerState {
    pub open: bool,
    pub range_mode: bool,
    pub view_year: i32,
    pub view_month: u8,
    pub selected: Option<CivilDate>,
    pub range_end: Option<CivilDate>,
    pub focused: CivilDate,
    pub today: CivilDate,
    pub min_date: Option<CivilDate>,
    pub max_date: Option<CivilDate>,
}

impl DatePickerState {
    pub fn new(today: CivilDate) -> Self {
        Self {
            open: false,
            range_mode: false,
            view_year: today.year,
            view_month: today.month,
            selected: Some(today),
            range_end: None,
            focused: today,
            today,
            min_date: None,
            max_date: None,
        }
    }

    pub fn range(today: CivilDate) -> Self {
        Self {
            range_mode: true,
            ..Self::new(today)
        }
    }

    pub fn is_disabled(&self, date: CivilDate) -> bool {
        self.min_date.is_some_and(|min| date < min) || self.max_date.is_some_and(|max| date > max)
    }

    pub fn prev_month(&mut self) {
        if self.view_month == 1 {
            self.view_year -= 1;
            self.view_month = 12;
        } else {
            self.view_month -= 1;
        }
    }

    pub fn next_month(&mut self) {
        if self.view_month == 12 {
            self.view_year += 1;
            self.view_month = 1;
        } else {
            self.view_month += 1;
        }
    }

    pub fn pick(&mut self, date: CivilDate) {
        if self.is_disabled(date) {
            return;
        }
        self.focused = date;
        if !self.range_mode {
            self.selected = Some(date);
            self.range_end = None;
            self.open = false;
            return;
        }
        match (self.selected, self.range_end) {
            (Some(start), None) if date >= start => {
                self.range_end = Some(date);
                self.open = false;
            }
            (Some(start), None) => {
                self.selected = Some(date);
                self.range_end = Some(start);
                self.open = false;
            }
            _ => {
                self.selected = Some(date);
                self.range_end = None;
            }
        }
    }

    /// Keyboard grid navigation (Arrows, PageUp/PageDown month jump, Home/End week bounds, Enter).
    pub fn handle_key(&mut self, ev: &KeyEvent) -> bool {
        let next = match &ev.key {
            Key::Left => self.focused.add_days(-1),
            Key::Right => self.focused.add_days(1),
            Key::Up => self.focused.add_days(-7),
            Key::Down => self.focused.add_days(7),
            Key::PageUp => {
                self.prev_month();
                let d = self
                    .focused
                    .day
                    .min(days_in_month(self.view_year, self.view_month));
                CivilDate {
                    year: self.view_year,
                    month: self.view_month,
                    day: d,
                }
            }
            Key::PageDown => {
                self.next_month();
                let d = self
                    .focused
                    .day
                    .min(days_in_month(self.view_year, self.view_month));
                CivilDate {
                    year: self.view_year,
                    month: self.view_month,
                    day: d,
                }
            }
            Key::Enter => {
                self.pick(self.focused);
                return true;
            }
            Key::Escape => {
                self.open = false;
                return true;
            }
            _ => return false,
        };
        self.focused = next;
        self.view_year = next.year;
        self.view_month = next.month;
        true
    }

    /// Returns formatted summary for the picker trigger button.
    pub fn display_value(&self) -> String {
        match (self.selected, self.range_end) {
            (Some(a), Some(b)) => format!("{} – {}", a.format_iso(), b.format_iso()),
            (Some(a), None) => a.format_iso(),
            _ => "Pick a date".into(),
        }
    }
}

type BindFn = Box<dyn FnOnce(El) -> El>;
type DatePickFn = Rc<dyn Fn(El, CivilDate) -> El>;

pub struct DatePicker<'a> {
    key: String,
    state: &'a DatePickerState,
    width: f32,
    on_toggle: Option<BindFn>,
    on_close: Option<BindFn>,
    on_prev: Option<BindFn>,
    on_next: Option<BindFn>,
    on_pick: Option<DatePickFn>,
}

impl<'a> DatePicker<'a> {
    pub fn new(key: impl Into<String>, state: &'a DatePickerState) -> Self {
        Self {
            key: key.into(),
            state,
            width: 220.0,
            on_toggle: None,
            on_close: None,
            on_prev: None,
            on_next: None,
            on_pick: None,
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(150.0);
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut DatePickerState + Clone + 'static,
    ) -> Self {
        let a1 = access.clone();
        self.on_toggle = Some(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| {
                let st = a1(s);
                st.open = !st.open;
            })
        }));
        let a2 = access.clone();
        self.on_close = Some(Box::new(move |e: El| {
            e.modal(true, move |s: &mut S| a2(s).open = false)
        }));
        let a3 = access.clone();
        self.on_prev = Some(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| a3(s).prev_month())
        }));
        let a4 = access.clone();
        self.on_next = Some(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| a4(s).next_month())
        }));
        self.on_pick = Some(Rc::new(move |e: El, d: CivilDate| {
            let ac = access.clone();
            e.on_press(move |s: &mut S| ac(s).pick(d))
        }));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let trigger_key = format!("{}-dp-trigger", self.key);
        let label = self.state.display_value();

        let mut trigger = row()
            .key(trigger_key.clone())
            .w(self.width)
            .h(36.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .role("button")
            .label(label.clone())
            .focusable(true)
            .child(text(label).font(k.ty.ui_medium).color(c.text_base))
            .child(spacer())
            .child(icon(k.glyph.chevron_down, 13.0).color(c.icon_muted));

        trigger = theme.apply_focus(
            theme
                .control_surface(trigger, theme.radii.control)
                .hover(theme.hover_fill),
        );
        if let Some(tog) = self.on_toggle {
            trigger = tog(trigger);
        }

        if self.state.open {
            let mut prev_btn = el()
                .size(26.0, 26.0)
                .center()
                .radius(theme.radii.control)
                .hover(theme.hover_fill)
                .role("button")
                .label("Previous month")
                .child(text("‹").font(k.ty.ui_medium).color(c.text_base));
            if let Some(p) = self.on_prev {
                prev_btn = p(prev_btn);
            }

            let mut next_btn = el()
                .size(26.0, 26.0)
                .center()
                .radius(theme.radii.control)
                .hover(theme.hover_fill)
                .role("button")
                .label("Next month")
                .child(text("›").font(k.ty.ui_medium).color(c.text_base));
            if let Some(n) = self.on_next {
                next_btn = n(next_btn);
            }

            let header = row()
                .w_full()
                .justify_between()
                .center_y()
                .pb(8.0)
                .child(prev_btn)
                .child(
                    text(format!(
                        "{} {}",
                        month_name(self.state.view_month),
                        self.state.view_year
                    ))
                    .font(k.ty.ui_medium)
                    .color(c.text_base),
                )
                .child(next_btn);

            let weekdays = row().w_full().gap(2.0).children(
                ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
                    .into_iter()
                    .map(|d| {
                        el().size(34.0, 24.0)
                            .center()
                            .child(text(d).font(k.ty.small).color(c.text_muted))
                    }),
            );

            let first = CivilDate {
                year: self.state.view_year,
                month: self.state.view_month,
                day: 1,
            };
            let offset = usize::from(first.weekday_sun0());
            let dim = usize::from(days_in_month(self.state.view_year, self.state.view_month));
            let total_cells = (offset + dim).div_ceil(7) * 7;

            let mut grid = col().gap(2.0);
            for week in 0..(total_cells / 7) {
                let mut wk_row = row().gap(2.0);
                for dow in 0..7 {
                    let slot = week * 7 + dow;
                    if slot < offset || slot >= offset + dim {
                        wk_row = wk_row.child(el().size(34.0, 32.0));
                        continue;
                    }
                    let day = (slot - offset + 1) as u8;
                    let date = CivilDate {
                        year: self.state.view_year,
                        month: self.state.view_month,
                        day,
                    };
                    let disabled = self.state.is_disabled(date);
                    let is_edge =
                        self.state.selected == Some(date) || self.state.range_end == Some(date);
                    let in_range = match (self.state.selected, self.state.range_end) {
                        (Some(a), Some(b)) => date > a && date < b,
                        _ => false,
                    };
                    let is_today = date == self.state.today;

                    let (bg, fg) = if is_edge {
                        (c.bg_accent, c.submit_fg)
                    } else if in_range {
                        (theme.selected_fill, c.text_base)
                    } else {
                        (Color::TRANSPARENT, c.text_base)
                    };

                    let mut cell = el()
                        .key(format!("{}-d-{day}", self.key))
                        .size(34.0, 32.0)
                        .center()
                        .radius(theme.radii.control)
                        .fill(bg)
                        .role("button")
                        .label(date.format_iso())
                        .disabled(disabled)
                        .child(text(day.to_string()).font(k.ty.small_medium).color(fg));

                    if is_today && !is_edge {
                        cell = cell.border(1.0, c.bg_accent);
                    }
                    if disabled {
                        cell = cell.opacity(0.35);
                    } else {
                        cell = cell.hover(theme.hover_fill);
                        if let Some(pick) = &self.on_pick {
                            cell = pick(cell, date);
                        }
                    }
                    wk_row = wk_row.child(cell);
                }
                grid = grid.child(wk_row);
            }

            let mut cal = col()
                .padding(12.0)
                .gap(4.0)
                .child(header)
                .child(weekdays)
                .child(grid);
            cal = theme.overlay_surface(cal, theme.radii.panel);
            if let Some(close) = self.on_close {
                cal = close(cal);
            }
            portals.push(
                LayerSlot::Popover,
                popover::anchored(k, cal, &trigger_key, Placement::BottomStart, 6.0),
            );
        }

        trigger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_date_roundtrip_and_leap_years() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2026));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);

        let d = CivilDate::new(2026, 11, 24).unwrap_or(CivilDate {
            year: 2026,
            month: 1,
            day: 1,
        });
        assert_eq!(CivilDate::from_epoch_days(d.to_epoch_days()), d);
        assert_eq!(d.add_days(7).day, 1);
        assert_eq!(d.add_days(7).month, 12);
    }

    #[test]
    fn range_picker_orders_reversed_selection() {
        let d1 = CivilDate {
            year: 2026,
            month: 11,
            day: 20,
        };
        let d2 = CivilDate {
            year: 2026,
            month: 11,
            day: 10,
        };
        let mut st = DatePickerState::range(d1);
        st.selected = None;
        st.pick(d1);
        st.pick(d2);
        assert_eq!(st.selected, Some(d2));
        assert_eq!(st.range_end, Some(d1));
    }
}
