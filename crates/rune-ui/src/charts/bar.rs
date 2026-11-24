//! Tremor / Recharts-level BarChart supporting grouped or stacked bars, vertical or horizontal
//! orientation, animated reveal, hover highlight, legend, and floating tooltip.

use std::rc::Rc;

use rune::{El, LineCap, Shape, Tx, canvas, col, el, row, text};

use crate::charts::frame::{ChartSeries, chart_legend, chart_tooltip_card};
use crate::charts::scale::{format_tick, nice_ticks};
use crate::theme::UiTheme;

type HoverFn = Rc<dyn Fn(El, Option<usize>) -> El>;

pub struct BarChart {
    key: String,
    categories: Vec<String>,
    series: Vec<ChartSeries>,
    width: f32,
    height: f32,
    stacked: bool,
    horizontal: bool,
    show_legend: bool,
    unit: String,
    progress: f32,
    hovered: Option<usize>,
    on_hover: Option<HoverFn>,
}

impl BarChart {
    pub fn new(key: impl Into<String>, categories: Vec<String>, series: Vec<ChartSeries>) -> Self {
        Self {
            key: key.into(),
            categories,
            series,
            width: 520.0,
            height: 220.0,
            stacked: false,
            horizontal: false,
            show_legend: true,
            unit: String::new(),
            progress: 1.0,
            hovered: None,
            on_hover: None,
        }
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = w.max(180.0);
        self.height = h.max(100.0);
        self
    }

    pub fn stacked(mut self, on: bool) -> Self {
        self.stacked = on;
        self
    }

    pub fn horizontal(mut self, on: bool) -> Self {
        self.horizontal = on;
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
                fc(s, if entered { idx } else { None })
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

        let prog = if theme.effect_gate.allows_animation() {
            self.progress
        } else {
            1.0
        };
        let mut data_max = 1.0f32;
        if self.stacked {
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
                    data_max = data_max.max(v);
                }
            }
        }
        let axis = nice_ticks(0.0, data_max, 5);
        let span = (axis.max - axis.min).max(1e-5);

        let pad_l = if self.horizontal { 76.0 } else { 42.0 };
        let pad_r = 12.0;
        let pad_t = 12.0;
        let pad_b = 26.0;
        let plot_w = (self.width - pad_l - pad_r).max(40.0);
        let plot_h = (self.height - pad_t - pad_b).max(40.0);

        let mut grid_shape = Shape::new().view_box(self.width, self.height);
        for &tv in &axis.ticks {
            let norm = ((tv - axis.min) / span).clamp(0.0, 1.0);
            if self.horizontal {
                let x = pad_l + plot_w * norm;
                grid_shape = grid_shape.line(
                    [x, pad_t],
                    [x, pad_t + plot_h],
                    1.0,
                    LineCap::Butt,
                    theme.chart.grid,
                );
            } else {
                let y = pad_t + plot_h * (1.0 - norm);
                grid_shape = grid_shape.line(
                    [pad_l, y],
                    [pad_l + plot_w, y],
                    1.0,
                    LineCap::Butt,
                    theme.chart.grid,
                );
            }
        }

        let mut stage = el()
            .key(format!("{}-stage", self.key))
            .size(self.width, self.height)
            .child(
                canvas(grid_shape)
                    .absolute(0.0, 0.0)
                    .size(self.width, self.height),
            );

        // Axis labels
        for (ti, &tv) in axis.ticks.iter().enumerate() {
            let norm = ((tv - axis.min) / span).clamp(0.0, 1.0);
            if self.horizontal {
                let x = pad_l + plot_w * norm;
                stage = stage.child(
                    text(format_tick(tv))
                        .key(format!("{}-xt-{ti}", self.key))
                        .font(k.ty.small)
                        .color(theme.chart.axis)
                        .absolute((x - 10.0).max(pad_l), pad_t + plot_h + 6.0),
                );
            } else {
                let y = pad_t + plot_h * (1.0 - norm);
                stage = stage.child(
                    text(format_tick(tv))
                        .key(format!("{}-yt-{ti}", self.key))
                        .font(k.ty.small)
                        .color(theme.chart.axis)
                        .absolute(2.0, (y - 7.0).max(0.0)),
                );
            }
        }

        let s_count = self.series.len().max(1);
        for i in 0..n {
            let is_hov = self.hovered == Some(i);
            let cat = self.categories.get(i).cloned().unwrap_or_default();

            if self.horizontal {
                let slot_h = plot_h / n as f32;
                let y0 = pad_t + i as f32 * slot_h;
                stage = stage.child(
                    text(cat)
                        .font(k.ty.small)
                        .color(theme.chart.axis)
                        .absolute(4.0, y0 + slot_h * 0.3),
                );
                let mut x_acc = pad_l;
                let bar_h = if self.stacked {
                    (slot_h * 0.58).clamp(8.0, 26.0)
                } else {
                    ((slot_h * 0.72) / s_count as f32).clamp(6.0, 18.0)
                };
                for (si, ser) in self.series.iter().enumerate() {
                    let v = ser.values.get(i).copied().unwrap_or(0.0).max(0.0);
                    let w = ((v / span) * plot_w * prog).max(2.0);
                    let col_c = ser.resolved_color(si, theme);
                    let by = if self.stacked {
                        y0 + (slot_h - bar_h) * 0.5
                    } else {
                        y0 + 4.0 + si as f32 * (bar_h + 2.0)
                    };
                    let bx = if self.stacked { x_acc } else { pad_l };
                    stage = stage.child(
                        el().absolute(bx, by)
                            .size(w, bar_h)
                            .radius(4.0)
                            .fill(if is_hov { col_c } else { col_c.alpha(0.88) }),
                    );
                    if self.stacked {
                        x_acc += w;
                    }
                }
                if let Some(hov) = &self.on_hover {
                    let hit = el().absolute(pad_l, y0).size(plot_w, slot_h);
                    stage = stage.child(hov(hit, Some(i)));
                }
            } else {
                let slot_w = plot_w / n as f32;
                let x0 = pad_l + i as f32 * slot_w;
                stage = stage.child(
                    text(cat)
                        .font(k.ty.small)
                        .color(theme.chart.axis)
                        .absolute(x0 + (slot_w * 0.5 - 14.0).max(0.0), pad_t + plot_h + 6.0),
                );

                let group_w = (slot_w * 0.68).max(8.0);
                let gx = x0 + (slot_w - group_w) * 0.5;
                let mut y_acc = pad_t + plot_h;
                let bar_w = if self.stacked {
                    group_w.clamp(8.0, 44.0)
                } else {
                    ((group_w - (s_count.saturating_sub(1) as f32 * 3.0)) / s_count as f32)
                        .clamp(6.0, 32.0)
                };

                for (si, ser) in self.series.iter().enumerate() {
                    let v = ser.values.get(i).copied().unwrap_or(0.0).max(0.0);
                    let h = ((v / span) * plot_h * prog).max(2.0);
                    let col_c = ser.resolved_color(si, theme);
                    let (bx, by) = if self.stacked {
                        y_acc -= h;
                        (x0 + (slot_w - bar_w) * 0.5, y_acc)
                    } else {
                        (gx + si as f32 * (bar_w + 3.0), pad_t + plot_h - h)
                    };
                    stage = stage.child(
                        el().absolute(bx, by)
                            .size(bar_w, h)
                            .radius(4.0)
                            .fill(if is_hov { col_c } else { col_c.alpha(0.88) }),
                    );
                }
                if let Some(hov) = &self.on_hover {
                    let hit = el().absolute(x0, pad_t).size(slot_w, plot_h);
                    stage = stage.child(hov(hit, Some(i)));
                }
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
            let tx = (pad_l + (hi as f32 / n as f32) * plot_w + 16.0).min(self.width - 165.0);
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
