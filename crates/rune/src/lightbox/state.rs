use crate::{Drag, Phase, Pixels};

const DEFAULT_MIN_ZOOM: f32 = 1.0;
const DEFAULT_MAX_ZOOM: f32 = 4.0;
const DEFAULT_ZOOM_STEP: f32 = 1.5;
const DEFAULT_DOUBLE_CLICK_ZOOM: f32 = 2.0;
pub(super) const ZOOM_EPSILON: f32 = 0.01;

/// One image slide shown inside a [`super::Lightbox`].
#[derive(Clone, Debug)]
pub struct LightboxItem {
    pub pixels: Pixels,
    pub caption: Option<String>,
    pub alt: Option<String>,
}

impl LightboxItem {
    /// Creates a lightbox slide backed by `pixels`.
    pub fn new(pixels: Pixels) -> Self {
        Self {
            pixels,
            caption: None,
            alt: None,
        }
    }

    /// Optional caption displayed below the image.
    pub fn caption(mut self, caption: impl Into<String>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// Optional accessibility description for the image.
    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Mode {
    #[default]
    Closed,
    Open {
        index: usize,
        zoom: f32,
        pan: [f32; 2],
        pan_origin: Option<[f32; 2]>,
    },
}

/// Model and interaction state for [`super::Lightbox`]. Store this in your app state.
#[derive(Clone, Debug, PartialEq)]
pub struct LightboxState {
    mode: Mode,
    wrap: bool,
    min_zoom: f32,
    max_zoom: f32,
    zoom_step: f32,
    double_click_zoom: f32,
    stage_size: [f32; 2],
}

impl Default for LightboxState {
    fn default() -> Self {
        Self::new()
    }
}

impl LightboxState {
    /// Creates a closed lightbox with `1.0..=4.0` zoom bounds and non-wrapping navigation.
    pub fn new() -> Self {
        Self {
            mode: Mode::Closed,
            wrap: false,
            min_zoom: DEFAULT_MIN_ZOOM,
            max_zoom: DEFAULT_MAX_ZOOM,
            zoom_step: DEFAULT_ZOOM_STEP,
            double_click_zoom: DEFAULT_DOUBLE_CLICK_ZOOM,
            stage_size: [0.0, 0.0],
        }
    }

    /// Whether `prev` from index `0` wraps to the last item and `next` from the last wraps to `0`.
    pub fn with_wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// Sets the minimum and maximum zoom scale (minimum is clamped to at least `1.0`).
    pub fn with_zoom_bounds(mut self, min: f32, max: f32) -> Self {
        let min = if min.is_finite() {
            min.max(1.0)
        } else {
            DEFAULT_MIN_ZOOM
        };
        let max = if max.is_finite() {
            max.max(min)
        } else {
            DEFAULT_MAX_ZOOM.max(min)
        };
        self.min_zoom = min;
        self.max_zoom = max;
        if let Mode::Open { zoom, pan, .. } = &mut self.mode {
            *zoom = zoom.clamp(min, max);
            if *zoom <= min + ZOOM_EPSILON {
                *zoom = min;
                *pan = [0.0, 0.0];
            }
        }
        self
    }

    /// Opens the lightbox on slide `index` at fit zoom.
    pub fn open(&mut self, index: usize) {
        self.mode = Mode::Open {
            index,
            zoom: self.min_zoom,
            pan: [0.0, 0.0],
            pan_origin: None,
        };
    }

    /// Closes the lightbox.
    pub fn close(&mut self) {
        self.mode = Mode::Closed;
    }

    /// True while the lightbox overlay is open.
    pub fn is_open(&self) -> bool {
        matches!(self.mode, Mode::Open { .. })
    }

    /// The open slide index, or `None` when closed.
    pub fn index(&self) -> Option<usize> {
        match self.mode {
            Mode::Open { index, .. } => Some(index),
            Mode::Closed => None,
        }
    }

    /// The open slide index clamped to `0..count`, or `None` when closed or `count == 0`.
    pub fn current_index(&self, count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        self.index().map(|i| i.min(count - 1))
    }

    /// Current zoom scale (`min_zoom` when closed).
    pub fn zoom(&self) -> f32 {
        match self.mode {
            Mode::Open { zoom, .. } => zoom,
            Mode::Closed => self.min_zoom,
        }
    }

    pub(super) fn min_zoom(&self) -> f32 {
        self.min_zoom
    }

    pub(super) fn max_zoom(&self) -> f32 {
        self.max_zoom
    }

    /// True when open and zoomed past fit (`min_zoom`).
    pub fn is_zoomed(&self) -> bool {
        self.is_open() && self.zoom() > self.min_zoom + ZOOM_EPSILON
    }

    /// Current pan translation in points (`[0.0, 0.0]` when closed or at fit zoom).
    pub fn pan(&self) -> [f32; 2] {
        match self.mode {
            Mode::Open { pan, .. } => pan,
            Mode::Closed => [0.0, 0.0],
        }
    }

    /// Whether navigating to the previous slide is available for a gallery of `count` items.
    pub fn can_prev(&self, count: usize) -> bool {
        let Some(idx) = self.current_index(count) else {
            return false;
        };
        count > 1 && (self.wrap || idx > 0)
    }

    /// Whether navigating to the next slide is available for a gallery of `count` items.
    pub fn can_next(&self, count: usize) -> bool {
        let Some(idx) = self.current_index(count) else {
            return false;
        };
        count > 1 && (self.wrap || idx + 1 < count)
    }

    /// Steps to the previous slide, resetting zoom and pan. Returns `true` if the slide changed.
    pub fn prev(&mut self, count: usize) -> bool {
        let Some(cur) = self.current_index(count) else {
            return false;
        };
        if count <= 1 {
            return false;
        }
        let next = if cur > 0 {
            cur - 1
        } else if self.wrap {
            count - 1
        } else {
            return false;
        };
        self.set_slide(next)
    }

    /// Steps to the next slide, resetting zoom and pan. Returns `true` if the slide changed.
    pub fn next(&mut self, count: usize) -> bool {
        let Some(cur) = self.current_index(count) else {
            return false;
        };
        if count <= 1 {
            return false;
        }
        let next = if cur + 1 < count {
            cur + 1
        } else if self.wrap {
            0
        } else {
            return false;
        };
        self.set_slide(next)
    }

    /// Jumps to slide `index` (clamped to `count - 1`), resetting zoom and pan.
    pub fn go_to(&mut self, index: usize, count: usize) -> bool {
        if count == 0 || !self.is_open() {
            return false;
        }
        let clamped = index.min(count - 1);
        self.set_slide(clamped)
    }

    fn set_slide(&mut self, next: usize) -> bool {
        let Mode::Open {
            index,
            zoom,
            pan,
            pan_origin,
        } = &mut self.mode
        else {
            return false;
        };
        let changed = *index != next;
        *index = next;
        *zoom = self.min_zoom;
        *pan = [0.0, 0.0];
        *pan_origin = None;
        changed
    }

    /// Sets the zoom scale, clamping to `[min_zoom, max_zoom]` and clamping pan to overflow.
    pub fn set_zoom(&mut self, target: f32) {
        if !target.is_finite() {
            return;
        }
        let (min, max, stage) = (self.min_zoom, self.max_zoom, self.stage_size);
        let Mode::Open {
            zoom,
            pan,
            pan_origin,
            ..
        } = &mut self.mode
        else {
            return;
        };
        let mut next = target.clamp(min, max);
        if next <= min + ZOOM_EPSILON {
            next = min;
            *pan = [0.0, 0.0];
            *pan_origin = None;
        } else {
            *pan = clamp_pan(*pan, next, stage);
        }
        *zoom = next;
    }

    /// Multiplies the current zoom by the step factor (`1.5`).
    pub fn zoom_in(&mut self) {
        let next = self.zoom() * self.zoom_step;
        self.set_zoom(next);
    }

    /// Divides the current zoom by the step factor (`1.5`), snapping back to fit near `min_zoom`.
    pub fn zoom_out(&mut self) {
        let next = self.zoom() / self.zoom_step;
        self.set_zoom(next);
    }

    /// Multiplies zoom by `factor` (for pinch / wheel gestures).
    pub fn zoom_by(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let next = self.zoom() * factor;
        self.set_zoom(next);
    }

    /// Resets zoom to fit and clears any pan offset.
    pub fn reset_zoom(&mut self) {
        self.set_zoom(self.min_zoom);
    }

    /// Toggles between fit zoom (`min_zoom`) and `2.0x` (clamped to `[min_zoom, max_zoom]`).
    pub fn toggle_zoom(&mut self) {
        if self.is_zoomed() {
            self.reset_zoom();
        } else {
            self.set_zoom(self.double_click_zoom);
        }
    }

    /// Sets the pan offset in points (clamped when zoomed; zeroed at fit zoom).
    pub fn set_pan(&mut self, target: [f32; 2]) {
        if !target[0].is_finite() || !target[1].is_finite() {
            return;
        }
        let (min, stage) = (self.min_zoom, self.stage_size);
        let Mode::Open { zoom, pan, .. } = &mut self.mode else {
            return;
        };
        if *zoom <= min + ZOOM_EPSILON {
            *pan = [0.0, 0.0];
        } else {
            *pan = clamp_pan(target, *zoom, stage);
        }
    }

    /// Handles an `Escape` press: if zoomed, resets zoom and returns `false` (stays open);
    /// otherwise closes the lightbox and returns `true`.
    pub fn handle_escape(&mut self) -> bool {
        if !self.is_open() {
            return false;
        }
        if self.is_zoomed() {
            self.reset_zoom();
            false
        } else {
            self.close();
            true
        }
    }

    pub(super) fn handle_stage_drag(&mut self, drag: &Drag) {
        self.stage_size = [drag.w, drag.h];
        let (min, stage) = (self.min_zoom, self.stage_size);
        let Mode::Open {
            zoom,
            pan,
            pan_origin,
            ..
        } = &mut self.mode
        else {
            return;
        };
        if *zoom <= min + ZOOM_EPSILON {
            *pan_origin = None;
            return;
        }
        match drag.phase {
            Phase::Down => {
                *pan_origin = Some(*pan);
            }
            Phase::Move => {
                if let Some(origin) = *pan_origin {
                    *pan = clamp_pan([origin[0] + drag.dx, origin[1] + drag.dy], *zoom, stage);
                }
            }
            Phase::Up => {
                if drag.cancelled()
                    && let Some(origin) = *pan_origin
                {
                    *pan = clamp_pan(origin, *zoom, stage);
                }
                *pan_origin = None;
            }
        }
    }
}

fn clamp_pan(pan: [f32; 2], zoom: f32, stage: [f32; 2]) -> [f32; 2] {
    if stage[0] <= 0.0 || stage[1] <= 0.0 {
        return pan;
    }
    let max_x = (stage[0] * (zoom - 1.0) * 0.5).max(0.0);
    let max_y = (stage[1] * (zoom - 1.0) * 0.5).max(0.0);
    [pan[0].clamp(-max_x, max_x), pan[1].clamp(-max_y, max_y)]
}
