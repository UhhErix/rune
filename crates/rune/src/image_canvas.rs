//! Interactive image annotation & crop canvas (`ImageCanvas`, `ImageCanvasState`) for snip/image
//! editing: freehand strokes, arrows, rectangles, 8-handle crop rect, undo/redo, and PNG export.

mod raster;

use std::rc::Rc;

use crate::color::Color;
use crate::el::{Cursor, Drag, El, Phase, Tx, canvas, el};
use crate::image::{ImageError, Pixels};
use crate::shape::{LineCap, Path, Shape};

type StateAccess<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut ImageCanvasState>;
type ChangeFn<S> = Rc<dyn Fn(&mut S, &mut Tx)>;

/// Active editing tool on an [`ImageCanvas`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasTool {
    /// Draw freehand polyline strokes.
    #[default]
    Freehand,
    /// Drag from tail to tip to draw an arrow with arrowhead.
    Arrow,
    /// Drag opposite corners to draw a rectangle (stroked and optionally filled).
    Rect,
    /// Drag crop handles or body to adjust the export crop rectangle.
    Crop,
}

/// One vector annotation stored in image-pixel coordinates (`0..width`, `0..height`).
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasAnnotation {
    /// Freehand stroke polyline.
    Stroke {
        points: Vec<[f32; 2]>,
        color: Color,
        width: f32,
    },
    /// Straight arrow from `from` to `to`.
    Arrow {
        from: [f32; 2],
        to: [f32; 2],
        color: Color,
        width: f32,
    },
    /// Axis-aligned rectangle (`w >= 0`, `h >= 0`).
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
        width: f32,
        fill: Option<Color>,
    },
}

/// Crop rectangle in image-pixel coordinates (`0..width`, `0..height`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CropRect {
    /// Creates a crop rect clamped within `[0..max_w, 0..max_h]` with at least `min_side`.
    pub fn clamped(x: f32, y: f32, w: f32, h: f32, max_w: f32, max_h: f32, min_side: f32) -> Self {
        let max_w = max_w.max(1.0);
        let max_h = max_h.max(1.0);
        let min_w = min_side.min(max_w).max(1.0);
        let min_h = min_side.min(max_h).max(1.0);
        let w = w.clamp(min_w, max_w);
        let h = h.clamp(min_h, max_h);
        let x = x.clamp(0.0, (max_w - w).max(0.0));
        let y = y.clamp(0.0, (max_h - h).max(0.0));
        Self { x, y, w, h }
    }

    /// Full image bounds `[0, 0, width, height]`.
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: width.max(1) as f32,
            h: height.max(1) as f32,
        }
    }

    /// Integer pixel bounds `(x, y, w, h)` clamped inside `(img_w, img_h)`.
    pub fn to_pixels(self, img_w: u32, img_h: u32) -> (u32, u32, u32, u32) {
        let x0 = (self.x.round().max(0.0) as u32).min(img_w.saturating_sub(1));
        let y0 = (self.y.round().max(0.0) as u32).min(img_h.saturating_sub(1));
        let x1 = ((self.x + self.w).round().max(0.0) as u32).clamp(x0 + 1, img_w.max(1));
        let y1 = ((self.y + self.h).round().max(0.0) as u32).clamp(y0 + 1, img_h.max(1));
        (x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
    }
}

/// Interactive handle on a [`CropRect`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    Move,
}

/// Explicit state machine for an active pointer gesture on [`ImageCanvas`].
#[derive(Clone, Debug, Default, PartialEq)]
enum CanvasGesture {
    #[default]
    Idle,
    Drawing(CanvasAnnotation),
    Cropping {
        handle: CropHandle,
        origin_crop: CropRect,
        start: [f32; 2],
    },
}

#[derive(Clone, Debug)]
struct Snapshot {
    annotations: Vec<CanvasAnnotation>,
    crop: Option<CropRect>,
}

/// App-owned state for [`ImageCanvas`]: annotations, active tool/style, crop rect, active drag
/// state machine, and undo/redo history.
#[derive(Clone, Debug)]
pub struct ImageCanvasState {
    natural: [u32; 2],
    tool: CanvasTool,
    color: Color,
    stroke_width: f32,
    rect_fill: Option<Color>,
    annotations: Vec<CanvasAnnotation>,
    crop: Option<CropRect>,
    gesture: CanvasGesture,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl ImageCanvasState {
    /// Creates an empty canvas state for an image of `width` × `height` pixels.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            natural: [width.max(1), height.max(1)],
            tool: CanvasTool::Freehand,
            color: Color {
                r: 0.96,
                g: 0.26,
                b: 0.21,
                a: 1.0,
            },
            stroke_width: 4.0,
            rect_fill: None,
            annotations: Vec::new(),
            crop: None,
            gesture: CanvasGesture::Idle,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Natural image dimensions `[width, height]` in pixels.
    pub const fn natural_size(&self) -> [u32; 2] {
        self.natural
    }

    /// Active editing tool.
    pub const fn tool(&self) -> CanvasTool {
        self.tool
    }

    /// Switches the active tool. Selecting [`CanvasTool::Crop`] initializes the crop rect to the
    /// full image if none is set yet.
    pub fn set_tool(&mut self, tool: CanvasTool) {
        self.tool = tool;
        self.gesture = CanvasGesture::Idle;
        if tool == CanvasTool::Crop && self.crop.is_none() {
            self.crop = Some(CropRect::full(self.natural[0], self.natural[1]));
        }
    }

    /// Active stroke/arrow/rect outline colour.
    pub const fn color(&self) -> Color {
        self.color
    }

    /// Sets the colour for new annotations.
    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    /// Active stroke width in image pixels.
    pub const fn stroke_width(&self) -> f32 {
        self.stroke_width
    }

    /// Sets the stroke width in image pixels for new annotations.
    pub fn set_stroke_width(&mut self, width: f32) {
        self.stroke_width = width.clamp(1.0, 64.0);
    }

    /// Optional fill colour used by [`CanvasTool::Rect`].
    pub const fn rect_fill(&self) -> Option<Color> {
        self.rect_fill
    }

    /// Sets optional fill colour for [`CanvasTool::Rect`] annotations.
    pub fn set_rect_fill(&mut self, fill: Option<Color>) {
        self.rect_fill = fill;
    }

    /// Committed annotations in paint order.
    pub fn annotations(&self) -> &[CanvasAnnotation] {
        &self.annotations
    }

    /// Pushes an annotation onto the canvas, recording an undo step.
    pub fn push_annotation(&mut self, annotation: CanvasAnnotation) {
        self.record_undo();
        self.annotations.push(annotation);
    }

    /// Removes all committed annotations, recording an undo step if non-empty.
    pub fn clear_annotations(&mut self) {
        if self.annotations.is_empty() {
            return;
        }
        self.record_undo();
        self.annotations.clear();
    }

    /// Active crop rectangle in image pixels, if set.
    pub const fn crop(&self) -> Option<CropRect> {
        self.crop
    }

    /// Sets (or clears) the crop rectangle, clamped to image bounds, recording an undo step.
    pub fn set_crop(&mut self, crop: Option<CropRect>) {
        let clamped = crop.map(|c| {
            CropRect::clamped(
                c.x,
                c.y,
                c.w,
                c.h,
                self.natural[0] as f32,
                self.natural[1] as f32,
                16.0,
            )
        });
        if self.crop == clamped {
            return;
        }
        self.record_undo();
        self.crop = clamped;
    }

    /// Resets the crop rectangle to `None` (full image).
    pub fn reset_crop(&mut self) {
        self.set_crop(None);
    }

    /// Whether an undo step is available.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether a redo step is available.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Restores the previous snapshot; returns `true` if state changed.
    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.annotations = prev.annotations;
        self.crop = prev.crop;
        self.gesture = CanvasGesture::Idle;
        true
    }

    /// Re-applies the next undone snapshot; returns `true` if state changed.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.annotations = next.annotations;
        self.crop = next.crop;
        self.gesture = CanvasGesture::Idle;
        true
    }

    /// Composites source `pixels`, all committed annotations, and the active crop rect into a
    /// straight-alpha sRGB RGBA8 buffer `(width, height, rgba)`.
    pub fn export_rgba(&self, pixels: &Pixels) -> Result<(u32, u32, Vec<u8>), ImageError> {
        raster::render_rgba(pixels, &self.annotations, self.crop)
    }

    /// Composites source `pixels`, all committed annotations, and the active crop rect and
    /// encodes the result as PNG bytes.
    pub fn export_png(&self, pixels: &Pixels) -> Result<Vec<u8>, ImageError> {
        let (w, h, rgba) = self.export_rgba(pixels)?;
        crate::image::encode_png(w, h, &rgba)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            annotations: self.annotations.clone(),
            crop: self.crop,
        }
    }

    fn record_undo(&mut self) {
        self.undo.push(self.snapshot());
        self.redo.clear();
    }

    /// Steps the drag gesture machine from a pointer drag event in the image frame's local points.
    /// Returns `true` when committed state changed (for `on_change`).
    pub(crate) fn step_drag(&mut self, d: &Drag) -> bool {
        let (nw, nh) = (self.natural[0] as f32, self.natural[1] as f32);
        let img_pt = [
            ((d.x / d.w.max(1.0)) * nw).clamp(0.0, nw),
            ((d.y / d.h.max(1.0)) * nh).clamp(0.0, nh),
        ];
        let handle_radius_img = (12.0 / d.w.max(1.0) * nw).max(12.0 / d.h.max(1.0) * nh);

        match d.phase {
            Phase::Down => {
                self.gesture = match self.tool {
                    CanvasTool::Freehand => CanvasGesture::Drawing(CanvasAnnotation::Stroke {
                        points: vec![img_pt],
                        color: self.color,
                        width: self.stroke_width,
                    }),
                    CanvasTool::Arrow => CanvasGesture::Drawing(CanvasAnnotation::Arrow {
                        from: img_pt,
                        to: img_pt,
                        color: self.color,
                        width: self.stroke_width,
                    }),
                    CanvasTool::Rect => CanvasGesture::Drawing(CanvasAnnotation::Rect {
                        x: img_pt[0],
                        y: img_pt[1],
                        w: 0.0,
                        h: 0.0,
                        color: self.color,
                        width: self.stroke_width,
                        fill: self.rect_fill,
                    }),
                    CanvasTool::Crop => {
                        let current = self
                            .crop
                            .unwrap_or_else(|| CropRect::full(self.natural[0], self.natural[1]));
                        self.crop = Some(current);
                        match hit_crop_handle(current, img_pt, handle_radius_img) {
                            Some(handle) => CanvasGesture::Cropping {
                                handle,
                                origin_crop: current,
                                start: img_pt,
                            },
                            None => CanvasGesture::Idle,
                        }
                    }
                };
                false
            }
            Phase::Move => {
                match &mut self.gesture {
                    CanvasGesture::Idle => {}
                    CanvasGesture::Drawing(ann) => match ann {
                        CanvasAnnotation::Stroke { points, .. } => {
                            let far = points
                                .last()
                                .is_none_or(|p| (p[0] - img_pt[0]).hypot(p[1] - img_pt[1]) >= 1.5);
                            if far {
                                points.push(img_pt);
                            }
                        }
                        CanvasAnnotation::Arrow { to, .. } => *to = img_pt,
                        CanvasAnnotation::Rect { x, y, w, h, .. } => {
                            let sx = ((d.start_x / d.w.max(1.0)) * nw).clamp(0.0, nw);
                            let sy = ((d.start_y / d.h.max(1.0)) * nh).clamp(0.0, nh);
                            *x = sx.min(img_pt[0]);
                            *y = sy.min(img_pt[1]);
                            *w = (img_pt[0] - sx).abs();
                            *h = (img_pt[1] - sy).abs();
                        }
                    },
                    CanvasGesture::Cropping {
                        handle,
                        origin_crop,
                        start,
                    } => {
                        let dx = img_pt[0] - start[0];
                        let dy = img_pt[1] - start[1];
                        self.crop = Some(apply_crop_drag(*origin_crop, *handle, dx, dy, nw, nh));
                    }
                }
                false
            }
            Phase::Up => {
                let gesture = std::mem::take(&mut self.gesture);
                if d.cancelled() {
                    if let CanvasGesture::Cropping { origin_crop, .. } = gesture {
                        self.crop = Some(origin_crop);
                    }
                    return false;
                }
                match gesture {
                    CanvasGesture::Idle => false,
                    CanvasGesture::Drawing(ann) => {
                        let keep = match &ann {
                            CanvasAnnotation::Stroke { points, .. } => !points.is_empty(),
                            CanvasAnnotation::Arrow { from, to, .. } => {
                                (to[0] - from[0]).hypot(to[1] - from[1]) >= 2.0
                            }
                            CanvasAnnotation::Rect { w, h, .. } => *w >= 2.0 && *h >= 2.0,
                        };
                        if keep {
                            self.record_undo();
                            self.annotations.push(ann);
                            true
                        } else {
                            false
                        }
                    }
                    CanvasGesture::Cropping { origin_crop, .. } => {
                        if self.crop != Some(origin_crop) {
                            let next = self.crop;
                            self.crop = Some(origin_crop);
                            self.record_undo();
                            self.crop = next;
                            true
                        } else {
                            false
                        }
                    }
                }
            }
        }
    }
}

fn hit_crop_handle(c: CropRect, p: [f32; 2], r: f32) -> Option<CropHandle> {
    let corners = [
        (CropHandle::TopLeft, [c.x, c.y]),
        (CropHandle::TopRight, [c.x + c.w, c.y]),
        (CropHandle::BottomRight, [c.x + c.w, c.y + c.h]),
        (CropHandle::BottomLeft, [c.x, c.y + c.h]),
        (CropHandle::Top, [c.x + c.w * 0.5, c.y]),
        (CropHandle::Right, [c.x + c.w, c.y + c.h * 0.5]),
        (CropHandle::Bottom, [c.x + c.w * 0.5, c.y + c.h]),
        (CropHandle::Left, [c.x, c.y + c.h * 0.5]),
    ];
    for (h, pos) in corners {
        if (p[0] - pos[0]).abs() <= r && (p[1] - pos[1]).abs() <= r {
            return Some(h);
        }
    }
    if p[0] >= c.x && p[0] <= c.x + c.w && p[1] >= c.y && p[1] <= c.y + c.h {
        return Some(CropHandle::Move);
    }
    None
}

fn apply_crop_drag(
    origin: CropRect,
    handle: CropHandle,
    dx: f32,
    dy: f32,
    max_w: f32,
    max_h: f32,
) -> CropRect {
    let min_s = 16.0f32.min(max_w).min(max_h).max(1.0);
    let (mut l, mut t, mut r, mut b) =
        (origin.x, origin.y, origin.x + origin.w, origin.y + origin.h);
    match handle {
        CropHandle::Move => {
            let x = (origin.x + dx).clamp(0.0, (max_w - origin.w).max(0.0));
            let y = (origin.y + dy).clamp(0.0, (max_h - origin.h).max(0.0));
            return CropRect {
                x,
                y,
                w: origin.w,
                h: origin.h,
            };
        }
        CropHandle::TopLeft => {
            l = (l + dx).clamp(0.0, r - min_s);
            t = (t + dy).clamp(0.0, b - min_s);
        }
        CropHandle::Top => t = (t + dy).clamp(0.0, b - min_s),
        CropHandle::TopRight => {
            r = (r + dx).clamp(l + min_s, max_w);
            t = (t + dy).clamp(0.0, b - min_s);
        }
        CropHandle::Right => r = (r + dx).clamp(l + min_s, max_w),
        CropHandle::BottomRight => {
            r = (r + dx).clamp(l + min_s, max_w);
            b = (b + dy).clamp(t + min_s, max_h);
        }
        CropHandle::Bottom => b = (b + dy).clamp(t + min_s, max_h),
        CropHandle::BottomLeft => {
            l = (l + dx).clamp(0.0, r - min_s);
            b = (b + dy).clamp(t + min_s, max_h);
        }
        CropHandle::Left => l = (l + dx).clamp(0.0, r - min_s),
    }
    CropRect {
        x: l,
        y: t,
        w: (r - l).max(1.0),
        h: (b - t).max(1.0),
    }
}

/// Computes the two arrowhead wing tip points for an arrow from `from` to `to`.
pub(crate) fn arrow_wings(
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
) -> Option<([f32; 2], [f32; 2])> {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = dx.hypot(dy);
    if len < 1.0 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = (width * 3.5).clamp(10.0, len * 0.45);
    let head_half = head_len * 0.55;
    let base_x = to[0] - ux * head_len;
    let base_y = to[1] - uy * head_len;
    let px = -uy * head_half;
    let py = ux * head_half;
    Some(([base_x + px, base_y + py], [base_x - px, base_y - py]))
}

/// Builder for an interactive image annotation & crop canvas element.
pub struct ImageCanvas<S> {
    state: ImageCanvasState,
    pixels: Pixels,
    key: String,
    size: [f32; 2],
    crop_scrim: Color,
    crop_border: Color,
    handle_color: Color,
    access: Option<StateAccess<S>>,
    on_change: Option<ChangeFn<S>>,
}

impl<S: 'static> ImageCanvas<S> {
    /// Creates an image canvas displaying `pixels` with annotations and crop from `state`.
    pub fn new(state: &ImageCanvasState, pixels: &Pixels) -> Self {
        let [nw, nh] = state.natural;
        Self {
            state: state.clone(),
            pixels: pixels.clone(),
            key: "image-canvas".to_owned(),
            size: [nw as f32, nh as f32],
            crop_scrim: Color::BLACK.alpha(0.55),
            crop_border: Color::WHITE,
            handle_color: Color::WHITE,
            access: None,
            on_change: None,
        }
    }

    /// Stable key for the canvas container.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Viewport bounding size `[width, height]` in logical points (the image fits inside preserving
    /// its aspect ratio).
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = [w.max(16.0), h.max(16.0)];
        self
    }

    /// Dimming colour drawn outside the active crop rectangle.
    pub fn crop_scrim(mut self, color: Color) -> Self {
        self.crop_scrim = color;
        self
    }

    /// Outline colour of the active crop rectangle.
    pub fn crop_border(mut self, color: Color) -> Self {
        self.crop_border = color;
        self
    }

    /// Fill colour of the 8 crop resize handles.
    pub fn handle_color(mut self, color: Color) -> Self {
        self.handle_color = color;
        self
    }

    /// Lens into the app's [`ImageCanvasState`].
    pub fn on_state(
        mut self,
        f: impl for<'a> Fn(&'a mut S) -> &'a mut ImageCanvasState + 'static,
    ) -> Self {
        self.access = Some(Rc::new(f));
        self
    }

    /// Called after a drawing stroke/arrow/rect or crop drag commits.
    pub fn on_change(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Builds the interactive image canvas element.
    pub fn build(self) -> El {
        let [nw, nh] = [self.state.natural[0] as f32, self.state.natural[1] as f32];
        let [vw, vh] = self.size;
        let k = (vw / nw).min(vh / nh);
        let (fw, fh) = ((nw * k).max(1.0), (nh * k).max(1.0));

        let mut shape = Shape::new().view_box(nw, nh);
        for ann in &self.state.annotations {
            shape = paint_annotation(shape, ann);
        }
        if let CanvasGesture::Drawing(live) = &self.state.gesture {
            shape = paint_annotation(shape, live);
        }
        if let Some(crop) = self.state.crop {
            shape = paint_crop_overlay(
                shape,
                crop,
                nw,
                nh,
                k,
                self.crop_scrim,
                self.crop_border,
                self.handle_color,
                self.state.tool == CanvasTool::Crop,
            );
        }

        let cursor = match self.state.tool {
            CanvasTool::Crop => Cursor::Grab,
            _ => Cursor::Crosshair,
        };
        let mut stage = el()
            .key(format!("{}-stage", self.key))
            .size(fw, fh)
            .pixels(&self.pixels)
            .cursor(cursor)
            .child(
                canvas(shape)
                    .absolute(0.0, 0.0)
                    .size(fw, fh)
                    .instant_changes()
                    .inert(),
            );

        if let Some(access) = self.access {
            let on_change = self.on_change;
            stage = stage.on_drag(move |s: &mut S, d: &mut Drag, tx: &mut Tx| {
                let changed = access(s).step_drag(d);
                if changed && let Some(cb) = &on_change {
                    cb(s, tx);
                }
            });
        }

        el().key(self.key).size(vw, vh).center().child(stage)
    }
}

fn paint_annotation(mut shape: Shape, ann: &CanvasAnnotation) -> Shape {
    match ann {
        CanvasAnnotation::Stroke {
            points,
            color,
            width,
        } => {
            if points.len() == 1 {
                let p = points[0];
                shape = shape.line(p, [p[0] + 0.01, p[1]], *width, LineCap::Round, *color);
            } else if points.len() >= 2 {
                shape = shape.stroke(points.clone(), *width, *color);
            }
        }
        CanvasAnnotation::Arrow {
            from,
            to,
            color,
            width,
        } => {
            shape = shape.line(*from, *to, *width, LineCap::Round, *color);
            if let Some((w1, w2)) = arrow_wings(*from, *to, *width) {
                shape = shape.line(*to, w1, *width, LineCap::Round, *color);
                shape = shape.line(*to, w2, *width, LineCap::Round, *color);
            }
        }
        CanvasAnnotation::Rect {
            x,
            y,
            w,
            h,
            color,
            width,
            fill,
        } => {
            let path = rect_path(*x, *y, *w, *h);
            if let Some(bg) = fill {
                shape = shape.fill_path(&path, *bg);
            }
            if *width > 0.0 {
                shape = shape.stroke_path(&path, *width, LineCap::Butt, *color);
            }
        }
    }
    shape
}

#[allow(clippy::too_many_arguments)]
fn paint_crop_overlay(
    mut shape: Shape,
    c: CropRect,
    nw: f32,
    nh: f32,
    scale: f32,
    scrim: Color,
    border: Color,
    handle_color: Color,
    show_handles: bool,
) -> Shape {
    // Four dimmed bands outside the crop rectangle.
    if c.y > 0.0 {
        shape = shape.fill_path(&rect_path(0.0, 0.0, nw, c.y), scrim);
    }
    if c.y + c.h < nh {
        shape = shape.fill_path(&rect_path(0.0, c.y + c.h, nw, nh - (c.y + c.h)), scrim);
    }
    if c.x > 0.0 {
        shape = shape.fill_path(&rect_path(0.0, c.y, c.x, c.h), scrim);
    }
    if c.x + c.w < nw {
        shape = shape.fill_path(&rect_path(c.x + c.w, c.y, nw - (c.x + c.w), c.h), scrim);
    }
    let inv = 1.0 / scale.max(0.05);
    shape = shape.stroke_path(
        &rect_path(c.x, c.y, c.w, c.h),
        1.5 * inv,
        LineCap::Butt,
        border,
    );
    if show_handles {
        let hs = 8.0 * inv;
        let half = hs * 0.5;
        let pts = [
            [c.x, c.y],
            [c.x + c.w * 0.5, c.y],
            [c.x + c.w, c.y],
            [c.x + c.w, c.y + c.h * 0.5],
            [c.x + c.w, c.y + c.h],
            [c.x + c.w * 0.5, c.y + c.h],
            [c.x, c.y + c.h],
            [c.x, c.y + c.h * 0.5],
        ];
        for [hx, hy] in pts {
            shape = shape.fill_path(&rect_path(hx - half, hy - half, hs, hs), handle_color);
        }
    }
    shape
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Path {
    Path::new()
        .move_to([x, y])
        .line_to([x + w, y])
        .line_to([x + w, y + h])
        .line_to([x, y + h])
        .close()
}

#[cfg(test)]
#[path = "image_canvas/tests.rs"]
mod tests;
