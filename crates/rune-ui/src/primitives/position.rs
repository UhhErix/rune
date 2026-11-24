//! Anchored floating positioning with flip, cross-axis shift, collision padding, and arrow
//! placement (Floating UI / Radix Popper semantics).

use rune::{Align, Side};
use rune_kit::Placement;

/// Axis-aligned rectangle in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub fn center_x(self) -> f32 {
        self.x + self.w * 0.5
    }
    pub fn center_y(self) -> f32 {
        self.y + self.h * 0.5
    }
}

/// Configuration for [`compute_position`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionSpec {
    pub placement: Placement,
    pub side_offset: f32,
    pub align_offset: f32,
    pub collision_padding: f32,
    pub flip: bool,
    pub shift: bool,
    pub arrow_size: f32,
}

impl Default for PositionSpec {
    fn default() -> Self {
        Self {
            placement: Placement::BottomStart,
            side_offset: 6.0,
            align_offset: 0.0,
            collision_padding: 8.0,
            flip: true,
            shift: true,
            arrow_size: 0.0,
        }
    }
}

/// Resolved floating coordinates, side/align after collision handling, available viewport space,
/// and optional arrow offset along the surface's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComputedPosition {
    pub x: f32,
    pub y: f32,
    pub side: Side,
    pub align: Align,
    pub flipped: bool,
    pub shifted: bool,
    pub arrow_offset: Option<f32>,
    pub available_width: f32,
    pub available_height: f32,
}

fn opposite(side: Side) -> Side {
    match side {
        Side::Top => Side::Bottom,
        Side::Bottom => Side::Top,
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    }
}

fn main_coord(anchor: Rect, floating: (f32, f32), side: Side, offset: f32) -> f32 {
    match side {
        Side::Top => anchor.y - offset - floating.1,
        Side::Bottom => anchor.bottom() + offset,
        Side::Left => anchor.x - offset - floating.0,
        Side::Right => anchor.right() + offset,
    }
}

fn cross_coord(anchor: Rect, floating: (f32, f32), side: Side, align: Align, offset: f32) -> f32 {
    match side {
        Side::Top | Side::Bottom => {
            let base = match align {
                Align::Start => anchor.x,
                Align::Center => anchor.center_x() - floating.0 * 0.5,
                Align::End => anchor.right() - floating.0,
            };
            base + offset
        }
        Side::Left | Side::Right => {
            let base = match align {
                Align::Start => anchor.y,
                Align::Center => anchor.center_y() - floating.1 * 0.5,
                Align::End => anchor.bottom() - floating.1,
            };
            base + offset
        }
    }
}

fn overflows_main(
    coord: f32,
    floating: (f32, f32),
    viewport: (f32, f32),
    side: Side,
    pad: f32,
) -> bool {
    match side {
        Side::Top => coord < pad,
        Side::Bottom => coord + floating.1 > viewport.1 - pad,
        Side::Left => coord < pad,
        Side::Right => coord + floating.0 > viewport.0 - pad,
    }
}

fn available_main(anchor: Rect, viewport: (f32, f32), side: Side, offset: f32, pad: f32) -> f32 {
    match side {
        Side::Top => (anchor.y - offset - pad).max(0.0),
        Side::Bottom => (viewport.1 - pad - anchor.bottom() - offset).max(0.0),
        Side::Left => (anchor.x - offset - pad).max(0.0),
        Side::Right => (viewport.0 - pad - anchor.right() - offset).max(0.0),
    }
}

fn split_placement(p: Placement) -> (Side, Align) {
    match p {
        Placement::Top => (Side::Top, Align::Center),
        Placement::TopStart => (Side::Top, Align::Start),
        Placement::TopEnd => (Side::Top, Align::End),
        Placement::Bottom => (Side::Bottom, Align::Center),
        Placement::BottomStart => (Side::Bottom, Align::Start),
        Placement::BottomEnd => (Side::Bottom, Align::End),
        Placement::Left => (Side::Left, Align::Center),
        Placement::Right => (Side::Right, Align::Center),
        Placement::RightStart => (Side::Right, Align::Start),
    }
}

/// Computes floating surface coordinates relative to `anchor` within `viewport`, applying
/// main-axis flip and cross-axis shift with collision padding.
pub fn compute_position(
    anchor: Rect,
    floating: (f32, f32),
    viewport: (f32, f32),
    spec: PositionSpec,
) -> ComputedPosition {
    let (mut side, align) = split_placement(spec.placement);
    let pad = spec.collision_padding.max(0.0);
    let mut flipped = false;

    let mut main = main_coord(anchor, floating, side, spec.side_offset);
    if spec.flip && overflows_main(main, floating, viewport, side, pad) {
        let alt = opposite(side);
        let alt_main = main_coord(anchor, floating, alt, spec.side_offset);
        let avail_cur = available_main(anchor, viewport, side, spec.side_offset, pad);
        let avail_alt = available_main(anchor, viewport, alt, spec.side_offset, pad);
        if !overflows_main(alt_main, floating, viewport, alt, pad) || avail_alt > avail_cur {
            side = alt;
            main = alt_main;
            flipped = true;
        }
    }

    let raw_cross = cross_coord(anchor, floating, side, align, spec.align_offset);
    let mut cross = raw_cross;
    let mut shifted = false;

    if spec.shift {
        let (max_cross, size_cross) = match side {
            Side::Top | Side::Bottom => (viewport.0 - pad, floating.0),
            Side::Left | Side::Right => (viewport.1 - pad, floating.1),
        };
        let min_c = pad;
        let max_c = (max_cross - size_cross).max(min_c);
        cross = cross.clamp(min_c, max_c);
        shifted = (cross - raw_cross).abs() > 0.01;

        let (max_main, size_main) = match side {
            Side::Top | Side::Bottom => (viewport.1 - pad, floating.1),
            Side::Left | Side::Right => (viewport.0 - pad, floating.0),
        };
        let max_m = (max_main - size_main).max(pad);
        main = main.clamp(pad, max_m);
    }

    let (x, y) = match side {
        Side::Top | Side::Bottom => (cross, main),
        Side::Left | Side::Right => (main, cross),
    };

    let (available_width, available_height) = match side {
        Side::Top | Side::Bottom => (
            (viewport.0 - 2.0 * pad).max(0.0),
            available_main(anchor, viewport, side, spec.side_offset, pad),
        ),
        Side::Left | Side::Right => (
            available_main(anchor, viewport, side, spec.side_offset, pad),
            (viewport.1 - 2.0 * pad).max(0.0),
        ),
    };

    let arrow_offset = if spec.arrow_size > 0.0 {
        let half = spec.arrow_size * 0.5;
        let offset = match side {
            Side::Top | Side::Bottom => {
                (anchor.center_x() - x).clamp(half, (floating.0 - half).max(half))
            }
            Side::Left | Side::Right => {
                (anchor.center_y() - y).clamp(half, (floating.1 - half).max(half))
            }
        };
        Some(offset)
    } else {
        None
    };

    ComputedPosition {
        x,
        y,
        side,
        align,
        flipped,
        shifted,
        arrow_offset,
        available_width,
        available_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_when_bottom_overflows_viewport() {
        let anchor = Rect::new(40.0, 540.0, 120.0, 32.0);
        let pos = compute_position(
            anchor,
            (200.0, 160.0),
            (800.0, 600.0),
            PositionSpec::default(),
        );
        assert!(pos.flipped);
        assert_eq!(pos.side, Side::Top);
        assert!((pos.y - (540.0 - 6.0 - 160.0)).abs() < 0.01);
    }

    #[test]
    fn shifts_horizontally_when_near_right_edge_and_clamps_arrow() {
        let anchor = Rect::new(720.0, 100.0, 60.0, 32.0);
        let spec = PositionSpec {
            placement: Placement::BottomStart,
            arrow_size: 10.0,
            ..PositionSpec::default()
        };
        let pos = compute_position(anchor, (220.0, 120.0), (800.0, 600.0), spec);
        assert!(!pos.flipped);
        assert!(pos.shifted);
        assert!((pos.x - (800.0 - 8.0 - 220.0)).abs() < 0.01);
        let arrow = pos.arrow_offset.unwrap_or(0.0);
        assert!(arrow > 100.0 && arrow < 220.0);
    }
}
