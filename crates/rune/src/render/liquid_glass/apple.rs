//! Single source of truth for Liquid Glass optical, adaptive, interactive and accessibility
//! parameters.
//!
//! Optical targets follow Apple's Liquid Glass design measurements (`docs/exo/liquid-glass.md` §0).
//! Bevel height-field normal, `ratio^8` smoothstep refraction profile, Fresnel `pow 6`, cursor-leaning
//! rim highlight and hover/press spring constants adapt math from `bilalgursen/liquid-glass` (MIT;
//! see `THIRD_PARTY_NOTICES.md`).

#![allow(dead_code)]

use rune_motion::Spring;

/// Minimum and maximum bevel widths in points (`0.17 * min(w, h) / 2`, clamped to `2..12` pt).
pub const BEVEL_FRACTION: f32 = 0.17;
pub const BEVEL_MIN_PT: f32 = 2.0;
pub const BEVEL_MAX_PT: f32 = 12.0;

/// Subtle refraction multiplier relative to bevel width (keeps 1080p rims crisp and non-stepping).
pub const REFRACTION_MULTIPLIER: f32 = 1.65;

/// Chromatic dispersion per-channel offset fraction (`R * (1 - d), B * (1 + d)`).
pub const DISPERSION: f32 = 0.03;

/// Share of the body tint and dimming the bevel lip sheds at the rim, so the refracted backdrop reads
/// through the edge of dark glass (`LIP_CLARITY` in `liquid_glass.wgsl`; 0 in the flat interior).
pub const LIP_CLARITY: f32 = 0.70;

/// Fresnel exponent (`pow(1 - normal.z, 6.0)` from `bilalgursen/liquid-glass`).
pub const FRESNEL_POW: f32 = 6.0;

/// Refraction radial falloff exponent (`pow(ratio, 8.0)` from `bilalgursen/liquid-glass`).
pub const REFRACTION_RATIO_POW: f32 = 8.0;

/// Size-derived frost for `LiquidGlass::regular()`: `clamp(2 + 0.04 * min(w, h), 3, 10)` pt.
pub const REGULAR_FROST_BASE_PT: f32 = 2.0;
pub const REGULAR_FROST_SLOPE: f32 = 0.04;
pub const REGULAR_FROST_MIN_PT: f32 = 3.0;
pub const REGULAR_FROST_MAX_PT: f32 = 10.0;

/// Backdrop saturation for Apple `Regular` glass ("picking colors from nearby content").
pub const REGULAR_SATURATION: f32 = 1.35;

/// Adaptive tone mix amounts for `Regular` glass (toward white over bright, black over dark).
pub const REGULAR_LIGHT_MIX: f32 = 0.20;
pub const REGULAR_DARK_MIX: f32 = 0.24;

/// Elements larger than 200 pt on both axes adapt subtly without flipping dark.
pub const LARGE_ELEMENT_PT: f32 = 200.0;

/// Apple `Clear` frost sigma in points and dark dimming cap over bright backdrops (HIG: 35%).
pub const CLEAR_SIGMA_PT: f32 = 0.6;
pub const CLEAR_DIM_MAX: f32 = 0.35;

/// Apple `Tint` overlay alpha (maps tint to backdrop luminance: `mix(c * 0.75, c, L)`).
pub const TINT_ALPHA: f32 = 0.55;

/// Adaptive shadow reach and opacity range (flat light backdrop -> high-detail / text backdrop).
pub const SHADOW_OFFSET_Y_PT: f32 = 2.0;
pub const SHADOW_BLUR_FRACTION: f32 = 0.25;
pub const SHADOW_BLUR_MIN_PT: f32 = 6.0;
pub const SHADOW_BLUR_MAX_PT: f32 = 18.0;
pub const SHADOW_OPACITY_MIN: f32 = 0.06;
pub const SHADOW_OPACITY_MAX: f32 = 0.20;

/// Interactive glass physics: press scale, gel stretch, drag follow, touch glow, hover brighten.
pub const PRESS_SCALE: f32 = 1.10;
pub const PRESS_STRETCH_MAX: f32 = 1.18;
pub const DRAG_FOLLOW_RATIO: f32 = 0.25;
pub const DRAG_FOLLOW_CAP: f32 = 0.12;
pub const TOUCH_GLOW_CENTER: f32 = 0.22;
pub const TOUCH_GLOW_RIM: f32 = 0.10;
pub const HOVER_BRIGHTEN: f32 = 0.05;
pub const HOVER_SWELL_X: f32 = 1.03;
pub const HOVER_SWELL_Y: f32 = 1.02;

/// Press spring: response 0.21 s, damping ratio 0.6 (~10% overshoot; release uses the same spring).
pub fn press_spring(reduced_motion: bool) -> Spring {
    if reduced_motion {
        Spring::new(0.21, 0.0)
    } else {
        Spring::from_response(0.21, 0.6)
    }
}

/// Hover swell springs (`HOVER_X`: ω=17, ζ=0.42; `HOVER_Y`: ω=12, ζ=0.34 from `bilalgursen/liquid-glass`),
/// critically damped under Reduce Motion so glass never wobbles when motion is reduced.
pub fn hover_springs(reduced_motion: bool) -> (Spring, Spring) {
    if reduced_motion {
        (Spring::new(0.18, 0.0), Spring::new(0.18, 0.0))
    } else {
        (
            Spring::from_omega_zeta(17.0, 0.42),
            Spring::from_omega_zeta(12.0, 0.34),
        )
    }
}

/// Touch glow expansion spring (~0.2 s from 0 to element diagonal).
pub fn touch_glow_spring() -> Spring {
    Spring::new(0.20, 0.0)
}

/// Materialize spring for newly appearing unmatched glass elements.
pub fn materialize_spring(reduced_motion: bool) -> Spring {
    if reduced_motion {
        Spring::new(0.12, 0.0)
    } else {
        Spring::new(0.28, 0.0)
    }
}

/// Luminance smoothing time constant (seconds) and dark/light hysteresis thresholds for `Cx::glass_is_dark`.
pub const PROBE_TAU_SEC: f32 = 0.150;
pub const PROBE_DARK_ENTER: f32 = 0.45;
pub const PROBE_DARK_EXIT: f32 = 0.55;

/// Accessibility overrides (`docs/exo/liquid-glass.md` §0).
pub const REDUCE_TRANSPARENCY_SIGMA_SCALE: f32 = 2.5;
pub const REDUCE_TRANSPARENCY_SIGMA_MIN_PT: f32 = 10.0;
pub const REDUCE_TRANSPARENCY_TONE_MIX: f32 = 0.75;
pub const INCREASE_CONTRAST_TONE_MIX: f32 = 0.90;
pub const INCREASE_CONTRAST_BORDER_PT: f32 = 1.0;
pub const INCREASE_CONTRAST_BORDER_ALPHA: f32 = 0.60;

/// Bevel width in points for a shape of `(w_pt, h_pt)`: `clamp(0.17 * min(w, h) / 2, 2, 12)`.
#[inline]
pub fn bevel_pt(w_pt: f32, h_pt: f32, explicit_bevel: f32) -> f32 {
    if explicit_bevel > 0.0 {
        return explicit_bevel.min(w_pt.min(h_pt) * 0.5);
    }
    let min_half = w_pt.min(h_pt).max(0.0) * 0.5;
    (BEVEL_FRACTION * min_half)
        .clamp(BEVEL_MIN_PT, BEVEL_MAX_PT)
        .min(min_half.max(1.0))
}

/// Maximum inward refraction displacement in points for a shape of `(w_pt, h_pt)` and lensing scale.
#[inline]
pub fn refraction_max_pt(w_pt: f32, h_pt: f32, explicit_bevel: f32, scale: f32) -> f32 {
    if scale <= 0.0 {
        return 0.0;
    }
    bevel_pt(w_pt, h_pt, explicit_bevel) * REFRACTION_MULTIPLIER * scale
}

/// Size-derived frost standard deviation in points for `LiquidGlass::regular()`.
#[inline]
pub fn regular_frost_pt(w_pt: f32, h_pt: f32) -> f32 {
    (REGULAR_FROST_BASE_PT + REGULAR_FROST_SLOPE * w_pt.min(h_pt).max(0.0))
        .clamp(REGULAR_FROST_MIN_PT, REGULAR_FROST_MAX_PT)
}

/// Adaptive drop shadow blur in points: `clamp(0.25 * min(w, h), 6, 18)`.
#[inline]
pub fn shadow_blur_pt(w_pt: f32, h_pt: f32) -> f32 {
    (SHADOW_BLUR_FRACTION * w_pt.min(h_pt).max(0.0)).clamp(SHADOW_BLUR_MIN_PT, SHADOW_BLUR_MAX_PT)
}

/// Applies Reduce Transparency frost scaling (`sigma * 2.5`, at least `10.0` pt).
#[inline]
pub fn reduced_transparency_sigma_pt(sigma_pt: f32) -> f32 {
    (sigma_pt * REDUCE_TRANSPARENCY_SIGMA_SCALE).max(REDUCE_TRANSPARENCY_SIGMA_MIN_PT)
}

/// Smoothly updates a shape's tracked luminance with exponential time constant `PROBE_TAU_SEC`
/// and applies hysteresis `(0.45 / 0.55)` to compute `is_dark`.
#[inline]
pub fn step_probe_luminance(prev_l: f32, prev_dark: bool, sample_l: f32, dt: f32) -> (f32, bool) {
    let alpha = if dt <= 0.0 {
        1.0
    } else {
        1.0 - (-dt / PROBE_TAU_SEC).exp()
    };
    let next_l = prev_l + (sample_l.clamp(0.0, 1.0) - prev_l) * alpha;
    let next_dark = if prev_dark {
        next_l < PROBE_DARK_EXIT
    } else {
        next_l < PROBE_DARK_ENTER
    };
    (next_l, next_dark)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_derived_values_match_spec_for_control_and_menu() {
        // 44 pt control: bevel = 0.17 * 22 = 3.74 pt, frost = 2 + 0.04 * 44 = 3.76 pt
        let b_ctrl = bevel_pt(120.0, 44.0, 0.0);
        assert!((b_ctrl - 3.74).abs() < 1e-3, "44pt bevel={b_ctrl}");
        let f_ctrl = regular_frost_pt(120.0, 44.0);
        assert!((f_ctrl - 3.76).abs() < 1e-3, "44pt frost={f_ctrl}");
        let r_ctrl = refraction_max_pt(120.0, 44.0, 0.0, 1.0);
        assert!((r_ctrl - b_ctrl * REFRACTION_MULTIPLIER).abs() < 1e-3);

        // 300 pt menu: bevel clamped to 12 pt, frost clamped to 10 pt
        let b_menu = bevel_pt(300.0, 360.0, 0.0);
        assert!((b_menu - 12.0).abs() < 1e-3, "300pt bevel={b_menu}");
        let f_menu = regular_frost_pt(300.0, 360.0);
        assert!((f_menu - 10.0).abs() < 1e-3, "300pt frost={f_menu}");
        let s_menu = shadow_blur_pt(300.0, 360.0);
        assert!((s_menu - 18.0).abs() < 1e-3, "300pt shadow={s_menu}");
    }

    #[test]
    fn probe_hysteresis_avoids_flicker() {
        let (l, dark) = step_probe_luminance(0.5, false, 0.2, 1.0);
        assert!(l < 0.25 && dark);
        // Rises to 0.50: stays dark until past 0.55.
        let (_, still_dark) = step_probe_luminance(l, dark, 0.50, 10.0);
        assert!(still_dark);
        // Rises to 0.60: flips light.
        let (_, now_light) = step_probe_luminance(0.50, still_dark, 0.60, 10.0);
        assert!(!now_light);
        // Drops to 0.48: stays light until below 0.45.
        let (_, still_light) = step_probe_luminance(0.60, now_light, 0.48, 10.0);
        assert!(!still_light);
    }
}
