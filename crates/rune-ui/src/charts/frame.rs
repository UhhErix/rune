//! Shared Tremor/Recharts-style chart series model, legend, and interactive hover tooltip.

use rune::{Color, El, col, el, row, spacer, text};

use crate::charts::scale::format_tick;
use crate::theme::UiTheme;

/// One named numeric data series.
#[derive(Clone, Debug)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f32>,
    pub color: Option<Color>,
}

impl ChartSeries {
    pub fn new(name: impl Into<String>, values: Vec<f32>) -> Self {
        Self {
            name: name.into(),
            values,
            color: None,
        }
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn resolved_color(&self, idx: usize, theme: &UiTheme) -> Color {
        self.color.unwrap_or_else(|| theme.chart.color(idx))
    }
}

/// Renders a horizontal color-dot chart legend.
pub fn chart_legend(series: &[ChartSeries], theme: &UiTheme) -> El {
    let k = &theme.kit;
    let items = series.iter().enumerate().map(|(i, s)| {
        row()
            .gap(6.0)
            .center_y()
            .child(
                el().size(8.0, 8.0)
                    .radius(theme.radii.pill)
                    .fill(s.resolved_color(i, theme)),
            )
            .child(
                text(s.name.clone())
                    .font(k.ty.small_medium)
                    .color(k.c.text_muted),
            )
    });
    row().gap(14.0).wrap().center_y().children(items)
}

/// Renders a Tremor-style floating chart tooltip card for the hovered data index.
pub fn chart_tooltip_card(
    category: &str,
    entries: &[(String, Color, f32)],
    unit: &str,
    theme: &UiTheme,
) -> El {
    let k = &theme.kit;
    let c = &k.c;
    let mut box_el = col()
        .min_w(140.0)
        .padding(10.0)
        .gap(5.0)
        .child(
            text(category.to_owned())
                .font(k.ty.small_medium)
                .color(c.text_base),
        )
        .child(el().w_full().h(1.0).fill(theme.hairline));

    for (name, col_c, val) in entries {
        box_el = box_el.child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(el().size(7.0, 7.0).radius(theme.radii.pill).fill(*col_c))
                .child(text(name.clone()).font(k.ty.small).color(c.text_muted))
                .child(spacer())
                .child(
                    text(format!("{}{}", format_tick(*val), unit))
                        .font(k.ty.small_medium)
                        .color(c.text_base),
                ),
        );
    }
    theme.overlay_surface(box_el, theme.radii.control)
}
