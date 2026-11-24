//! Tremor / Recharts-level DonutChart (and PieChart when `inner_ratio == 0.0`) with vector arc
//! polygons, center KPI label/value, interactive legend, hover pop-out, and sweep animation.

use std::f32::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

use rune::{Color, El, Shape, Tx, canvas, col, el, row, spacer, text};

use crate::charts::scale::format_tick;
use crate::theme::UiTheme;

#[derive(Clone, Debug)]
pub struct DonutSlice {
    pub label: String,
    pub value: f32,
    pub color: Option<Color>,
}

impl DonutSlice {
    pub fn new(label: impl Into<String>, value: f32) -> Self {
        Self {
            label: label.into(),
            value: value.max(0.0),
            color: None,
        }
    }
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }
}

type HoverFn = Rc<dyn Fn(El, Option<usize>) -> El>;

pub struct DonutChart {
    key: String,
    slices: Vec<DonutSlice>,
    diameter: f32,
    inner_ratio: f32,
    center_label: Option<String>,
    center_value: Option<String>,
    unit: String,
    progress: f32,
    hovered: Option<usize>,
    show_legend: bool,
    on_hover: Option<HoverFn>,
}

impl DonutChart {
    pub fn new(key: impl Into<String>, slices: Vec<DonutSlice>) -> Self {
        Self {
            key: key.into(),
            slices,
            diameter: 180.0,
            inner_ratio: 0.62,
            center_label: None,
            center_value: None,
            unit: String::new(),
            progress: 1.0,
            hovered: None,
            show_legend: true,
            on_hover: None,
        }
    }

    pub fn diameter(mut self, d: f32) -> Self {
        self.diameter = d.max(80.0);
        self
    }

    /// `0.0` renders a solid Pie chart; `0.62` (default) renders a Tremor Donut chart.
    pub fn inner_ratio(mut self, ratio: f32) -> Self {
        self.inner_ratio = ratio.clamp(0.0, 0.88);
        self
    }

    pub fn center_kpi(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.center_value = Some(value.into());
        self.center_label = Some(label.into());
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

    pub fn show_legend(mut self, on: bool) -> Self {
        self.show_legend = on;
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
        let total: f32 = self.slices.iter().map(|s| s.value.max(0.0)).sum();
        let prog = if theme.effect_gate.allows_animation() {
            self.progress
        } else {
            1.0
        };

        let pad = 8.0;
        let box_sz = self.diameter + pad * 2.0;
        let cx = box_sz * 0.5;
        let cy = box_sz * 0.5;
        let r_out = self.diameter * 0.5;
        let r_in = r_out * self.inner_ratio;

        let mut shape = Shape::new().view_box(box_sz, box_sz);
        if total > 1e-5 {
            let mut angle = -FRAC_PI_2;
            let gap = if self.slices.len() > 1 { 0.02 } else { 0.0 };
            for (i, slice) in self.slices.iter().enumerate() {
                let frac = slice.value / total;
                let sweep = (frac * TAU * prog - gap).max(0.0);
                if sweep <= 0.002 {
                    angle += frac * TAU * prog;
                    continue;
                }
                let a0 = angle + gap * 0.5;
                let a1 = a0 + sweep;
                let mid = (a0 + a1) * 0.5;
                let pop = if self.hovered == Some(i) { 5.0 } else { 0.0 };
                let ox = cx + pop * mid.cos();
                let oy = cy + pop * mid.sin();
                let steps = ((sweep * 24.0).ceil() as usize).clamp(6, 48);

                let mut poly = Vec::with_capacity(steps * 2 + 2);
                for s in 0..=steps {
                    let a = a0 + (s as f32 / steps as f32) * sweep;
                    poly.push([ox + r_out * a.cos(), oy + r_out * a.sin()]);
                }
                if r_in > 1.0 {
                    for s in (0..=steps).rev() {
                        let a = a0 + (s as f32 / steps as f32) * sweep;
                        poly.push([ox + r_in * a.cos(), oy + r_in * a.sin()]);
                    }
                } else {
                    poly.push([ox, oy]);
                }

                let base_col = slice.color.unwrap_or_else(|| theme.chart.color(i));
                let draw_col = match self.hovered {
                    Some(hi) if hi != i => base_col.alpha(0.55),
                    _ => base_col,
                };
                shape = shape.fill(poly, draw_col);
                angle += frac * TAU * prog;
            }
        }

        let mut ring = el()
            .size(box_sz, box_sz)
            .center()
            .child(canvas(shape).absolute(0.0, 0.0).size(box_sz, box_sz));

        if self.inner_ratio >= 0.45 {
            let (kpi_val, kpi_lbl) = if let Some(hi) = self.hovered.and_then(|i| self.slices.get(i))
            {
                (
                    format!("{}{}", format_tick(hi.value), self.unit),
                    hi.label.clone(),
                )
            } else {
                (
                    self.center_value
                        .unwrap_or_else(|| format!("{}{}", format_tick(total), self.unit)),
                    self.center_label.unwrap_or_else(|| "Total".into()),
                )
            };
            ring = ring.child(
                col()
                    .center()
                    .gap(2.0)
                    .child(text(kpi_val).font(k.ty.ui_medium).color(c.text_base))
                    .child(text(kpi_lbl).font(k.ty.small).color(c.text_muted)),
            );
        }

        if !self.show_legend {
            return ring.key(self.key);
        }

        let mut legend = col().gap(6.0).min_w(150.0);
        for (i, slice) in self.slices.iter().enumerate() {
            let col_c = slice.color.unwrap_or_else(|| theme.chart.color(i));
            let pct = if total > 1e-5 {
                slice.value / total * 100.0
            } else {
                0.0
            };
            let mut item = row()
                .key(format!("{}-leg-{i}", self.key))
                .w_full()
                .px(8.0)
                .py(4.0)
                .gap(8.0)
                .center_y()
                .radius(theme.radii.control)
                .fill(if self.hovered == Some(i) {
                    theme.selected_fill
                } else {
                    Color::TRANSPARENT
                })
                .child(el().size(8.0, 8.0).radius(theme.radii.pill).fill(col_c))
                .child(
                    text(slice.label.clone())
                        .font(k.ty.small_medium)
                        .color(c.text_base),
                )
                .child(spacer())
                .child(
                    text(format!("{pct:.0}%"))
                        .font(k.ty.small)
                        .color(c.text_muted),
                );
            if let Some(hov) = &self.on_hover {
                item = hov(item, Some(i));
            }
            legend = legend.child(item);
        }

        row()
            .key(self.key)
            .gap(20.0)
            .center_y()
            .child(ring)
            .child(legend)
    }
}
