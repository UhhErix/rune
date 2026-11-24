//! Radix-style Dialog and AlertDialog with focus trapping, return focus, backdrop scrim,
//! header/body/footer slots, and optional Liquid Glass surface.

use rune::{Color, El, col, el, icon, row, spacer, text};

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

type CloseFn = Box<dyn FnOnce(El) -> El>;

pub struct UiDialog {
    key: String,
    title: String,
    description: Option<String>,
    open: bool,
    alert: bool,
    width: f32,
    body: Vec<El>,
    footer: Vec<El>,
    on_close_modal: Option<CloseFn>,
    on_close_btn: Option<CloseFn>,
}

impl UiDialog {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: None,
            open: false,
            alert: false,
            width: 460.0,
            body: Vec::new(),
            footer: Vec::new(),
            on_close_modal: None,
            on_close_btn: None,
        }
    }

    /// Creates a non-dismissable AlertDialog (requires explicit footer action).
    pub fn alert(key: impl Into<String>, title: impl Into<String>) -> Self {
        let mut d = Self::new(key, title);
        d.alert = true;
        d
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w.max(280.0);
        self
    }

    pub fn child(mut self, child: El) -> Self {
        self.body.push(child);
        self
    }

    pub fn footer_action(mut self, action: El) -> Self {
        self.footer.push(action);
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        let f_btn = f.clone();
        self.on_close_btn = Some(Box::new(move |e: El| e.on_press(f_btn)));
        self.on_close_modal = Some(Box::new(move |e: El| e.modal(true, f)));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) {
        if !self.open {
            return;
        }
        let k = &theme.kit;
        let c = &k.c;

        let mut header_copy = col().grow(1.0).shrink().gap(4.0).child(
            text(self.title.clone())
                .font(k.ty.ui_medium)
                .color(c.text_base),
        );
        if let Some(desc) = &self.description {
            header_copy = header_copy.child(
                text(desc.clone())
                    .font(k.ty.small)
                    .color(c.text_muted)
                    .wrap(),
            );
        }

        let mut header = row()
            .w_full()
            .px(20.0)
            .pt(18.0)
            .pb(12.0)
            .items_start()
            .child(header_copy);
        if !self.alert
            && let Some(close_btn) = self.on_close_btn
        {
            let btn = close_btn(
                el().key(format!("{}-close", self.key))
                    .size(26.0, 26.0)
                    .center()
                    .radius(theme.radii.pill)
                    .hover(theme.hover_fill)
                    .role("button")
                    .label("Close")
                    .child(icon(k.glyph.close, 13.0).color(c.icon_muted)),
            );
            header = header.child(btn);
        }

        let mut panel = col()
            .key(format!("{}-dialog", self.key))
            .w(self.width)
            .role(if self.alert { "alertdialog" } else { "dialog" })
            .label(self.title)
            .child(header)
            .child(
                col()
                    .w_full()
                    .px(20.0)
                    .pb(16.0)
                    .gap(10.0)
                    .children(self.body),
            );

        if !self.footer.is_empty() {
            panel = panel
                .child(el().w_full().h(1.0).fill(theme.hairline))
                .child(
                    row()
                        .w_full()
                        .px(20.0)
                        .py(14.0)
                        .gap(8.0)
                        .child(spacer())
                        .children(self.footer),
                );
        }

        panel = theme
            .overlay_surface(panel, theme.radii.dialog)
            .motion(theme.springs.base);
        panel = if self.alert {
            panel.modal_trap()
        } else if let Some(close) = self.on_close_modal {
            close(panel)
        } else {
            panel.modal_trap()
        };

        let scrim = el()
            .key(format!("{}-scrim", self.key))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(Color::BLACK.alpha(0.58))
            .a11y_hidden();

        let center_wrap = col()
            .key(format!("{}-dialog-layer", self.key))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .padding(20.0)
            .center()
            .child(panel);

        portals.push(LayerSlot::Modal, scrim);
        portals.push(LayerSlot::Modal, center_wrap);
    }
}
