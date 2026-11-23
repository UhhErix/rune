//! Masked password input with a reveal toggle hook, blocked copy/cut/undo and IME disabled.

use std::rc::Rc;

use crate::color::Color;
use crate::edit::TextEdit;
use crate::el::{El, Tx, el, row, text, text_input};
use crate::states::{downcast, edit_fn};
use crate::tok::TextStyle;

type RevealToggleFn = Rc<dyn Fn(crate::states::States<'_>, bool, &mut Tx)>;
type RevealButtonFn = Box<dyn Fn(bool) -> El>;

/// Convenience constructor for a single-line masked [`text_input`] with copy, cut, undo and IME
/// disabled. Pair with [`El::revealed`] or use [`PasswordInput`] for a built-in reveal toggle hook.
pub fn password_input(edit: &TextEdit, style: TextStyle) -> El {
    text_input(edit, style).single_line().secure()
}

/// Builder for a masked password field with an opt-in reveal toggle hook (`on_reveal_toggle`),
/// guaranteed no copy-out and IME disabled.
pub struct PasswordInput {
    edit: TextEdit,
    style: TextStyle,
    key: String,
    label: Option<String>,
    placeholder: Option<(String, Color)>,
    color: Option<Color>,
    caret: Option<(Color, Color)>,
    revealed: bool,
    disabled: bool,
    access: Option<crate::el::EditFn>,
    submit: Option<crate::el::PressFn>,
    change: Option<crate::el::PressFn>,
    reveal_toggle: Option<RevealToggleFn>,
    reveal_button: Option<RevealButtonFn>,
}

impl PasswordInput {
    /// Creates a masked password field backed by `edit` using `style`.
    pub fn new(edit: &TextEdit, style: TextStyle) -> Self {
        let mut copy = edit.clone();
        copy.set_secure(true);
        let revealed = copy.is_revealed();
        Self {
            edit: copy,
            style,
            key: "password".to_owned(),
            label: None,
            placeholder: None,
            color: None,
            caret: None,
            revealed,
            disabled: false,
            access: None,
            submit: None,
            change: None,
            reveal_toggle: None,
            reveal_button: None,
        }
    }

    /// Stable key prefix for the row, inner input (`"{key}-input"`), and toggle button (`"{key}-reveal"`).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Accessible label for the password field (default `"Password"`).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Placeholder text and colour shown while empty.
    pub fn placeholder(mut self, text: impl Into<String>, color: Color) -> Self {
        self.placeholder = Some((text.into(), color));
        self
    }

    /// Text and bullet colour.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Caret and selection colours.
    pub fn caret(mut self, caret: Color, selection: Color) -> Self {
        self.caret = Some((caret, selection));
        self
    }

    /// Overrides whether plaintext characters are currently revealed (still keeping copy/cut/IME off).
    pub fn revealed(mut self, revealed: bool) -> Self {
        self.revealed = revealed;
        self.edit.set_revealed(revealed);
        self
    }

    /// Disables editing and the reveal toggle button.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Routes edits to the app's [`TextEdit`] (automatically enforcing `secure`).
    pub fn on_edit<S: 'static>(
        mut self,
        f: impl for<'a> Fn(&'a mut S) -> &'a mut TextEdit + 'static,
    ) -> Self {
        self.access = Some(Rc::new(edit_fn(move |s| f(downcast::<S>(s)))));
        self
    }

    /// Called when Return is pressed in the field.
    pub fn on_submit<S: 'static>(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.submit = Some(Rc::new(move |s, tx| f(downcast::<S>(s), tx)));
        self
    }

    /// Called after the password text changes.
    pub fn on_change<S: 'static>(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.change = Some(Rc::new(move |s, tx| f(downcast::<S>(s), tx)));
        self
    }

    /// Hook called with the next revealed state (`!self.revealed`) when the reveal button is pressed.
    /// Attaching this mounts the trailing reveal toggle button; press preserves focus in the field.
    pub fn on_reveal_toggle<S: 'static>(
        mut self,
        f: impl Fn(&mut S, bool, &mut Tx) + 'static,
    ) -> Self {
        self.reveal_toggle = Some(Rc::new(move |s, next, tx| f(downcast::<S>(s), next, tx)));
        self
    }

    /// Custom visual element for the reveal toggle button (receives `revealed: bool`).
    pub fn reveal_button(mut self, build: impl Fn(bool) -> El + 'static) -> Self {
        self.reveal_button = Some(Box::new(build));
        self
    }

    /// Builds the password input row element.
    pub fn build(self) -> El {
        let mut field = text_input(&self.edit, self.style)
            .single_line()
            .secure()
            .revealed(self.revealed)
            .key(format!("{}-input", self.key))
            .label(self.label.unwrap_or_else(|| "Password".to_owned()))
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .disabled(self.disabled);
        if let Some((ph, c)) = self.placeholder {
            field = field.placeholder(ph, c);
        }
        if let Some(c) = self.color {
            field = field.color(c);
        }
        if let Some((caret, sel)) = self.caret {
            field = field.caret(caret, sel);
        }
        if !self.disabled {
            if let Some(access) = self.access {
                field
                    .cold_mut()
                    .input
                    .as_deref_mut()
                    .expect("text_input has InputSpec")
                    .access = Some(access);
            }
            if let Some(submit) = self.submit {
                field
                    .cold_mut()
                    .input
                    .as_deref_mut()
                    .expect("text_input has InputSpec")
                    .submit = Some(submit);
            }
            if let Some(change) = self.change {
                field
                    .cold_mut()
                    .input
                    .as_deref_mut()
                    .expect("text_input has InputSpec")
                    .change = Some(change);
            }
        }
        let mut container = row().key(self.key.clone()).center_y().gap(6.0).child(field);
        if let Some(toggle) = self.reveal_toggle {
            let next = !self.revealed;
            let btn_label = if self.revealed {
                "Hide password"
            } else {
                "Show password"
            };
            let glyph = match self.reveal_button {
                Some(build) => build(self.revealed),
                None => text(if self.revealed { "Hide" } else { "Show" })
                    .font(self.style)
                    .color(self.color.unwrap_or(Color::WHITE)),
            };
            let mut btn = el()
                .key(format!("{}-reveal", self.key))
                .role("button")
                .label(btn_label)
                .focusable(!self.disabled)
                .preserve_focus_on_press(true)
                .disabled(self.disabled)
                .center()
                .px(6.0)
                .py(2.0)
                .child(glyph);
            if !self.disabled {
                btn.handlers.on_press = Some(Rc::new(move |s, tx| toggle(s, next, tx)));
            }
            container = container.child(btn);
        }
        container
    }
}

#[cfg(test)]
#[path = "password_tests.rs"]
mod tests;
