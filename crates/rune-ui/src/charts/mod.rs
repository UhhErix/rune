//! Tremor / Recharts-level native vector charts: LineChart, AreaChart, BarChart, DonutChart,
//! and Sparkline with nice ticks, monotone splines, legends, animations, and hover tooltips.

pub mod bar;
pub mod donut;
pub mod frame;
pub mod line;
pub mod scale;
pub mod sparkline;

pub use bar::BarChart;
pub use donut::{DonutChart, DonutSlice};
pub use frame::{ChartSeries, chart_legend, chart_tooltip_card};
pub use line::{AreaChart, LineChart};
pub use scale::{AxisTicks, format_tick, monotone_cubic_points, nice_ticks};
pub use sparkline::{Sparkline, SparklineKind};
