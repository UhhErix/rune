//! Liquid Glass material (`El::liquid_glass`), containers (`El::glass_container`, `El::glass_group`),
//! shared-element morphing (`El::glass_id`), unions (`El::glass_union`) and accessibility policy
//! (`GlassPolicy`, `App::glass`, `Tx::set_glass_policy`).

use std::rc::Rc;

use crate::Color;
use crate::El;

/// Material variant for [`LiquidGlass`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlassVariant {
    /// Apple Liquid Glass regular: adaptive light/dark tone from backdrop, size-derived frost,
    /// subtle rim lensing, dispersion and adaptive shadow.
    Regular,
    /// Apple Liquid Glass clear: minimal frost (0.6 pt), adaptive 35% dark dimming over bright
    /// content, rim lensing and specular highlight.
    Clear,
    /// Apple Liquid Glass identity: no glass effect (paints nothing).
    Identity,
    /// Web/Chromium-style explicit backdrop-filter glass (`control`, `panel`, `pane`, `badge`,
    /// `immersive`, or custom brightness/contrast/saturation).
    Custom,
}

/// How Rune decides whether [`El::liquid_glass`] draws live glass or falls back to its solid
/// `still` fill (`App::glass`, `Tx::set_glass_policy`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GlassPolicy {
    /// Follow the system's Reduce Transparency / Increase Contrast / Contrast Theme settings.
    #[default]
    FollowSystem,
    /// Always render live Liquid Glass regardless of the OS Reduce Transparency toggle
    /// (Windows high-contrast themes still use system contrast colours).
    Glass,
    /// Always paint the solid `still` fallback with zero backdrop capture or blur work.
    Solid,
}

/// A Liquid Glass material that frosts, tones, tints, refracts and lights what this window has
/// already drawn behind the element, sharing a per-epoch backdrop capture and frost pyramid.
///
/// Fields are private; use the constructors (`regular`, `clear`, `identity`, `control`, `panel`,
/// `pane`, `badge`, `immersive`) and chainable builders.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidGlass {
    pub(crate) variant: GlassVariant,
    /// Explicit frost standard deviation in points, or `None` to derive from size (`Regular`) or
    /// variant default (`Clear` = 0.6 pt).
    pub(crate) frost_sigma: Option<f32>,
    /// Backdrop brightness multiplier for `Custom` (1.0 = unchanged).
    pub(crate) brightness: f32,
    /// Backdrop contrast multiplier for `Custom` (1.0 = unchanged).
    pub(crate) contrast: f32,
    /// Backdrop saturation multiplier (default 1.35 for Apple `Regular`, 1.0 for `Clear`, 1.8 for
    /// web presets).
    pub(crate) saturation: f32,
    /// Tint colour laid over the toned frost (mapped to backdrop brightness in Apple variants, or
    /// premultiplied overlay in `Custom`).
    pub(crate) tint: Color,
    /// Whether `tint` was explicitly set by `.tint(..)`.
    pub(crate) has_tint: bool,
    /// Edge rim alpha `[top, side, bottom]`.
    pub(crate) rim: [f32; 3],
    /// Rim light colour (default white).
    pub(crate) rim_color: Color,
    /// Rim hairline width in points; 0.0 means one device pixel (`max(1.0, k_px)`).
    pub(crate) rim_width: f32,
    /// Inset glow / inner shadow `[offset_y_pt, blur_pt, spread_pt]`.
    pub(crate) inner_shadow: [f32; 3],
    /// Inset glow / inner shadow colour.
    pub(crate) inner_shadow_color: Color,
    /// Top sheen `[alpha, stop_fraction]`.
    pub(crate) sheen: [f32; 2],
    /// Lensing scale (1.0 = subtle default, 0.0 = flat).
    pub(crate) refraction: f32,
    /// Explicit bevel width in points; 0.0 derives from element size (`0.17 * min(w,h)/2`, clamped 2..12 pt).
    pub(crate) bevel: f32,
    /// Chromatic dispersion fraction per channel (default 0.03 in the bevel band).
    pub(crate) dispersion: f32,
    /// Specular highlight `[intensity, light_angle_rad]`.
    pub(crate) specular: [f32; 2],
    /// Whether the rim highlight leans toward the pointer cursor.
    pub(crate) cursor_light: bool,
    /// Apple interactive glass: press spring, hover swell springs, gel stretch, drag follow, touch glow.
    pub(crate) interactive: bool,
    /// Solid fallback colour used when glass is off (`GlassPolicy::Solid` or capture unavailable).
    pub(crate) still: Color,
}

#[inline]
fn clean_non_neg(v: f32, default: f32) -> f32 {
    if v.is_finite() { v.max(0.0) } else { default }
}

#[inline]
fn clean_clamped(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        default
    }
}

#[inline]
fn clean_finite(v: f32, default: f32) -> f32 {
    if v.is_finite() { v } else { default }
}

impl Default for LiquidGlass {
    fn default() -> Self {
        Self::regular()
    }
}

impl LiquidGlass {
    /// Apple Liquid Glass `Glass.regular`: adaptive light/dark tone from backdrop, size-derived
    /// frost (`clamp(2 + 0.04 * min(w, h), 3, 10)` pt), saturation 1.35, subtle bevel refraction,
    /// ±3% dispersion, Fresnel + two-lobe specular rim, and adaptive shadow.
    pub fn regular() -> Self {
        Self {
            variant: GlassVariant::Regular,
            frost_sigma: None,
            brightness: 1.0,
            contrast: 1.0,
            saturation: crate::render::liquid_glass::apple::REGULAR_SATURATION,
            tint: Color::TRANSPARENT,
            has_tint: false,
            rim: [0.16, 0.06, 0.07],
            rim_color: Color::WHITE,
            rim_width: 0.0,
            inner_shadow: [0.0; 3],
            inner_shadow_color: Color::TRANSPARENT,
            sheen: [0.0, 0.0],
            refraction: 1.0,
            bevel: 0.0,
            dispersion: crate::render::liquid_glass::apple::DISPERSION,
            specular: [1.0, -std::f32::consts::FRAC_PI_4 * 3.0],
            cursor_light: true,
            interactive: false,
            still: Color {
                r: 0.14,
                g: 0.14,
                b: 0.16,
                a: 0.88,
            },
        }
    }

    /// Apple Liquid Glass `Glass.clear`: minimal frost (0.6 pt), adaptive 35% dark dimming over
    /// bright backdrops, subtle bevel refraction, ±3% dispersion, and crisp Fresnel rim.
    pub fn clear() -> Self {
        Self {
            variant: GlassVariant::Clear,
            frost_sigma: Some(crate::render::liquid_glass::apple::CLEAR_SIGMA_PT),
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.15,
            tint: Color::TRANSPARENT,
            has_tint: false,
            rim: [0.18, 0.06, 0.08],
            rim_color: Color::WHITE,
            rim_width: 0.0,
            inner_shadow: [0.0; 3],
            inner_shadow_color: Color::TRANSPARENT,
            sheen: [0.0, 0.0],
            refraction: 1.0,
            bevel: 0.0,
            dispersion: crate::render::liquid_glass::apple::DISPERSION,
            specular: [1.0, -std::f32::consts::FRAC_PI_4 * 3.0],
            cursor_light: true,
            interactive: false,
            still: Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.35,
            },
        }
    }

    /// Apple Liquid Glass `Glass.identity`: disables the glass effect (paints nothing).
    pub fn identity() -> Self {
        Self {
            variant: GlassVariant::Identity,
            frost_sigma: Some(0.0),
            refraction: 0.0,
            dispersion: 0.0,
            rim: [0.0; 3],
            specular: [0.0, 0.0],
            cursor_light: false,
            ..Self::regular()
        }
    }

    /// Web launcher control glass preset: blur 10 pt, brightness 0.62, saturation 1.8, hairline
    /// rim (.17/.06/.03), top inner glow, and subtle bevel refraction.
    pub fn control() -> Self {
        Self {
            variant: GlassVariant::Custom,
            frost_sigma: Some(10.0),
            brightness: 0.62,
            contrast: 1.0,
            saturation: 1.8,
            tint: Color::TRANSPARENT,
            has_tint: false,
            rim: [0.17, 0.06, 0.03],
            rim_color: Color::WHITE,
            rim_width: 0.0,
            inner_shadow: [16.0, 28.0, -22.0],
            inner_shadow_color: Color::WHITE.alpha(0.1),
            sheen: [0.0, 0.0],
            refraction: 0.65,
            bevel: 0.0,
            dispersion: 0.025,
            specular: [0.65, -std::f32::consts::FRAC_PI_4 * 3.0],
            cursor_light: true,
            interactive: false,
            still: Color::BLACK.alpha(0.65),
        }
    }

    /// Web launcher large panel / popover glass preset: blur 16 pt, brightness 0.66, saturation
    /// 1.8, black 0.36 tint, hairline rim, top inner glow, and subtle bevel refraction.
    pub fn panel() -> Self {
        Self {
            frost_sigma: Some(16.0),
            brightness: 0.66,
            saturation: 1.8,
            tint: Color::BLACK.alpha(0.36),
            has_tint: true,
            still: Color::BLACK.alpha(0.92),
            ..Self::control()
        }
    }

    /// Web launcher pane glass preset: blur 10 pt, brightness 0.62, saturation 1.8.
    pub fn pane() -> Self {
        Self {
            still: Color::BLACK.alpha(0.75),
            ..Self::control()
        }
    }

    /// Web launcher badge glass preset: blur 10 pt, brightness 0.34, contrast 0.9, saturation 1.6.
    pub fn badge() -> Self {
        Self {
            frost_sigma: Some(10.0),
            brightness: 0.34,
            contrast: 0.9,
            saturation: 1.6,
            still: Color::BLACK.alpha(0.8),
            ..Self::control()
        }
    }

    /// Web launcher immersive glass over artwork: blur 18 pt, brightness 0.78, saturation 1.9,
    /// white 0.06 tint.
    pub fn immersive() -> Self {
        Self {
            frost_sigma: Some(18.0),
            brightness: 0.78,
            saturation: 1.9,
            tint: Color::WHITE.alpha(0.06),
            has_tint: true,
            still: Color::BLACK.alpha(0.8),
            ..Self::control()
        }
    }

    /// Maps `color` onto the glass material (adaptive to backdrop luminance on Apple variants;
    /// premultiplied tint overlay on web presets).
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self.has_tint = color.a > 0.0;
        self
    }

    /// Opts into interactive Liquid Glass physics: press spring (0.21 s, damping 0.6), hover swell
    /// springs, gel stretch along drag, drag follow, pointer touch glow, and hover brightening.
    pub fn interactive(mut self) -> Self {
        self.interactive = true;
        self.cursor_light = true;
        self
    }

    /// Overrides the frost standard deviation in points (as in CSS `blur()`). `0.0` disables frost.
    pub fn frost(mut self, sigma_pt: f32) -> Self {
        self.frost_sigma = Some(clean_clamped(sigma_pt, 0.0, 128.0, 0.0));
        self
    }

    /// Alias for [`Self::frost`]: sets the frost blur standard deviation in points.
    pub fn blur(self, sigma_pt: f32) -> Self {
        self.frost(sigma_pt)
    }

    /// Scales rim lensing / refraction (`1.0` = subtle default, `0.0` = flat).
    pub fn refraction(mut self, k: f32) -> Self {
        self.refraction = clean_clamped(k, 0.0, 4.0, 1.0);
        self
    }

    /// Explicit bevel width in points (`0.0` derives from element size, clamped to 2..12 pt).
    pub fn bevel(mut self, points: f32) -> Self {
        self.bevel = clean_clamped(points, 0.0, 64.0, 0.0);
        self
    }

    /// Chromatic dispersion fraction per channel in the bevel band (`0.0` = off, default `0.03`).
    pub fn dispersion(mut self, k: f32) -> Self {
        self.dispersion = clean_clamped(k, 0.0, 0.5, 0.0);
        self
    }

    /// Backdrop brightness multiplier for custom / web presets (`1.0` = unchanged).
    pub fn brightness(mut self, k: f32) -> Self {
        self.brightness = clean_clamped(k, 0.0, 4.0, 1.0);
        self
    }

    /// Backdrop contrast multiplier for custom / web presets (`1.0` = unchanged).
    pub fn contrast(mut self, k: f32) -> Self {
        self.contrast = clean_clamped(k, 0.0, 4.0, 1.0);
        self
    }

    /// Backdrop saturation multiplier (`1.0` = unchanged, `0.0` = greyscale).
    pub fn saturation(mut self, k: f32) -> Self {
        self.saturation = clean_clamped(k, 0.0, 4.0, 1.0);
        self
    }

    /// Hairline rim light alpha by edge (`top`, `side`, `bottom`).
    pub fn rim(mut self, top: f32, side: f32, bottom: f32) -> Self {
        self.rim = [
            clean_clamped(top, 0.0, 1.0, 0.0),
            clean_clamped(side, 0.0, 1.0, 0.0),
            clean_clamped(bottom, 0.0, 1.0, 0.0),
        ];
        self
    }

    /// Rim light colour (default `Color::WHITE`).
    pub fn rim_color(mut self, color: Color) -> Self {
        self.rim_color = color;
        self
    }

    /// Rim hairline width in points (`0.0` = one device pixel).
    pub fn rim_width(mut self, points: f32) -> Self {
        self.rim_width = clean_clamped(points, 0.0, 16.0, 0.0);
        self
    }

    /// Inset glow or inner shadow (`offset_y`, `blur`, `spread` in points, plus `color`).
    pub fn inner_shadow(mut self, y: f32, blur: f32, spread: f32, color: Color) -> Self {
        self.inner_shadow = [
            clean_finite(y, 0.0),
            clean_non_neg(blur, 0.0),
            clean_finite(spread, 0.0),
        ];
        self.inner_shadow_color = color;
        self
    }

    /// Top sheen wash (`alpha` at top edge fading out by `stop` fraction of element height).
    pub fn sheen(mut self, alpha: f32, stop: f32) -> Self {
        self.sheen = [
            clean_clamped(alpha, 0.0, 1.0, 0.0),
            clean_clamped(stop, 0.0, 1.0, 0.0),
        ];
        self
    }

    /// Light-angle specular highlight (`intensity`, `light_angle` in radians).
    pub fn specular(mut self, intensity: f32, light_angle: f32) -> Self {
        self.specular = [
            clean_clamped(intensity, 0.0, 4.0, 0.0),
            clean_finite(light_angle, 0.0),
        ];
        self
    }

    /// Whether the rim specular highlight leans toward the cursor when hovered.
    pub fn cursor_light(mut self, enabled: bool) -> Self {
        self.cursor_light = enabled;
        self
    }

    /// Solid fallback fill colour used when glass is disabled (`GlassPolicy::Solid`, or when
    /// backdrop capture is unavailable).
    pub fn still(mut self, color: Color) -> Self {
        self.still = color;
        self
    }

    /// True for `LiquidGlass::identity()`, which emits no glass record.
    #[inline]
    pub fn is_identity(&self) -> bool {
        self.variant == GlassVariant::Identity
    }

    /// True if interactive glass physics are enabled.
    #[inline]
    pub fn is_interactive(&self) -> bool {
        self.interactive
    }
}

/// Optional Liquid Glass metadata stored on `VisualCold` so non-glass elements pay zero extra size.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GlassNode {
    /// This element's own glass material (`El::liquid_glass`).
    pub(crate) glass: Option<LiquidGlass>,
    /// Container smooth-union merge spacing in points (`Some(spacing)` for `El::glass_container`,
    /// `Some(0.0)` for `El::glass_group`).
    pub(crate) container_spacing: Option<f32>,
    /// Morph & luminance-probe identity (`El::glass_id`).
    pub(crate) glass_id: Option<Rc<str>>,
    /// Seamless SDF union identity within a container (`El::glass_union`).
    pub(crate) glass_union: Option<Rc<str>>,
}

impl El {
    /// Draws this element's background as Liquid Glass (`glass`), clipped to its capsule (for Apple
    /// variants when no `radius` is set) or rounded/continuous corners, clip and fades.
    /// Children, fills, borders and shadows draw on top.
    pub fn liquid_glass(mut self, glass: LiquidGlass) -> Self {
        if glass.is_identity() {
            if let Some(cold) = self.vis.cold.get_mut_if_some() {
                if let Some(gn) = cold.liquid_glass.as_deref_mut() {
                    gn.glass = Some(glass);
                }
            }
            return self;
        }
        let gn = self.vis.cold_mut().liquid_glass.get_or_insert_with(|| {
            Box::new(GlassNode {
                glass: None,
                container_spacing: None,
                glass_id: None,
                glass_union: None,
            })
        });
        gn.glass = Some(glass);
        self
    }

    /// Groups Liquid Glass descendants into a single shared backdrop capture and combined composite
    /// draw (matching SwiftUI `GlassEffectContainer(spacing:)`). Adjacent shapes within `spacing`
    /// points smoothly merge via polynomial smooth-minimum (`smin`); glass members do not sample
    /// each other, and all glass draws behind every member's content.
    pub fn glass_container(mut self, spacing: f32) -> Self {
        let sp = clean_clamped(spacing, 0.0, 256.0, 0.0);
        let gn = self.vis.cold_mut().liquid_glass.get_or_insert_with(|| {
            Box::new(GlassNode {
                glass: None,
                container_spacing: None,
                glass_id: None,
                glass_union: None,
            })
        });
        gn.container_spacing = Some(sp);
        self
    }

    /// Declares that all Liquid Glass descendants in this subtree share one backdrop capture
    /// without SDF shape merging (equivalent to `glass_container(0.0)`).
    pub fn glass_group(self) -> Self {
        self.glass_container(0.0)
    }

    /// Assigns a Liquid Glass identity (`glassEffectID`) for shared-element morph flights across
    /// containers (`El::matched`) and asynchronous backdrop luminance reads (`Cx::glass_is_dark`).
    pub fn glass_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        let match_id = format!("__rune_glass:{id}");
        let gn = self.vis.cold_mut().liquid_glass.get_or_insert_with(|| {
            Box::new(GlassNode {
                glass: None,
                container_spacing: None,
                glass_id: None,
                glass_union: None,
            })
        });
        gn.glass_id = Some(Rc::from(id));
        self.matched(match_id)
    }

    /// Combines Liquid Glass elements sharing `id` inside the same container into one unified shape
    /// (`glassEffectUnion`).
    pub fn glass_union(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        let gn = self.vis.cold_mut().liquid_glass.get_or_insert_with(|| {
            Box::new(GlassNode {
                glass: None,
                container_spacing: None,
                glass_id: None,
                glass_union: None,
            })
        });
        gn.glass_union = Some(Rc::from(id));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_clamp_non_finite_and_out_of_range_values() {
        let g = LiquidGlass::regular()
            .frost(f32::NAN)
            .refraction(f32::INFINITY)
            .dispersion(-5.0)
            .brightness(f32::NEG_INFINITY)
            .contrast(99.0)
            .saturation(f32::NAN)
            .rim(f32::NAN, -1.0, 2.5)
            .rim_width(f32::INFINITY)
            .bevel(-3.0)
            .inner_shadow(f32::NAN, -4.0, f32::INFINITY, Color::WHITE)
            .sheen(2.0, -0.5)
            .specular(f32::NAN, f32::INFINITY);

        assert_eq!(g.frost_sigma, Some(0.0));
        assert_eq!(g.refraction, 1.0);
        assert_eq!(g.dispersion, 0.0);
        assert_eq!(g.brightness, 1.0);
        assert_eq!(g.contrast, 4.0);
        assert_eq!(g.saturation, 1.0);
        assert_eq!(g.rim, [0.0, 0.0, 1.0]);
        assert_eq!(g.rim_width, 0.0);
        assert_eq!(g.bevel, 0.0);
        assert_eq!(g.inner_shadow, [0.0, 0.0, 0.0]);
        assert_eq!(g.sheen, [1.0, 0.0]);
        assert_eq!(g.specular, [0.0, 0.0]);
    }

    #[test]
    fn presets_match_expected_web_and_apple_tokens() {
        assert!(LiquidGlass::identity().is_identity());
        assert!(!LiquidGlass::regular().is_identity());
        assert_eq!(LiquidGlass::control().frost_sigma, Some(10.0));
        assert_eq!(LiquidGlass::panel().frost_sigma, Some(16.0));
        assert_eq!(LiquidGlass::pane().frost_sigma, Some(10.0));
        assert_eq!(LiquidGlass::badge().frost_sigma, Some(10.0));
        assert_eq!(LiquidGlass::immersive().frost_sigma, Some(18.0));
        assert_eq!(
            LiquidGlass::clear().frost_sigma,
            Some(crate::render::liquid_glass::apple::CLEAR_SIGMA_PT)
        );
        assert_eq!(LiquidGlass::regular().frost_sigma, None);
        assert!(LiquidGlass::regular().interactive().is_interactive());
    }
}
