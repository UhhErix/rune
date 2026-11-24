//! Shared-element transitions (`layoutId` analogue) powered by Rune's native [`El::matched`].

use rune::{Color, El, Spring, el};

use crate::theme::UiTheme;

/// Wraps an element so it smoothly morphs in position, size, and corner radius between any two
/// views that share the same `shared_id` (`El::matched`).
pub struct SharedElement {
    key: String,
    shared_id: String,
    spring: Option<Spring>,
    child: El,
}

impl SharedElement {
    pub fn new(key: impl Into<String>, shared_id: impl Into<String>, child: El) -> Self {
        Self {
            key: key.into(),
            shared_id: shared_id.into(),
            spring: None,
            child,
        }
    }

    pub fn spring(mut self, spring: Spring) -> Self {
        self.spring = Some(spring);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let mut node = el().key(self.key).matched(self.shared_id).child(self.child);
        if theme.effect_gate.allows_animation() {
            node = node.motion(self.spring.unwrap_or(theme.springs.flight));
        }
        node
    }
}

/// Sliding active indicator pill/bar that travels between items via [`El::matched`].
pub fn shared_indicator(
    key: impl Into<String>,
    shared_id: impl Into<String>,
    color: Color,
    radius: f32,
    theme: &UiTheme,
) -> El {
    let mut pill = el()
        .key(key.into())
        .matched(shared_id.into())
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .radius(radius)
        .fill(color);
    if theme.effect_gate.allows_animation() {
        pill = pill.motion(theme.springs.move_);
    }
    pill
}
