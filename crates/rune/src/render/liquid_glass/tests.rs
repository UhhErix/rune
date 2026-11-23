//! Unit and GPU pixel tests for Liquid Glass (`El::liquid_glass`).

use crate::render::liquid_glass::pyramid::pyramid_bytes;
use crate::render::{
    Gpu, KIND_BACKDROP_BLUR, KIND_LAYER, KIND_LIQUID_GLASS, OFFSCREEN_FORMAT, Offscreen, Renderer,
    cpu_boot,
};
use crate::{Color, Cx, Env, GlassPolicy, LiquidGlass, Runtime, el, row};

fn render_runtime<S: 'static>(
    rt: &mut Runtime<S>,
    renderer: &mut Renderer,
    gpu: &Gpu,
    w: u32,
    h: u32,
) -> Vec<u8> {
    renderer.set_srgb_blend(rt.srgb_blend());
    renderer.set_fade_stops(rt.fade_stops());
    renderer.set_max_backdrop_blurs(rt.max_backdrop_blurs());
    renderer.set_liquid_glass(rt.liquid_glass_frame());
    renderer.prepare(
        &gpu.device,
        &gpu.queue,
        &mut rt.text.atlas,
        &rt.instances,
        (w, h),
    );
    let format = renderer.format();
    let target = Offscreen::with_format(&gpu.device, w, h, format);
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut enc, &target.view, rt.clear_color());
    target.copy(&mut enc);
    gpu.queue.submit([enc.finish()]);
    target.read(&gpu.device)
}

fn px_at(pixels: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn luma_u8(p: [u8; 4]) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
}

#[test]
fn no_cap_on_70_liquid_glass_elements_and_backdrop_blur_cap_preserved() {
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            let mut root = el().size(800.0, 200.0).glass_group();
            for i in 0..70 {
                let col = (i % 10) as f32;
                let row = (i / 10) as f32;
                root = root.child(
                    el().key(format!("g-{i}"))
                        .absolute(col * 24.0, row * 24.0)
                        .size(16.0, 16.0)
                        .liquid_glass(LiquidGlass::regular()),
                );
            }
            root
        },
        (800.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);
    assert_eq!(rt.liquid_glass_frame().shapes.len(), 70);
    // 70 shapes split cleanly into 3 chunks of <=32 shapes (32 + 32 + 6).
    let lg_recs = rt
        .instances
        .iter()
        .filter(|i| i.params[1] == KIND_LIQUID_GLASS)
        .count();
    assert_eq!(lg_recs, 3);

    // Verify backdrop_blur still respects default 4 and configurable 16 cap.
    let mut rt_blur = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            let mut root = el().size(400.0, 100.0);
            for i in 0..10 {
                root = root.child(
                    el().key(format!("b-{i}"))
                        .size(20.0, 20.0)
                        .backdrop_blur(8.0),
                );
            }
            root
        },
        (400.0, 100.0),
        Env::new(true, 1.0),
    );
    rt_blur.frame(0.0);
    assert_eq!(
        rt_blur
            .instances
            .iter()
            .filter(|i| i.params[1] == KIND_BACKDROP_BLUR)
            .count(),
        crate::MAX_BACKDROP_BLURS
    );
    rt_blur.set_max_backdrop_blurs(16);
    rt_blur.frame(0.0);
    assert_eq!(
        rt_blur
            .instances
            .iter()
            .filter(|i| i.params[1] == KIND_BACKDROP_BLUR)
            .count(),
        10
    );
}

#[test]
fn container_of_three_is_one_record_and_content_paints_after_and_identity_emits_nothing() {
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            row()
                .size(400.0, 100.0)
                .gap(8.0)
                .glass_container(10.0)
                .child(
                    el().key("c1")
                        .size(60.0, 44.0)
                        .liquid_glass(LiquidGlass::regular())
                        .child(el().size(10.0, 10.0).fill(Color::WHITE)),
                )
                .child(
                    el().key("c2")
                        .size(60.0, 44.0)
                        .liquid_glass(LiquidGlass::regular())
                        .child(el().size(10.0, 10.0).fill(Color::WHITE)),
                )
                .child(
                    el().key("c3")
                        .size(60.0, 44.0)
                        .liquid_glass(LiquidGlass::regular())
                        .child(el().size(10.0, 10.0).fill(Color::WHITE)),
                )
                .child(
                    el().key("id")
                        .size(60.0, 44.0)
                        .liquid_glass(LiquidGlass::identity()),
                )
        },
        (400.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);
    let lg_pos: Vec<usize> = rt
        .instances
        .iter()
        .enumerate()
        .filter(|(_, i)| i.params[1] == KIND_LIQUID_GLASS)
        .map(|(idx, _)| idx)
        .collect();
    assert_eq!(
        lg_pos.len(),
        1,
        "container of 3 should emit 1 KIND_LIQUID_GLASS record"
    );
    assert_eq!(
        rt.liquid_glass_frame().shapes.len(),
        3,
        "identity() must emit no shape"
    );
    assert_eq!(rt.instances[lg_pos[0]].uv[0], 3.0);

    // All 3 white content rects must appear strictly AFTER the container's glass record.
    let white_positions: Vec<usize> = rt
        .instances
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            i.params[1] == crate::render::KIND_RECT && i.fill[0] > 0.9 && i.rect[2] == 10.0
        })
        .map(|(idx, _)| idx)
        .collect();
    assert_eq!(white_positions.len(), 3);
    for wp in white_positions {
        assert!(
            wp > lg_pos[0],
            "member content at {wp} must paint after glass record at {}",
            lg_pos[0]
        );
    }
}

#[test]
fn frame_1_equals_frame_2_differs_from_still_and_cpu_boot_refuses() {
    let gpu = Gpu::headless();
    let (w, h) = (320u32, 200u32);
    let make_rt = |policy: GlassPolicy| {
        let mut rt = Runtime::new(
            (),
            |_: &(), _: &mut Cx<()>| {
                el().size(320.0, 200.0)
                    .fill(Color {
                        r: 0.05,
                        g: 0.15,
                        b: 0.45,
                        a: 1.0,
                    })
                    .child(el().size(160.0, 200.0).fill(Color {
                        r: 0.9,
                        g: 0.2,
                        b: 0.1,
                        a: 1.0,
                    }))
                    .child(
                        el().absolute(60.0, 50.0)
                            .size(200.0, 100.0)
                            .radius(24.0)
                            .liquid_glass(LiquidGlass::regular().still(Color {
                                r: 0.1,
                                g: 0.1,
                                b: 0.1,
                                a: 1.0,
                            })),
                    )
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(policy);
        rt.frame(0.0);
        rt
    };

    let mut rt_glass = make_rt(GlassPolicy::Glass);
    let scene = cpu_boot::Scene::new(
        &rt_glass.instances,
        &rt_glass.text.atlas,
        &rt_glass.images,
        (w, h),
        rt_glass.clear_color(),
    );
    assert_eq!(scene.needs_gpu(), Some(cpu_boot::NeedsGpu::LiquidGlass));

    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let f1 = render_runtime(&mut rt_glass, &mut renderer, &gpu, w, h);
    let f2 = render_runtime(&mut rt_glass, &mut renderer, &gpu, w, h);
    assert_eq!(
        f1, f2,
        "GPU frame 1 must equal frame 2 byte-for-byte on a fresh renderer"
    );

    let mut rt_still = make_rt(GlassPolicy::Solid);
    let still_px = render_runtime(&mut rt_still, &mut renderer, &gpu, w, h);
    let g_mid = px_at(&f1, w, 160, 100);
    let s_mid = px_at(&still_px, w, 160, 100);
    assert_ne!(
        g_mid, s_mid,
        "live glass must differ from still fallback: glass={g_mid:?} still={s_mid:?}"
    );
}

#[test]
fn refraction_displaces_bevel_inward_and_center_matches_flat_frost() {
    let gpu = Gpu::headless();
    let (w, h) = (320u32, 240u32);
    let render_with_refract = |refract_scale: f32| {
        let mut rt = Runtime::new(
            refract_scale,
            |&scale: &f32, _: &mut Cx<f32>| {
                let mut root = el().size(320.0, 240.0).fill(Color {
                    r: 0.05,
                    g: 0.08,
                    b: 0.25,
                    a: 1.0,
                });
                for k in 0..16 {
                    if k % 2 == 0 {
                        root =
                            root.child(el().absolute((k * 20) as f32, 0.0).size(10.0, 240.0).fill(
                                Color {
                                    r: 0.95,
                                    g: 0.85,
                                    b: 0.15,
                                    a: 1.0,
                                },
                            ));
                    }
                }
                root.child(
                    el().absolute(40.0, 40.0)
                        .size(240.0, 160.0)
                        .radius(32.0)
                        .liquid_glass(
                            LiquidGlass::clear()
                                .frost(0.6)
                                .refraction(scale)
                                .dispersion(0.03),
                        ),
                )
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        render_runtime(&mut rt, &mut renderer, &gpu, w, h)
    };

    let bent = render_with_refract(1.2);
    let flat = render_with_refract(0.0);

    // Flat centre (160, 120): ratio ≈ 0 -> refraction offset is 0, so bent == flat.
    let c_bent = px_at(&bent, w, 160, 120);
    let c_flat = px_at(&flat, w, 160, 120);
    for ch in 0..3 {
        assert!(
            (c_bent[ch] as i32 - c_flat[ch] as i32).abs() <= 2,
            "center should match flat frost: bent={c_bent:?} flat={c_flat:?}"
        );
    }

    // Near the left bevel (x = 42..54, y = 120): refracted pixels must measurably shift relative to flat.
    let mut max_bevel_diff = 0i32;
    for x in 42..54 {
        let b = px_at(&bent, w, x, 120);
        let f = px_at(&flat, w, x, 120);
        for ch in 0..3 {
            max_bevel_diff = max_bevel_diff.max((b[ch] as i32 - f[ch] as i32).abs());
        }
    }
    assert!(
        max_bevel_diff >= 20,
        "expected visible bevel refraction shift >= 20, got {max_bevel_diff}"
    );
}

#[test]
fn rim_is_present_and_leans_toward_cursor() {
    let gpu = Gpu::headless();
    let (w, h) = (320u32, 160u32);
    let render_with_pointer = |ptr: Option<(f32, f32)>| {
        let mut rt = Runtime::new(
            (),
            |_: &(), _: &mut Cx<()>| {
                el().size(320.0, 160.0)
                    .fill(Color {
                        r: 0.04,
                        g: 0.05,
                        b: 0.08,
                        a: 1.0,
                    })
                    .child(
                        el().absolute(60.0, 40.0)
                            .size(200.0, 80.0)
                            .liquid_glass(LiquidGlass::regular().cursor_light(true)),
                    )
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        if let Some((x, y)) = ptr {
            rt.pointer_move(x, y);
            rt.frame(1.0 / 60.0);
        }
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        render_runtime(&mut rt, &mut renderer, &gpu, w, h)
    };

    let rest = render_with_pointer(None);
    let interior_l = luma_u8(px_at(&rest, w, 160, 80));
    let top_rim_l = (40..=42)
        .map(|y| luma_u8(px_at(&rest, w, 160, y)))
        .fold(0.0f32, f32::max);
    assert!(
        top_rim_l > interior_l + 0.04,
        "rim must brighten edge over interior: top_rim={top_rim_l:.3} interior={interior_l:.3}"
    );

    let ptr_left = render_with_pointer(Some((62.0, 80.0)));
    let ptr_right = render_with_pointer(Some((258.0, 80.0)));
    let right_edge_when_right = (257..=259)
        .map(|x| luma_u8(px_at(&ptr_right, w, x, 80)))
        .fold(0.0f32, f32::max);
    let right_edge_when_left = (257..=259)
        .map(|x| luma_u8(px_at(&ptr_left, w, x, 80)))
        .fold(0.0f32, f32::max);
    assert!(
        right_edge_when_right > right_edge_when_left + 0.015,
        "cursor on right must brighten right rim: right={right_edge_when_right:.3} vs left={right_edge_when_left:.3}"
    );
}

#[test]
fn no_stair_step_at_1080p_on_gradient() {
    let gpu = Gpu::headless();
    let (w, h) = (1920u32, 1080u32);
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(1920.0, 1080.0)
                .gradient(
                    Color {
                        r: 0.02,
                        g: 0.04,
                        b: 0.12,
                        a: 1.0,
                    },
                    Color {
                        r: 0.09,
                        g: 0.20,
                        b: 0.38,
                        a: 1.0,
                    },
                )
                .child(
                    el().absolute(560.0, 340.0)
                        .size(800.0, 400.0)
                        .radius(200.0)
                        .liquid_glass(LiquidGlass::regular().refraction(1.0)),
                )
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);

    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let pixels = render_runtime(&mut rt, &mut renderer, &gpu, w, h);

    // Measure exact sub-pixel edge position across 170 rows (y=365..=535, 61 deg down to 3 deg)
    // by linearly interpolating the 45% rising-edge crossing against a 3-row rim envelope so
    // neither rim saturation nor 100-px interior rounding biases the crossing. Analytic AA tracks
    // the true circle within < 0.25 px/row; stair-stepping snaps to integers (~1.0 px jumps).
    let rows: Vec<u32> = (365u32..=535).collect();
    let row_peaks: Vec<(f32, f32, f32, u32)> = rows
        .iter()
        .map(|&y| {
            let bg_l = luma_u8(px_at(&pixels, w, 500, y));
            let interior_l = luma_u8(px_at(&pixels, w, 760, y));
            let mut peak_l = bg_l;
            let mut peak_x = 550u32;
            for x in 550u32..=700 {
                let l = luma_u8(px_at(&pixels, w, x, y));
                if l > peak_l {
                    peak_l = l;
                    peak_x = x;
                }
            }
            if y <= 455 {
                assert!(
                    peak_l > interior_l + 0.02,
                    "rim missing on upper arc row {y}: int={interior_l:.3} peak={peak_l:.3}"
                );
            }
            (bg_l, interior_l, peak_l, peak_x)
        })
        .collect();

    let mut errs = Vec::with_capacity(rows.len());
    let mut peak_ls = Vec::new();
    for (idx, &y) in rows.iter().enumerate() {
        let (bg_l, _, peak_l, peak_x) = row_peaks[idx];
        let mut peak_ref = peak_l;
        if idx > 0 {
            peak_ref = peak_ref.max(row_peaks[idx - 1].2);
        }
        if idx + 1 < row_peaks.len() {
            peak_ref = peak_ref.max(row_peaks[idx + 1].2);
        }
        let target = bg_l + 0.45 * (peak_ref - bg_l);
        let mut crossing = None;
        for x in 550u32..peak_x {
            let l0 = luma_u8(px_at(&pixels, w, x, y));
            let l1 = luma_u8(px_at(&pixels, w, x + 1, y));
            if l0 <= target && l1 >= target && (l1 - l0) > 1e-5 {
                crossing = Some(x as f32 + 0.5 + (target - l0) / (l1 - l0));
                break;
            }
        }
        let measured_x =
            crossing.unwrap_or_else(|| panic!("capsule edge crossing not found on row {y}"));
        let dy = (540.0 - (y as f32 + 0.5)) / 200.0;
        let expected_x = 760.0 - (1.0 - dy * dy).sqrt() * 200.0;
        let err = measured_x - expected_x;
        assert!(
            err.abs() < 0.75,
            "capsule edge drifted off analytical arc on row {y}: measured={measured_x:.3} expected={expected_x:.3}"
        );
        if y <= 455 {
            peak_ls.push((y, peak_l));
        }
        errs.push((y, err));
    }

    for pair in errs.windows(2) {
        let jump = (pair[1].1 - pair[0].1).abs();
        assert!(
            jump < 0.35,
            "stair-step detected on 1080p capsule edge at row {}: err {:.3} -> {:.3} (jump={:.3})",
            pair[1].0,
            pair[0].1,
            pair[1].1,
            jump
        );
    }
    for pair in peak_ls.windows(2) {
        assert!(
            (pair[0].1 - pair[1].1).abs() < 0.08,
            "beading detected on 1080p capsule rim peak at row {}: {:.3} -> {:.3}",
            pair[1].0,
            pair[0].1,
            pair[1].1
        );
    }

    // Verify both horizontal (y=540) and vertical (x=960) bevel refraction zones are smooth over
    // the 1080p gradient with zero quantization jumps or stair-step bands.
    let mut prev = luma_u8(px_at(&pixels, w, 575, 540));
    for x in 576u32..=720 {
        let cur = luma_u8(px_at(&pixels, w, x, 540));
        assert!(
            (cur - prev).abs() < 0.035,
            "interior horizontal refraction stair-step at x={x}: {prev:.4} -> {cur:.4}"
        );
        prev = cur;
    }
    let mut prev_y = luma_u8(px_at(&pixels, w, 960, 348));
    for y in 349u32..=460 {
        let cur = luma_u8(px_at(&pixels, w, 960, y));
        assert!(
            (cur - prev_y).abs() < 0.035,
            "interior vertical bevel refraction stair-step at y={y}: {prev_y:.4} -> {cur:.4}"
        );
        prev_y = cur;
    }
}

#[test]
fn frost_line_spread_within_10_percent_and_shimmer_free() {
    let gpu = Gpu::headless();
    let (w, h) = (320u32, 160u32);
    for sigma in [3.0f32, 6.0, 10.0, 16.0] {
        let mut rt = Runtime::new(
            sigma,
            |&sig: &f32, _: &mut Cx<f32>| {
                el().size(320.0, 160.0)
                    .fill(Color::BLACK)
                    .child(
                        el().absolute(160.0, 0.0)
                            .size(160.0, 160.0)
                            .fill(Color::WHITE),
                    )
                    .child(
                        el().absolute(20.0, 20.0).size(280.0, 120.0).liquid_glass(
                            LiquidGlass::control()
                                .frost(sig)
                                .brightness(1.0)
                                .contrast(1.0)
                                .saturation(1.0)
                                .tint(Color::TRANSPARENT)
                                .rim(0.0, 0.0, 0.0)
                                .specular(0.0, 0.0)
                                .inner_shadow(0.0, 0.0, 0.0, Color::TRANSPARENT)
                                .refraction(0.0)
                                .dispersion(0.0),
                        ),
                    )
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let px = render_runtime(&mut rt, &mut renderer, &gpu, w, h);

        let find_x = |target: f32| -> f32 {
            for x in 30..(w - 31) {
                let v0 = px_at(&px, w, x, 80)[0] as f32 / 255.0;
                let v1 = px_at(&px, w, x + 1, 80)[0] as f32 / 255.0;
                if v0 <= target && v1 >= target && (v1 - v0) > 1e-5 {
                    return x as f32 + (target - v0) / (v1 - v0);
                }
            }
            panic!("crossing {target} not found for sigma={sigma}");
        };
        let x_lo = find_x(0.158655);
        let x_hi = find_x(0.841345);
        let measured_sigma = (x_hi - x_lo) * 0.5;
        let rel_err = (measured_sigma - sigma).abs() / sigma;
        assert!(
            rel_err <= 0.10,
            "frost sigma={sigma} measured={measured_sigma:.2} (rel_err={:.1}%)",
            rel_err * 100.0
        );
    }

    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let mut mids = Vec::new();
    for shift in 0..8 {
        let mut rt = Runtime::new(
            shift,
            |&sh: &i32, _: &mut Cx<i32>| {
                el().size(320.0, 160.0)
                    .fill(Color::BLACK)
                    .child(
                        el().absolute((160 + sh) as f32, 0.0)
                            .size(160.0, 160.0)
                            .fill(Color::WHITE),
                    )
                    .child(
                        el().absolute(40.0, 20.0).size(240.0, 120.0).liquid_glass(
                            LiquidGlass::control()
                                .frost(10.0)
                                .brightness(1.0)
                                .contrast(1.0)
                                .saturation(1.0)
                                .tint(Color::TRANSPARENT)
                                .rim(0.0, 0.0, 0.0)
                                .specular(0.0, 0.0)
                                .inner_shadow(0.0, 0.0, 0.0, Color::TRANSPARENT)
                                .refraction(0.0),
                        ),
                    )
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        let px = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
        mids.push(px_at(&px, w, 160, 80)[0] as f32 / 255.0);
    }
    for pair in mids.windows(2) {
        assert!(
            pair[0] >= pair[1] - 0.01,
            "scrolling edge crawled upward: {mids:?}"
        );
    }
}

#[test]
fn regular_adapts_to_white_vs_black_and_clear_dims_over_white() {
    let gpu = Gpu::headless();
    let (w, h) = (160u32, 160u32);
    let sample = |bg: Color, glass: LiquidGlass| {
        let mut rt = Runtime::new(
            (bg, glass),
            |&(bg, g): &(Color, LiquidGlass), _: &mut Cx<(Color, LiquidGlass)>| {
                el().size(160.0, 160.0)
                    .fill(bg)
                    .child(el().absolute(30.0, 40.0).size(100.0, 80.0).liquid_glass(g))
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let px = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
        luma_u8(px_at(&px, w, 80, 80))
    };

    let reg_white = sample(Color::WHITE, LiquidGlass::regular());
    let reg_black = sample(Color::BLACK, LiquidGlass::regular());
    assert!(
        reg_white > reg_black + 0.5,
        "regular over white ({reg_white:.3}) must be much lighter than over black ({reg_black:.3})"
    );

    let clear_white = sample(Color::WHITE, LiquidGlass::clear());
    assert!(
        clear_white < 0.78 && clear_white > 0.45,
        "clear over white should apply ~35% dimming, got luma={clear_white:.3}"
    );
}

#[test]
fn container_smin_merges_when_gap_within_spacing_and_splits_when_outside() {
    let gpu = Gpu::headless();
    let (w, h) = (240u32, 120u32);
    let render_gap = |spacing: f32| {
        let mut rt = Runtime::new(
            spacing,
            |&sp: &f32, _: &mut Cx<f32>| {
                el().size(240.0, 120.0)
                    .fill(Color {
                        r: 0.9,
                        g: 0.1,
                        b: 0.1,
                        a: 1.0,
                    })
                    .child(
                        el().size(240.0, 120.0)
                            .glass_container(sp)
                            .child(el().absolute(40.0, 38.0).size(76.0, 44.0).liquid_glass(
                                LiquidGlass::regular().tint(Color {
                                    r: 0.0,
                                    g: 0.6,
                                    b: 1.0,
                                    a: 0.7,
                                }),
                            ))
                            .child(el().absolute(124.0, 38.0).size(76.0, 44.0).liquid_glass(
                                LiquidGlass::regular().tint(Color {
                                    r: 0.0,
                                    g: 0.6,
                                    b: 1.0,
                                    a: 0.7,
                                }),
                            )),
                    )
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let px = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
        px_at(&px, w, 120, 60)
    };

    let merged = render_gap(10.0);
    let separated = render_gap(4.0);
    assert!(
        merged[2] > separated[2] + 60,
        "spacing=10 should fill 8pt gap with merged glass (merged={merged:?}), spacing=4 should not (separated={separated:?})"
    );
}

#[test]
fn hoisting_out_of_fade_cache_and_blur_layers_samples_real_backdrop() {
    let gpu = Gpu::headless();
    let (w, h) = (200u32, 160u32);
    let check_wrapper = |mode: u8| {
        let mut rt = Runtime::new(
            mode,
            |&m: &u8, _: &mut Cx<u8>| {
                let inner = el()
                    .absolute(40.0, 30.0)
                    .size(120.0, 100.0)
                    .radius(16.0)
                    .liquid_glass(LiquidGlass::regular());
                let wrapped = match m {
                    0 => el().size(200.0, 160.0).opacity(0.6).child(inner),
                    1 => el().size(200.0, 160.0).cache().child(inner),
                    _ => el().size(200.0, 160.0).blur(4.0).child(inner),
                };
                el().size(200.0, 160.0)
                    .fill(Color {
                        r: 0.1,
                        g: 0.85,
                        b: 0.2,
                        a: 1.0,
                    })
                    .child(wrapped)
            },
            (w as f32, h as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        let first_lg = rt
            .instances
            .iter()
            .position(|i| i.params[1] == KIND_LIQUID_GLASS)
            .expect("glass emitted");
        if let Some(first_layer) = rt.instances.iter().position(|i| i.params[1] == KIND_LAYER) {
            assert!(
                first_lg < first_layer,
                "mode {mode}: glass at {first_lg} must hoist before layer at {first_layer}"
            );
        }
        let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        let px = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
        let mid = px_at(&px, w, 100, 80);
        assert!(
            mid[1] > 140,
            "mode {mode}: hoisted glass must sample green backdrop, got {mid:?}"
        );
    };
    check_wrapper(0);
    check_wrapper(1);
    check_wrapper(2);
}

#[test]
fn luminance_probe_updates_glass_is_dark_without_blocking() {
    let gpu = Gpu::headless();
    let (w, h) = (160u32, 160u32);
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let mut rt = Runtime::new(
        false,
        |&dark_bg: &bool, _: &mut Cx<bool>| {
            let bg = if dark_bg { Color::BLACK } else { Color::WHITE };
            el().size(160.0, 160.0).fill(bg).child(
                el().absolute(30.0, 40.0)
                    .size(100.0, 80.0)
                    .glass_id("pill")
                    .liquid_glass(LiquidGlass::regular()),
            )
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);

    for f in 0..8 {
        rt.frame(f as f64 / 60.0);
        let _ = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
        let samples = renderer.poll_glass_probes(&gpu.device);
        if !samples.is_empty() {
            rt.apply_glass_probes(&samples, 0.25);
        }
    }
    assert!(
        !rt.glass_is_dark("pill"),
        "over white should read light (is_dark=false)"
    );
    assert_eq!(renderer.glass_probe_blocking_polls(), 0);

    rt.update(|s, _| {
        *s = true;
    });
    for f in 8..24 {
        rt.frame(f as f64 / 60.0);
        let _ = render_runtime(&mut rt, &mut renderer, &gpu, w, h);
        let samples = renderer.poll_glass_probes(&gpu.device);
        if !samples.is_empty() {
            rt.apply_glass_probes(&samples, 0.25);
        }
    }
    assert!(
        rt.glass_is_dark("pill"),
        "over black should flip to dark (is_dark=true)"
    );
    assert_eq!(renderer.glass_probe_blocking_polls(), 0);
}

#[test]
fn interactive_press_grows_and_overshoots_on_release_and_respects_reduce_motion() {
    let run = |interactive: bool, reduced_motion: bool| {
        let mut rt = Runtime::new(
            interactive,
            |&inter: &bool, _: &mut Cx<bool>| {
                let g = if inter {
                    LiquidGlass::regular().interactive()
                } else {
                    LiquidGlass::regular()
                };
                el().size(300.0, 200.0).child(
                    el().key("btn")
                        .absolute(100.0, 70.0)
                        .size(100.0, 50.0)
                        .liquid_glass(g),
                )
            },
            (300.0, 200.0),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.set_system_reduced_motion(reduced_motion);
        rt.frame(0.0);
        let rest_w = rt.liquid_glass_frame().shapes[0].rect[2];

        rt.pointer_move(150.0, 95.0);
        rt.pointer_down(150.0, 95.0);
        let mut peak_w = rest_w;
        let mut peak_glow = 0.0f32;
        for _ in 1..=25 {
            rt.frame(1.0 / 60.0);
            let sh = rt.liquid_glass_frame().shapes[0];
            peak_w = peak_w.max(sh.rect[2]);
            peak_glow = peak_glow.max(sh.glow[3]);
        }

        rt.pointer_up(150.0, 95.0);
        rt.pointer_move(10.0, 10.0);
        let mut min_release_w = f32::INFINITY;
        for _ in 1..=45 {
            rt.frame(1.0 / 60.0);
            let sh = rt.liquid_glass_frame().shapes[0];
            min_release_w = min_release_w.min(sh.rect[2]);
        }
        (rest_w, peak_w, min_release_w, peak_glow)
    };

    let (rest_w, peak_w, min_rel_w, peak_glow) = run(true, false);
    assert!(
        peak_w >= rest_w * 1.09,
        "interactive press must scale toward 1.10: rest={rest_w} peak={peak_w}"
    );
    assert!(
        min_rel_w < rest_w - 0.1,
        "release spring should overshoot below rest width: rest={rest_w} min={min_rel_w}"
    );
    assert!(
        peak_glow > 0.5,
        "interactive press must activate touch glow: peak_glow={peak_glow}"
    );

    let (ni_rest, ni_peak, _, ni_glow) = run(false, false);
    assert!(
        (ni_peak - ni_rest).abs() < 0.01,
        "non-interactive glass must not scale"
    );
    assert_eq!(ni_glow, 0.0);

    let (rm_rest, _, rm_min_rel, _) = run(true, true);
    assert!(
        rm_min_rel >= rm_rest - 0.05,
        "reduce motion must stop release overshoot: rest={rm_rest} min={rm_min_rel}"
    );
}

#[test]
fn accessibility_policies_and_1080p_pyramid_memory() {
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(200.0, 100.0).child(
                el().size(100.0, 44.0)
                    .liquid_glass(LiquidGlass::regular().still(Color::BLACK)),
            )
        },
        (200.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::FollowSystem);
    rt.set_system_reduced_transparency(true);
    rt.frame(0.0);
    let sh_rt = rt.liquid_glass_frame().shapes[0];
    assert!(
        sh_rt.tone[3] >= 10.0,
        "reduced transparency sigma must be >= 10 pt, got {}",
        sh_rt.tone[3]
    );
    assert_ne!(
        (sh_rt.light[1] as u32) & crate::render::liquid_glass::FLAG_REDUCED_TRANSPARENCY,
        0
    );
    assert!(!rt.glass_on());

    rt.set_system_reduced_transparency(false);
    rt.set_system_increased_contrast(true);
    rt.frame(0.016);
    let sh_ic = rt.liquid_glass_frame().shapes[0];
    assert_ne!(
        (sh_ic.light[1] as u32) & crate::render::liquid_glass::FLAG_INCREASED_CONTRAST,
        0
    );

    rt.set_system_increased_contrast(false);
    rt.set_system_reduced_transparency(true);
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.032);
    let sh_g = rt.liquid_glass_frame().shapes[0];
    assert_eq!(
        (sh_g.light[1] as u32) & crate::render::liquid_glass::FLAG_REDUCED_TRANSPARENCY,
        0
    );
    assert!(rt.glass_on());

    rt.set_glass_policy(GlassPolicy::Solid);
    rt.frame(0.048);
    assert!(rt.liquid_glass_frame().shapes.is_empty());
    assert_eq!(
        rt.instances
            .iter()
            .filter(|i| i.params[1] == KIND_LIQUID_GLASS)
            .count(),
        0
    );

    let bytes = pyramid_bytes([960, 576]);
    assert!(
        bytes > 5_000_000 && bytes < 6_500_000,
        "1080p pyramid bytes={bytes}"
    );
}

#[test]
fn glass_id_morph_flies_inside_container() {
    let mut rt = Runtime::new(
        false,
        |&moved: &bool, _: &mut Cx<bool>| {
            let mut cont = el().size(300.0, 100.0).glass_container(12.0);
            if !moved {
                cont = cont.child(
                    el().key("a")
                        .absolute(20.0, 20.0)
                        .size(60.0, 44.0)
                        .glass_id("chip")
                        .liquid_glass(LiquidGlass::regular()),
                );
            } else {
                cont = cont
                    .child(
                        el().key("anchor")
                            .absolute(100.0, 20.0)
                            .size(60.0, 44.0)
                            .liquid_glass(LiquidGlass::regular()),
                    )
                    .child(
                        el().key("b")
                            .absolute(200.0, 20.0)
                            .size(60.0, 44.0)
                            .glass_id("chip")
                            .liquid_glass(LiquidGlass::regular()),
                    );
            }
            cont
        },
        (300.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);
    rt.update(|m, tx| {
        *m = true;
        tx.animated();
    });
    rt.frame(0.05);
    assert_eq!(
        rt.instances
            .iter()
            .filter(|i| i.params[1] == KIND_LIQUID_GLASS)
            .count(),
        1
    );
    assert_eq!(rt.liquid_glass_frame().shapes.len(), 2);
}
