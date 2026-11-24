//! Semantic theme layer for `rune-ui`.
//!
//! Wraps [`rune_kit::Kit`] with surface, glass, focus-ring, radius, motion, and
//! effect-gate tokens so upstream Rune apps get clean neutral/dark defaults out of
//! the box while downstream apps (such as Exo Launcher) can layer custom palettes,
//! typography, Liquid Glass, and focus-ring policies on top without forking components.

use rune::{Color, El, LiquidGlass, Spring};
use rune_kit::Kit;

/// Controls how keyboard focus rings are drawn by `rune-ui` components.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FocusRingPolicy {
    /// Draw a standard focus outline (`width`, `offset`, `color`).
    Outline {
        width: f32,
        offset: f32,
        color: Color,
    },
    /// Subtle fill/tint highlight without a high-contrast white selection outline
    /// (used by themes like Exo Void/Liquid Glass).
    SubtleFill { fill: Color, border: Color },
    /// Suppress default focus outlines completely.
    None,
}

/// Optional Liquid Glass surfaces a theme can supply for floating overlays and controls.
#[derive(Clone, Copy, Debug, Default)]
pub struct GlassTokens {
    pub control: Option<LiquidGlass>,
    pub panel: Option<LiquidGlass>,
    pub overlay: Option<LiquidGlass>,
    pub capsule: Option<LiquidGlass>,
}

/// Radius tokens for `rune-ui` surfaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiRadii {
    pub control: f32,
    pub card: f32,
    pub menu: f32,
    pub panel: f32,
    pub dialog: f32,
    pub sheet: f32,
    pub pill: f32,
}

impl Default for UiRadii {
    fn default() -> Self {
        Self {
            control: 8.0,
            card: 12.0,
            menu: 10.0,
            panel: 14.0,
            dialog: 16.0,
            sheet: 20.0,
            pill: 999.0,
        }
    }
}

/// Spring presets used across `rune-ui` controls, overlays, drawers, and charts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSprings {
    pub press: Spring,
    pub quick: Spring,
    pub base: Spring,
    pub move_: Spring,
    pub flight: Spring,
    pub press_scale: f32,
}

impl Default for UiSprings {
    fn default() -> Self {
        Self {
            press: Spring::new(0.09, 0.0),
            quick: Spring::new(0.12, 0.0),
            base: Spring::new(0.16, 0.0),
            move_: Spring::new(0.22, 0.0),
            flight: Spring::new(0.30, 0.0),
            press_scale: 0.97,
        }
    }
}

/// Runtime gate controlling GPU shader effects and continuous animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectGate {
    /// OS or app reduced-motion preference is active.
    pub reduced_motion: bool,
    /// A fullscreen or foreground game is running: all ambient effects pause at zero GPU cost.
    pub game_running: bool,
    /// GPU shader path is enabled; when false, effects render pure zero-GPU static fallbacks.
    pub gpu_enabled: bool,
}

impl Default for EffectGate {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            game_running: false,
            gpu_enabled: true,
        }
    }
}

impl EffectGate {
    /// True when continuous animations and shader time advances are allowed.
    pub fn allows_animation(self) -> bool {
        !self.reduced_motion && !self.game_running
    }

    /// True when custom wgpu shader passes should execute.
    pub fn allows_gpu_fx(self) -> bool {
        self.gpu_enabled && !self.reduced_motion && !self.game_running
    }
}

/// Semantic chart color series palette.
#[derive(Clone, Copy, Debug)]
pub struct ChartPalette {
    pub series: [Color; 6],
    pub grid: Color,
    pub axis: Color,
    pub crosshair: Color,
}

impl ChartPalette {
    pub fn from_kit(kit: &Kit) -> Self {
        Self {
            series: [
                kit.c.bg_accent,
                kit.c.success.icon,
                kit.c.warning.icon,
                kit.c.info.icon,
                kit.c.danger.icon,
                kit.c.text_muted,
            ],
            grid: kit.c.border_muted,
            axis: kit.c.text_muted,
            crosshair: kit.c.border_strong,
        }
    }

    pub fn color(&self, index: usize) -> Color {
        self.series[index % self.series.len()]
    }
}

/// Complete theme handed to `rune-ui` builders.
#[derive(Clone, Debug)]
pub struct UiTheme {
    pub kit: Kit,
    pub focus_ring: FocusRingPolicy,
    pub glass: GlassTokens,
    pub radii: UiRadii,
    pub springs: UiSprings,
    pub effect_gate: EffectGate,
    pub chart: ChartPalette,
    pub hover_fill: Color,
    pub pressed_fill: Color,
    pub selected_fill: Color,
    pub hairline: Color,
    pub float_line: Color,
}

impl UiTheme {
    /// Neutral light or dark theme backed by [`Kit::neutral`].
    pub fn neutral(dark: bool) -> Self {
        Self::from_kit(Kit::neutral(dark))
    }

    /// Builds a `UiTheme` from any [`rune_kit::Kit`].
    pub fn from_kit(kit: Kit) -> Self {
        let chart = ChartPalette::from_kit(&kit);
        let focus_color = kit.c.border_focus;
        let hover_fill = kit.c.hover;
        let pressed_fill = kit.c.pressed;
        let selected_fill = kit.c.surface_hover;
        let hairline = kit.c.border_muted;
        let float_line = kit.c.border_base;
        Self {
            kit,
            focus_ring: FocusRingPolicy::Outline {
                width: 2.0,
                offset: 2.0,
                color: focus_color,
            },
            glass: GlassTokens::default(),
            radii: UiRadii::default(),
            springs: UiSprings::default(),
            effect_gate: EffectGate::default(),
            chart,
            hover_fill,
            pressed_fill,
            selected_fill,
            hairline,
            float_line,
        }
    }

    pub fn with_focus_ring(mut self, policy: FocusRingPolicy) -> Self {
        self.focus_ring = policy;
        self
    }

    pub fn with_glass(mut self, glass: GlassTokens) -> Self {
        self.glass = glass;
        self
    }

    pub fn with_radii(mut self, radii: UiRadii) -> Self {
        self.radii = radii;
        self
    }

    pub fn with_springs(mut self, springs: UiSprings) -> Self {
        self.springs = springs;
        self
    }

    pub fn with_effect_gate(mut self, gate: EffectGate) -> Self {
        self.effect_gate = gate;
        self
    }

    pub fn with_chart_palette(mut self, chart: ChartPalette) -> Self {
        self.chart = chart;
        self
    }

    /// Applies the theme's focus-ring policy to an interactive element.
    pub fn apply_focus(&self, el: El) -> El {
        match self.focus_ring {
            FocusRingPolicy::Outline {
                width,
                offset,
                color,
            } => el.focus_ring(width, offset, color),
            FocusRingPolicy::SubtleFill { fill, .. } => el.no_focus_ring().focus_fill(fill),
            FocusRingPolicy::None => el.no_focus_ring(),
        }
    }

    /// Applies either the theme's control Liquid Glass surface or a solid surface fallback.
    pub fn control_surface(&self, el: El, radius: f32) -> El {
        let el = el.radius(radius);
        match self.glass.control {
            Some(glass) if self.effect_gate.gpu_enabled => el.liquid_glass(glass),
            _ => el
                .fill(self.kit.c.bg_button_neutral)
                .border(1.0, self.float_line),
        }
    }

    /// Applies either the theme's overlay Liquid Glass surface or an elevated solid surface.
    pub fn overlay_surface(&self, el: El, radius: f32) -> El {
        let el = el.radius(radius);
        match self.glass.overlay {
            Some(glass) if self.effect_gate.gpu_enabled => el.liquid_glass(glass),
            _ => self.kit.elevate(
                el.fill(self.kit.c.layer_01).border(1.0, self.float_line),
                &self.kit.elevation.overlay,
            ),
        }
    }

    /// Applies either the theme's panel Liquid Glass surface or a solid card surface.
    pub fn panel_surface(&self, el: El, radius: f32) -> El {
        let el = el.radius(radius);
        match self.glass.panel {
            Some(glass) if self.effect_gate.gpu_enabled => el.liquid_glass(glass),
            _ => self.kit.elevate(
                el.fill(self.kit.c.surface_raised)
                    .border(1.0, self.float_line),
                &self.kit.elevation.md,
            ),
        }
    }
}
