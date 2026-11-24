//! Display & feedback components: Avatar, AvatarGroup, UiBadge, Chip, Skeleton, UiProgress.

use rune::{Color, El, Icon, LineCap, Pixels, Shape, SurfaceFit, col, el, icon, row, text};

use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
    Info,
}

impl Tone {
    pub fn colors(self, theme: &UiTheme) -> (Color, Color, Color) {
        let c = &theme.kit.c;
        match self {
            Self::Neutral => (theme.selected_fill, c.text_base, theme.float_line),
            Self::Accent => (
                c.bg_accent.alpha(0.18),
                c.text_accent,
                c.bg_accent.alpha(0.45),
            ),
            Self::Success => (c.success.bg, c.success.fg, c.success.border),
            Self::Warning => (c.warning.bg, c.warning.fg, c.warning.border),
            Self::Danger => (c.danger.bg, c.danger.fg, c.danger.border),
            Self::Info => (c.info.bg, c.info.fg, c.info.border),
        }
    }
}

/// HeroUI / Radix Avatar with image (`Pixels`), deterministic initials fallback, and status dot.
pub struct Avatar<'a> {
    name: String,
    image: Option<&'a Pixels>,
    size: f32,
    rounded: bool,
    status: Option<Tone>,
}

impl<'a> Avatar<'a> {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: None,
            size: 36.0,
            rounded: false,
            status: None,
        }
    }

    pub fn pixels(mut self, img: &'a Pixels) -> Self {
        self.image = Some(img);
        self
    }

    pub fn size(mut self, sz: f32) -> Self {
        self.size = sz.max(18.0);
        self
    }

    pub fn rounded_square(mut self) -> Self {
        self.rounded = true;
        self
    }

    pub fn status(mut self, tone: Tone) -> Self {
        self.status = Some(tone);
        self
    }

    pub fn initials(name: &str) -> String {
        let mut out = String::new();
        for part in name.split_whitespace().take(2) {
            if let Some(ch) = part.chars().next() {
                out.extend(ch.to_uppercase());
            }
        }
        if out.is_empty() { "?".into() } else { out }
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let rad = if self.rounded {
            theme.radii.control
        } else {
            theme.radii.pill
        };

        let mut base = el()
            .size(self.size, self.size)
            .radius(rad)
            .clip_rounded(rad)
            .center()
            .fill(theme.selected_fill)
            .border(1.0, theme.float_line)
            .label(self.name.clone());

        base = if let Some(img) = self.image {
            base.child(el().w_full().h_full().pixels_fit(img, SurfaceFit::Fill))
        } else {
            base.child(
                text(Self::initials(&self.name))
                    .font(k.ty.small_medium)
                    .color(c.text_base),
            )
        };

        if let Some(tone) = self.status {
            let dot_sz = (self.size * 0.28).clamp(8.0, 12.0);
            let (_, fg, _) = tone.colors(theme);
            el().size(self.size, self.size).child(base).child(
                el().absolute(self.size - dot_sz, self.size - dot_sz)
                    .size(dot_sz, dot_sz)
                    .radius(theme.radii.pill)
                    .fill(fg)
                    .border(2.0, c.page),
            )
        } else {
            base
        }
    }
}

/// Overlapping AvatarGroup with `+N` overflow badge.
pub struct AvatarGroup<'a> {
    avatars: Vec<Avatar<'a>>,
    max: usize,
    size: f32,
}

impl<'a> AvatarGroup<'a> {
    pub fn new(avatars: Vec<Avatar<'a>>) -> Self {
        Self {
            avatars,
            max: 4,
            size: 32.0,
        }
    }

    pub fn max(mut self, max: usize) -> Self {
        self.max = max.max(1);
        self
    }

    pub fn size(mut self, sz: f32) -> Self {
        self.size = sz.max(20.0);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let total = self.avatars.len();
        let shown = total.min(self.max);
        let overflow = total.saturating_sub(shown);
        let mut r = row().center_y();

        for (i, av) in self.avatars.into_iter().take(shown).enumerate() {
            let mut item = av.size(self.size).build(theme);
            if i > 0 {
                item = item.ml(-(self.size * 0.28));
            }
            r = r.child(item);
        }
        if overflow > 0 {
            r = r.child(
                el().ml(-(self.size * 0.28))
                    .size(self.size, self.size)
                    .radius(theme.radii.pill)
                    .center()
                    .fill(theme.kit.c.bg_button_neutral)
                    .border(1.5, theme.kit.c.page)
                    .child(
                        text(format!("+{overflow}"))
                            .font(theme.kit.ty.small_medium)
                            .color(theme.kit.c.text_base),
                    ),
            );
        }
        r
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiBadgeVariant {
    #[default]
    Soft,
    Solid,
    Outline,
    Dot,
}

pub struct UiBadge {
    label: String,
    tone: Tone,
    variant: UiBadgeVariant,
}

impl UiBadge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            tone: Tone::Neutral,
            variant: UiBadgeVariant::Soft,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn variant(mut self, variant: UiBadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let (soft_bg, fg, border) = self.tone.colors(theme);
        let k = &theme.kit;
        let (bg, text_c, line) = match self.variant {
            UiBadgeVariant::Soft => (soft_bg, fg, Color::TRANSPARENT),
            UiBadgeVariant::Solid => (fg, k.c.text_inverse, Color::TRANSPARENT),
            UiBadgeVariant::Outline => (Color::TRANSPARENT, fg, border),
            UiBadgeVariant::Dot => (soft_bg, k.c.text_base, border),
        };
        let mut r = row()
            .px(8.0)
            .py(3.0)
            .gap(5.0)
            .center_y()
            .radius(theme.radii.pill)
            .fill(bg);
        if line.a > 0.0 {
            r = r.border(1.0, line);
        }
        if self.variant == UiBadgeVariant::Dot {
            r = r.child(el().size(6.0, 6.0).radius(theme.radii.pill).fill(fg));
        }
        r.child(text(self.label).font(k.ty.small_medium).color(text_c))
    }
}

type ActFn = Box<dyn FnOnce(El) -> El>;

/// Interactive filter or removable tag Chip.
pub struct Chip {
    label: String,
    selected: bool,
    tone: Tone,
    leading: Option<&'static Icon>,
    on_press: Option<ActFn>,
    on_remove: Option<ActFn>,
}

impl Chip {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            selected: false,
            tone: Tone::Neutral,
            leading: None,
            on_press: None,
            on_remove: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn icon(mut self, glyph: &'static Icon) -> Self {
        self.leading = Some(glyph);
        self
    }

    pub fn on_press<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_press = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }

    pub fn on_remove<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_remove = Some(Box::new(move |e: El| e.on_press(f)));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let (soft_bg, fg, border) = self.tone.colors(theme);
        let bg = if self.selected {
            soft_bg
        } else {
            Color::TRANSPARENT
        };

        let mut r = row()
            .h(28.0)
            .px(10.0)
            .gap(6.0)
            .center_y()
            .radius(theme.radii.pill)
            .fill(bg)
            .border(
                1.0,
                if self.selected {
                    border
                } else {
                    theme.float_line
                },
            )
            .hover(theme.hover_fill)
            .role("button")
            .label(self.label.clone());

        if let Some(g) = self.leading {
            r = r.child(icon(g, 12.0).color(fg));
        }
        r = r.child(
            text(self.label)
                .font(k.ty.small_medium)
                .color(if self.selected { fg } else { k.c.text_base }),
        );

        if let Some(rm) = self.on_remove {
            let btn = rm(el()
                .size(16.0, 16.0)
                .center()
                .radius(theme.radii.pill)
                .hover(theme.hover_fill)
                .role("button")
                .label("Remove")
                .child(icon(k.glyph.close, 10.0).color(k.c.icon_muted)));
            r = r.child(btn);
        }
        if let Some(cb) = self.on_press {
            r = cb(r);
        }
        theme.apply_focus(r)
    }
}

/// Loading Skeleton placeholder (rect, circle, or text lines) with zero-GPU static fallback.
pub struct Skeleton {
    width: f32,
    height: f32,
    circle: bool,
    lines: usize,
    phase: f32,
}

impl Skeleton {
    pub fn rect(w: f32, h: f32) -> Self {
        Self {
            width: w,
            height: h,
            circle: false,
            lines: 0,
            phase: 0.0,
        }
    }

    pub fn circle(diameter: f32) -> Self {
        Self {
            width: diameter,
            height: diameter,
            circle: true,
            lines: 0,
            phase: 0.0,
        }
    }

    pub fn text_lines(width: f32, lines: usize) -> Self {
        Self {
            width,
            height: 12.0,
            circle: false,
            lines: lines.max(1),
            phase: 0.0,
        }
    }

    pub fn phase(mut self, t: f32) -> Self {
        self.phase = t;
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let pulse = if theme.effect_gate.allows_animation() {
            0.55 + 0.35 * (self.phase * 2.4).sin().abs()
        } else {
            0.75
        };
        let fill = theme.kit.c.border_base.alpha(pulse);

        if self.lines > 0 {
            let mut c = col().w(self.width).gap(8.0);
            for i in 0..self.lines {
                let w = if i + 1 == self.lines && self.lines > 1 {
                    self.width * 0.68
                } else {
                    self.width
                };
                c = c.child(el().size(w, self.height).radius(4.0).fill(fill));
            }
            return c;
        }

        let rad = if self.circle {
            theme.radii.pill
        } else {
            theme.radii.control
        };
        el().size(self.width, self.height).radius(rad).fill(fill)
    }
}

/// Linear or circular Progress indicator with determinate & indeterminate modes.
pub struct UiProgress {
    value: Option<f32>,
    label: Option<String>,
    width: f32,
    circular: Option<f32>,
    phase: f32,
    tone: Tone,
}

impl UiProgress {
    pub fn linear(value: f32) -> Self {
        Self {
            value: Some(value.clamp(0.0, 1.0)),
            label: None,
            width: 240.0,
            circular: None,
            phase: 0.0,
            tone: Tone::Accent,
        }
    }

    pub fn indeterminate(phase: f32) -> Self {
        Self {
            value: None,
            label: None,
            width: 240.0,
            circular: None,
            phase,
            tone: Tone::Accent,
        }
    }

    pub fn circular(diameter: f32, value: f32) -> Self {
        Self {
            value: Some(value.clamp(0.0, 1.0)),
            label: None,
            width: diameter,
            circular: Some(diameter.max(20.0)),
            phase: 0.0,
            tone: Tone::Accent,
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(40.0);
        self
    }

    pub fn label(mut self, lbl: impl Into<String>) -> Self {
        self.label = Some(lbl.into());
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let (_, fg, _) = self.tone.colors(theme);
        let fill_color = if self.tone == Tone::Accent {
            c.bg_accent
        } else {
            fg
        };

        if let Some(diam) = self.circular {
            let v = self.value.unwrap_or(0.35);
            let r = diam * 0.5 - 3.5;
            let cx = diam * 0.5;
            let cy = diam * 0.5;
            let segments = 48usize;
            let ring_bg: Vec<[f32; 2]> = (0..=segments)
                .map(|i| {
                    let a = (i as f32 / segments as f32) * std::f32::consts::TAU;
                    [cx + r * a.cos(), cy + r * a.sin()]
                })
                .collect();
            let arc_n = ((v * segments as f32).round() as usize).max(2);
            let start = -std::f32::consts::FRAC_PI_2;
            let ring_fg: Vec<[f32; 2]> = (0..=arc_n)
                .map(|i| {
                    let a = start + (i as f32 / segments as f32) * std::f32::consts::TAU;
                    [cx + r * a.cos(), cy + r * a.sin()]
                })
                .collect();
            let shape = Shape::new()
                .stroke_with(ring_bg, true, 4.0, LineCap::Round, c.border_base)
                .stroke_with(ring_fg, false, 4.0, LineCap::Round, fill_color)
                .view_box(diam, diam);
            return el()
                .size(diam, diam)
                .center()
                .role("progressbar")
                .range_value(f64::from(v * 100.0), 0.0, 100.0)
                .child(rune::canvas(shape).absolute(0.0, 0.0).size(diam, diam))
                .child(
                    text(format!("{:.0}%", v * 100.0))
                        .font(k.ty.small_medium)
                        .color(c.text_base),
                );
        }

        let bar = match self.value {
            Some(v) => el()
                .w(self.width)
                .h(6.0)
                .radius(theme.radii.pill)
                .fill(c.border_base)
                .child(
                    el().w(self.width * v)
                        .h(6.0)
                        .radius(theme.radii.pill)
                        .fill(fill_color)
                        .motion(theme.springs.base),
                ),
            None => {
                let seg_w = self.width * 0.34;
                let frac = if theme.effect_gate.allows_animation() {
                    (self.phase * 0.85).fract()
                } else {
                    0.33
                };
                let x = frac * (self.width - seg_w);
                el().w(self.width)
                    .h(6.0)
                    .radius(theme.radii.pill)
                    .fill(c.border_base)
                    .child(
                        el().absolute(x, 0.0)
                            .w(seg_w)
                            .h(6.0)
                            .radius(theme.radii.pill)
                            .fill(fill_color),
                    )
            }
        }
        .role("progressbar");

        let mut root = col().w(self.width).gap(4.0);
        if let Some(lbl) = self.label {
            let mut hdr = row()
                .w_full()
                .justify_between()
                .child(text(lbl).font(k.ty.small_medium).color(c.text_base));
            if let Some(v) = self.value {
                hdr = hdr.child(
                    text(format!("{:.0}%", v * 100.0))
                        .font(k.ty.small)
                        .color(c.text_muted),
                );
            }
            root = root.child(hdr);
        }
        root.child(bar)
    }
}
