//! Mantine/HeroUI Pagination (with smart ellipsis windows) and Embla/shadcn Carousel
//! (with drag/fling velocity snapping, loop mode, arrows, and dot indicators).

use std::rc::Rc;

use rune::{Color, Drag, El, Key, KeyEvent, Phase, Tx, col, el, row, text};
use rune_motion::{VelocityTracker, deceleration, project};

use crate::theme::UiTheme;

/// Item emitted by [`pagination_items`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageItem {
    Page(usize),
    Ellipsis,
}

/// Computes Mantine/HeroUI smart pagination sequence (1-based page numbers with siblings & boundaries).
pub fn pagination_items(
    current: usize,
    total: usize,
    siblings: usize,
    boundaries: usize,
) -> Vec<PageItem> {
    if total == 0 {
        return Vec::new();
    }
    let cur = current.clamp(1, total);
    let total_numbers = siblings * 2 + 3 + boundaries * 2;
    if total <= total_numbers {
        return (1..=total).map(PageItem::Page).collect();
    }

    let left_sibling = cur.saturating_sub(siblings).max(boundaries + 1);
    let right_sibling = (cur + siblings).min(total.saturating_sub(boundaries));
    let show_left_dots = left_sibling > boundaries + 2;
    let show_right_dots = right_sibling < total.saturating_sub(boundaries + 1);

    let mut out = Vec::new();
    for p in 1..=boundaries {
        out.push(PageItem::Page(p));
    }
    if show_left_dots {
        out.push(PageItem::Ellipsis);
    } else {
        for p in (boundaries + 1)..left_sibling {
            out.push(PageItem::Page(p));
        }
    }
    for p in left_sibling..=right_sibling {
        out.push(PageItem::Page(p));
    }
    if show_right_dots {
        out.push(PageItem::Ellipsis);
    } else {
        for p in (right_sibling + 1)..=(total - boundaries) {
            out.push(PageItem::Page(p));
        }
    }
    for p in (total - boundaries + 1)..=total {
        out.push(PageItem::Page(p));
    }
    out
}

type PageClickFn = Rc<dyn Fn(El, usize) -> El>;

pub struct Pagination {
    key: String,
    current: usize,
    total: usize,
    siblings: usize,
    boundaries: usize,
    on_change: Option<PageClickFn>,
}

impl Pagination {
    pub fn new(key: impl Into<String>, current: usize, total: usize) -> Self {
        Self {
            key: key.into(),
            current: current.max(1),
            total: total.max(1),
            siblings: 1,
            boundaries: 1,
            on_change: None,
        }
    }

    pub fn siblings(mut self, n: usize) -> Self {
        self.siblings = n;
        self
    }

    pub fn boundaries(mut self, n: usize) -> Self {
        self.boundaries = n.max(1);
        self
    }

    pub fn on_change<S: 'static>(mut self, f: impl Fn(&mut S, usize) + Clone + 'static) -> Self {
        self.on_change = Some(Rc::new(move |e: El, page: usize| {
            let fc = f.clone();
            e.on_press(move |s: &mut S| fc(s, page))
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let cur = self.current.clamp(1, self.total);
        let items = pagination_items(cur, self.total, self.siblings, self.boundaries);

        let mut nav = row()
            .key(self.key.clone())
            .gap(4.0)
            .center_y()
            .role("navigation")
            .label("Pagination");

        let mut prev = el()
            .size(32.0, 32.0)
            .center()
            .radius(theme.radii.control)
            .role("button")
            .label("Previous page")
            .disabled(cur <= 1)
            .child(text("‹").font(k.ty.ui_medium).color(c.text_base));
        if cur > 1 {
            prev = theme.apply_focus(prev.hover(theme.hover_fill));
            if let Some(cb) = &self.on_change {
                prev = cb(prev, cur - 1);
            }
        } else {
            prev = prev.opacity(0.35);
        }
        nav = nav.child(prev);

        for (i, item) in items.into_iter().enumerate() {
            match item {
                PageItem::Ellipsis => {
                    nav = nav.child(
                        el().size(28.0, 32.0)
                            .center()
                            .child(text("…").font(k.ty.ui).color(c.text_muted)),
                    );
                }
                PageItem::Page(p) => {
                    let active = p == cur;
                    let mut btn = el()
                        .key(format!("{}-p-{i}-{p}", self.key))
                        .min_w(32.0)
                        .h(32.0)
                        .px(8.0)
                        .center()
                        .radius(theme.radii.control)
                        .fill(if active {
                            c.bg_accent
                        } else {
                            Color::TRANSPARENT
                        })
                        .role("button")
                        .label(format!("Page {p}"))
                        .focusable(true)
                        .child(
                            text(p.to_string())
                                .font(k.ty.small_medium)
                                .color(if active { c.submit_fg } else { c.text_base }),
                        );
                    if !active {
                        btn = btn.hover(theme.hover_fill);
                    }
                    btn = theme.apply_focus(btn);
                    if let Some(cb) = &self.on_change {
                        btn = cb(btn, p);
                    }
                    nav = nav.child(btn);
                }
            }
        }

        let mut next = el()
            .size(32.0, 32.0)
            .center()
            .radius(theme.radii.control)
            .role("button")
            .label("Next page")
            .disabled(cur >= self.total)
            .child(text("›").font(k.ty.ui_medium).color(c.text_base));
        if cur < self.total {
            next = theme.apply_focus(next.hover(theme.hover_fill));
            if let Some(cb) = &self.on_change {
                next = cb(next, cur + 1);
            }
        } else {
            next = next.opacity(0.35);
        }
        nav.child(next)
    }
}

/// Owned Carousel state supporting drag/fling velocity snap, loop mode, and keyboard arrows.
#[derive(Clone, Debug)]
pub struct CarouselState {
    pub index: usize,
    pub count: usize,
    pub loop_slides: bool,
    pub drag_dx: f32,
    velocity: VelocityTracker<f32>,
}

impl CarouselState {
    pub fn new(count: usize) -> Self {
        Self {
            index: 0,
            count,
            loop_slides: true,
            drag_dx: 0.0,
            velocity: VelocityTracker::new(),
        }
    }

    pub fn next(&mut self) {
        if self.count == 0 {
            return;
        }
        if self.index + 1 < self.count {
            self.index += 1;
        } else if self.loop_slides {
            self.index = 0;
        }
    }

    pub fn prev(&mut self) {
        if self.count == 0 {
            return;
        }
        if self.index > 0 {
            self.index -= 1;
        } else if self.loop_slides {
            self.index = self.count - 1;
        }
    }

    pub fn go_to(&mut self, idx: usize) {
        if self.count > 0 {
            self.index = idx.min(self.count - 1);
        }
    }

    pub fn drag_move(&mut self, dx: f32, now: f64) {
        self.drag_dx = dx;
        self.velocity.add(now, dx);
    }

    pub fn drag_end(&mut self, slide_width: f32, cancelled: bool) {
        let vel = self.velocity.velocity();
        let projected = project(self.drag_dx, vel, deceleration::NORMAL);
        self.drag_dx = 0.0;
        self.velocity.reset();
        if cancelled || self.count == 0 {
            return;
        }
        let threshold = (slide_width * 0.25).max(40.0);
        if projected <= -threshold {
            self.next();
        } else if projected >= threshold {
            self.prev();
        }
    }

    pub fn handle_key(&mut self, ev: &KeyEvent) -> bool {
        match &ev.key {
            Key::Left => {
                self.prev();
                true
            }
            Key::Right => {
                self.next();
                true
            }
            _ => false,
        }
    }
}

type BindFn = Box<dyn FnOnce(El) -> El>;

pub struct Carousel<'a> {
    key: String,
    state: &'a CarouselState,
    slides: Vec<El>,
    width: f32,
    height: f32,
    show_arrows: bool,
    show_dots: bool,
    on_prev: Option<BindFn>,
    on_next: Option<BindFn>,
    on_dot: Option<PageClickFn>,
    on_drag: Option<BindFn>,
}

impl<'a> Carousel<'a> {
    pub fn new(key: impl Into<String>, state: &'a CarouselState, slides: Vec<El>) -> Self {
        Self {
            key: key.into(),
            state,
            slides,
            width: 480.0,
            height: 220.0,
            show_arrows: true,
            show_dots: true,
            on_prev: None,
            on_next: None,
            on_dot: None,
            on_drag: None,
        }
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w.max(160.0);
        self.height = h.max(80.0);
        self
    }

    pub fn bind_state<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut CarouselState + Clone + 'static,
    ) -> Self {
        let slide_w = self.width;
        let a1 = access.clone();
        self.on_prev = Some(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| a1(s).prev())
        }));
        let a2 = access.clone();
        self.on_next = Some(Box::new(move |e: El| {
            e.on_press(move |s: &mut S| a2(s).next())
        }));
        let a3 = access.clone();
        self.on_dot = Some(Rc::new(move |e: El, idx: usize| {
            let ac = a3.clone();
            e.on_press(move |s: &mut S| ac(s).go_to(idx))
        }));
        self.on_drag = Some(Box::new(move |e: El| {
            e.on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
                let st = access(s);
                match d.phase {
                    Phase::Down | Phase::Move => st.drag_move(d.dx, tx.clock()),
                    Phase::Up => st.drag_end(slide_w, d.cancelled()),
                }
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let count = self.slides.len();
        let active = self.state.index.min(count.saturating_sub(1));
        let track_x = -(active as f32 * self.width) + self.state.drag_dx;

        let track = row()
            .key(format!("{}-track", self.key))
            .offset(track_x, 0.0)
            .motion(theme.springs.move_)
            .children(self.slides.into_iter().map(|s| {
                el().size(self.width, self.height)
                    .min_w(self.width)
                    .child(s)
            }));

        let mut viewport = el()
            .key(format!("{}-vp", self.key))
            .size(self.width, self.height)
            .radius(theme.radii.card)
            .clip_rounded(theme.radii.card)
            .child(track);

        if let Some(dr) = self.on_drag {
            viewport = dr(viewport);
        }

        if self.show_arrows && count > 1 {
            let mut prev = el()
                .absolute(10.0, (self.height - 32.0) * 0.5)
                .size(32.0, 32.0)
                .center()
                .role("button")
                .label("Previous slide")
                .child(text("‹").font(k.ty.ui_medium).color(c.text_base));
            prev = theme.control_surface(prev, theme.radii.pill);
            if let Some(p) = self.on_prev {
                prev = p(prev);
            }

            let mut next = el()
                .absolute(self.width - 42.0, (self.height - 32.0) * 0.5)
                .size(32.0, 32.0)
                .center()
                .role("button")
                .label("Next slide")
                .child(text("›").font(k.ty.ui_medium).color(c.text_base));
            next = theme.control_surface(next, theme.radii.pill);
            if let Some(n) = self.on_next {
                next = n(next);
            }
            viewport = viewport.child(prev).child(next);
        }

        let mut root = col()
            .key(self.key.clone())
            .gap(10.0)
            .items_center()
            .child(viewport);
        if self.show_dots && count > 1 {
            let mut dots = row().gap(6.0).center_y();
            for i in 0..count {
                let is_cur = i == active;
                let mut dot = el()
                    .key(format!("{}-dot-{i}", self.key))
                    .size(if is_cur { 18.0 } else { 7.0 }, 7.0)
                    .radius(theme.radii.pill)
                    .fill(if is_cur { c.bg_accent } else { c.border_strong })
                    .motion(theme.springs.quick)
                    .role("button")
                    .label(format!("Slide {}", i + 1));
                if let Some(cb) = &self.on_dot {
                    dot = cb(dot, i);
                }
                dots = dots.child(dot);
            }
            root = root.child(dots);
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_smart_ellipsis_matches_mantine() {
        let items = pagination_items(5, 10, 1, 1);
        assert_eq!(
            items,
            vec![
                PageItem::Page(1),
                PageItem::Ellipsis,
                PageItem::Page(4),
                PageItem::Page(5),
                PageItem::Page(6),
                PageItem::Ellipsis,
                PageItem::Page(10),
            ]
        );
    }

    #[test]
    fn carousel_drag_fling_advances_and_loops() {
        let mut c = CarouselState::new(3);
        c.drag_move(-20.0, 0.0);
        c.drag_move(-140.0, 0.02);
        c.drag_end(400.0, false);
        assert_eq!(c.index, 1);
        c.next();
        c.next();
        assert_eq!(c.index, 0);
    }
}
