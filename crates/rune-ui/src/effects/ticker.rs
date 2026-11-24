//! NumberTicker (spring rolling odometer) and Marquee (seamless infinite scroll strip with
//! pause-on-hover and automatic pause when reduced motion or a game is running).

use std::f32::consts::TAU;

use rune::{Color, El, LineCap, Shape, TextStyle, canvas, el, row};
use rune_kit::RollingText;

use crate::theme::UiTheme;

/// Clean-room ClickSpark burst: radial expanding spark lines at a click origin.
/// Renders nothing when `progress <= 0.0`, `progress >= 1.0`, or animations are paused.
pub struct ClickSpark {
    pub x: f32,
    pub y: f32,
    pub progress: f32,
    pub radius: f32,
    pub count: usize,
    pub color: Option<Color>,
}

impl ClickSpark {
    pub fn new(x: f32, y: f32, progress: f32) -> Self {
        Self {
            x,
            y,
            progress,
            radius: 24.0,
            count: 8,
            color: None,
        }
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r.max(8.0);
        self
    }

    pub fn count(mut self, n: usize) -> Self {
        self.count = n.clamp(4, 16);
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let p = self.progress;
        if !theme.effect_gate.allows_animation() || p <= 0.0 || p >= 1.0 {
            return el().size(0.0, 0.0).a11y_hidden();
        }
        let ease = 1.0 - (1.0 - p).powi(3);
        let alpha = (1.0 - p).clamp(0.0, 1.0);
        let col_c = self.color.unwrap_or(theme.kit.c.bg_accent).alpha(alpha);
        let box_sz = self.radius * 2.0 + 8.0;
        let cx = box_sz * 0.5;
        let cy = box_sz * 0.5;
        let r0 = self.radius * (0.25 + 0.65 * ease);
        let r1 = (r0 + self.radius * 0.35 * (1.0 - ease)).min(self.radius);

        let mut shape = Shape::new().view_box(box_sz, box_sz);
        for i in 0..self.count {
            let a = (i as f32 / self.count as f32) * TAU;
            shape = shape.line(
                [cx + r0 * a.cos(), cy + r0 * a.sin()],
                [cx + r1 * a.cos(), cy + r1 * a.sin()],
                2.0,
                LineCap::Round,
                col_c,
            );
        }
        canvas(shape)
            .absolute(self.x - cx, self.y - cy)
            .size(box_sz, box_sz)
            .a11y_hidden()
    }
}

/// Spring-animated numeric counter built on [`rune_kit::RollingText`] with tabular numerals.
pub struct NumberTicker {
    key: String,
    value: f64,
    decimals: usize,
    prefix: String,
    suffix: String,
    style: Option<TextStyle>,
    color: Option<Color>,
}

impl NumberTicker {
    pub fn new(key: impl Into<String>, value: f64) -> Self {
        Self {
            key: key.into(),
            value,
            decimals: 0,
            prefix: String::new(),
            suffix: String::new(),
            style: None,
            color: None,
        }
    }

    pub fn decimals(mut self, d: usize) -> Self {
        self.decimals = d.min(6);
        self
    }

    pub fn prefix(mut self, p: impl Into<String>) -> Self {
        self.prefix = p.into();
        self
    }

    pub fn suffix(mut self, s: impl Into<String>) -> Self {
        self.suffix = s.into();
        self
    }

    pub fn font(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn formatted(&self) -> String {
        let num = format!("{:.*}", self.decimals, self.value);
        format!("{}{}{}", self.prefix, num, self.suffix)
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let formatted = self.formatted();
        let style = self
            .style
            .unwrap_or(theme.kit.ty.ui_medium)
            .features("'tnum' 1");
        let color = self.color.unwrap_or(theme.kit.c.text_base);
        if !theme.effect_gate.allows_animation() {
            return rune::text(formatted).key(self.key).font(style).color(color);
        }
        RollingText::new(self.key, formatted)
            .value(self.value)
            .font(style)
            .color(color)
            .spring(theme.springs.move_)
            .build(&theme.kit)
    }
}

/// Seamless horizontal Marquee strip. Automatically pauses on hover or when
/// `!theme.effect_gate.allows_animation()` (reduced motion or game running).
pub struct Marquee {
    key: String,
    width: f32,
    content_width: f32,
    gap: f32,
    speed_px_per_sec: f32,
    reverse: bool,
    paused: bool,
    time_secs: f32,
    items: Vec<El>,
    items_clone: Vec<El>,
}

impl Marquee {
    pub fn new(
        key: impl Into<String>,
        width: f32,
        content_width: f32,
        time_secs: f32,
        build_items: impl Fn() -> Vec<El>,
    ) -> Self {
        Self {
            key: key.into(),
            width: width.max(80.0),
            content_width: content_width.max(80.0),
            gap: 16.0,
            speed_px_per_sec: 48.0,
            reverse: false,
            paused: false,
            time_secs,
            items: build_items(),
            items_clone: build_items(),
        }
    }

    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g.max(0.0);
        self
    }

    pub fn speed(mut self, px_per_sec: f32) -> Self {
        self.speed_px_per_sec = px_per_sec.max(1.0);
        self
    }

    pub fn reverse(mut self, rev: bool) -> Self {
        self.reverse = rev;
        self
    }

    pub fn paused(mut self, paused: bool) -> Self {
        self.paused = paused;
        self
    }

    pub fn offset_for(time_secs: f32, speed: f32, span: f32, reverse: bool) -> f32 {
        let cycle = span.max(1.0);
        let travel = (time_secs * speed).rem_euclid(cycle);
        if reverse { travel - cycle } else { -travel }
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let span = self.content_width + self.gap;
        let offset_x = if self.paused || !theme.effect_gate.allows_animation() {
            0.0
        } else {
            Self::offset_for(self.time_secs, self.speed_px_per_sec, span, self.reverse)
        };

        let strip = row()
            .gap(self.gap)
            .center_y()
            .offset(offset_x, 0.0)
            .child(row().gap(self.gap).center_y().children(self.items))
            .child(row().gap(self.gap).center_y().children(self.items_clone));

        el().key(self.key)
            .w(self.width)
            .clip_rounded(0.0)
            .child(strip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_offset_wraps_cleanly_in_both_directions() {
        let fwd = Marquee::offset_for(2.5, 40.0, 200.0, false);
        assert!((fwd + 100.0).abs() < 0.01);
        let rev = Marquee::offset_for(2.5, 40.0, 200.0, true);
        assert!((rev + 100.0).abs() < 0.01);
    }

    #[test]
    fn number_ticker_formats_prefix_suffix_and_decimals() {
        let t = NumberTicker::new("fps", 144.25).decimals(1).suffix(" FPS");
        assert_eq!(t.formatted(), "144.2 FPS");
    }
}
