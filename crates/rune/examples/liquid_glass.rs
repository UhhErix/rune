//! Liquid Glass showcase over a high-detail photographic backdrop.
//!
//! Run interactively:
//!   cargo run -p rune --example liquid_glass
//!
//! Dump 1920x1080 headless screenshots (`rest.png`, `hover.png`, `press.png`):
//!   cargo run -p rune --example liquid_glass -- --dump-shots docs/exo/liquid-glass-shots

use std::path::PathBuf;

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer, write_png};
use rune::{
    App, AppSpec, Color, Cx, El, Env, GlassPolicy, LiquidGlass, Pixels, Runtime, TextStyle, col,
    el, row, text,
};

const W: u32 = 1920;
const H: u32 = 1080;

struct State {
    backdrop: Pixels,
    active_dock: usize,
    clicks: u32,
}

impl State {
    fn new() -> Self {
        let backdrop = Pixels::new();
        backdrop.set(W, H, generate_busy_photo(W, H));
        Self {
            backdrop,
            active_dock: 1,
            clicks: 0,
        }
    }
}

/// Deterministic, photographic-density 1920x1080 alpine aurora & lakeside city scene with crisp
/// architectural bokeh, high-contrast sunset ridges, fine water reflections, and luminous accents
/// so frost, bevel refraction, chromatic dispersion, adaptive tone, and Fresnel rims read clearly.
fn generate_busy_photo(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let wf = w as f32;
    let hf = h as f32;

    // Precompute bokeh city lights and sunlit alpine bokeh orbs.
    let orbs: &[(f32, f32, f32, [f32; 3])] = &[
        (0.14, 0.22, 0.085, [0.16, 0.92, 0.84]),
        (0.32, 0.34, 0.110, [0.98, 0.42, 0.56]),
        (0.49, 0.24, 0.095, [1.00, 0.72, 0.24]),
        (0.68, 0.19, 0.120, [1.00, 0.84, 0.46]),
        (0.84, 0.28, 0.105, [0.46, 0.62, 1.00]),
        (0.24, 0.62, 0.075, [0.98, 0.58, 0.22]),
        (0.42, 0.56, 0.090, [0.22, 0.88, 0.96]),
        (0.62, 0.58, 0.080, [0.94, 0.36, 0.72]),
        (0.79, 0.64, 0.095, [0.34, 0.96, 0.68]),
        (0.54, 0.86, 0.085, [1.00, 0.54, 0.28]),
        (0.36, 0.88, 0.070, [0.32, 0.72, 1.00]),
        (0.71, 0.88, 0.075, [0.92, 0.44, 0.88]),
    ];

    let hash2 = |x: i32, y: i32| -> f32 {
        let n =
            (x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263)).wrapping_mul(1_274_126_177);
        ((n ^ (n >> 16)) as u32 & 0xffff) as f32 / 65535.0
    };

    for y in 0..h {
        let v = y as f32 / hf;
        let row_off = (y * w * 4) as usize;
        for x in 0..w {
            let u = x as f32 / wf;

            // Rich twilight sky gradient warming toward the upper-right sunset glow.
            let mut r = 0.05
                + 0.24 * v
                + 0.48 * (-(u - 0.76).powi(2) * 5.0 - (v - 0.18).powi(2) * 9.0).exp();
            let mut g = 0.07
                + 0.12 * v
                + 0.36 * (-(u - 0.76).powi(2) * 5.0 - (v - 0.18).powi(2) * 9.0).exp();
            let mut b = 0.18
                + 0.20 * (1.0 - v)
                + 0.22 * (-(u - 0.22).powi(2) * 4.5 - (v - 0.26).powi(2) * 6.0).exp();

            // Sweeping multi-hue aurora ribbons (rich mid-luminance jewel tones so white typography
            // over adaptive Regular glass stays razor-sharp while bokeh highlights pop).
            let wave1 = 0.24 + 0.08 * (u * 7.5 + 0.4).sin() + 0.03 * (u * 19.0).cos();
            let band1 = (-(v - wave1).powi(2) * 210.0).exp();
            let wave2 = 0.37 + 0.07 * (u * 5.8 + 2.1).cos() + 0.025 * (u * 23.0).sin();
            let band2 = (-(v - wave2).powi(2) * 165.0).exp();
            let wave3 = 0.51 + 0.055 * (u * 9.2 - 1.1).sin();
            let band3 = (-(v - wave3).powi(2) * 150.0).exp();

            r += 0.08 * band1 + 0.44 * band2 + 0.54 * band3;
            g += 0.52 * band1 + 0.14 * band2 + 0.24 * band3;
            b += 0.48 * band1 + 0.56 * band2 + 0.16 * band3;

            // Crisp diagonal prismatic light shafts so edge refraction bends sharp lines visibly.
            let diag = (u * 18.0 + v * 10.0).sin();
            let stripe = ((diag - 0.72) * 4.5).clamp(0.0, 1.0);
            r += stripe * 0.14;
            g += stripe * 0.18;
            b += stripe * 0.22;

            // Distant sunlit alpine ridge 1 & dark midground ridge 2.
            let ridge1 = 0.54
                - 0.09 * (u * 4.2 + 0.3).sin().abs()
                - 0.045 * (u * 11.7 + 1.4).sin()
                - 0.018 * (u * 29.0).cos();
            let ridge2 = 0.66
                - 0.065 * (u * 5.6 + 2.0).cos().abs()
                - 0.028 * (u * 16.3).sin()
                - 0.010 * (u * 41.0).cos();

            if v > ridge1 && v <= 0.71 {
                let slope = ((v - ridge1) * 18.0).clamp(0.0, 1.0);
                let rim = (-(v - ridge1) * 90.0).exp();
                r = r * 0.22 + 0.08 + 0.52 * rim + 0.10 * (1.0 - slope);
                g = g * 0.22 + 0.06 + 0.26 * rim;
                b = b * 0.28 + 0.16 + 0.30 * rim;
            }
            if v > ridge2 && v <= 0.71 {
                let rim = (-(v - ridge2) * 110.0).exp();
                r = 0.04 + 0.28 * rim + 0.04 * (u * 24.0).sin().abs();
                g = 0.05 + 0.20 * rim;
                b = 0.11 + 0.32 * rim;
            }

            // Shimmering reflective lake below v = 0.71 with fine horizontal ripples & bright shoreline lights.
            if v > 0.71 {
                let depth = (v - 0.71) / 0.29;
                let ripple = (v * 260.0 + (u * 35.0).sin() * 2.4).sin();
                let mirror_r = 0.08
                    + 0.34 * (-(u - 0.74).powi(2) * 6.0).exp()
                    + 0.26 * (-(u - 0.35).powi(2) * 8.0).exp();
                let mirror_g = 0.11
                    + 0.28 * (-(u - 0.45).powi(2) * 7.0).exp()
                    + 0.22 * (-(u - 0.74).powi(2) * 6.0).exp();
                let mirror_b = 0.20 + 0.34 * (-(u - 0.24).powi(2) * 5.0).exp();
                let shimmer = (0.78 + 0.22 * ripple) * (1.0 - depth * 0.35);
                r = mirror_r * shimmer + (ripple.max(0.0).powi(4)) * 0.26;
                g = mirror_g * shimmer + (ripple.max(0.0).powi(4)) * 0.20;
                b = mirror_b * shimmer + (ripple.max(0.0).powi(4)) * 0.28;
            }

            // Crisp architectural shoreline skyline & glowing window dots right along the horizon.
            if (0.64..=0.715).contains(&v) {
                let col_id = (u * 96.0) as i32;
                let bh = 0.015 + hash2(col_id, 17) * 0.055;
                if v > 0.712 - bh {
                    r = 0.035;
                    g = 0.045;
                    b = 0.085;
                    let wx = ((u * 96.0).fract() * 4.0) as i32;
                    let wy = ((v * 140.0).fract() * 3.0) as i32;
                    if wx % 2 == 1 && wy == 1 && hash2(col_id * 7 + wx, (v * 140.0) as i32) > 0.42 {
                        let warm = hash2(col_id, 91);
                        r = 0.96;
                        g = 0.72 + warm * 0.22;
                        b = 0.36 + (1.0 - warm) * 0.48;
                    }
                }
            }

            // Luminous out-of-focus bokeh orbs & bright crisp rings so lensing/dispersion pops.
            for &(ox, oy, rad, col) in orbs {
                let dx = (u - ox) * (wf / hf);
                let dy = v - oy;
                let dist = (dx * dx + dy * dy).sqrt() / rad;
                if dist < 1.25 {
                    let core = (1.0 - dist).clamp(0.0, 1.0);
                    let ring = (-(dist - 0.82).powi(2) * 42.0).exp();
                    let k = core * core * 0.22 + ring * 0.20;
                    r += col[0] * k;
                    g += col[1] * k;
                    b += col[2] * k;
                }
            }

            // Fine luminous architectural measurement grid lines (high-contrast 1-2px details behind edges).
            let gx = (x % 96) as i32;
            let gy = (y % 96) as i32;
            if gx == 0 || gy == 0 {
                r += 0.09;
                g += 0.11;
                b += 0.14;
            }

            // Crisp stars in the upper sky.
            if v < 0.48 {
                let sx = (x / 4) as i32;
                let sy = (y / 4) as i32;
                if x % 4 == 1 && y % 4 == 1 && hash2(sx, sy) > 0.992 {
                    let s_bright = 0.55 + 0.45 * hash2(sx + 13, sy + 29);
                    r += s_bright;
                    g += s_bright * 0.96;
                    b += s_bright;
                }
            }

            let idx = row_off + (x * 4) as usize;
            out[idx] = (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[idx + 1] = (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[idx + 2] = (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            out[idx + 3] = 255;
        }
    }
    out
}

fn chip(label: &str, value: &str, key: &str) -> El {
    col()
        .key(key)
        .px(18.0)
        .py(14.0)
        .gap(4.0)
        .radius(18.0)
        .liquid_glass(LiquidGlass::control().interactive())
        .child(
            text(label)
                .font(TextStyle::new(11.0, 600.0))
                .color(Color::WHITE.alpha(0.65)),
        )
        .child(
            text(value)
                .font(TextStyle::new(17.0, 600.0))
                .color(Color::WHITE),
        )
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let status_bar = row()
        .key("top-bar")
        .absolute(56.0, 36.0)
        .w(1808.0)
        .justify_between()
        .items_center()
        .glass_group()
        .child(
            row()
                .key("brand-pill")
                .px(20.0)
                .py(11.0)
                .gap(12.0)
                .items_center()
                .radius(24.0)
                .liquid_glass(LiquidGlass::control().interactive())
                .child(el().size(10.0, 10.0).radius(5.0).fill(Color {
                    r: 0.28,
                    g: 0.92,
                    b: 0.72,
                    a: 1.0,
                }))
                .child(
                    text("RUNE  ·  LIQUID GLASS")
                        .font(TextStyle::new(13.0, 600.0))
                        .color(Color::WHITE),
                ),
        )
        .child(
            row()
                .key("telemetry-pill")
                .px(20.0)
                .py(11.0)
                .gap(18.0)
                .items_center()
                .radius(24.0)
                .liquid_glass(LiquidGlass::control())
                .child(
                    text("1920 × 1080  ·  DX12")
                        .font(TextStyle::new(13.0, 500.0))
                        .color(Color::WHITE.alpha(0.86)),
                )
                .child(
                    text("Bevel r⁸  ·  Fresnel⁶  ·  B-Spline Frost")
                        .font(TextStyle::new(13.0, 500.0))
                        .color(Color::WHITE.alpha(0.72)),
                ),
        );

    // Clear glass card over the bright upper-right sunset glow (demonstrates adaptive 35% dimming).
    let clear_badge = col()
        .key("clear-card")
        .absolute(1336.0, 132.0)
        .w(500.0)
        .px(26.0)
        .py(22.0)
        .gap(8.0)
        .radius(28.0)
        .liquid_glass(LiquidGlass::clear().interactive())
        .child(
            text("CLEAR MATERIAL  ·  ADAPTIVE DIMMING")
                .font(TextStyle::new(11.0, 600.0))
                .color(Color::WHITE.alpha(0.75)),
        )
        .child(
            text("Minimal 0.6 pt frost with automatic darkening over sunlit highlights")
                .font(TextStyle::new(16.0, 500.0))
                .color(Color::WHITE)
                .wrap(),
        );

    // Hero Liquid Glass Regular panel (center-left) with subtle dark twilight tint so typography
    // stays high-contrast across busy aurora hues while bevel refraction & Fresnel rim shine.
    let hero_panel = col()
        .key("hero-panel")
        .absolute(112.0, 224.0)
        .w(820.0)
        .padding(40.0)
        .gap(24.0)
        .radius(38.0)
        .liquid_glass(
            LiquidGlass::regular()
                .tint(Color {
                    r: 0.06,
                    g: 0.09,
                    b: 0.16,
                    a: 0.32,
                })
                .cursor_light(true),
        )
        .child(
            col()
                .gap(10.0)
                .child(
                    text("OPTICAL MATERIAL SYSTEM")
                        .font(TextStyle::new(12.0, 600.0))
                        .color(Color::WHITE.alpha(0.70)),
                )
                .child(
                    text("Liquid Glass")
                        .font(TextStyle::new(54.0, 700.0))
                        .color(Color::WHITE),
                )
                .child(
                    text(
                        "Single-capture 7-mip Jimenez pyramid with cubic B-spline frost, \
                         analytic height-field bevel refraction, ±3% dispersion, cursor-leaning \
                         Fresnel rim, and interactive spring physics.",
                    )
                    .font(TextStyle::new(17.0, 400.0))
                    .color(Color::WHITE.alpha(0.86))
                    .wrap(),
                ),
        )
        .child(
            row()
                .gap(24.0)
                .glass_group()
                .child(
                    row()
                        .key("primary-cta")
                        .px(28.0)
                        .py(16.0)
                        .items_center()
                        .radius(28.0)
                        .liquid_glass(
                            LiquidGlass::regular()
                                .tint(Color {
                                    r: 0.18,
                                    g: 0.52,
                                    b: 1.0,
                                    a: 0.48,
                                })
                                .interactive(),
                        )
                        .on_press(|s: &mut State| s.clicks += 1)
                        .child(
                            text(if s.clicks == 0 {
                                "Interact With Glass".to_string()
                            } else {
                                format!("Pressed {}×", s.clicks)
                            })
                            .font(TextStyle::new(16.0, 600.0))
                            .color(Color::WHITE),
                        ),
                )
                .child(
                    row()
                        .key("secondary-cta")
                        .px(24.0)
                        .py(16.0)
                        .items_center()
                        .radius(28.0)
                        .liquid_glass(LiquidGlass::clear().interactive())
                        .child(
                            text("Inspect Bevel")
                                .font(TextStyle::new(16.0, 500.0))
                                .color(Color::WHITE),
                        ),
                ),
        )
        .child(
            row()
                .gap(14.0)
                .glass_group()
                .child(chip("FROST SIGMA", "3 – 16 pt", "stat-frost"))
                .child(chip("REFRACTION", "Bevel r⁸", "stat-refract"))
                .child(chip("DISPERSION", "±3.0% RGB", "stat-disp"))
                .child(chip("RIM LIGHT", "Fresnel⁶ + Cursor", "stat-rim")),
        );

    // Side Web-matched Panel preset (sigma 16, saturation 1.8) on the right.
    let inspector_panel =
        col()
            .key("inspector-panel")
            .absolute(1012.0, 296.0)
            .w(824.0)
            .padding(34.0)
            .gap(22.0)
            .radius(34.0)
            .liquid_glass(LiquidGlass::panel().cursor_light(true))
            .child(
                row()
                    .justify_between()
                    .items_center()
                    .child(
                        col()
                            .gap(4.0)
                            .child(
                                text("PANEL PRESET  ·  SIGMA 16")
                                    .font(TextStyle::new(11.0, 600.0))
                                    .color(Color::WHITE.alpha(0.62)),
                            )
                            .child(
                                text("Atmospheric Frost & Saturation")
                                    .font(TextStyle::new(26.0, 600.0))
                                    .color(Color::WHITE),
                            ),
                    )
                    .child(
                        row()
                            .key("live-badge")
                            .px(14.0)
                            .py(8.0)
                            .radius(16.0)
                            .liquid_glass(LiquidGlass::badge())
                            .child(text("LIVE GPU").font(TextStyle::new(12.0, 600.0)).color(
                                Color {
                                    r: 0.42,
                                    g: 0.96,
                                    b: 0.78,
                                    a: 1.0,
                                },
                            )),
                    ),
            )
            .child(
                text(
                    "Matches Chromium backdrop-filter blur(16px) brightness(0.45) contrast(1.05) \
                 saturate(1.8) in sRGB blur space while adding specular rim highlights and \
                 smooth edge lensing without stair-stepping.",
                )
                .font(TextStyle::new(15.0, 400.0))
                .color(Color::WHITE.alpha(0.80))
                .wrap(),
            )
            .child(
                row()
                    .gap(16.0)
                    .glass_container(16.0)
                    .child(
                        col()
                            .key("swatch-a")
                            .w(236.0)
                            .padding(20.0)
                            .gap(6.0)
                            .radius(24.0)
                            .liquid_glass(LiquidGlass::regular().interactive())
                            .child(
                                text("Regular")
                                    .font(TextStyle::new(17.0, 600.0))
                                    .color(Color::WHITE),
                            )
                            .child(
                                text("Adaptive tone + size frost")
                                    .font(TextStyle::new(13.0, 400.0))
                                    .color(Color::WHITE.alpha(0.75)),
                            ),
                    )
                    .child(
                        col()
                            .key("swatch-b")
                            .w(236.0)
                            .padding(20.0)
                            .gap(6.0)
                            .radius(24.0)
                            .liquid_glass(
                                LiquidGlass::regular()
                                    .tint(Color {
                                        r: 0.94,
                                        g: 0.38,
                                        b: 0.62,
                                        a: 0.36,
                                    })
                                    .interactive(),
                            )
                            .child(
                                text("Tinted")
                                    .font(TextStyle::new(17.0, 600.0))
                                    .color(Color::WHITE),
                            )
                            .child(
                                text("Backdrop-mapped hue")
                                    .font(TextStyle::new(13.0, 400.0))
                                    .color(Color::WHITE.alpha(0.75)),
                            ),
                    )
                    .child(
                        col()
                            .key("swatch-c")
                            .w(236.0)
                            .padding(20.0)
                            .gap(6.0)
                            .radius(24.0)
                            .liquid_glass(LiquidGlass::control().interactive())
                            .child(
                                text("Control")
                                    .font(TextStyle::new(17.0, 600.0))
                                    .color(Color::WHITE),
                            )
                            .child(
                                text("Web sigma 10 + glow")
                                    .font(TextStyle::new(13.0, 400.0))
                                    .color(Color::WHITE.alpha(0.75)),
                            ),
                    ),
            );

    // Floating bottom Dock in a smooth-union glass_container(16.0) so nearby capsules couple cleanly.
    let dock_labels = ["Library", "Discover", "Music", "Captures", "Settings"];
    let mut dock = row()
        .key("dock-container")
        .absolute(520.0, 916.0)
        .w(880.0)
        .h(96.0)
        .px(24.0)
        .py(16.0)
        .gap(18.0)
        .justify_center()
        .items_center()
        .glass_container(16.0);
    for (idx, label) in dock_labels.iter().enumerate() {
        let active = s.active_dock == idx;
        let glass = if active {
            LiquidGlass::regular()
                .tint(Color {
                    r: 0.32,
                    g: 0.72,
                    b: 1.0,
                    a: 0.36,
                })
                .interactive()
        } else {
            LiquidGlass::regular().interactive()
        };
        dock = dock.child(
            row()
                .key(format!("dock-item-{idx}"))
                .px(26.0)
                .py(18.0)
                .items_center()
                .radius(32.0)
                .liquid_glass(glass)
                .on_press(move |st: &mut State| st.active_dock = idx)
                .child(
                    text(*label)
                        .font(TextStyle::new(15.0, if active { 600.0 } else { 500.0 }))
                        .color(Color::WHITE),
                ),
        );
    }

    el().w_full()
        .h_full()
        .child(el().w_full().h_full().pixels(&s.backdrop))
        .child(status_bar)
        .child(clear_badge)
        .child(hero_panel)
        .child(inspector_panel)
        .child(dock)
}

fn render_shot(
    rt: &mut Runtime<State>,
    renderer: &mut Renderer,
    gpu: &Gpu,
    target: &Offscreen,
    out_path: &std::path::Path,
) {
    renderer.upload_images(&gpu.device, &gpu.queue, &mut rt.images);
    renderer.set_fade_stops(rt.fade_stops());
    renderer.set_liquid_glass(rt.liquid_glass_frame());
    renderer.prepare(
        &gpu.device,
        &gpu.queue,
        &mut rt.text.atlas,
        &rt.instances,
        (W, H),
    );
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut enc, &target.view, [0.0; 4]);
    target.copy(&mut enc);
    renderer.submit(&gpu.queue, enc);
    let rgba = target.read(&gpu.device);
    write_png(out_path, W, H, &rgba).expect("write screenshot PNG");
    println!("wrote {}", out_path.display());
}

fn dump_shots(dir: PathBuf) {
    let gpu = Gpu::headless();
    let target = Offscreen::new(&gpu.device, W, H);
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
    let mut rt = Runtime::new(
        State::new(),
        view,
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);

    // 1. Rest state (studio top-left lighting, no pointer).
    render_shot(&mut rt, &mut renderer, &gpu, &target, &dir.join("rest.png"));

    // Locate the interactive primary CTA button for hover and press captures.
    let cta_rect = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("primary-cta"))
        .map(|e| e.rect)
        .unwrap_or([152.0, 440.0, 210.0, 56.0]);
    let cx = cta_rect[0] + cta_rect[2] * 0.72;
    let cy = cta_rect[1] + cta_rect[3] * 0.50;
    println!("primary-cta rect={cta_rect:?} pointer=({cx:.1}, {cy:.1})");

    // 2. Hover state (pointer over right-center of primary CTA: hover swell + cursor-leaning rim).
    rt.pointer_move(cx, cy);
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    render_shot(
        &mut rt,
        &mut renderer,
        &gpu,
        &target,
        &dir.join("hover.png"),
    );

    // 3. Press state (held press near spring peak: ~1.10x scale + radial touch glow + gel stretch).
    rt.pointer_down(cx, cy);
    for _ in 0..10 {
        rt.frame(1.0 / 60.0);
    }
    render_shot(
        &mut rt,
        &mut renderer,
        &gpu,
        &target,
        &dir.join("press.png"),
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--dump-shots" {
            let dir = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("docs/exo/liquid-glass-shots"));
            dump_shots(dir);
            return;
        }
    }
    App::new(AppSpec::new(
        "Liquid Glass",
        (1440.0, 860.0),
        State::new,
        view,
    ))
    .glass(GlassPolicy::Glass)
    .run();
}
