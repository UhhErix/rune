//! One-time-code (OTP / 2FA / Steam Guard) input: N single-character boxes with auto-advance,
//! backspace step-back, arrow navigation, and full-code paste distribution across all boxes.

use std::rc::Rc;

use crate::color::Color;
use crate::edit::{Key, KeyEvent, TextEdit};
use crate::el::{Align, El, Tx, el, row, text_input};
use crate::tok::TextStyle;

type OtpAccess<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut OtpState>;
type OtpCallback<S> = Rc<dyn Fn(&mut S, &str, &mut Tx)>;

/// Character filter applied to typed and pasted OTP input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OtpKind {
    /// Decimal digits `'0'..='9'` only (standard 6-digit TOTP / SMS / email codes).
    #[default]
    Digits,
    /// ASCII letters and digits, preserving case.
    Alphanumeric,
    /// ASCII letters and digits, automatically uppercased (e.g., 5-character Steam Guard codes).
    AlphanumericUpper,
}

impl OtpKind {
    /// Normalizes `c` if accepted by this kind, returning `None` for ignored characters
    /// (such as whitespace, dashes, or non-digits in [`OtpKind::Digits`]).
    pub fn normalize(self, c: char) -> Option<char> {
        match self {
            Self::Digits => c.is_ascii_digit().then_some(c),
            Self::Alphanumeric => c.is_ascii_alphanumeric().then_some(c),
            Self::AlphanumericUpper => c.is_ascii_alphanumeric().then(|| c.to_ascii_uppercase()),
        }
    }
}

/// App-owned state for an N-box one-time-code input ([`OtpInput`]).
#[derive(Clone, Debug)]
pub struct OtpState {
    slots: Vec<TextEdit>,
    kind: OtpKind,
    masked: bool,
}

impl OtpState {
    /// Creates an empty OTP state with `len` boxes (clamped to at least 1), defaulting to [`OtpKind::Digits`].
    pub fn new(len: usize) -> Self {
        let len = len.max(1);
        let slots = (0..len).map(|_| TextEdit::new("")).collect();
        Self {
            slots,
            kind: OtpKind::Digits,
            masked: false,
        }
    }

    /// Sets the character filter (digits, alphanumeric, or uppercase alphanumeric).
    pub fn kind(mut self, kind: OtpKind) -> Self {
        self.kind = kind;
        self
    }

    /// Masks entered characters as bullets (disabling copy-out and IME on every box, like a PIN).
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        for slot in &mut self.slots {
            slot.set_secure(masked);
        }
        self
    }

    /// Number of boxes.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether every box is currently empty.
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|s| s.text.is_empty())
    }

    /// Whether every box has a character.
    pub fn is_complete(&self) -> bool {
        self.slots.iter().all(|s| !s.text.is_empty())
    }

    /// Concatenated value of all filled boxes.
    pub fn value(&self) -> String {
        let mut out = String::with_capacity(self.slots.len());
        for s in &self.slots {
            if let Some(ch) = s.text.chars().next() {
                out.push(ch);
            }
        }
        out
    }

    /// Character currently in box `index`, if any.
    pub fn slot(&self, index: usize) -> Option<char> {
        self.slots.get(index).and_then(|s| s.text.chars().next())
    }

    /// Clears all boxes.
    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.text.clear();
            slot.anchor = 0;
            slot.focus = 0;
            slot.sync_invariants();
        }
    }

    /// Fills boxes from box `0` with accepted characters from `text`, clearing remaining boxes.
    pub fn set_value(&mut self, text: &str) {
        self.clear();
        let chars: Vec<char> = text
            .chars()
            .filter_map(|c| self.kind.normalize(c))
            .collect();
        for (i, ch) in chars.into_iter().take(self.slots.len()).enumerate() {
            self.set_slot(i, Some(ch));
        }
    }

    /// Distributes pasted `text` across the boxes starting at `index` (or at `0` when the pasted
    /// accepted code fills the whole input). Returns the next slot index to focus when anything
    /// changed.
    pub fn paste_at(&mut self, index: usize, text: &str) -> Option<usize> {
        let chars: Vec<char> = text
            .chars()
            .filter_map(|c| self.kind.normalize(c))
            .collect();
        if chars.is_empty() {
            return None;
        }
        let len = self.slots.len();
        let start = if chars.len() >= len || index >= len {
            0
        } else {
            index
        };
        let mut last = start;
        for (offset, ch) in chars.into_iter().enumerate() {
            let pos = start + offset;
            if pos >= len {
                break;
            }
            self.set_slot(pos, Some(ch));
            last = pos;
        }
        let next = (last + 1).min(len - 1);
        Some(next)
    }

    fn set_slot(&mut self, index: usize, ch: Option<char>) {
        if let Some(slot) = self.slots.get_mut(index) {
            slot.text.clear();
            if let Some(c) = ch {
                slot.text.push(c);
            }
            slot.anchor = slot.text.len();
            slot.focus = slot.text.len();
            slot.sync_invariants();
            if self.masked {
                slot.set_secure(true);
            }
        }
    }
}

/// Builder for an N-box one-time-code input backed by [`OtpState`].
pub struct OtpInput<S> {
    state: OtpState,
    key: String,
    label: String,
    box_size: [f32; 2],
    gap: f32,
    radius: f32,
    fill: Color,
    border: Color,
    focus_border: Color,
    color: Color,
    text_style: TextStyle,
    disabled: bool,
    access: Option<OtpAccess<S>>,
    on_change: Option<OtpCallback<S>>,
    on_complete: Option<OtpCallback<S>>,
}

impl<S: 'static> OtpInput<S> {
    /// Creates an OTP input row for `state`.
    pub fn new(state: &OtpState) -> Self {
        Self {
            state: state.clone(),
            key: "otp".to_owned(),
            label: "One-time code".to_owned(),
            box_size: [40.0, 48.0],
            gap: 8.0,
            radius: 8.0,
            fill: Color::WHITE.alpha(0.06),
            border: Color::WHITE.alpha(0.16),
            focus_border: Color {
                r: 0.38,
                g: 0.65,
                b: 1.0,
                a: 0.9,
            },
            color: Color::WHITE,
            text_style: TextStyle::new(20.0, 600.0).lh(24.0),
            disabled: false,
            access: None,
            on_change: None,
            on_complete: None,
        }
    }

    /// Key prefix for the row and its boxes (`"{key}-0"`, `"{key}-1"`, ...).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Accessible name prefix for each slot (`"{label} 1 of N"`).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Width and height of each character box in points.
    pub fn box_size(mut self, w: f32, h: f32) -> Self {
        self.box_size = [w.max(16.0), h.max(16.0)];
        self
    }

    /// Spacing between boxes in points.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }

    /// Corner radius of each box in points.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(0.0);
        self
    }

    /// Background fill colour of each box.
    pub fn fill(mut self, fill: Color) -> Self {
        self.fill = fill;
        self
    }

    /// Resting and focused border colours of each box.
    pub fn border(mut self, rest: Color, focused: Color) -> Self {
        self.border = rest;
        self.focus_border = focused;
        self
    }

    /// Text colour inside each box.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Font style of the character in each box.
    pub fn text_style(mut self, style: TextStyle) -> Self {
        self.text_style = style;
        self
    }

    /// Disables editing on all boxes.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Lens into the app's [`OtpState`].
    pub fn on_state(mut self, f: impl for<'a> Fn(&'a mut S) -> &'a mut OtpState + 'static) -> Self {
        self.access = Some(Rc::new(f));
        self
    }

    /// Called whenever any slot changes, with the current concatenated value.
    pub fn on_change(mut self, f: impl Fn(&mut S, &str, &mut Tx) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Called when all N boxes become filled (or Return is pressed while complete).
    pub fn on_complete(mut self, f: impl Fn(&mut S, &str, &mut Tx) + 'static) -> Self {
        self.on_complete = Some(Rc::new(f));
        self
    }

    /// Builds the N-box OTP input row.
    pub fn build(self) -> El {
        let count = self.state.len();
        let mut strip = row().key(self.key.clone()).gap(self.gap).center_y();
        for i in 0..count {
            let slot_key = format!("{}-{i}", self.key);
            let slot_edit = &self.state.slots[i];
            let focused = slot_edit.focused && !self.disabled;
            let border_color = if focused {
                self.focus_border
            } else {
                self.border
            };
            let mut input = text_input(slot_edit, self.text_style)
                .single_line()
                .key(slot_key.clone())
                .label(format!("{} {} of {count}", self.label, i + 1))
                .align(Align::Center)
                .w_full()
                .color(self.color)
                .caret(self.color, self.focus_border.alpha(0.3))
                .disabled(self.disabled);
            if self.state.masked {
                input = input.secure();
            }
            if !self.disabled
                && let Some(access) = &self.access
            {
                let acc_edit = access.clone();
                input = input.on_edit(move |s: &mut S| &mut acc_edit(s).slots[i]);

                let acc_key = access.clone();
                let on_change_k = self.on_change.clone();
                let on_complete_k = self.on_complete.clone();
                let prefix_k = self.key.clone();
                input = input.on_input_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| {
                    if ev.mods.cmd || ev.mods.ctrl || ev.mods.alt || ev.mods.logo {
                        return false;
                    }
                    let otp = acc_key(s);
                    let len = otp.len();
                    match &ev.key {
                        Key::Text(t) => {
                            let chars: Vec<char> =
                                t.chars().filter_map(|c| otp.kind.normalize(c)).collect();
                            if chars.is_empty() {
                                return true;
                            }
                            let next_focus = if chars.len() == 1 {
                                otp.set_slot(i, Some(chars[0]));
                                (i + 1 < len).then_some(i + 1)
                            } else {
                                otp.paste_at(i, t)
                            };
                            let val = otp.value();
                            let complete = otp.is_complete();
                            if let Some(next) = next_focus {
                                tx.focus(format!("{prefix_k}-{next}"), false);
                            }
                            if let Some(cb) = &on_change_k {
                                cb(s, &val, tx);
                            }
                            if complete && let Some(cb) = &on_complete_k {
                                cb(s, &val, tx);
                            }
                            true
                        }
                        Key::Backspace => {
                            let changed = if otp.slot(i).is_some() {
                                otp.set_slot(i, None);
                                true
                            } else if i > 0 {
                                otp.set_slot(i - 1, None);
                                tx.focus(format!("{prefix_k}-{}", i - 1), false);
                                true
                            } else {
                                false
                            };
                            if changed {
                                let val = otp.value();
                                if let Some(cb) = &on_change_k {
                                    cb(s, &val, tx);
                                }
                            }
                            true
                        }
                        Key::Delete => {
                            if otp.slot(i).is_some() {
                                otp.set_slot(i, None);
                                let val = otp.value();
                                if let Some(cb) = &on_change_k {
                                    cb(s, &val, tx);
                                }
                            }
                            true
                        }
                        Key::Left => {
                            if i > 0 {
                                tx.focus(format!("{prefix_k}-{}", i - 1), false);
                            }
                            true
                        }
                        Key::Right => {
                            if i + 1 < len {
                                tx.focus(format!("{prefix_k}-{}", i + 1), false);
                            }
                            true
                        }
                        Key::Home => {
                            tx.focus(format!("{prefix_k}-0"), false);
                            true
                        }
                        Key::End => {
                            tx.focus(format!("{prefix_k}-{}", len - 1), false);
                            true
                        }
                        Key::Enter => {
                            if otp.is_complete()
                                && let Some(cb) = &on_complete_k
                            {
                                let val = otp.value();
                                cb(s, &val, tx);
                            }
                            true
                        }
                        _ => false,
                    }
                });

                let acc_paste = access.clone();
                let on_change_p = self.on_change.clone();
                let on_complete_p = self.on_complete.clone();
                let prefix_p = self.key.clone();
                input = input.on_input_paste(move |s: &mut S, content, tx| {
                    let otp = acc_paste(s);
                    let Some(next) = otp.paste_at(i, content.text()) else {
                        return true;
                    };
                    let val = otp.value();
                    let complete = otp.is_complete();
                    tx.focus(format!("{prefix_p}-{next}"), false);
                    if let Some(cb) = &on_change_p {
                        cb(s, &val, tx);
                    }
                    if complete && let Some(cb) = &on_complete_p {
                        cb(s, &val, tx);
                    }
                    true
                });
            }
            let focus_key = slot_key.clone();
            let mut cell = el()
                .key(format!("{}-box-{i}", self.key))
                .size(self.box_size[0], self.box_size[1])
                .radius(self.radius)
                .fill(self.fill)
                .border(1.5, border_color)
                .center()
                .child(input);
            if !self.disabled {
                cell = cell.on_press_tx(move |_: &mut S, tx| tx.focus(focus_key.clone(), false));
            }
            strip = strip.child(cell);
        }
        strip
    }
}

#[cfg(test)]
#[path = "otp_tests.rs"]
mod tests;
