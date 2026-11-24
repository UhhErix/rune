//! Clean-room text effects: BlurInText, SplitText, ScrambleText, GradientText, ShinyText.
//! Every effect respects [`EffectGate`] (zero-GPU static fallback under reduced motion or while
//! a game is running).

use rune::{Color, El, TextStyle, Transition, row, text};
use rune_motion::{Effect, SpringPair, Vec2};

use crate::theme::UiTheme;

/// Staggered blur-in text entrance (word by word). Under reduced motion or game-running,
/// renders immediately as plain static text.
pub struct BlurInText {
    key: String,
    content: String,
    style: Option<TextStyle>,
    color: Option<Color>,
    stagger_secs: f32,
    blur_radius: f32,
}

impl BlurInText {
    pub fn new(key: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            content: content.into(),
            style: None,
            color: None,
            stagger_secs: 0.05,
            blur_radius: 6.0,
        }
    }

    pub fn font(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn stagger(mut self, secs: f32) -> Self {
        self.stagger_secs = secs.max(0.0);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let style = self.style.unwrap_or(theme.kit.ty.ui_medium);
        let color = self.color.unwrap_or(theme.kit.c.text_base);
        if !theme.effect_gate.allows_animation() {
            return text(self.content).key(self.key).font(style).color(color);
        }
        let tr = Transition {
            insertion: Effect {
                opacity: 0.0,
                blur: self.blur_radius.min(4.0),
                offset: Vec2::new(0.0, 6.0),
                ..Effect::IDENTITY
            },
            removal: Effect::opacity(0.0),
            springs: SpringPair::new(theme.springs.move_, theme.springs.quick),
            ..Transition::identity()
        };
        let words: Vec<El> = self
            .content
            .split_whitespace()
            .enumerate()
            .map(|(i, w)| {
                text(w.to_owned())
                    .key(format!("{}-w-{i}", self.key))
                    .font(style)
                    .color(color)
                    .transition(tr)
                    .enter_delay(i as f32 * self.stagger_secs)
            })
            .collect();
        row().key(self.key).gap(5.0).wrap().children(words)
    }
}

/// Per-character or per-word spring reveal (`SplitText`).
pub struct SplitText {
    key: String,
    content: String,
    by_word: bool,
    style: Option<TextStyle>,
    color: Option<Color>,
    stagger_secs: f32,
    lift: f32,
}

impl SplitText {
    pub fn new(key: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            content: content.into(),
            by_word: false,
            style: None,
            color: None,
            stagger_secs: 0.025,
            lift: 10.0,
        }
    }

    pub fn by_word(mut self) -> Self {
        self.by_word = true;
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

    pub fn stagger(mut self, secs: f32) -> Self {
        self.stagger_secs = secs.max(0.0);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let style = self.style.unwrap_or(theme.kit.ty.ui_medium);
        let color = self.color.unwrap_or(theme.kit.c.text_base);
        if !theme.effect_gate.allows_animation() {
            return text(self.content).key(self.key).font(style).color(color);
        }
        let tr = Transition {
            insertion: Effect {
                opacity: 0.0,
                offset: Vec2::new(0.0, self.lift),
                ..Effect::IDENTITY
            },
            removal: Effect::opacity(0.0),
            springs: SpringPair::new(theme.springs.base, theme.springs.quick),
            ..Transition::identity()
        };
        if self.by_word {
            let parts: Vec<El> = self
                .content
                .split_whitespace()
                .enumerate()
                .map(|(i, w)| {
                    text(w.to_owned())
                        .key(format!("{}-sw-{i}", self.key))
                        .font(style)
                        .color(color)
                        .transition(tr)
                        .enter_delay(i as f32 * self.stagger_secs)
                })
                .collect();
            row().key(self.key).gap(5.0).wrap().children(parts)
        } else {
            let parts: Vec<El> = self
                .content
                .chars()
                .enumerate()
                .map(|(i, ch)| {
                    text(ch.to_string())
                        .key(format!("{}-sc-{i}", self.key))
                        .font(style)
                        .color(color)
                        .transition(tr)
                        .enter_delay(i as f32 * self.stagger_secs)
                })
                .collect();
            row().key(self.key).children(parts)
        }
    }
}

const SCRAMBLE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789#@%&*";

/// Deterministic glyph scramble decode at normalized `progress` in `0.0..=1.0`.
pub fn scramble_frame(target: &str, progress: f32, seed: u32) -> String {
    let p = progress.clamp(0.0, 1.0);
    if p >= 0.999 {
        return target.to_owned();
    }
    let chars: Vec<char> = target.chars().collect();
    let revealed = (p * chars.len() as f32).floor() as usize;
    let mut out = String::with_capacity(target.len());
    for (i, &ch) in chars.iter().enumerate() {
        if i < revealed || ch.is_whitespace() {
            out.push(ch);
        } else {
            let h = (i as u32)
                .wrapping_mul(2_654_435_761)
                .wrapping_add(seed.wrapping_mul(1_013_904_223))
                .wrapping_add((p * 60.0) as u32 * 37);
            let idx = (h as usize) % SCRAMBLE_ALPHABET.len();
            out.push(char::from(SCRAMBLE_ALPHABET[idx]));
        }
    }
    out
}

/// Text that scrambles through deterministic glyphs before resolving to `target`.
pub struct ScrambleText {
    key: String,
    target: String,
    progress: f32,
    seed: u32,
    style: Option<TextStyle>,
    color: Option<Color>,
}

impl ScrambleText {
    pub fn new(key: impl Into<String>, target: impl Into<String>, progress: f32) -> Self {
        Self {
            key: key.into(),
            target: target.into(),
            progress,
            seed: 1,
            style: None,
            color: None,
        }
    }

    pub fn font(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let style = self.style.unwrap_or(theme.kit.ty.mono);
        let color = self.color.unwrap_or(theme.kit.c.text_base);
        let shown = if theme.effect_gate.allows_animation() {
            scramble_frame(&self.target, self.progress, self.seed)
        } else {
            self.target
        };
        text(shown).key(self.key).font(style).color(color)
    }
}

/// Multi-stop animated gradient text using Rune's native [`El::text_gradient`].
pub struct GradientText {
    content: String,
    style: Option<TextStyle>,
    colors: Vec<Color>,
    phase: f32,
}

impl GradientText {
    pub fn new(content: impl Into<String>, colors: Vec<Color>) -> Self {
        Self {
            content: content.into(),
            style: None,
            colors,
            phase: 0.0,
        }
    }

    pub fn font(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn phase(mut self, t: f32) -> Self {
        self.phase = t;
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let style = self.style.unwrap_or(theme.kit.ty.ui_medium);
        let base = text(self.content).font(style);
        if self.colors.len() < 2 {
            let fg = self
                .colors
                .first()
                .copied()
                .unwrap_or(theme.kit.c.text_base);
            return base.color(fg);
        }
        let shift = if theme.effect_gate.allows_animation() {
            (self.phase * 0.25).sin() * 0.18
        } else {
            0.0
        };
        let n = self.colors.len();
        let stops: Vec<(f32, Color)> = self
            .colors
            .into_iter()
            .enumerate()
            .map(|(i, col)| {
                let t = (i as f32 / (n - 1) as f32 + shift).clamp(0.0, 1.0);
                (t, col)
            })
            .collect();
        base.text_gradient(90.0, stops)
    }
}

/// Sweeping specular sheen across text using Rune's native [`El::text_gradient`].
pub struct ShinyText {
    content: String,
    style: Option<TextStyle>,
    base_color: Option<Color>,
    shine_color: Option<Color>,
    phase: f32,
}

impl ShinyText {
    pub fn new(content: impl Into<String>, phase: f32) -> Self {
        Self {
            content: content.into(),
            style: None,
            base_color: None,
            shine_color: None,
            phase,
        }
    }

    pub fn font(mut self, style: TextStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn colors(mut self, base: Color, shine: Color) -> Self {
        self.base_color = Some(base);
        self.shine_color = Some(shine);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let style = self.style.unwrap_or(theme.kit.ty.ui_medium);
        let base_c = self.base_color.unwrap_or(theme.kit.c.text_muted);
        let shine_c = self.shine_color.unwrap_or(theme.kit.c.text_base);
        if !theme.effect_gate.allows_animation() {
            return text(self.content).font(style).color(shine_c);
        }
        let center = (self.phase * 0.45).rem_euclid(1.4) - 0.2;
        let lo = (center - 0.16).clamp(0.0, 1.0);
        let mid = center.clamp(0.0, 1.0);
        let hi = (center + 0.16).clamp(0.0, 1.0);
        let stops = [
            (0.0, base_c),
            (lo, base_c),
            (mid, shine_c),
            (hi, base_c),
            (1.0, base_c),
        ];
        text(self.content).font(style).text_gradient(105.0, stops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scramble_resolves_deterministically_to_target() {
        let mid = scramble_frame("EXO LAUNCHER", 0.5, 42);
        assert_eq!(mid.chars().count(), "EXO LAUNCHER".chars().count());
        assert!(mid.starts_with("EXO LA"));
        assert_eq!(scramble_frame("EXO LAUNCHER", 1.0, 42), "EXO LAUNCHER");
    }
}
