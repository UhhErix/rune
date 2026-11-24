//! Tremor / Recharts-level LineChart and AreaChart rendered via Rune native vector `Shape`
//! with Fritsch-Carlson monotone cubic splines, stacked or overlapping areas, animated reveal,
//! hover crosshair, interactive column hit targets, and floating tooltip.

use std::rc::Rc;

use rune::{El, LineCap, Shape, Tx, canvas, col, el, row, text};

use crate::charts::frame::{ChartSeries, chart_legend, chart_tooltip_card};
use crate::charts::scale::{format_tick, monotone_cubic_points, nice_ticks};
use crate::theme::UiTheme;

type HoverFn = Rc<dyn Fn(El, Option<usize>) -> El>;

/// Multi-series LineChart or AreaChart (stacked or overlapping).
pub struct LineChart {
    key: String,
    categories: Vec<String>,
    series: Vec<ChartSeries>,
    width: f32,
    height: f32,
    smooth: bool,
    area: bool,
    stacked: bool,
    show_dots: bool,
    show_legend: bool,
    unit: String,
    progress: f32,
    hovered: Option<usize>,
    on_hover: Option<HoverFn>,
}

impl LineChart {
    pub fn new(key: impl Into<String>, categories: Vec<String>, series: Vec<ChartSeries>) -> Self {
        Self {
            key: key.into(),
            categories,
            series,
            width: 520.0,
            height: 220.0,
            smooth: true,
            area: false,
            stacked: false,
            show_dots: true,
            show_legend: true,
            unit: String::new(),
            progress: 1.0,
            hovered: None,
            on_hover: None,
        }
    }

    pub fn area_chart(
        key: impl Into<String>,
        categories: Vec<String>,
        series: Vec<ChartSeries>,
    ) -> Self {
        Self {
            area: true,
            ..Self::new(key, categories, series)
        }
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w.max(180.0);
        self.height = h.max(100.0);
        self
    }

    pub fn smooth(mut self, on: bool) -> Self {
        self.smooth = on;
        self
    }

    pub fn stacked(mut self, on: bool) -> Self {
        self.stacked = on;
        self
    }

    pub fn show_dots(mut self, on: bool) -> Self {
        self.show_dots = on;
        self
    }

    pub fn show_legend(mut self, on: bool) -> Self {
        self.show_legend = on;
        self
    }

    pub fn unit(mut self, u: impl Into<String>) -> Self {
        self.unit = u.into();
        self
    }

    pub fn progress(mut self, p: f32) -> Self {
        self.progress = p.clamp(0.0, 1.0);
        self
    }

    pub fn hovered(mut self, idx: Option<usize>) -> Self {
        self.hovered = idx;
        self
    }

    pub fn on_hover<S: 'static>(
        mut self,
        f: impl Fn(&mut S, Option<usize>) + Clone + 'static,
    ) -> Self {
        self.on_hover = Some(Rc::new(move |e: El, idx: Option<usize>| {
            let fc = f.clone();
            e.on_hover_change(move |s: &mut S, entered: bool, _tx: &mut Tx| {
                fc(s, if entered { idx } else { None });
            })
        }));
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let k = &theme.kit;
        let c = &k.c;
        let n = self.categories.len().max(
            self.series
                .iter()
                .map(|s| s.values.len())
                .max()
                .unwrap_or(0),
        );
        if n == 0 || self.series.is_empty() {
            return el()
                .key(self.key)
                .size(self.width, self.height)
                .center()
                .child(text("No chart data").font(k.ty.small).color(c.text_muted));
        }

        let pad_l = 42.0;
        let pad_r = 12.0;
        let pad_t = 12.0;
        let pad_b = 26.0;
        let plot_w = (self.width - pad_l - pad_r).max(40.0);
        let plot_h = (self.height - pad_t - pad_b).max(40.0);
        let prog = if theme.effect_gate.allows_animation() {
            self.progress
        } else {
            1.0
        };

        let mut data_min = 0.0f32;
        let mut data_max = 1.0f32;
        if self.stacked && self.area {
            for i in 0..n {
                let sum: f32 = self
                    .series
                    .iter()
                    .map(|s| s.values.get(i).copied().unwrap_or(0.0).max(0.0))
                    .sum();
                data_max = data_max.max(sum);
            }
        } else {
            for s in &self.series {
                for &v in &s.values {
                    data_min = data_min.min(v);
                    data_max = data_max.max(v);
                }
            }
        }

        let axis = nice_ticks(data_min, data_max, 5);
        let span = (axis.max - axis.min).max(1e-5);
        let x_of = |i: usize| -> f32 {
            if n <= 1 {
                pad_l + plot_w * 0.5
            } else {
                pad_l + (i as f32 / (n - 1) as f32) * plot_w
            }
        };
        let y_of = |v: f32| -> f32 {
            let norm = ((v - axis.min) / span).clamp(0.0, 1.0) * prog;
            pad_t + plot_h * (1.0 - norm)
        };

        let mut shape = Shape::new().view_box(self.width, self.height);
        for &tv in &axis.ticks {
            let y = pad_t + plot_h * (1.0 - ((tv - axis.min) / span).clamp(0.0, 1.0));
            shape = shape.line(
                [pad_l, y],
                [pad_l + plot_w, y],
                1.0,
                LineCap::Butt,
                theme.chart.grid,
            );
        }

        let mut prev_top: Vec<f32> = vec![axis.min; n];
        for (si, ser) in self.series.iter().enumerate() {
            let color = ser.resolved_color(si, theme);
            let mut cur_vals = Vec::with_capacity(n);
            let mut pts = Vec::with_capacity(n);
            for (i, &base_v) in prev_top.iter().enumerate().take(n) {
                let raw = ser.values.get(i).copied().unwrap_or(0.0);
                let top = if self.stacked && self.area {
                    base_v + raw.max(0.0)
                } else {
                    raw
                };
                cur_vals.push(top);
                pts.push([x_of(i), y_of(top)]);
            }
            let curve = if self.smooth {
                monotone_cubic_points(&pts, 6)
            } else {
                pts
            };

            if self.area && curve.len() >= 2 {
                let mut poly = curve.clone();
                if self.stacked {
                    let base_pts: Vec<[f32; 2]> =
                        (0..n).map(|i| [x_of(i), y_of(prev_top[i])]).collect();
                    let base_curve = if self.smooth {
                        monotone_cubic_points(&base_pts, 6)
                    } else {
                        base_pts
                    };
                    poly.extend(base_curve.into_iter().rev());
                } else {
                    let base_y = pad_t + plot_h;
                    poly.push([x_of(n - 1), base_y]);
                    poly.push([x_of(0), base_y]);
                }
                shape = shape.fill(poly, color.alpha(0.22));
            }

            shape = shape.stroke(curve, 2.0, color);
            if self.stacked && self.area {
                prev_top = cur_vals;
            }
        }

        if let Some(hi) = self.hovered.filter(|&i| i < n) {
            let hx = x_of(hi);
            shape = shape.line(
                [hx, pad_t],
                [hx, pad_t + plot_h],
                1.0,
                LineCap::Butt,
                theme.chart.crosshair,
            );
        }

        let mut stage = el()
            .key(format!("{}-stage", self.key))
            .size(self.width, self.height)
            .child(
                canvas(shape)
                    .absolute(0.0, 0.0)
                    .size(self.width, self.height),
            );

        for (ti, &tv) in axis.ticks.iter().enumerate() {
            let y = pad_t + plot_h * (1.0 - ((tv - axis.min) / span).clamp(0.0, 1.0));
            stage = stage.child(
                text(format_tick(tv))
                    .key(format!("{}-yt-{ti}", self.key))
                    .font(k.ty.small)
                    .color(theme.chart.axis)
                    .absolute(2.0, (y - 7.0).max(0.0)),
            );
        }

        let x_stride = (n / 6).max(1);
        for (i, cat) in self.categories.iter().enumerate() {
            if i % x_stride == 0 || i + 1 == n {
                let x = (x_of(i) - 16.0).clamp(pad_l, self.width - 36.0);
                stage = stage.child(
                    text(cat.clone())
                        .key(format!("{}-xt-{i}", self.key))
                        .font(k.ty.small)
                        .color(theme.chart.axis)
                        .absolute(x, pad_t + plot_h + 6.0),
                );
            }
        }

        let col_w = plot_w / n as f32;
        for i in 0..n {
            let is_hov = self.hovered == Some(i);
            if self.show_dots && is_hov {
                for (si, ser) in self.series.iter().enumerate() {
                    if let Some(&v) = ser.values.get(i) {
                        let cx = x_of(i);
                        let cy = y_of(v);
                        stage = stage.child(
                            el().absolute(cx - 4.0, cy - 4.0)
                                .size(8.0, 8.0)
                                .radius(theme.radii.pill)
                                .fill(ser.resolved_color(si, theme))
                                .border(2.0, c.page),
                        );
                    }
                }
            }
            if let Some(hov) = &self.on_hover {
                let hit = el()
                    .key(format!("{}-hit-{i}", self.key))
                    .absolute(pad_l + i as f32 * col_w, pad_t)
                    .size(col_w.max(8.0), plot_h);
                stage = stage.child(hov(hit, Some(i)));
            }
        }

        if let Some(hi) = self.hovered.filter(|&i| i < n) {
            let cat = self.categories.get(hi).map_or("", String::as_str);
            let entries: Vec<_> = self
                .series
                .iter()
                .enumerate()
                .map(|(si, s)| {
                    (
                        s.name.clone(),
                        s.resolved_color(si, theme),
                        s.values.get(hi).copied().unwrap_or(0.0),
                    )
                })
                .collect();
            let tip = chart_tooltip_card(cat, &entries, &self.unit, theme);
            let tx = (x_of(hi) + 12.0).min(self.width - 165.0).max(pad_l);
            stage = stage.child(tip.absolute(tx, pad_t + 6.0));
        }

        let mut root = col().key(self.key).gap(8.0);
        if self.show_legend {
            root = root.child(
                row()
                    .w(self.width)
                    .justify_end()
                    .child(chart_legend(&self.series, theme)),
            );
        }
        root.child(stage)
    }
}

/// Alias builder for multi-series AreaChart (`LineChart::area_chart`).
pub type AreaChart = LineChart;
