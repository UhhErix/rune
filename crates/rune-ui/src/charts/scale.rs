//! Nice axis ticks (Wilkinson/Heckbert `1, 2, 5 × 10^n`) and Fritsch-Carlson monotone cubic
//! spline interpolation (smooth curves with zero overshoot, matching Recharts `type="monotone"`).

/// Computed axis domain and human-readable ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct AxisTicks {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub ticks: Vec<f32>,
}

fn nice_num(range: f32, round: bool) -> f32 {
    if range <= 0.0 || !range.is_finite() {
        return 1.0;
    }
    let exp = range.log10().floor();
    let frac = range / 10.0_f32.powf(exp);
    let nice = if round {
        if frac < 1.5 {
            1.0
        } else if frac < 3.0 {
            2.0
        } else if frac < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if frac <= 1.0 {
        1.0
    } else if frac <= 2.0 {
        2.0
    } else if frac <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * 10.0_f32.powf(exp)
}

/// Computes clean rounded axis min/max/step and tick marks for `[data_min, data_max]`.
pub fn nice_ticks(data_min: f32, data_max: f32, target_ticks: usize) -> AxisTicks {
    let count = target_ticks.clamp(2, 12);
    let (mut lo, mut hi) = if data_min <= data_max {
        (data_min, data_max)
    } else {
        (data_max, data_min)
    };
    if (hi - lo).abs() < 1e-6 {
        lo = (lo - 1.0).min(0.0);
        hi += 1.0;
    }
    let range = nice_num(hi - lo, false);
    let step = nice_num(range / (count - 1) as f32, true).max(1e-6);
    let nice_min = (lo / step).floor() * step;
    let nice_max = (hi / step).ceil() * step;

    let mut ticks = Vec::new();
    let mut v = nice_min;
    let limit = count * 3 + 4;
    for _ in 0..limit {
        if v > nice_max + step * 0.25 {
            break;
        }
        ticks.push(if v.abs() < 1e-5 { 0.0 } else { v });
        v += step;
    }
    AxisTicks {
        min: nice_min,
        max: nice_max,
        step,
        ticks,
    }
}

/// Formats a numeric axis label concisely (`1.2K`, `3.4M`, `42`, `0.5`).
pub fn format_tick(v: f32) -> String {
    let abs = v.abs();
    if abs >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if abs >= 10_000.0 {
        format!("{:.0}K", v / 1_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else if (v - v.round()).abs() < 0.05 {
        format!("{:.0}", v.round())
    } else {
        format!("{v:.1}")
    }
}

/// Samples a Fritsch-Carlson monotone cubic Hermite spline through `pts` (monotonic in `x`).
/// Guarantees smooth curves with zero overshoot between data points.
pub fn monotone_cubic_points(pts: &[[f32; 2]], samples_per_seg: usize) -> Vec<[f32; 2]> {
    let n = pts.len();
    if n <= 2 || samples_per_seg <= 1 {
        return pts.to_vec();
    }

    let mut dx = vec![0.0f32; n - 1];
    let mut slopes = vec![0.0f32; n - 1];
    for i in 0..(n - 1) {
        dx[i] = (pts[i + 1][0] - pts[i][0]).max(1e-5);
        slopes[i] = (pts[i + 1][1] - pts[i][1]) / dx[i];
    }

    let mut tangents = vec![0.0f32; n];
    tangents[0] = slopes[0];
    tangents[n - 1] = slopes[n - 2];
    for i in 1..(n - 1) {
        if slopes[i - 1] * slopes[i] <= 0.0 {
            tangents[i] = 0.0;
        } else {
            let w1 = 2.0 * dx[i] + dx[i - 1];
            let w2 = dx[i] + 2.0 * dx[i - 1];
            tangents[i] = (w1 + w2) / (w1 / slopes[i - 1] + w2 / slopes[i]);
        }
    }

    let mut out = Vec::with_capacity((n - 1) * samples_per_seg + 1);
    for i in 0..(n - 1) {
        let (x0, y0) = (pts[i][0], pts[i][1]);
        let (x1, y1) = (pts[i + 1][0], pts[i + 1][1]);
        let h = x1 - x0;
        let m0 = tangents[i];
        let m1 = tangents[i + 1];
        for s in 0..samples_per_seg {
            let t = s as f32 / samples_per_seg as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
            let h10 = t3 - 2.0 * t2 + t;
            let h01 = -2.0 * t3 + 3.0 * t2;
            let h11 = t3 - t2;
            let x = x0 + t * h;
            let y = h00 * y0 + h10 * h * m0 + h01 * y1 + h11 * h * m1;
            out.push([x, y]);
        }
    }
    out.push(pts[n - 1]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ticks_produces_clean_intervals() {
        let ax = nice_ticks(3.0, 94.0, 5);
        assert_eq!(ax.min, 0.0);
        assert!(ax.max >= 94.0);
        assert!(ax.ticks.len() >= 4);
    }

    #[test]
    fn monotone_cubic_never_overshoots_flat_plateaus() {
        let pts = [[0.0, 10.0], [10.0, 50.0], [20.0, 50.0], [30.0, 10.0]];
        let sampled = monotone_cubic_points(&pts, 8);
        for p in sampled {
            assert!(p[1] >= 9.99 && p[1] <= 50.01, "overshoot at {:?}", p);
        }
    }
}
