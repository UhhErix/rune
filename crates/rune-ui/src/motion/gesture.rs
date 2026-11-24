//! 1:1 drag, elastic rubber-band bounds, and velocity fling gesture primitives built directly
//! on `rune_motion::VelocityTracker`, `rubber_band`, and `project`.

use rune_motion::{VelocityTracker, deceleration, project, rubber_band};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DragAxis {
    X,
    Y,
    #[default]
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragBounds {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    /// Soft rubber-band stretch dimension beyond bounds (`0.0` = hard clamp).
    pub elastic: f32,
}

impl Default for DragBounds {
    fn default() -> Self {
        Self {
            min_x: f32::NEG_INFINITY,
            max_x: f32::INFINITY,
            min_y: f32::NEG_INFINITY,
            max_y: f32::INFINITY,
            elastic: 80.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlingResult {
    pub projected_x: f32,
    pub projected_y: f32,
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub clamped_x: f32,
    pub clamped_y: f32,
}

/// Reusable 2D drag & fling state machine (`drag`, `dragConstraints`, `dragElastic` analogue).
#[derive(Clone, Debug)]
pub struct DragGestureState {
    pub axis: DragAxis,
    pub bounds: DragBounds,
    pub origin_x: f32,
    pub origin_y: f32,
    pub x: f32,
    pub y: f32,
    pub dragging: bool,
    vel_x: VelocityTracker<f32>,
    vel_y: VelocityTracker<f32>,
}

impl Default for DragGestureState {
    fn default() -> Self {
        Self::new(DragAxis::Both, DragBounds::default())
    }
}

impl DragGestureState {
    pub fn new(axis: DragAxis, bounds: DragBounds) -> Self {
        Self {
            axis,
            bounds,
            origin_x: 0.0,
            origin_y: 0.0,
            x: 0.0,
            y: 0.0,
            dragging: false,
            vel_x: VelocityTracker::new(),
            vel_y: VelocityTracker::new(),
        }
    }

    pub fn start(&mut self, t_secs: f64) {
        self.origin_x = self.x;
        self.origin_y = self.y;
        self.dragging = true;
        self.vel_x.reset();
        self.vel_y.reset();
        self.vel_x.add(t_secs, self.x);
        self.vel_y.add(t_secs, self.y);
    }

    pub fn update(&mut self, dx: f32, dy: f32, t_secs: f64) {
        if !self.dragging {
            self.start(t_secs);
        }
        let raw_x = match self.axis {
            DragAxis::X | DragAxis::Both => self.origin_x + dx,
            DragAxis::Y => self.origin_x,
        };
        let raw_y = match self.axis {
            DragAxis::Y | DragAxis::Both => self.origin_y + dy,
            DragAxis::X => self.origin_y,
        };
        self.x = apply_elastic(
            raw_x,
            self.bounds.min_x,
            self.bounds.max_x,
            self.bounds.elastic,
        );
        self.y = apply_elastic(
            raw_y,
            self.bounds.min_y,
            self.bounds.max_y,
            self.bounds.elastic,
        );
        self.vel_x.add(t_secs, self.x);
        self.vel_y.add(t_secs, self.y);
    }

    /// Ends the drag, projecting momentum and clamping to bounds (or restoring origin on cancel).
    pub fn end(&mut self, cancelled: bool) -> FlingResult {
        self.dragging = false;
        if cancelled {
            self.x = self.origin_x;
            self.y = self.origin_y;
            self.vel_x.reset();
            self.vel_y.reset();
            return FlingResult {
                projected_x: self.x,
                projected_y: self.y,
                velocity_x: 0.0,
                velocity_y: 0.0,
                clamped_x: self.x,
                clamped_y: self.y,
            };
        }
        let vx = self.vel_x.velocity();
        let vy = self.vel_y.velocity();
        let px = project(self.x, vx, deceleration::NORMAL);
        let py = project(self.y, vy, deceleration::NORMAL);
        let cx = px.clamp(self.bounds.min_x, self.bounds.max_x);
        let cy = py.clamp(self.bounds.min_y, self.bounds.max_y);
        self.x = cx;
        self.y = cy;
        self.vel_x.reset();
        self.vel_y.reset();
        FlingResult {
            projected_x: px,
            projected_y: py,
            velocity_x: vx,
            velocity_y: vy,
            clamped_x: cx,
            clamped_y: cy,
        }
    }
}

fn apply_elastic(val: f32, min: f32, max: f32, elastic: f32) -> f32 {
    if val < min {
        if elastic <= 0.0 {
            min
        } else {
            min - rubber_band(min - val, elastic)
        }
    } else if val > max {
        if elastic <= 0.0 {
            max
        } else {
            max + rubber_band(val - max, elastic)
        }
    } else {
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elastic_bounds_resist_overdrag_and_fling_clamps() {
        let bounds = DragBounds {
            min_x: 0.0,
            max_x: 200.0,
            min_y: 0.0,
            max_y: 0.0,
            elastic: 100.0,
        };
        let mut g = DragGestureState::new(DragAxis::X, bounds);
        g.start(0.0);
        g.update(300.0, 50.0, 0.02);
        assert!(g.x > 200.0 && g.x < 300.0);
        assert_eq!(g.y, 0.0);
        let res = g.end(false);
        assert_eq!(res.clamped_x, 200.0);
        assert!(res.velocity_x > 1000.0);
    }
}
