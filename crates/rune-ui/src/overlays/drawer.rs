//! Vaul-inspired Sheet / Drawer with snap points, 1:1 drag tracking, rubber-band overdrag,
//! velocity-projected fling & dismiss threshold, and optional background scale-down.

use rune::{Color, Drag, El, Key, KeyEvent, Phase, Tx, col, el, row, text};
use rune_motion::{VelocityTracker, deceleration, project, rubber_band};

use crate::primitives::portal::{LayerSlot, PortalHost};
use crate::theme::UiTheme;

/// Edge of the viewport from which a [`Sheet`] / [`Drawer`] slides in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawerSide {
    #[default]
    Bottom,
    Right,
    Left,
    Top,
}

impl DrawerSide {
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Bottom | Self::Top)
    }

    /// Sign along the drag axis that moves the drawer toward dismissal (`+1` or `-1`).
    pub fn dismiss_sign(self) -> f32 {
        match self {
            Self::Bottom | Self::Right => 1.0,
            Self::Top | Self::Left => -1.0,
        }
    }
}

/// Outcome returned when a drawer drag ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawerRelease {
    Dismissed,
    Snapped(usize),
    Cancelled,
}

/// Owned Vaul drawer physics state.
#[derive(Clone, Debug)]
pub struct DrawerState {
    pub open: bool,
    pub side: DrawerSide,
    /// Normalized snap fractions in `(0.0, 1.0]`, sorted ascending (e.g. `[0.4, 0.85]`).
    pub snap_points: Vec<f32>,
    pub active_snap: usize,
    /// Current drag offset along the dismiss axis in points (`> 0` pulling toward close, `< 0` overdrag).
    pub drag_offset: f32,
    pub dragging: bool,
    pub dismissible: bool,
    pub dismiss_velocity: f32,
    pub dismiss_fraction: f32,
    velocity: VelocityTracker<f32>,
}

impl Default for DrawerState {
    fn default() -> Self {
        Self::new(DrawerSide::Bottom)
    }
}

impl DrawerState {
    pub fn new(side: DrawerSide) -> Self {
        Self {
            open: false,
            side,
            snap_points: vec![0.5, 0.9],
            active_snap: 0,
            drag_offset: 0.0,
            dragging: false,
            dismissible: true,
            dismiss_velocity: 550.0,
            dismiss_fraction: 0.35,
            velocity: VelocityTracker::new(),
        }
    }

    pub fn with_snap_points(mut self, mut snaps: Vec<f32>) -> Self {
        snaps.retain(|s| s.is_finite() && *s > 0.05 && *s <= 1.0);
        if snaps.is_empty() {
            snaps.push(0.5);
        }
        snaps.sort_by(|a, b| a.total_cmp(b));
        self.snap_points = snaps;
        self.active_snap = self.active_snap.min(self.snap_points.len() - 1);
        self
    }

    pub fn open_at(&mut self, snap_index: usize) {
        self.open = true;
        self.active_snap = snap_index.min(self.snap_points.len().saturating_sub(1));
        self.drag_offset = 0.0;
        self.dragging = false;
        self.velocity.reset();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.drag_offset = 0.0;
        self.dragging = false;
        self.velocity.reset();
    }

    /// Current active snap fraction (`0.0..=1.0`).
    pub fn current_snap_fraction(&self) -> f32 {
        self.snap_points
            .get(self.active_snap)
            .copied()
            .unwrap_or(0.5)
    }

    /// Current effective open extent in points for a viewport span `viewport_extent`.
    pub fn visible_extent(&self, viewport_extent: f32) -> f32 {
        let base = self.current_snap_fraction() * viewport_extent;
        (base - self.drag_offset).max(0.0)
    }

    /// Vaul-style background scale factor (`1.0` when closed, down to `0.96` when fully open).
    pub fn background_scale(&self, viewport_extent: f32) -> f32 {
        if !self.open || viewport_extent <= 1.0 {
            return 1.0;
        }
        let progress = (self.visible_extent(viewport_extent) / viewport_extent).clamp(0.0, 1.0);
        1.0 - 0.04 * progress
    }

    /// Vaul-style background corner radius (`0.0` when closed, up to `12.0` when open).
    pub fn background_radius(&self, viewport_extent: f32) -> f32 {
        if !self.open || viewport_extent <= 1.0 {
            return 0.0;
        }
        let progress = (self.visible_extent(viewport_extent) / viewport_extent).clamp(0.0, 1.0);
        12.0 * progress
    }

    /// Feeds a drag pointer sample (`raw_delta` along the drawer's axis, `t_secs`, `viewport_extent`).
    pub fn drag_move(&mut self, raw_delta: f32, t_secs: f64, viewport_extent: f32) {
        self.dragging = true;
        let signed = raw_delta * self.side.dismiss_sign();
        self.velocity.add(t_secs, signed);

        let max_snap = self.snap_points.last().copied().unwrap_or(1.0) * viewport_extent;
        let cur_snap = self.current_snap_fraction() * viewport_extent;
        let headroom = (max_snap - cur_snap).max(0.0);

        if signed < -headroom {
            let over = (-signed) - headroom;
            let damped = rubber_band(over, viewport_extent.max(120.0) * 0.35);
            self.drag_offset = -(headroom + damped);
        } else {
            self.drag_offset = signed;
        }
    }

    /// Finishes a drag gesture, projecting momentum with `rune_motion::project` to either
    /// dismiss or snap to the closest snap point.
    pub fn drag_end(&mut self, cancelled: bool, viewport_extent: f32) -> DrawerRelease {
        self.dragging = false;
        if cancelled {
            self.drag_offset = 0.0;
            self.velocity.reset();
            return DrawerRelease::Cancelled;
        }

        let vel = self.velocity.velocity();
        let projected_offset = project(self.drag_offset, vel, deceleration::NORMAL);
        let cur_extent = self.current_snap_fraction() * viewport_extent;
        let projected_extent = cur_extent - projected_offset;
        let lowest_extent = self.snap_points.first().copied().unwrap_or(0.5) * viewport_extent;

        self.drag_offset = 0.0;
        self.velocity.reset();

        let fast_dismiss = vel >= self.dismiss_velocity && projected_extent < lowest_extent;
        let far_dismiss = projected_extent < lowest_extent * (1.0 - self.dismiss_fraction);
        if self.dismissible && (fast_dismiss || far_dismiss) {
            self.open = false;
            return DrawerRelease::Dismissed;
        }

        let mut best_idx = 0;
        let mut best_dist = f32::INFINITY;
        for (i, &frac) in self.snap_points.iter().enumerate() {
            let snap_px = frac * viewport_extent;
            let dist = (projected_extent - snap_px).abs();
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        self.active_snap = best_idx;
        DrawerRelease::Snapped(best_idx)
    }
}

type DrawerHandler = Box<dyn FnOnce(El) -> El>;

/// Vaul-style Sheet / Drawer builder.
pub struct Sheet {
    key: String,
    title: String,
    description: Option<String>,
    side: DrawerSide,
    open: bool,
    size_px: f32,
    drag_offset: f32,
    show_handle: bool,
    dismissible: bool,
    body: Vec<El>,
    footer: Option<El>,
    on_close: Option<DrawerHandler>,
    on_drag: Option<DrawerHandler>,
}

impl Sheet {
    pub fn new(key: impl Into<String>, title: impl Into<String>, state: &DrawerState) -> Self {
        let base_size = if state.side.is_vertical() {
            (state.current_snap_fraction() * 680.0).clamp(220.0, 640.0)
        } else {
            (state.current_snap_fraction() * 520.0).clamp(280.0, 520.0)
        };
        Self {
            key: key.into(),
            title: title.into(),
            description: None,
            side: state.side,
            open: state.open,
            size_px: base_size,
            drag_offset: state.drag_offset,
            show_handle: state.side == DrawerSide::Bottom || state.side == DrawerSide::Top,
            dismissible: state.dismissible,
            body: Vec::new(),
            footer: None,
            on_close: None,
            on_drag: None,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn size_px(mut self, px: f32) -> Self {
        self.size_px = px.max(180.0);
        self
    }

    pub fn show_handle(mut self, show: bool) -> Self {
        self.show_handle = show;
        self
    }

    pub fn child(mut self, child: El) -> Self {
        self.body.push(child);
        self
    }

    pub fn footer(mut self, footer: El) -> Self {
        self.footer = Some(footer);
        self
    }

    pub fn on_close<S: 'static>(mut self, f: impl Fn(&mut S) + Clone + 'static) -> Self {
        let f_key = f.clone();
        self.on_close = Some(Box::new(move |e: El| {
            e.modal(true, f)
                .on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
                    if ev.key == Key::Escape {
                        f_key(s);
                        true
                    } else {
                        false
                    }
                })
        }));
        self
    }

    /// Binds 1:1 Vaul drag/fling physics directly to app state's [`DrawerState`].
    pub fn bind_drag<S: 'static>(
        mut self,
        access: impl Fn(&mut S) -> &mut DrawerState + 'static,
        viewport_extent: f32,
    ) -> Self {
        let vertical = self.side.is_vertical();
        self.on_drag = Some(Box::new(move |e: El| {
            e.on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
                let st = access(s);
                let delta = if vertical { d.dy } else { d.dx };
                let now = tx.clock();
                match d.phase {
                    Phase::Down | Phase::Move => st.drag_move(delta, now, viewport_extent),
                    Phase::Up => {
                        let _ = st.drag_end(d.cancelled(), viewport_extent);
                    }
                }
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme, portals: &mut PortalHost) {
        if !self.open {
            return;
        }
        let k = &theme.kit;
        let c = &k.c;

        let mut header = col().w_full().gap(4.0).px(20.0).py(14.0);
        if self.show_handle {
            let mut handle_bar = row().w_full().justify_center().pb(8.0).child(
                el().size(40.0, 4.0)
                    .radius(theme.radii.pill)
                    .fill(c.border_strong),
            );
            if let Some(drag_bind) = self.on_drag {
                handle_bar = drag_bind(handle_bar);
            }
            header = header.child(handle_bar);
        }
        header = header.child(
            text(self.title.clone())
                .font(k.ty.ui_medium)
                .color(c.text_base),
        );
        if let Some(desc) = self.description {
            header = header.child(
                text(desc)
                    .font(k.ty.small)
                    .color(c.text_muted)
                    .wrap()
                    .w_full(),
            );
        }

        let mut panel = col()
            .key(format!("{}-sheet", self.key))
            .role("dialog")
            .label(self.title)
            .child(header)
            .child(
                col()
                    .w_full()
                    .grow(1.0)
                    .shrink()
                    .min_h(0.0)
                    .px(20.0)
                    .gap(10.0)
                    .children(self.body),
            );

        if let Some(foot) = self.footer {
            panel = panel
                .child(el().w_full().h(1.0).fill(theme.hairline))
                .child(
                    row()
                        .w_full()
                        .px(20.0)
                        .py(14.0)
                        .justify_end()
                        .gap(8.0)
                        .child(foot),
                );
        }

        let eff_size = (self.size_px - self.drag_offset).max(120.0);
        panel = match self.side {
            DrawerSide::Bottom | DrawerSide::Top => panel.w_full().h(eff_size),
            DrawerSide::Left | DrawerSide::Right => panel.w(eff_size).h_full(),
        };
        panel = theme
            .overlay_surface(panel, theme.radii.sheet)
            .motion(theme.springs.move_);

        if self.dismissible {
            if let Some(close) = self.on_close {
                panel = close(panel);
            } else {
                panel = panel.modal_trap();
            }
        } else {
            panel = panel.modal_trap();
        }

        let scrim = el()
            .key(format!("{}-scrim", self.key))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(Color::BLACK.alpha(0.55))
            .a11y_hidden();

        let container = match self.side {
            DrawerSide::Bottom => col().justify_end().items_center(),
            DrawerSide::Top => col().justify_start().items_center(),
            DrawerSide::Right => row().justify_end().items_stretch(),
            DrawerSide::Left => row().justify_start().items_stretch(),
        }
        .key(format!("{}-sheet-layer", self.key))
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(panel);

        portals.push(LayerSlot::Modal, scrim);
        portals.push(LayerSlot::Modal, container);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vaul_snap_points_and_rubber_band_overdrag() {
        let mut d = DrawerState::new(DrawerSide::Bottom).with_snap_points(vec![0.35, 0.75]);
        d.open_at(0);
        assert_eq!(d.active_snap, 0);
        assert!(d.background_scale(1000.0) < 1.0);

        // Drag upward past highest snap -> rubber-band resists
        d.drag_move(-700.0, 0.05, 1000.0);
        assert!(d.drag_offset < -400.0 && d.drag_offset > -700.0);
        assert_eq!(d.drag_end(false, 1000.0), DrawerRelease::Snapped(1));
        assert_eq!(d.active_snap, 1);
    }

    #[test]
    fn vaul_fast_fling_dismisses_and_cancel_restores() {
        let mut d = DrawerState::new(DrawerSide::Bottom).with_snap_points(vec![0.4, 0.8]);
        d.open_at(0);
        d.drag_move(20.0, 0.0, 800.0);
        d.drag_move(140.0, 0.05, 800.0);
        assert_eq!(d.drag_end(true, 800.0), DrawerRelease::Cancelled);
        assert!(d.open);

        d.drag_move(10.0, 0.0, 800.0);
        d.drag_move(120.0, 0.02, 800.0);
        d.drag_move(260.0, 0.04, 800.0);
        assert_eq!(d.drag_end(false, 800.0), DrawerRelease::Dismissed);
        assert!(!d.open);
    }
}
