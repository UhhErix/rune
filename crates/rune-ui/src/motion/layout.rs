//! Framer-Motion-style layout animations, enter/exit presets, and staggered presence lists
//! built directly on Rune's analytic springs (`rune_motion`).

use rune::{El, Spring, Transition, col, el, row};
use rune_motion::{Effect, SpringPair, Vec2};

use crate::theme::UiTheme;

/// Declarative enter/exit motion presets. Under reduced motion, all presets downgrade to a pure
/// opacity fade (zero spatial travel, zero bounce).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionPreset {
    Fade,
    Scale(f32),
    SlideUp(f32),
    SlideDown(f32),
    SlideLeft(f32),
    SlideRight(f32),
    Pop,
    BlurFade(f32),
}

impl MotionPreset {
    pub fn to_transition(self, spring: Spring, reduced_motion: bool) -> Transition {
        let pair = SpringPair::new(spring, spring);
        if reduced_motion {
            return Transition {
                insertion: Effect::opacity(0.0),
                removal: Effect::opacity(0.0),
                springs: pair,
                ..Transition::identity()
            };
        }
        let effect = match self {
            Self::Fade => Effect::opacity(0.0),
            Self::Scale(s) => Effect {
                opacity: 0.0,
                scale: s,
                ..Effect::IDENTITY
            },
            Self::SlideUp(dy) => Effect {
                opacity: 0.0,
                offset: Vec2::new(0.0, dy),
                ..Effect::IDENTITY
            },
            Self::SlideDown(dy) => Effect {
                opacity: 0.0,
                offset: Vec2::new(0.0, -dy),
                ..Effect::IDENTITY
            },
            Self::SlideLeft(dx) => Effect {
                opacity: 0.0,
                offset: Vec2::new(dx, 0.0),
                ..Effect::IDENTITY
            },
            Self::SlideRight(dx) => Effect {
                opacity: 0.0,
                offset: Vec2::new(-dx, 0.0),
                ..Effect::IDENTITY
            },
            Self::Pop => Effect {
                opacity: 0.0,
                scale: 0.92,
                ..Effect::IDENTITY
            },
            Self::BlurFade(r) => Effect {
                opacity: 0.0,
                blur: r.min(4.0),
                ..Effect::IDENTITY
            },
        };
        Transition {
            insertion: effect,
            removal: effect,
            springs: pair,
            ..Transition::identity()
        }
    }
}

/// Declarative animated container (`motion.div` analogue) with layout spring and enter/exit preset.
pub struct MotionBox {
    key: String,
    preset: MotionPreset,
    spring: Option<Spring>,
    layout_animate: bool,
    child: El,
}

impl MotionBox {
    pub fn new(key: impl Into<String>, child: El) -> Self {
        Self {
            key: key.into(),
            preset: MotionPreset::Fade,
            spring: None,
            layout_animate: true,
            child,
        }
    }

    pub fn preset(mut self, preset: MotionPreset) -> Self {
        self.preset = preset;
        self
    }

    pub fn spring(mut self, spring: Spring) -> Self {
        self.spring = Some(spring);
        self
    }

    pub fn layout(mut self, animate: bool) -> Self {
        self.layout_animate = animate;
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let reduced = !theme.effect_gate.allows_animation();
        let spring = self.spring.unwrap_or(theme.springs.base);
        let mut node = el()
            .key(self.key)
            .transition(self.preset.to_transition(spring, reduced))
            .child(self.child);
        if self.layout_animate && !reduced {
            node = node.motion(spring);
        }
        node
    }
}

/// Staggered list choreography (`StaggerChildren` analogue) using `El::enter_delay`.
pub struct StaggerList {
    key: String,
    horizontal: bool,
    gap: f32,
    stagger_secs: f32,
    initial_delay_secs: f32,
    preset: MotionPreset,
    items: Vec<(String, El)>,
}

impl StaggerList {
    pub fn column(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            horizontal: false,
            gap: 8.0,
            stagger_secs: 0.04,
            initial_delay_secs: 0.0,
            preset: MotionPreset::SlideUp(8.0),
            items: Vec::new(),
        }
    }

    pub fn row(key: impl Into<String>) -> Self {
        Self {
            horizontal: true,
            ..Self::column(key)
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn stagger(mut self, secs: f32) -> Self {
        self.stagger_secs = secs.max(0.0);
        self
    }

    pub fn initial_delay(mut self, secs: f32) -> Self {
        self.initial_delay_secs = secs.max(0.0);
        self
    }

    pub fn preset(mut self, preset: MotionPreset) -> Self {
        self.preset = preset;
        self
    }

    pub fn item(mut self, key: impl Into<String>, child: El) -> Self {
        self.items.push((key.into(), child));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let reduced = !theme.effect_gate.allows_animation();
        let tr = self.preset.to_transition(theme.springs.base, reduced);
        let children = self.items.into_iter().enumerate().map(|(i, (k, child))| {
            let delay = if reduced {
                0.0
            } else {
                self.initial_delay_secs + i as f32 * self.stagger_secs
            };
            let mut wrap = el().key(k).transition(tr).child(child);
            if delay > 0.0 {
                wrap = wrap.enter_delay(delay);
            }
            if !reduced {
                wrap = wrap.motion(theme.springs.move_);
            }
            wrap
        });
        let container = if self.horizontal { row() } else { col() };
        container.key(self.key).gap(self.gap).children(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_downgrades_preset_to_pure_fade() {
        let t = MotionPreset::SlideUp(24.0).to_transition(Spring::new(0.2, 0.0), true);
        assert_eq!(t.insertion, Effect::opacity(0.0));
        assert_eq!(t.removal, Effect::opacity(0.0));
    }
}
