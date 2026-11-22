use super::gpu_tests::{Gpu, OFFSCREEN_FORMAT};
use super::place::{Align, SurfacePlace, image_rect};
use super::*;
use rune_image::pixels::fit;

const TOP_LEFT: SurfacePlace = SurfacePlace::align(Align::Start, Align::Start);
const TOP_LEFT_1X: SurfacePlace = SurfacePlace::align(Align::Start, Align::Start).unscaled(true);
fn near(got: [f32; 4], want: [f32; 4], what: &str) {
    assert!(got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-3), "{what}: {got:?} vs {want:?}");
}

const STALE: [u32; 2] = [100, 50];

#[test]
fn centred_default_is_the_old_placement() {
    let c = SurfacePlace::default();
    assert_eq!(c, SurfacePlace::CENTRED);
    assert_eq!(image_rect([0.0, 0.0, 120.0, 50.0], STALE, SurfaceFit::Fit, c), [10.0, 0.0, 100.0, 50.0]);
    near(image_rect([0.0, 0.0, 120.0, 50.0], STALE, SurfaceFit::Fill, c), [0.0, -5.0, 120.0, 60.0], "fill wide");
    assert_eq!(image_rect([0.0, 0.0, 80.0, 60.0], STALE, SurfaceFit::Fit, c), [0.0, 10.0, 80.0, 40.0]);
    near(image_rect([0.0, 0.0, 80.0, 60.0], STALE, SurfaceFit::Fill, c), [-20.0, 0.0, 120.0, 60.0], "fill tall");
}

#[test]
fn top_left_anchor_pins_the_corner_under_fit_and_fill() {
    let at = |node, f| image_rect(node, STALE, f, TOP_LEFT);
    assert_eq!(at([0.0, 0.0, 120.0, 50.0], SurfaceFit::Fit), [0.0, 0.0, 100.0, 50.0], "gap on the right");
    near(at([0.0, 0.0, 120.0, 50.0], SurfaceFit::Fill), [0.0, 0.0, 120.0, 60.0], "crops the bottom only");
    assert_eq!(at([0.0, 0.0, 80.0, 60.0], SurfaceFit::Fit), [0.0, 0.0, 80.0, 40.0], "gap below");
    near(at([0.0, 0.0, 80.0, 60.0], SurfaceFit::Fill), [0.0, 0.0, 120.0, 60.0], "crops the right only");
}

#[test]
fn unscaled_keeps_texels_and_loses_only_the_far_edges() {
    let at = |node| image_rect(node, STALE, SurfaceFit::Fill, TOP_LEFT_1X);
    assert_eq!(at([0.0, 0.0, 120.0, 50.0]), [0.0, 0.0, 100.0, 50.0], "grow: gap at the right");
    assert_eq!(at([0.0, 0.0, 80.0, 60.0]), [0.0, 0.0, 100.0, 50.0], "shrink: node clips the right; gap below");
    assert_eq!(image_rect([7.0, 9.0, 120.0, 50.0], STALE, SurfaceFit::Fit, TOP_LEFT_1X), [7.0, 9.0, 100.0, 50.0], "fit is ignored");
    let end = SurfacePlace::align(Align::End, Align::End).unscaled(true);
    assert_eq!(image_rect([0.0, 0.0, 80.0, 60.0], STALE, SurfaceFit::Fit, end), [-20.0, 10.0, 100.0, 50.0]);
}

#[test]
fn fractional_anchor_matches_object_position_for_surfaces_and_pixels() {
    use SurfaceFit::{Fill, Fit};
    let pos = SurfacePlace::fraction(0.5, 0.22);
    // 200x200 source filling a 200x100 node overflows vertically by 100px: 22% cropped above, 78% below.
    near(image_rect([0.0, 0.0, 200.0, 100.0], [200, 200], Fill, pos), [0.0, -22.0, 200.0, 200.0], "fill 50% 22%");
    let (rect, crop) = fit(Some(Fill), pos, [0.0, 0.0, 200.0, 100.0], (200, 200));
    assert_eq!(rect, [0.0, 0.0, 200.0, 100.0]);
    near(crop, [0.0, 0.11, 1.0, 0.61], "pixels fill 50% 22%");

    // 200x200 source filling a 100x200 node with x=0.25 overflows horizontally by 100px: 25px cropped left.
    let left_quarter = SurfacePlace::fraction(0.25, 0.5);
    near(image_rect([0.0, 0.0, 100.0, 200.0], [200, 200], Fill, left_quarter), [-25.0, 0.0, 200.0, 200.0], "fill 25% 50%");
    let (_, crop) = fit(Some(Fill), left_quarter, [0.0, 0.0, 100.0, 200.0], (200, 200));
    near(crop, [0.125, 0.0, 0.625, 1.0], "pixels fill 25% 50%");

    // Fit letterboxes by the same fraction.
    near(image_rect([0.0, 0.0, 200.0, 100.0], [100, 100], Fit, left_quarter), [25.0, 0.0, 100.0, 100.0], "fit 25% 50%");

    // Clamping and non-finite fallback keep UVs within [0, 1].
    assert_eq!(SurfacePlace::fraction(-1.0, 2.0).share(), [0.0, 1.0]);
    assert_eq!(SurfacePlace::fraction(f32::NAN, f32::INFINITY).share(), [0.5, 0.5]);
    assert_eq!(SurfacePlace::fraction(0.0, 1.0), SurfacePlace::align(Align::Start, Align::End));
}

#[test]
fn pixels_follow_the_same_anchor() {
    use SurfaceFit::{Fill, Fit};
    let size = (100, 50);
    let wide = [0.0, 0.0, 120.0, 50.0];
    let tall = [0.0, 0.0, 80.0, 60.0];
    assert_eq!(fit(Some(Fit), TOP_LEFT, wide, size), ([0.0, 0.0, 100.0, 50.0], [0.0, 0.0, 1.0, 1.0]));
    assert_eq!(fit(Some(Fit), TOP_LEFT, tall, size), ([0.0, 0.0, 80.0, 40.0], [0.0, 0.0, 1.0, 1.0]));
    let (rect, crop) = fit(Some(Fill), TOP_LEFT, tall, size);
    assert_eq!((rect, crop[0], crop[1]), (tall, 0.0, 0.0));
    assert!((crop[3] - 1.0).abs() < 1e-6);
    assert!((crop[2] - 80.0 / 120.0).abs() < 1e-6, "keeps the left two thirds: {crop:?}");
    let (rect, crop) = fit(Some(Fill), TOP_LEFT, wide, size);
    assert_eq!((rect, crop[0], crop[1]), (wide, 0.0, 0.0));
    assert!((crop[2] - 1.0).abs() < 1e-6);
    assert!((crop[3] - 50.0 / 60.0).abs() < 1e-6, "drops the bottom: {crop:?}");
    assert_eq!(fit(Some(Fill), TOP_LEFT_1X, wide, size), ([0.0, 0.0, 100.0, 50.0], [0.0, 0.0, 1.0, 1.0]));
    assert_eq!(fit(Some(Fit), TOP_LEFT_1X, tall, size), ([0.0, 0.0, 80.0, 50.0], [0.0, 0.0, 0.8, 1.0]));
    assert_eq!(fit(None, TOP_LEFT_1X, tall, size), (tall, [0.0, 0.0, 1.0, 1.0]), "a stretched image ignores placement");
}

#[test]
fn a_stale_frame_draws_pinned_top_left_on_the_gpu() {
    let gpu = Gpu::headless_wgpu();
    let mut renderer = SurfaceRenderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let (frame, src) = super::gpu_tests::bgra_ramp(8, 4);
    let surface = NativeSurface::new();
    surface.push_bytes(frame, 1).unwrap();
    let mut d = super::gpu_tests::draw(&surface, [0.0, 0.0, 10.0, 6.0]);
    d.surface = surface.placed(TOP_LEFT_1X);
    d.fit = SurfaceFit::Fill;
    let out = super::gpu_tests::render(&gpu, &mut renderer, &[d.clone()], (10, 6));
    for y in 0..4 {
        for x in 0..8 {
            assert_eq!(super::gpu_tests::pixel(&out, 10, x, y), src[(y * 8 + x) as usize], "texel ({x},{y})");
        }
    }
    for (x, y) in [(8, 0), (9, 3), (0, 4), (9, 5)] {
        assert_eq!(super::gpu_tests::pixel(&out, 10, x, y)[3], 0, "empty at ({x},{y})");
    }
    d.rect = [0.0, 0.0, 6.0, 3.0];
    let out = super::gpu_tests::render(&gpu, &mut renderer, &[d], (6, 3));
    for y in 0..3 {
        for x in 0..6 {
            assert_eq!(super::gpu_tests::pixel(&out, 6, x, y), src[(y * 8 + x) as usize], "cropped texel ({x},{y})");
        }
    }
}
