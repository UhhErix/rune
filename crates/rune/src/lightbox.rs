//! Full-window modal image lightbox with prev/next navigation, zoom and drag-to-pan.
//!
//! Designed for screenshot galleries and media previews (such as store detail carousels):
//! - Modal overlay (`El::modal_tx`) that traps focus and makes background content inert.
//! - Prev/Next buttons and `Left`/`Right`/`Home`/`End` keyboard navigation (optional wrapping).
//! - Zoom via `+`/`-`/`0` buttons and keys, double-click toggle, pinch (`on_magnify`), and
//!   Ctrl/Cmd + wheel (`on_wheel`), plus clamped drag-to-pan while zoomed.
//! - `Escape` resets zoom first when zoomed, then closes on the next press; clicking the dimmed
//!   backdrop closes immediately (opt-out via [`Lightbox::close_on_backdrop`]).

use std::rc::Rc;

use crate::{
    Color, El, Key, KeyEvent, MagnifyEvent, SurfaceFit, TextStyle, Tx, WheelEvent, col, el, row,
    text,
};

mod state;

use state::ZOOM_EPSILON;
pub use state::{LightboxItem, LightboxState};

type StateGet<S> = Rc<dyn Fn(&S) -> &LightboxState>;
type StateGetMut<S> = Rc<dyn Fn(&mut S) -> &mut LightboxState>;
type CloseCb<S> = Rc<dyn Fn(&mut S, &mut Tx)>;
type IndexCb<S> = Rc<dyn Fn(&mut S, usize, &mut Tx)>;

/// Builder for a full-window modal image lightbox overlay.
///
/// ```ignore
/// col().w_full().h_full()
///     .child(gallery_content(s))
///     .children(
///         Lightbox::new("shots-lightbox", items, |s: &App| &s.lightbox, |s| &mut s.lightbox)
///             .build(s),
///     )
/// ```
pub struct Lightbox<S: 'static> {
    key: String,
    items: Vec<LightboxItem>,
    get: StateGet<S>,
    get_mut: StateGetMut<S>,
    backdrop_color: Color,
    surface_color: Color,
    text_color: Color,
    muted_text_color: Color,
    border_color: Color,
    close_on_backdrop: bool,
    show_counter: bool,
    show_zoom_controls: bool,
    show_nav_buttons: bool,
    on_close: Option<CloseCb<S>>,
    on_index_change: Option<IndexCb<S>>,
}

/// Shorthand constructor for [`Lightbox::new`].
pub fn lightbox<S: 'static>(
    key: impl Into<String>,
    items: Vec<LightboxItem>,
    get: impl Fn(&S) -> &LightboxState + 'static,
    get_mut: impl Fn(&mut S) -> &mut LightboxState + 'static,
) -> Lightbox<S> {
    Lightbox::new(key, items, get, get_mut)
}

impl<S: 'static> Lightbox<S> {
    /// Creates a lightbox overlay bound to `LightboxState` inside app state `S`.
    pub fn new(
        key: impl Into<String>,
        items: Vec<LightboxItem>,
        get: impl Fn(&S) -> &LightboxState + 'static,
        get_mut: impl Fn(&mut S) -> &mut LightboxState + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            items,
            get: Rc::new(get),
            get_mut: Rc::new(get_mut),
            backdrop_color: Color {
                r: 0.04,
                g: 0.04,
                b: 0.06,
                a: 0.9,
            },
            surface_color: Color {
                r: 0.14,
                g: 0.15,
                b: 0.18,
                a: 0.92,
            },
            text_color: Color::WHITE,
            muted_text_color: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.55,
            },
            border_color: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.14,
            },
            close_on_backdrop: true,
            show_counter: true,
            show_zoom_controls: true,
            show_nav_buttons: true,
            on_close: None,
            on_index_change: None,
        }
    }

    /// Background scrim color behind the image stage.
    pub fn backdrop_color(mut self, color: Color) -> Self {
        self.backdrop_color = color;
        self
    }

    /// Surface color of the toolbar and navigation buttons.
    pub fn surface_color(mut self, color: Color) -> Self {
        self.surface_color = color;
        self
    }

    /// Primary text/icon color for controls and captions.
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    /// Border color around floating controls.
    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    /// Whether clicking the dimmed backdrop closes the lightbox (default `true`).
    pub fn close_on_backdrop(mut self, enabled: bool) -> Self {
        self.close_on_backdrop = enabled;
        self
    }

    /// Whether to show the `"1 / N"` slide counter (default `true`).
    pub fn show_counter(mut self, enabled: bool) -> Self {
        self.show_counter = enabled;
        self
    }

    /// Whether to show zoom `-`, `%` reset, and `+` buttons (default `true`).
    pub fn show_zoom_controls(mut self, enabled: bool) -> Self {
        self.show_zoom_controls = enabled;
        self
    }

    /// Whether to show left/right Prev/Next buttons when multiple items exist (default `true`).
    pub fn show_nav_buttons(mut self, enabled: bool) -> Self {
        self.show_nav_buttons = enabled;
        self
    }

    /// Called after the lightbox closes (via Escape, Close button, or backdrop click).
    pub fn on_close(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// Called whenever prev/next/keyboard navigation changes the active slide index.
    pub fn on_index_change(mut self, f: impl Fn(&mut S, usize, &mut Tx) + 'static) -> Self {
        self.on_index_change = Some(Rc::new(f));
        self
    }

    /// Builds the modal overlay [`El`] when open (and `items` is non-empty), or `None` when closed.
    pub fn build(self, state: &S) -> Option<El> {
        let lb = (self.get)(state);
        let count = self.items.len();
        let idx = lb.current_index(count)?;
        let item = &self.items[idx];
        let zoom = lb.zoom();
        let pan = lb.pan();
        let can_prev = lb.can_prev(count);
        let can_next = lb.can_next(count);
        let zoom_pct = (zoom * 100.0).round() as i32;

        let root_key = self.key;
        let get_mut = self.get_mut;
        let on_close = self.on_close;
        let on_index = self.on_index_change;

        // Backdrop scrim (fills whole window behind controls & image stage).
        let mut backdrop = el()
            .key(format!("{root_key}-backdrop"))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(self.backdrop_color);
        if self.close_on_backdrop {
            let get_mut_b = get_mut.clone();
            let on_close_b = on_close.clone();
            backdrop = backdrop.on_press_tx(move |s: &mut S, tx| {
                get_mut_b(s).close();
                if let Some(cb) = &on_close_b {
                    cb(s, tx);
                }
            });
        }

        // Top bar: slide counter on left, zoom controls + close on right.
        let control_font = TextStyle::new(13.0, 500.0);
        let pill = |k: String, label: &'static str, caption: &str, enabled: bool| {
            let fg = if enabled {
                self.text_color
            } else {
                self.muted_text_color
            };
            el().key(k)
                .role("button")
                .label(label)
                .focusable(enabled)
                .h(32.0)
                .min_w(32.0)
                .px(10.0)
                .radius(8.0)
                .fill(self.surface_color)
                .border(1.0, self.border_color)
                .justify_center()
                .items_center()
                .child(text(caption).font(control_font.clone()).color(fg))
        };

        let mut top_bar = row()
            .key(format!("{root_key}-topbar"))
            .w_full()
            .h(52.0)
            .px(16.0)
            .items_center()
            .justify_between();

        if self.show_counter {
            top_bar = top_bar.child(
                el().key(format!("{root_key}-counter"))
                    .px(10.0)
                    .h(28.0)
                    .radius(8.0)
                    .fill(self.surface_color)
                    .border(1.0, self.border_color)
                    .justify_center()
                    .items_center()
                    .child(
                        text(format!("{} / {}", idx + 1, count))
                            .font(control_font.clone())
                            .color(self.text_color),
                    ),
            );
        } else {
            top_bar = top_bar.child(el().w(1.0).h(1.0));
        }

        let mut actions = row().gap(8.0).items_center();
        if self.show_zoom_controls {
            let can_zoom_out = zoom > lb.min_zoom() + ZOOM_EPSILON;
            let can_zoom_in = zoom < lb.max_zoom() - ZOOM_EPSILON;
            let gm_out = get_mut.clone();
            let gm_reset = get_mut.clone();
            let gm_in = get_mut.clone();
            actions = actions
                .child(
                    pill(
                        format!("{root_key}-zoom-out"),
                        "Zoom out",
                        "-",
                        can_zoom_out,
                    )
                    .on_press(move |s: &mut S| {
                        gm_out(s).zoom_out();
                    }),
                )
                .child(
                    pill(
                        format!("{root_key}-zoom-reset"),
                        "Reset zoom",
                        &format!("{zoom_pct}%"),
                        true,
                    )
                    .on_press(move |s: &mut S| {
                        gm_reset(s).reset_zoom();
                    }),
                )
                .child(
                    pill(format!("{root_key}-zoom-in"), "Zoom in", "+", can_zoom_in).on_press(
                        move |s: &mut S| {
                            gm_in(s).zoom_in();
                        },
                    ),
                );
        }

        let gm_close = get_mut.clone();
        let on_close_btn = on_close.clone();
        actions = actions.child(
            pill(format!("{root_key}-close"), "Close viewer", "×", true).on_press_tx(
                move |s: &mut S, tx| {
                    gm_close(s).close();
                    if let Some(cb) = &on_close_btn {
                        cb(s, tx);
                    }
                },
            ),
        );
        top_bar = top_bar.child(actions);

        // Center image stage: supports double-click zoom, drag-to-pan when zoomed, pinch & Ctrl+wheel.
        let mut image_el = el()
            .key(format!("{root_key}-image"))
            .w_full()
            .h_full()
            .pixels_fit(&item.pixels, SurfaceFit::Fit)
            .scale(zoom)
            .offset(pan[0], pan[1]);
        if let Some(alt) = &item.alt {
            image_el = image_el.role("img").label(alt.clone());
        }

        let gm_drag = get_mut.clone();
        let gm_dbl = get_mut.clone();
        let gm_mag = get_mut.clone();
        let gm_wheel = get_mut.clone();
        let stage = el()
            .key(format!("{root_key}-stage"))
            .grow(1.0)
            .h_full()
            .clip_rounded(0.0)
            .justify_center()
            .items_center()
            .on_drag(move |s: &mut S, drag, _| {
                gm_drag(s).handle_stage_drag(drag);
            })
            .on_double_press(move |s: &mut S| {
                gm_dbl(s).toggle_zoom();
            })
            .on_magnify(move |s: &mut S, ev: MagnifyEvent, _| {
                let factor = (1.0 + ev.delta).max(0.1);
                gm_mag(s).zoom_by(factor);
                true
            })
            .on_wheel(move |s: &mut S, ev: WheelEvent, _| {
                if ev.mods.ctrl || ev.mods.cmd {
                    let factor = (1.0 + ev.dy / 200.0).max(0.1);
                    gm_wheel(s).zoom_by(factor);
                    true
                } else {
                    false
                }
            })
            .child(image_el);

        // Middle band: optional Prev / Next buttons flanking the image stage.
        let mut middle = row()
            .key(format!("{root_key}-middle"))
            .grow(1.0)
            .w_full()
            .px(16.0)
            .gap(12.0)
            .items_center();

        if self.show_nav_buttons && count > 1 {
            let gm_prev = get_mut.clone();
            let oi_prev = on_index.clone();
            let mut prev_btn = pill(format!("{root_key}-prev"), "Previous image", "<", can_prev)
                .h(44.0)
                .min_w(44.0);
            if can_prev {
                prev_btn = prev_btn.on_press_tx(move |s: &mut S, tx| {
                    let st = gm_prev(s);
                    if st.prev(count)
                        && let Some(new_idx) = st.current_index(count)
                        && let Some(cb) = &oi_prev
                    {
                        cb(s, new_idx, tx);
                    }
                });
            }
            middle = middle.child(prev_btn);
        }

        middle = middle.child(stage);

        if self.show_nav_buttons && count > 1 {
            let gm_next = get_mut.clone();
            let oi_next = on_index.clone();
            let mut next_btn = pill(format!("{root_key}-next"), "Next image", ">", can_next)
                .h(44.0)
                .min_w(44.0);
            if can_next {
                next_btn = next_btn.on_press_tx(move |s: &mut S, tx| {
                    let st = gm_next(s);
                    if st.next(count)
                        && let Some(new_idx) = st.current_index(count)
                        && let Some(cb) = &oi_next
                    {
                        cb(s, new_idx, tx);
                    }
                });
            }
            middle = middle.child(next_btn);
        }

        // Foreground chrome column laid over the backdrop scrim.
        let mut chrome = col()
            .key(format!("{root_key}-chrome"))
            .w_full()
            .h_full()
            .child(top_bar)
            .child(middle);

        if let Some(caption) = &item.caption {
            chrome = chrome.child(
                row()
                    .key(format!("{root_key}-caption"))
                    .w_full()
                    .h(44.0)
                    .px(24.0)
                    .justify_center()
                    .items_center()
                    .child(
                        text(caption.as_str())
                            .font(control_font)
                            .color(self.muted_text_color),
                    ),
            );
        } else {
            chrome = chrome.child(el().w_full().h(16.0));
        }

        // Keyboard shortcuts on the modal root.
        let gm_key = get_mut.clone();
        let oi_key = on_index;
        let gm_dismiss = get_mut;
        let on_close_dismiss = on_close;
        let focus_back = root_key.clone();

        Some(
            el().key(root_key)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .role("dialog")
                .label("Image viewer")
                .focusable(true)
                .tab_index(-1)
                .autofocus()
                .modal_tx(false, move |s: &mut S, tx| {
                    if gm_dismiss(s).handle_escape() {
                        if let Some(cb) = &on_close_dismiss {
                            cb(s, tx);
                        }
                    } else {
                        tx.focus(focus_back.clone(), false);
                    }
                })
                .on_key(move |s: &mut S, ev: &KeyEvent, tx| {
                    if ev.mods.alt || ev.mods.logo {
                        return false;
                    }
                    let st = gm_key(s);
                    match &ev.key {
                        Key::Left => {
                            if st.prev(count)
                                && let Some(new_idx) = st.current_index(count)
                                && let Some(cb) = &oi_key
                            {
                                cb(s, new_idx, tx);
                            }
                            true
                        }
                        Key::Right => {
                            if st.next(count)
                                && let Some(new_idx) = st.current_index(count)
                                && let Some(cb) = &oi_key
                            {
                                cb(s, new_idx, tx);
                            }
                            true
                        }
                        Key::Home => {
                            if st.go_to(0, count)
                                && let Some(cb) = &oi_key
                            {
                                cb(s, 0, tx);
                            }
                            true
                        }
                        Key::End => {
                            let last = count.saturating_sub(1);
                            if st.go_to(last, count)
                                && let Some(cb) = &oi_key
                            {
                                cb(s, last, tx);
                            }
                            true
                        }
                        Key::Text(t) if t == "+" || t == "=" => {
                            st.zoom_in();
                            true
                        }
                        Key::Text(t) if t == "-" || t == "_" => {
                            st.zoom_out();
                            true
                        }
                        Key::Text(t) if t == "0" => {
                            st.reset_zoom();
                            true
                        }
                        Key::Escape if st.is_zoomed() => {
                            st.reset_zoom();
                            true
                        }
                        _ => false,
                    }
                })
                .child(backdrop)
                .child(chrome),
        )
    }
}

#[cfg(test)]
mod tests;
