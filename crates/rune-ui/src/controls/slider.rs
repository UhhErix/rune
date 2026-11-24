//! Single-thumb and dual-thumb range sliders matching Radix Slider & React Aria semantics:
//! 1:1 pointer drag, click-to-jump, quantized stepping, tick marks, keyboard arrow/Page/Home/End,
//! and AccessKit `slider` role with numeric value/min/max/step.

use rune::{Drag, El, Key, KeyEvent, Phase, Tx, col, el, row, text};

use crate::primitives::a11y::slider_a11y;
use crate::theme::UiTheme;

/// Quantizes `raw` to `[min, max]` snapped to `step` (if `step > 0.0`).
pub fn quantize(raw: f32, min: f32, max: f32, step: f32) -> f32 {
    let clamped = raw.clamp(min, max);
    if step <= 0.0 || !step.is_finite() {
        return clamped;
    }
    let steps = ((clamped - min) / step).round();
    (min + steps * step).clamp(min, max)
}

/// Maps normalized fraction `t` in `0.0..=1.0` to quantized `[min, max]`.
pub fn value_from_fraction(t: f32, min: f32, max: f32, step: f32) -> f32 {
    let span = (max - min).max(f32::EPSILON);
    quantize(min + t.clamp(0.0, 1.0) * span, min, max, step)
}

/// Maps `value` to `0.0..=1.0`.
pub fn fraction_from_value(value: f32, min: f32, max: f32) -> f32 {
    let span = max - min;
    if span.abs() <= f32::EPSILON {
        0.0
    } else {
        ((value - min) / span).clamp(0.0, 1.0)
    }
}

/// Computes next slider value for keyboard Arrow / PageUp / PageDown / Home / End.
pub fn step_by_key(ev: &KeyEvent, current: f32, min: f32, max: f32, step: f32) -> Option<f32> {
    if ev.mods.ctrl || ev.mods.alt || ev.mods.cmd || ev.mods.logo {
        return None;
    }
    let fine = if step > 0.0 { step } else { (max - min) * 0.01 };
    let coarse = (fine * 10.0).min((max - min) * 0.2).max(fine);
    let next = match &ev.key {
        Key::Right | Key::Up => quantize(current + fine, min, max, step),
        Key::Left | Key::Down => quantize(current - fine, min, max, step),
        Key::PageUp => quantize(current + coarse, min, max, step),
        Key::PageDown => quantize(current - coarse, min, max, step),
        Key::Home => min,
        Key::End => max,
        _ => return None,
    };
    Some(next)
}

/// A labeled tick mark on a slider track.
#[derive(Clone, Debug)]
pub struct SliderMark {
    pub value: f32,
    pub label: Option<String>,
}

impl SliderMark {
    pub fn new(value: f32) -> Self {
        Self { value, label: None }
    }
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

type SliderDragFn = Box<dyn FnOnce(El) -> El>;

/// Single-thumb horizontal slider with Radix / React Aria semantics.
pub struct UiSlider {
    key: String,
    label: String,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    width: f32,
    show_value: bool,
    unit: String,
    marks: Vec<SliderMark>,
    disabled: bool,
    handlers: Vec<SliderDragFn>,
}

impl UiSlider {
    pub fn new(key: impl Into<String>, value: f32, min: f32, max: f32) -> Self {
        Self {
            key: key.into(),
            label: String::new(),
            value,
            min,
            max,
            step: 1.0,
            width: 220.0,
            show_value: false,
            unit: String::new(),
            marks: Vec::new(),
            disabled: false,
            handlers: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step.max(0.0);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(60.0);
        self
    }

    pub fn show_value(mut self, unit: impl Into<String>) -> Self {
        self.show_value = true;
        self.unit = unit.into();
        self
    }

    pub fn marks(mut self, marks: impl IntoIterator<Item = SliderMark>) -> Self {
        self.marks = marks.into_iter().collect();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Registers pointer drag, click-to-jump, keyboard stepping, and AccessKit `on_set_value`.
    pub fn on_change<S: 'static>(mut self, f: impl Fn(&mut S, f32) + Clone + 'static) -> Self {
        let min = self.min;
        let max = self.max;
        let step = self.step;
        let cur = self.value;

        let f_drag = f.clone();
        self.handlers.push(Box::new(move |e: El| {
            e.on_drag(move |s: &mut S, d: &mut Drag, _tx: &mut Tx| {
                if d.phase == Phase::Up && d.cancelled() {
                    return;
                }
                let t = if d.w > 1.0 {
                    (d.x / d.w).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                f_drag(s, value_from_fraction(t, min, max, step));
            })
        }));

        let f_key = f.clone();
        self.handlers.push(Box::new(move |e: El| {
            e.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                if let Some(next) = step_by_key(ev, cur, min, max, step) {
                    f_key(s, next);
                    true
                } else {
                    false
                }
            })
        }));

        self.handlers.push(Box::new(move |e: El| {
            e.on_set_value(move |s: &mut S, v: f64, _tx: &mut Tx| {
                f(s, quantize(v as f32, min, max, step));
            })
        }));

        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let frac = fraction_from_value(self.value, self.min, self.max);
        let thumb_size = 16.0;
        let track_h = 4.0;
        let travel = (self.width - thumb_size).max(0.0);
        let thumb_x = frac * travel;
        let fill_w = (thumb_x + thumb_size * 0.5).clamp(0.0, self.width);

        let mut track = el()
            .key(format!("{}-track", self.key))
            .w(self.width)
            .h(24.0)
            .focusable(!self.disabled)
            .child(
                el().absolute(0.0, (24.0 - track_h) * 0.5)
                    .w(self.width)
                    .h(track_h)
                    .radius(theme.radii.pill)
                    .fill(c.border_base),
            )
            .child(
                el().absolute(0.0, (24.0 - track_h) * 0.5)
                    .w(fill_w)
                    .h(track_h)
                    .radius(theme.radii.pill)
                    .fill(c.bg_accent),
            );

        for mark in &self.marks {
            let mf = fraction_from_value(mark.value, self.min, self.max);
            let mx = (mf * self.width).clamp(2.0, (self.width - 2.0).max(2.0));
            track = track.child(
                el().absolute(mx - 1.0, (24.0 - track_h) * 0.5)
                    .size(2.0, track_h)
                    .fill(c.text_muted),
            );
        }

        let thumb = el()
            .key(format!("{}-thumb", self.key))
            .absolute(thumb_x, (24.0 - thumb_size) * 0.5)
            .size(thumb_size, thumb_size)
            .radius(theme.radii.pill)
            .fill(c.knob_on)
            .border(1.5, c.bg_accent)
            .motion(theme.springs.quick);

        track = track.child(thumb);
        track = slider_a11y(
            track,
            if self.label.is_empty() {
                self.key.clone()
            } else {
                self.label.clone()
            },
            f64::from(self.value),
            f64::from(self.min),
            f64::from(self.max),
            f64::from(self.step.max(0.01)),
            self.disabled,
        );

        if self.disabled {
            track = track.opacity(0.45);
        } else {
            track = theme.apply_focus(track);
            for h in self.handlers {
                track = h(track);
            }
        }

        let mut root = col().key(self.key).gap(4.0);
        if !self.label.is_empty() || self.show_value {
            let mut hdr = row().w(self.width).justify_between().center_y();
            if !self.label.is_empty() {
                hdr = hdr.child(text(self.label).font(k.ty.small_medium).color(c.text_base));
            }
            if self.show_value {
                let formatted = if self.step >= 1.0 {
                    format!("{:.0}{}", self.value, self.unit)
                } else {
                    format!("{:.1}{}", self.value, self.unit)
                };
                hdr = hdr.child(text(formatted).font(k.ty.small).color(c.text_muted));
            }
            root = root.child(hdr);
        }
        root = root.child(track);

        if self.marks.iter().any(|m| m.label.is_some()) {
            let mut mark_row = el().w(self.width).h(14.0);
            for mark in &self.marks {
                if let Some(lbl) = &mark.label {
                    let mf = fraction_from_value(mark.value, self.min, self.max);
                    let mx = (mf * (self.width - 28.0)).clamp(0.0, (self.width - 28.0).max(0.0));
                    mark_row = mark_row.child(
                        text(lbl.clone())
                            .font(k.ty.small)
                            .color(c.text_muted)
                            .absolute(mx, 0.0),
                    );
                }
            }
            root = root.child(mark_row);
        }

        root
    }
}

/// Dual-thumb range slider selecting an ordered `(start, end)` interval.
pub struct RangeSlider {
    key: String,
    label: String,
    range: (f32, f32),
    min: f32,
    max: f32,
    step: f32,
    min_gap: f32,
    width: f32,
    disabled: bool,
    handler: Option<SliderDragFn>,
}

impl RangeSlider {
    pub fn new(key: impl Into<String>, start: f32, end: f32, min: f32, max: f32) -> Self {
        let a = start.min(end).clamp(min, max);
        let b = start.max(end).clamp(min, max);
        Self {
            key: key.into(),
            label: String::new(),
            range: (a, b),
            min,
            max,
            step: 1.0,
            min_gap: 0.0,
            width: 220.0,
            disabled: false,
            handler: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step.max(0.0);
        self
    }

    pub fn min_gap(mut self, gap: f32) -> Self {
        self.min_gap = gap.max(0.0);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(60.0);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change<S: 'static>(mut self, f: impl Fn(&mut S, (f32, f32)) + 'static) -> Self {
        let min = self.min;
        let max = self.max;
        let step = self.step;
        let min_gap = self.min_gap;
        let (a0, b0) = self.range;
        self.handler = Some(Box::new(move |e: El| {
            e.on_drag(move |s: &mut S, d: &mut Drag, _tx: &mut Tx| {
                if d.phase == Phase::Up && d.cancelled() {
                    return;
                }
                let t = if d.w > 1.0 {
                    (d.x / d.w).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let picked = value_from_fraction(t, min, max, step);
                if d.phase == Phase::Down {
                    let dist_a = (picked - a0).abs();
                    let dist_b = (picked - b0).abs();
                    d.data = if dist_a <= dist_b { 0.0 } else { 1.0 };
                }
                let next = if d.data < 0.5 {
                    let na = picked.min(b0 - min_gap).max(min);
                    (quantize(na, min, max, step), b0)
                } else {
                    let nb = picked.max(a0 + min_gap).min(max);
                    (a0, quantize(nb, min, max, step))
                };
                f(s, next);
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let fa = fraction_from_value(self.range.0, self.min, self.max);
        let fb = fraction_from_value(self.range.1, self.min, self.max);
        let thumb_size = 16.0;
        let track_h = 4.0;
        let travel = (self.width - thumb_size).max(0.0);
        let xa = fa * travel;
        let xb = fb * travel;
        let fill_x = xa + thumb_size * 0.5;
        let fill_w = (xb - xa).max(0.0);

        let mut track = el()
            .key(format!("{}-track", self.key))
            .w(self.width)
            .h(24.0)
            .focusable(!self.disabled)
            .child(
                el().absolute(0.0, (24.0 - track_h) * 0.5)
                    .w(self.width)
                    .h(track_h)
                    .radius(theme.radii.pill)
                    .fill(c.border_base),
            )
            .child(
                el().absolute(fill_x, (24.0 - track_h) * 0.5)
                    .w(fill_w)
                    .h(track_h)
                    .radius(theme.radii.pill)
                    .fill(c.bg_accent),
            )
            .child(
                el().key(format!("{}-lo", self.key))
                    .absolute(xa, (24.0 - thumb_size) * 0.5)
                    .size(thumb_size, thumb_size)
                    .radius(theme.radii.pill)
                    .fill(c.knob_on)
                    .border(1.5, c.bg_accent)
                    .motion(theme.springs.quick),
            )
            .child(
                el().key(format!("{}-hi", self.key))
                    .absolute(xb, (24.0 - thumb_size) * 0.5)
                    .size(thumb_size, thumb_size)
                    .radius(theme.radii.pill)
                    .fill(c.knob_on)
                    .border(1.5, c.bg_accent)
                    .motion(theme.springs.quick),
            );

        if self.disabled {
            track = track.opacity(0.45);
        } else {
            track = theme.apply_focus(track);
            if let Some(h) = self.handler {
                track = h(track);
            }
        }

        let mut root = col().key(self.key).gap(4.0);
        if !self.label.is_empty() {
            root = root.child(
                row()
                    .w(self.width)
                    .justify_between()
                    .child(text(self.label).font(k.ty.small_medium).color(c.text_base))
                    .child(
                        text(format!("{:.0}–{:.0}", self.range.0, self.range.1))
                            .font(k.ty.small)
                            .color(c.text_muted),
                    ),
            );
        }
        root.child(track)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_and_fraction_round_trip() {
        assert_eq!(quantize(23.4, 0.0, 100.0, 5.0), 25.0);
        assert_eq!(quantize(-10.0, 0.0, 100.0, 5.0), 0.0);
        assert_eq!(value_from_fraction(0.5, 10.0, 30.0, 2.0), 20.0);
        assert!((fraction_from_value(20.0, 10.0, 30.0) - 0.5).abs() < 1e-5);
    }
}
