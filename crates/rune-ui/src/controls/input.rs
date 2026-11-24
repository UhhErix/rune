//! HeroUI / Mantine / shadcn-style text input and textarea with label, description,
//! validation error, leading/trailing icons, clearable action, password visibility toggle,
//! and character counter.

use rune::{El, Icon, TextEdit, Tx, col, el, icon, row, text, text_input};

use crate::controls::button::UiSize;
use crate::theme::UiTheme;

type EditBinder = Box<dyn FnOnce(El) -> El>;

pub struct UiInput<'a> {
    key: String,
    edit: &'a TextEdit,
    label: Option<String>,
    placeholder: Option<String>,
    description: Option<String>,
    error: Option<String>,
    size: UiSize,
    leading: Option<&'static Icon>,
    trailing: Option<&'static Icon>,
    clearable: bool,
    password: bool,
    password_visible: bool,
    max_len: Option<usize>,
    disabled: bool,
    read_only: bool,
    on_edit: Option<EditBinder>,
    on_submit: Option<EditBinder>,
    on_clear: Option<EditBinder>,
    on_toggle_password: Option<EditBinder>,
}

impl<'a> UiInput<'a> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit) -> Self {
        Self {
            key: key.into(),
            edit,
            label: None,
            placeholder: None,
            description: None,
            error: None,
            size: UiSize::Md,
            leading: None,
            trailing: None,
            clearable: false,
            password: false,
            password_visible: false,
            max_len: None,
            disabled: false,
            read_only: false,
            on_edit: None,
            on_submit: None,
            on_clear: None,
            on_toggle_password: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn error(mut self, error: impl Into<String>) -> Self {
        let s = error.into();
        self.error = if s.is_empty() { None } else { Some(s) };
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

    pub fn clearable(mut self, enabled: bool) -> Self {
        self.clearable = enabled;
        self
    }

    pub fn password(mut self, masked: bool, visible: bool) -> Self {
        self.password = masked;
        self.password_visible = visible;
        self
    }

    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn on_edit<S: 'static>(
        mut self,
        f: impl for<'s> Fn(&'s mut S) -> &'s mut TextEdit + 'static,
    ) -> Self {
        self.on_edit = Some(Box::new(move |e| e.on_edit(f)));
        self
    }

    pub fn on_submit<S: 'static>(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_submit = Some(Box::new(move |e| e.on_submit(f)));
        self
    }

    pub fn on_clear<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear = Some(Box::new(move |e| e.on_press(f)));
        self
    }

    pub fn on_toggle_password<S: 'static>(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_password = Some(Box::new(move |e| e.on_press(f)));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let invalid = self.error.is_some();
        let h = self.size.height();
        let icon_sz = self.size.icon_size();

        let a11y_name = self.label.clone().unwrap_or_else(|| self.key.clone());
        let mut field = text_input(self.edit, k.ty.ui_compact)
            .key(format!("{}-field", self.key))
            .single_line()
            .label(a11y_name)
            .color(c.text_base)
            .grow(1.0)
            .shrink()
            .disabled(self.disabled)
            .read_only(self.read_only)
            .invalid(invalid);

        if let Some(ph) = self.placeholder {
            field = field.placeholder(ph, c.text_muted);
        }
        if self.password {
            field = field.revealed(self.password_visible);
        }
        if !self.disabled {
            if let Some(bind) = self.on_edit {
                field = bind(field);
            }
            if let Some(bind) = self.on_submit {
                field = bind(field);
            }
        }

        let mut shell = row()
            .key(format!("{}-shell", self.key))
            .w_full()
            .h(h)
            .px(10.0)
            .gap(8.0)
            .center_y()
            .radius(theme.radii.control);

        if let Some(glyph) = self.leading {
            shell = shell.child(icon(glyph, icon_sz).color(c.icon_muted));
        }
        shell = shell.child(field);

        if self.clearable && !self.edit.text.is_empty() && !self.disabled && !self.read_only {
            let mut btn = el()
                .key(format!("{}-clear", self.key))
                .size(20.0, 20.0)
                .center()
                .radius(theme.radii.pill)
                .hover(theme.hover_fill)
                .role("button")
                .label("Clear")
                .child(icon(k.glyph.clear, 12.0).color(c.icon_muted));
            if let Some(cb) = self.on_clear {
                btn = cb(btn);
            }
            shell = shell.child(btn);
        }

        if self.password && !self.disabled {
            let eye = if self.password_visible {
                k.glyph.eye_off
            } else {
                k.glyph.eye
            };
            let mut btn = el()
                .key(format!("{}-eye", self.key))
                .size(22.0, 22.0)
                .center()
                .radius(theme.radii.pill)
                .hover(theme.hover_fill)
                .role("button")
                .label(if self.password_visible {
                    "Hide password"
                } else {
                    "Show password"
                })
                .child(icon(eye, 13.0).color(c.icon_muted));
            if let Some(cb) = self.on_toggle_password {
                btn = cb(btn);
            }
            shell = shell.child(btn);
        }

        if let Some(glyph) = self.trailing {
            shell = shell.child(icon(glyph, icon_sz).color(c.icon_muted));
        }

        let border_color = if invalid {
            c.danger.fg
        } else if self.edit.focused {
            c.border_strong
        } else {
            theme.float_line
        };

        shell = if theme.glass.control.is_some() && theme.effect_gate.gpu_enabled {
            theme
                .control_surface(shell, theme.radii.control)
                .border(1.0, border_color)
        } else {
            shell.fill(c.bg_base).border(1.0, border_color)
        };
        if self.disabled {
            shell = shell.opacity(0.45);
        }

        let mut root = col().key(self.key).w_full().gap(5.0);
        if let Some(lbl) = self.label {
            let mut header = row().w_full().justify_between().center_y();
            header = header.child(text(lbl).font(k.ty.small_medium).color(c.text_base));
            if let Some(max) = self.max_len {
                let count = self.edit.text.chars().count();
                let count_color = if count > max {
                    c.danger.fg
                } else {
                    c.text_muted
                };
                header = header.child(
                    text(format!("{count}/{max}"))
                        .font(k.ty.small)
                        .color(count_color),
                );
            }
            root = root.child(header);
        }
        root = root.child(shell);
        if let Some(err) = self.error {
            root = root.child(text(err).font(k.ty.small).color(c.danger.fg));
        } else if let Some(desc) = self.description {
            root = root.child(text(desc).font(k.ty.small).color(c.text_muted));
        }
        root
    }
}

/// Multi-line text area with label, error, and character counter.
pub struct UiTextArea<'a> {
    key: String,
    edit: &'a TextEdit,
    label: Option<String>,
    placeholder: Option<String>,
    error: Option<String>,
    min_height: f32,
    max_len: Option<usize>,
    disabled: bool,
    on_edit: Option<EditBinder>,
}

impl<'a> UiTextArea<'a> {
    pub fn new(key: impl Into<String>, edit: &'a TextEdit) -> Self {
        Self {
            key: key.into(),
            edit,
            label: None,
            placeholder: None,
            error: None,
            min_height: 88.0,
            max_len: None,
            disabled: false,
            on_edit: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn error(mut self, error: impl Into<String>) -> Self {
        let s = error.into();
        self.error = if s.is_empty() { None } else { Some(s) };
        self
    }

    pub fn min_height(mut self, h: f32) -> Self {
        self.min_height = h.max(44.0);
        self
    }

    pub fn max_len(mut self, max: usize) -> Self {
        self.max_len = Some(max);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_edit<S: 'static>(
        mut self,
        f: impl for<'s> Fn(&'s mut S) -> &'s mut TextEdit + 'static,
    ) -> Self {
        self.on_edit = Some(Box::new(move |e| e.on_edit(f)));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let invalid = self.error.is_some();

        let a11y_name = self.label.clone().unwrap_or_else(|| self.key.clone());
        let mut area = text_input(self.edit, k.ty.ui)
            .key(format!("{}-area", self.key))
            .label(a11y_name)
            .color(c.text_base)
            .w_full()
            .min_h(self.min_height)
            .disabled(self.disabled)
            .invalid(invalid);
        if let Some(ph) = self.placeholder {
            area = area.placeholder(ph, c.text_muted);
        }
        if !self.disabled
            && let Some(bind) = self.on_edit
        {
            area = bind(area);
        }

        let border = if invalid {
            c.danger.fg
        } else if self.edit.focused {
            c.border_strong
        } else {
            theme.float_line
        };

        let mut shell = el()
            .w_full()
            .padding(10.0)
            .radius(theme.radii.control)
            .fill(c.bg_base)
            .border(1.0, border)
            .child(area);
        if self.disabled {
            shell = shell.opacity(0.45);
        }

        let mut root = col().key(self.key).w_full().gap(5.0);
        if let Some(lbl) = self.label {
            let mut header = row().w_full().justify_between().center_y();
            header = header.child(text(lbl).font(k.ty.small_medium).color(c.text_base));
            if let Some(max) = self.max_len {
                let count = self.edit.text.chars().count();
                header = header.child(text(format!("{count}/{max}")).font(k.ty.small).color(
                    if count > max {
                        c.danger.fg
                    } else {
                        c.text_muted
                    },
                ));
            }
            root = root.child(header);
        }
        root = root.child(shell);
        if let Some(err) = self.error {
            root = root.child(text(err).font(k.ty.small).color(c.danger.fg));
        }
        root
    }
}
