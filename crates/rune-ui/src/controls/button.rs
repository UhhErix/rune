//! HeroUI / shadcn-style themed button with 6 variants, 4 sizes, loading state, icons,
//! press-scale physics, and AccessKit button semantics.

use rune::{Color, El, Icon, Tx, el, icon, row, text};
use rune_kit::{Spinner, State};

use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
    Outline,
    Danger,
    Link,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum UiSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
}

impl UiSize {
    pub const fn height(self) -> f32 {
        match self {
            Self::Xs => 24.0,
            Self::Sm => 30.0,
            Self::Md => 36.0,
            Self::Lg => 42.0,
        }
    }

    pub const fn px(self) -> f32 {
        match self {
            Self::Xs => 8.0,
            Self::Sm => 12.0,
            Self::Md => 14.0,
            Self::Lg => 18.0,
        }
    }

    pub const fn icon_size(self) -> f32 {
        match self {
            Self::Xs => 12.0,
            Self::Sm => 14.0,
            Self::Md => 15.0,
            Self::Lg => 16.0,
        }
    }
}

type ActionFn = Box<dyn FnOnce(El) -> El>;

pub struct UiButton {
    key: Option<String>,
    label: String,
    variant: UiButtonVariant,
    size: UiSize,
    leading: Option<&'static Icon>,
    trailing: Option<&'static Icon>,
    loading: bool,
    disabled: bool,
    full_width: bool,
    pill: bool,
    state: State,
    actions: Vec<ActionFn>,
}

impl UiButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            key: None,
            label: label.into(),
            variant: UiButtonVariant::Primary,
            size: UiSize::Md,
            leading: None,
            trailing: None,
            loading: false,
            disabled: false,
            full_width: false,
            pill: false,
            state: State::Rest,
            actions: Vec::new(),
        }
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn variant(mut self, variant: UiButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: UiSize) -> Self {
        self.size = size;
        self
    }

    pub fn leading_icon(mut self, glyph: &'static Icon) -> Self {
        self.leading = Some(glyph);
        self
    }

    pub fn trailing_icon(mut self, glyph: &'static Icon) -> Self {
        self.trailing = Some(glyph);
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn full_width(mut self, full: bool) -> Self {
        self.full_width = full;
        self
    }

    pub fn pill(mut self, pill: bool) -> Self {
        self.pill = pill;
        self
    }

    pub fn state(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    pub fn on_press<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.actions.push(Box::new(move |e| e.on_press(f)));
        self
    }

    pub fn on_press_tx<S: 'static>(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.actions.push(Box::new(move |e| e.on_press_tx(f)));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let inactive = self.disabled || self.loading || self.state == State::Disabled;
        let radius = if self.pill {
            theme.radii.pill
        } else {
            theme.radii.control
        };
        let h = self.size.height();
        let px = if self.variant == UiButtonVariant::Link {
            4.0
        } else {
            self.size.px()
        };
        let icon_sz = self.size.icon_size();

        let (bg, fg, border, hover_bg, use_glass) = match self.variant {
            UiButtonVariant::Primary => (
                c.bg_inverse,
                c.text_inverse,
                Color::TRANSPARENT,
                c.contrast_hover,
                false,
            ),
            UiButtonVariant::Secondary => (
                c.bg_button_neutral,
                c.text_base,
                theme.float_line,
                theme.hover_fill,
                theme.glass.control.is_some(),
            ),
            UiButtonVariant::Ghost => (
                Color::TRANSPARENT,
                c.text_base,
                Color::TRANSPARENT,
                theme.hover_fill,
                false,
            ),
            UiButtonVariant::Outline => (
                Color::TRANSPARENT,
                c.text_base,
                c.border_strong,
                theme.hover_fill,
                false,
            ),
            UiButtonVariant::Danger => (
                c.danger.bg,
                c.danger.fg,
                c.danger.border,
                c.danger.bg.alpha((c.danger.bg.a + 0.12).min(1.0)),
                false,
            ),
            UiButtonVariant::Link => (
                Color::TRANSPARENT,
                c.text_accent,
                Color::TRANSPARENT,
                Color::TRANSPARENT,
                false,
            ),
        };

        let mut content = row().center().gap(6.0);
        if self.loading {
            content = content.child(Spinner::new().size(icon_sz).color(fg).build(k));
        } else if let Some(glyph) = self.leading {
            content = content.child(icon(glyph, icon_sz).color(fg));
        }
        if !self.label.is_empty() {
            let font = if matches!(self.size, UiSize::Xs) {
                k.ty.small_medium
            } else {
                k.ty.ui_medium
            };
            content = content.child(text(self.label.clone()).font(font).color(fg));
        }
        if let Some(glyph) = self.trailing {
            content = content.child(icon(glyph, icon_sz).color(fg));
        }

        let mut root = el()
            .h(h)
            .px(px)
            .center()
            .radius(radius)
            .role("button")
            .label(self.label)
            .busy(self.loading)
            .disabled(inactive)
            .focusable(!inactive)
            .motion(theme.springs.press)
            .child(content);

        if let Some(key) = self.key {
            root = root.key(key);
        }
        if self.full_width {
            root = root.w_full();
        }
        if use_glass {
            root = theme.control_surface(root, radius);
        } else {
            let effective_bg = match self.state {
                State::Hover if hover_bg.a > 0.0 => hover_bg,
                State::Pressed => theme.pressed_fill,
                _ => bg,
            };
            root = root.fill(effective_bg);
            if border.a > 0.0 {
                root = root.border(1.0, border);
            }
        }

        if inactive {
            root = root.opacity(0.45);
        } else {
            if hover_bg.a > 0.0 && self.state == State::Rest {
                root = root.hover(hover_bg);
            }
            if theme.effect_gate.allows_animation() {
                root = root.press_scale(theme.springs.press_scale);
            }
            root = theme.apply_focus(root);
            for act in self.actions {
                root = act(root);
            }
        }
        root
    }
}
