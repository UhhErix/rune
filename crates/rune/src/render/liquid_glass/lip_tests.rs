//! GPU pixel tests for the crystal lip: body tint and dimming thin out across the bevel.

use super::tests::{luma_u8, px_at, render_runtime};
use crate::render::{Gpu, OFFSCREEN_FORMAT, Renderer};
use crate::{Color, Cx, Env, GlassPolicy, LiquidGlass, Runtime, el};

fn rim_and_center(glass: LiquidGlass, policy: GlassPolicy) -> (f32, f32) {
    let gpu = Gpu::headless();
    let (w, h) = (200u32, 160u32);
    let mut rt = Runtime::new(
        glass,
        |&g: &LiquidGlass, _: &mut Cx<LiquidGlass>| {
            el().size(200.0, 160.0)
                .fill(Color::WHITE)
                .child(el().absolute(20.0, 20.0).size(160.0, 120.0).radius(24.0).liquid_glass(g))
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(policy);
    rt.frame(0.0);
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let px = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
    (luma_u8(px_at(&px, w, 100, 24)), luma_u8(px_at(&px, w, 100, 80)))
}

fn dark_glass() -> LiquidGlass {
    LiquidGlass::clear().frost(8.0).rim(0.0, 0.0, 0.0).specular(0.0, 0.0).tint(Color::BLACK.alpha(0.8))
}

#[test]
fn dark_tint_thins_at_the_rim_and_holds_in_the_interior() {
    let (rim, center) = rim_and_center(dark_glass(), GlassPolicy::Glass);
    assert!(center < 0.22, "the flat interior keeps its full tint over white, got luma={center:.3}");
    assert!(rim > center + 0.15, "the bevel lip lets the backdrop through (rim={rim:.3}, center={center:.3})");
}

#[test]
fn blur_only_and_flat_glass_keep_one_tone_to_the_edge() {
    let (rim, center) = rim_and_center(dark_glass(), GlassPolicy::BlurOnly);
    assert!((rim - center).abs() < 0.03, "BlurOnly has no lens, so no lip (rim={rim:.3}, center={center:.3})");
    let (rim, center) = rim_and_center(dark_glass().refraction(0.0), GlassPolicy::Glass);
    assert!((rim - center).abs() < 0.03, "refraction 0 is flat glass (rim={rim:.3}, center={center:.3})");
}