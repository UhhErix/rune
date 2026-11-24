//! Accessibility, modal scrim, Apple parity, and hardware-budget tests for Liquid Glass.

use super::budget::{AdapterClass, DOWNGRADE_STREAK, classify_adapter_parts};
use super::pyramid::pyramid_bytes;
use crate::render::liquid_glass::{
    FLAG_BLUR_ONLY, FLAG_CURSOR_LIGHT, FLAG_INCREASED_CONTRAST, FLAG_REDUCED_TRANSPARENCY,
};
use crate::render::{
    Gpu, KIND_BACKDROP_BLUR, KIND_LIQUID_GLASS, OFFSCREEN_FORMAT, Offscreen, Renderer,
};
use crate::{Color, Cx, Env, GlassPolicy, GlassTier, LiquidGlass, Runtime, el};

fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

fn render_gpu<S: 'static>(
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

fn px(pixels: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn luma(p: [u8; 4]) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
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
    assert_ne!((sh_rt.light[1] as u32) & FLAG_REDUCED_TRANSPARENCY, 0);
    assert!(!rt.glass_on());

    rt.set_system_reduced_transparency(false);
    rt.set_system_increased_contrast(true);
    rt.frame(0.016);
    let sh_ic = rt.liquid_glass_frame().shapes[0];
    assert_ne!((sh_ic.light[1] as u32) & FLAG_INCREASED_CONTRAST, 0);

    rt.set_system_increased_contrast(false);
    rt.set_system_reduced_transparency(true);
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.032);
    let sh_g = rt.liquid_glass_frame().shapes[0];
    assert_eq!((sh_g.light[1] as u32) & FLAG_REDUCED_TRANSPARENCY, 0);
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
    // Verify the morphed `glass_id("chip")` shape is in flight between start X=20 and end X=200.
    let mut xs: Vec<f32> = rt
        .liquid_glass_frame()
        .shapes
        .iter()
        .map(|s| s.rect[0])
        .collect();
    xs.sort_by(f32::total_cmp);
    assert!(
        xs.iter().any(|&x| x > 22.0 && x < 198.0),
        "morphed glass_id shape should be mid-flight between 20 and 200, got {xs:?}"
    );
}

#[test]
fn modal_scrim_dims_room_to_react_alpha_and_respects_4_blur_cap() {
    const W: u32 = 320;
    const H: u32 = 200;
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    let mut rt_plain = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| el().size(W as f32, H as f32).fill(rgb(0.20, 0.72, 0.90)),
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt_plain.frame(0.0);
    let plain = render_gpu(&mut rt_plain, &mut renderer, &gpu, W, H);

    // Modal with React-parity `--exo-scrim: rgb(0 0 0 / 0.72)`.
    let mut rt_modal = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(W as f32, H as f32)
                .fill(rgb(0.20, 0.72, 0.90))
                .child(
                    el().key("tour")
                        .absolute(80.0, 40.0)
                        .size(160.0, 120.0)
                        .radius(14.0)
                        .fill(rgb(0.14, 0.16, 0.22))
                        .modal(true, |_: &mut ()| {})
                        .modal_scrim(Color::BLACK.alpha(0.72)),
                )
        },
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt_modal.frame(0.0);
    let scrim_img = render_gpu(&mut rt_modal, &mut renderer, &gpu, W, H);

    let base_out = px(&plain, W, 20, 20);
    let dim_out = px(&scrim_img, W, 20, 20);
    let ratio = luma(dim_out) / luma(base_out).max(1e-4);
    assert!(
        dim_out[1] > 15 && dim_out[2] > 20,
        "room behind modal scrim must remain visible, never solid #000000: {dim_out:?}"
    );
    assert!(
        ratio > 0.12 && ratio < 0.65,
        "modal scrim should dim room (alpha 0.72), ratio={ratio:.3}, base={base_out:?}, dim={dim_out:?}"
    );

    // Optional scrim blur runs uncapped by default (usize::MAX) and respects explicit `max_backdrop_blurs`.
    let mut rt_cap = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            let mut root = el().size(W as f32, H as f32).fill(rgb(0.20, 0.72, 0.90));
            for i in 0..4 {
                root = root.child(
                    el().key(format!("b-{i}"))
                        .absolute(i as f32 * 50.0, 10.0)
                        .size(40.0, 40.0)
                        .backdrop_blur(8.0),
                );
            }
            root.child(
                el().key("m")
                    .absolute(80.0, 60.0)
                    .size(140.0, 100.0)
                    .modal(true, |_: &mut ()| {})
                    .modal_scrim_blur(Color::BLACK.alpha(0.72), 10.0),
            )
        },
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt_cap.frame(0.0);
    let uncapped_blurs = rt_cap
        .instances
        .iter()
        .filter(|i| i.params[1] == KIND_BACKDROP_BLUR)
        .count();
    assert_eq!(uncapped_blurs, 5, "default max_backdrop_blurs is uncapped");
    rt_cap.set_max_backdrop_blurs(4);
    rt_cap.frame(0.016);
    let blurs = rt_cap
        .instances
        .iter()
        .filter(|i| i.params[1] == KIND_BACKDROP_BLUR)
        .count();
    assert_eq!(blurs, 4, "modal_scrim_blur must respect explicit max_backdrop_blurs cap");
}

#[test]
fn modal_liquid_glass_sheet_blurs_dimmed_room_not_solid_black() {
    const W: u32 = 360;
    const H: u32 = 220;
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    // Two-tone room (warm red left, bright cyan right) behind a fading modal Liquid Glass sheet.
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(W as f32, H as f32)
                .child(
                    el().absolute(0.0, 0.0)
                        .size(W as f32 * 0.5, H as f32)
                        .fill(rgb(0.95, 0.15, 0.10)),
                )
                .child(
                    el().absolute(W as f32 * 0.5, 0.0)
                        .size(W as f32 * 0.5, H as f32)
                        .fill(rgb(0.08, 0.78, 0.96)),
                )
                .child(
                    el().key("sheet-fade")
                        .absolute(0.0, 0.0)
                        .size(W as f32, H as f32)
                        .opacity(0.95)
                        .child(
                            el().key("capture-sheet")
                                .absolute(60.0, 35.0)
                                .size(240.0, 150.0)
                                .radius(18.0)
                                .modal(true, |_: &mut ()| {})
                                .modal_scrim(Color::BLACK.alpha(0.72))
                                .liquid_glass(LiquidGlass::panel()),
                        ),
                )
        },
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);
    let img = render_gpu(&mut rt, &mut renderer, &gpu, W, H);

    // Outside modal: room is dimmed but preserves red left & cyan right (never #000000).
    let out_left = px(&img, W, 20, 110);
    let out_right = px(&img, W, 340, 110);
    assert!(
        out_left[0] > 25 && out_left[0] > out_left[2] + 15,
        "left room behind modal scrim must remain dimmed red, got {out_left:?}"
    );
    assert!(
        out_right[2] > 25 && out_right[2] > out_right[0] + 15,
        "right room behind modal scrim must remain dimmed cyan, got {out_right:?}"
    );

    // Inside translucent glass sheet: left half stays warmer than right half (blurs the dimmed room).
    let in_left = px(&img, W, 120, 110);
    let in_right = px(&img, W, 240, 110);
    assert!(
        in_left[0] > in_right[0] + 8 && in_right[2] > in_left[2] + 8,
        "glass sheet must blur dimmed room through translucent glass, left={in_left:?} right={in_right:?}"
    );
}

#[test]
fn glass_union_merges_matching_ids_and_splits_distinct_ids() {
    const W: u32 = 320;
    const H: u32 = 120;
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    // spacing = 0: A & B share `glass_union("nav")` so they merge across their 6px gap;
    // C uses `glass_union("other")` so its 6px gap from B stays open backdrop.
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(W as f32, H as f32)
                .fill(rgb(0.05, 0.08, 0.14))
                .child(
                    el().size(W as f32, H as f32)
                        .glass_container(0.0)
                        .child(
                            el().key("a")
                                .absolute(20.0, 30.0)
                                .size(80.0, 56.0)
                                .glass_union("nav")
                                .liquid_glass(LiquidGlass::regular().tint(rgb(0.85, 0.90, 1.0))),
                        )
                        .child(
                            el().key("b")
                                .absolute(106.0, 30.0)
                                .size(80.0, 56.0)
                                .glass_union("nav")
                                .liquid_glass(LiquidGlass::regular().tint(rgb(0.85, 0.90, 1.0))),
                        )
                        .child(
                            el().key("c")
                                .absolute(192.0, 30.0)
                                .size(80.0, 56.0)
                                .glass_union("other")
                                .liquid_glass(LiquidGlass::regular().tint(rgb(0.85, 0.90, 1.0))),
                        ),
                )
        },
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);
    let img = render_gpu(&mut rt, &mut renderer, &gpu, W, H);

    let bg_px = px(&img, W, 5, 5);
    let merged_gap = px(&img, W, 103, 58);
    let split_gap = px(&img, W, 189, 58);
    let diff = |a: [u8; 4], b: [u8; 4]| {
        (a[0] as i32 - b[0] as i32).abs()
            + (a[1] as i32 - b[1] as i32).abs()
            + (a[2] as i32 - b[2] as i32).abs()
    };
    assert!(
        diff(merged_gap, bg_px) > 25,
        "matching glass_union shapes must bridge gap: merged={merged_gap:?} bg={bg_px:?}"
    );
    assert!(
        diff(merged_gap, bg_px) > diff(split_gap, bg_px) + 15,
        "distinct glass_union ids must stay separate: merged={merged_gap:?} split={split_gap:?} bg={bg_px:?}"
    );
}

#[test]
fn tinted_regular_and_touch_glow_at_contact_point() {
    const W: u32 = 320;
    const H: u32 = 140;
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    let render_variant = |glass: LiquidGlass, renderer: &mut Renderer| {
        let mut rt = Runtime::new(
            glass,
            |&g: &LiquidGlass, _: &mut Cx<LiquidGlass>| {
                el().size(W as f32, H as f32)
                    .fill(rgb(0.35, 0.35, 0.35))
                    .child(
                        el().key("pill")
                            .absolute(40.0, 30.0)
                            .size(240.0, 80.0)
                            .liquid_glass(g),
                    )
            },
            (W as f32, H as f32),
            Env::new(true, 1.0),
        );
        rt.set_glass_policy(GlassPolicy::Glass);
        rt.frame(0.0);
        render_gpu(&mut rt, renderer, &gpu, W, H)
    };

    let untinted = render_variant(LiquidGlass::regular(), &mut renderer);
    let tinted = render_variant(
        LiquidGlass::regular().tint(rgb(0.05, 0.35, 0.98)),
        &mut renderer,
    );
    let u_c = px(&untinted, W, 160, 70);
    let t_c = px(&tinted, W, 160, 70);
    assert!(
        (t_c[2] as i32 - t_c[0] as i32) > (u_c[2] as i32 - u_c[0] as i32) + 20,
        "Tinted regular glass must shift hue toward blue tint: untinted={u_c:?} tinted={t_c:?}"
    );

    // Interactive press near LEFT side produces touch glow brighter near left than far right.
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(W as f32, H as f32)
                .fill(rgb(0.10, 0.12, 0.16))
                .child(
                    el().key("btn")
                        .absolute(40.0, 30.0)
                        .size(240.0, 80.0)
                        .liquid_glass(LiquidGlass::regular().interactive()),
                )
        },
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);
    rt.pointer_move(70.0, 70.0);
    rt.pointer_down(70.0, 70.0);
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
    let sh = rt.liquid_glass_frame().shapes[0];
    assert!(
        sh.glow[3] > 0.25 && (sh.glow[0] - 70.0).abs() < 4.0,
        "touch glow must track pointer contact point: glow={:?}",
        sh.glow
    );
    let pressed_img = render_gpu(&mut rt, &mut renderer, &gpu, W, H);
    let near_touch = px(&pressed_img, W, 75, 70);
    let far_side = px(&pressed_img, W, 245, 70);
    assert!(
        luma(near_touch) > luma(far_side) + 0.03,
        "touch glow should illuminate contact point more than far edge: near={near_touch:?} far={far_side:?}"
    );
}

#[test]
fn accessibility_reduce_transparency_increase_contrast_and_reduced_motion_pixels() {
    const W: u32 = 260;
    const H: u32 = 140;
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    let make_rt = || {
        Runtime::new(
            (),
            |_: &(), _: &mut Cx<()>| {
                el().size(W as f32, H as f32)
                    .child(
                        el().absolute(0.0, 0.0)
                            .size(W as f32 * 0.5, H as f32)
                            .fill(Color::BLACK),
                    )
                    .child(
                        el().absolute(W as f32 * 0.5, 0.0)
                            .size(W as f32 * 0.5, H as f32)
                            .fill(Color::WHITE),
                    )
                    .child(
                        el().key("card")
                            .absolute(30.0, 20.0)
                            .size(200.0, 100.0)
                            .radius(20.0)
                            .liquid_glass(LiquidGlass::regular().interactive()),
                    )
            },
            (W as f32, H as f32),
            Env::new(true, 1.0),
        )
    };

    // Normal full glass vs Reduce Transparency (solid frosted >= 88% opaque, zero refract/dispersion).
    let mut rt_full = make_rt();
    rt_full.set_glass_policy(GlassPolicy::FollowSystem);
    rt_full.frame(0.0);
    let full_img = render_gpu(&mut rt_full, &mut renderer, &gpu, W, H);

    let mut rt_red = make_rt();
    rt_red.set_glass_policy(GlassPolicy::FollowSystem);
    rt_red.set_system_reduced_transparency(true);
    rt_red.frame(0.0);
    let sh_rt = rt_red.liquid_glass_frame().shapes[0];
    assert_ne!((sh_rt.light[1] as u32) & FLAG_REDUCED_TRANSPARENCY, 0);
    assert!(sh_rt.tone[3] >= 10.0, "reduced transparency sigma >= 10");
    assert_eq!(sh_rt.bend[1], 0.0, "reduced transparency refraction == 0");
    assert_eq!(sh_rt.bend[2], 0.0, "reduced transparency dispersion == 0");
    let red_img = render_gpu(&mut rt_red, &mut renderer, &gpu, W, H);
    let contrast_delta = |img: &[u8]| (luma(px(img, W, 195, 70)) - luma(px(img, W, 65, 70))).abs();
    assert!(
        contrast_delta(&red_img) < contrast_delta(&full_img) * 0.55,
        "Reduce Transparency must flatten backdrop into high-opacity solid frosted fill: full={:.3} reduced={:.3}",
        contrast_delta(&full_img),
        contrast_delta(&red_img)
    );

    // Increase Contrast: solid rim at border + darker interior tint.
    let mut rt_ic = make_rt();
    rt_ic.set_glass_policy(GlassPolicy::FollowSystem);
    rt_ic.set_system_increased_contrast(true);
    rt_ic.frame(0.0);
    assert_ne!(
        (rt_ic.liquid_glass_frame().shapes[0].light[1] as u32) & FLAG_INCREASED_CONTRAST,
        0
    );
    let ic_img = render_gpu(&mut rt_ic, &mut renderer, &gpu, W, H);
    let rim_full = luma(px(&full_img, W, 31, 70));
    let rim_ic = luma(px(&ic_img, W, 31, 70));
    assert!(
        rim_ic > rim_full + 0.10,
        "Increase Contrast must draw solid contrasting rim: full={rim_full:.3} ic={rim_ic:.3}"
    );

    // Reduced Motion: disables spring scale bounce AND disables dispersion & cursor light.
    let mut rt_rm = make_rt();
    rt_rm.set_glass_policy(GlassPolicy::Glass);
    rt_rm.set_system_reduced_motion(true);
    rt_rm.frame(0.0);
    let rest_w = rt_rm.liquid_glass_frame().shapes[0].rect[2];
    rt_rm.pointer_move(130.0, 70.0);
    rt_rm.pointer_down(130.0, 70.0);
    for _ in 0..8 {
        rt_rm.frame(1.0 / 60.0);
    }
    let sh_rm = rt_rm.liquid_glass_frame().shapes[0];
    assert!(
        (sh_rm.rect[2] - rest_w).abs() < 0.01,
        "Reduced Motion must disable press bounce spring: rest={rest_w} pressed={}",
        sh_rm.rect[2]
    );
    assert_eq!(sh_rm.bend[2], 0.0, "Reduced Motion must disable dispersion");
    assert_eq!(
        (sh_rm.light[1] as u32) & FLAG_CURSOR_LIGHT,
        0,
        "Reduced Motion must disable cursor light"
    );
}

#[test]
fn blur_only_tier_and_hardware_auto_downgrade() {
    const W: u32 = 240;
    const H: u32 = 120;
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(W as f32, H as f32)
                .fill(rgb(0.15, 0.40, 0.70))
                .child(
                    el().key("g")
                        .absolute(30.0, 20.0)
                        .size(180.0, 80.0)
                        .radius(18.0)
                        .liquid_glass(LiquidGlass::panel()),
                )
        },
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::BlurOnly);
    rt.frame(0.0);
    let sh_bo = rt.liquid_glass_frame().shapes[0];
    assert_ne!((sh_bo.light[1] as u32) & FLAG_BLUR_ONLY, 0);
    assert_eq!(sh_bo.bend[1], 0.0, "BlurOnly disables refraction");
    assert_eq!(sh_bo.bend[2], 0.0, "BlurOnly disables dispersion");
    let bo_img = render_gpu(&mut rt, &mut renderer, &gpu, W, H);
    let bo_center = px(&bo_img, W, 120, 60);
    assert!(
        bo_center[2] > bo_center[0] + 15,
        "BlurOnly must still frost backdrop cleanly: {bo_center:?}"
    );

    // Hardware budget: WARP -> Solid, Intel UHD on battery -> BlurOnly, AC -> Full,
    // then sustained over-budget frames step Full -> BlurOnly -> Solid.
    rt.set_glass_policy(GlassPolicy::Auto);
    let mut warp_info = gpu.adapter.get_info();
    warp_info.name = "Microsoft Basic Render Driver".into();
    warp_info.device_type = wgpu::DeviceType::Cpu;
    assert_eq!(
        classify_adapter_parts(warp_info.device_type, &warp_info.name),
        AdapterClass::Software
    );
    rt.set_glass_adapter_info(&warp_info, false);
    rt.frame(0.016);
    assert_eq!(rt.glass_tier(), GlassTier::Solid);
    assert!(!rt.glass_on());
    assert!(rt.liquid_glass_frame().shapes.is_empty());

    let mut igpu_info = gpu.adapter.get_info();
    igpu_info.name = "Intel(R) UHD Graphics 770".into();
    igpu_info.device_type = wgpu::DeviceType::IntegratedGpu;
    rt.set_glass_adapter_info(&igpu_info, true);
    rt.frame(0.032);
    assert_eq!(rt.glass_tier(), GlassTier::BlurOnly);
    assert_ne!(
        (rt.liquid_glass_frame().shapes[0].light[1] as u32) & FLAG_BLUR_ONLY,
        0
    );

    rt.set_glass_power_battery(false);
    rt.frame(0.048);
    assert_eq!(rt.glass_tier(), GlassTier::Full);
    assert_eq!(
        (rt.liquid_glass_frame().shapes[0].light[1] as u32) & FLAG_BLUR_ONLY,
        0
    );

    for _ in 0..DOWNGRADE_STREAK {
        rt.record_glass_frame_ms(2.1);
    }
    rt.frame(0.064);
    assert_eq!(rt.glass_tier(), GlassTier::BlurOnly);
    for _ in 0..DOWNGRADE_STREAK {
        rt.record_glass_frame_ms(1.6);
    }
    rt.frame(0.080);
    assert_eq!(rt.glass_tier(), GlassTier::Solid);
    assert!(rt.liquid_glass_frame().shapes.is_empty());
}
