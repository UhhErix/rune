//! Tremor-style inline Sparkline micro-chart (Line, Area, or Bar) with zero chrome.

use rune::{Color, El, LineCap, Shape, canvas, el};

use crate::charts::scale::monotone_cubic_points;
use crate::theme::UiTheme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SparklineKind {
    Line,
    #[default]
    Area,
    Bar,
}

pub struct Sparkline {
    values: Vec<f32>,
    kind: SparklineKind,
    width: f32,
    height: f32,
    color: Option<Color>,
    smooth: bool,
}

impl Sparkline {
    pub fn new(values: Vec<f32>) -> Self {
        Self {
            values,
            kind: SparklineKind::Area,
            width: 120.0,
            height: 28.0,
            color: None,
            smooth: true,
        }
    }

    pub fn kind(mut self, k: SparklineKind) -> Self {
        self.kind = k;
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w.max(24.0);
        self.height = h.max(12.0);
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn smooth(mut self, on: bool) -> Self {
        self.smooth = on;
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let color = self.color.unwrap_or_else(|| theme.chart.color(0));
        let n = self.values.len();
        if n < 2 {
            return el().size(self.width, self.height);
        }
        let min = self.values.iter().copied().fold(f32::INFINITY, f32::min);
        let max = self
            .values
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let span = (max - min).max(1e-5);
        let pad = 2.0;
        let inner_h = (self.height - pad * 2.0).max(4.0);

        let mut shape = Shape::new().view_box(self.width, self.height);
        if self.kind == SparklineKind::Bar {
            let step = self.width / n as f32;
            let bw = (step * 0.7).max(2.0);
            for (i, &v) in self.values.iter().enumerate() {
                let norm = ((v - min) / span).clamp(0.08, 1.0);
                let x = i as f32 * step + step * 0.5;
                let y0 = self.height - pad;
                let y1 = y0 - norm * inner_h;
                shape = shape.line([x, y0], [x, y1], bw, LineCap::Round, color);
            }
        } else {
            let pts: Vec<[f32; 2]> = self
                .values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x = (i as f32 / (n - 1) as f32) * self.width;
                    let y = pad + inner_h * (1.0 - (v - min) / span);
                    [x, y]
                })
                .collect();
            let curve = if self.smooth {
                monotone_cubic_points(&pts, 4)
            } else {
                pts
            };
            if self.kind == SparklineKind::Area {
                let mut poly = curve.clone();
                poly.push([self.width, self.height]);
                poly.push([0.0, self.height]);
                shape = shape.fill(poly, color.alpha(0.22));
            }
            shape = shape.stroke(curve, 1.75, color);
        }

        canvas(shape).size(self.width, self.height)
    }
}
