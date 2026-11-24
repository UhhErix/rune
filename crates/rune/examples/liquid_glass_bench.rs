//! Headless 1920x1080 GPU frame-timing benchmark for Liquid Glass.
//!
//! Run with `--profile perf`:
//!   cargo run --locked --profile perf -p rune --example liquid_glass_bench
//!
//! Compares paired, alternating `GlassPolicy::Solid` vs `GlassPolicy::Glass` GPU render passes
//! via `wgpu::Features::TIMESTAMP_QUERY` and reports median, p95, and max (keeping outliers).
//! Note: headless GPU timestamps measure adapter execution of the render passes and omit host
//! CPU-to-GPU upload overhead and swapchain presentation.

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::{Color, Cx, El, Env, GlassPolicy, LiquidGlass, Runtime, el};

const W: u32 = 1920;
const H: u32 = 1080;
const WARMUP: usize = 10;
const SAMPLES: usize = 60;
const TOTAL: usize = WARMUP + SAMPLES;

fn bench_backdrop() -> El {
    let mut root = el().size(W as f32, H as f32).gradient(
        Color {
            r: 0.05,
            g: 0.09,
            b: 0.22,
            a: 1.0,
        },
        Color {
            r: 0.14,
            g: 0.06,
            b: 0.20,
            a: 1.0,
        },
    );
    for i in 0..24 {
        let x = (i % 6) as f32 * 320.0 + 24.0;
        let y = (i / 6) as f32 * 260.0 + 24.0;
        root = root.child(
            el().key(format!("tile-{i}"))
                .absolute(x, y)
                .size(272.0, 212.0)
                .radius(20.0)
                .fill(Color {
                    r: 0.12 + (i % 5) as f32 * 0.14,
                    g: 0.18 + ((i * 3) % 5) as f32 * 0.12,
                    b: 0.30 + ((i * 7) % 5) as f32 * 0.11,
                    a: 0.85,
                }),
        );
    }
    root
}

fn build_scene(kind: usize) -> El {
    let mut root = bench_backdrop();
    match kind {
        0 => {
            root = root.child(
                el().key("hero")
                    .absolute(720.0, 460.0)
                    .size(480.0, 160.0)
                    .radius(28.0)
                    .liquid_glass(LiquidGlass::regular()),
            );
        }
        1 => {
            root = root.child(
                el().key("ctrl")
                    .absolute(720.0, 460.0)
                    .size(480.0, 160.0)
                    .radius(28.0)
                    .liquid_glass(LiquidGlass::control()),
            );
        }
        2 => {
            root = root.child(
                el().key("panel")
                    .absolute(480.0, 270.0)
                    .size(960.0, 540.0)
                    .radius(32.0)
                    .liquid_glass(LiquidGlass::panel()),
            );
        }
        3 => {
            let mut layer0 = el().size(W as f32, H as f32).glass_group();
            for i in 0..4 {
                layer0 = layer0.child(
                    el().key(format!("badge-{i}"))
                        .absolute(60.0 + i as f32 * 220.0, 48.0)
                        .size(180.0, 44.0)
                        .radius(22.0)
                        .liquid_glass(LiquidGlass::badge()),
                );
                layer0 = layer0.child(
                    el().key(format!("status-{i}"))
                        .absolute(60.0 + i as f32 * 220.0, 112.0)
                        .size(180.0, 38.0)
                        .radius(19.0)
                        .liquid_glass(LiquidGlass::control()),
                );
            }
            let mut dock = el()
                .key("dock")
                .absolute(520.0, 960.0)
                .size(880.0, 84.0)
                .glass_container(16.0);
            for i in 0..6 {
                dock = dock.child(
                    el().key(format!("dock-{i}"))
                        .absolute(16.0 + i as f32 * 142.0, 12.0)
                        .size(126.0, 60.0)
                        .liquid_glass(LiquidGlass::regular().interactive()),
                );
            }
            let mini = el()
                .key("mini")
                .absolute(1440.0, 880.0)
                .size(420.0, 160.0)
                .radius(26.0)
                .liquid_glass(LiquidGlass::panel());
            let popover = el()
                .key("popover")
                .absolute(1380.0, 640.0)
                .size(480.0, 280.0)
                .radius(24.0)
                .liquid_glass(LiquidGlass::regular());
            root = root.child(layer0).child(dock).child(mini).child(popover);
        }
        4 => {
            let mut group = el().size(W as f32, H as f32).glass_group();
            for i in 0..64 {
                let col = (i % 8) as f32;
                let row = (i / 8) as f32;
                group = group.child(
                    el().key(format!("s64-{i}"))
                        .absolute(80.0 + col * 220.0, 60.0 + row * 120.0)
                        .size(180.0, 88.0)
                        .radius(20.0)
                        .liquid_glass(LiquidGlass::regular()),
                );
            }
            root = root.child(group);
        }
        _ => {
            for i in 0..16 {
                root = root.child(
                    el().key(format!("ov-{i}"))
                        .absolute(240.0 + i as f32 * 40.0, 160.0 + i as f32 * 30.0)
                        .size(320.0, 180.0)
                        .radius(22.0)
                        .liquid_glass(LiquidGlass::regular()),
                );
            }
        }
    }
    root
}

fn main() {
    let gpu = Gpu::headless();
    let info = gpu.adapter.get_info();
    assert!(
        gpu.device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY),
        "adapter {:?} lacks TIMESTAMP_QUERY",
        info
    );
    let period_ms = f64::from(gpu.queue.get_timestamp_period()) / 1e6;
    println!(
        "Liquid Glass 1920x1080 GPU bench (headless; omits upload & swapchain present): {} ({:?})",
        info.name, info.backend
    );

    let target = Offscreen::new(&gpu.device, W, H);
    let scenarios: &[(&str, usize, usize)] = &[
        ("single_regular_480x160", 0, 1),
        ("single_control_480x160", 1, 1),
        ("single_panel_960x540", 2, 1),
        ("exo_16_surfaces_3_epochs", 3, 16),
        ("stress_64_grouped", 4, 64),
        ("ungrouped_16_overlapping", 5, 16),
    ];

    let total_queries = (TOTAL * 4) as u32;
    let queries = gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("lg bench queries"),
        ty: wgpu::QueryType::Timestamp,
        count: total_queries,
    });
    let resolve_bytes = u64::from(total_queries) * 8;
    let resolve = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lg bench resolve"),
        size: resolve_bytes,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("lg bench read"),
        size: resolve_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    for &(name, kind, surfaces) in scenarios {
        let mut rt_solid = Runtime::new(
            kind,
            |&k: &usize, _: &mut Cx<usize>| build_scene(k),
            (W as f32, H as f32),
            Env::new(true, 1.0),
        );
        rt_solid.set_glass_policy(GlassPolicy::Solid);
        rt_solid.frame(0.0);

        let mut rt_glass = Runtime::new(
            kind,
            |&k: &usize, _: &mut Cx<usize>| build_scene(k),
            (W as f32, H as f32),
            Env::new(true, 1.0),
        );
        rt_glass.set_glass_policy(GlassPolicy::Glass);
        rt_glass.frame(0.0);

        let mut ren_solid = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        ren_solid.set_liquid_glass(rt_solid.liquid_glass_frame());
        ren_solid.prepare(
            &gpu.device,
            &gpu.queue,
            &mut rt_solid.text.atlas,
            &rt_solid.instances,
            (W, H),
        );

        let mut ren_glass = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);
        ren_glass.set_liquid_glass(rt_glass.liquid_glass_frame());
        ren_glass.prepare(
            &gpu.device,
            &gpu.queue,
            &mut rt_glass.text.atlas,
            &rt_glass.instances,
            (W, H),
        );
        let stats = ren_glass.liquid_glass_stats();

        for iter in 0..TOTAL {
            let base_q = (iter * 4) as u32;
            let order: [(u32, bool); 2] = if iter % 2 == 0 {
                [(0, false), (2, true)]
            } else {
                [(2, true), (0, false)]
            };
            for (slot, is_glass) in order {
                let ren = if is_glass {
                    &mut ren_glass
                } else {
                    &mut ren_solid
                };
                let mut enc = gpu.device.create_command_encoder(&Default::default());
                ren.draw_timed(
                    &mut enc,
                    &target.view,
                    [0.0; 4],
                    Some(wgpu::RenderPassTimestampWrites {
                        query_set: &queries,
                        beginning_of_pass_write_index: Some(base_q + slot),
                        end_of_pass_write_index: Some(base_q + slot + 1),
                    }),
                );
                ren.submit(&gpu.queue, enc);
                gpu.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
            }
        }

        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.resolve_query_set(&queries, 0..total_queries, &resolve, 0);
        enc.copy_buffer_to_buffer(&resolve, 0, &read, 0, resolve_bytes);
        gpu.queue.submit([enc.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();

        let (mut solid_ms, mut glass_ms, mut delta_ms) = (Vec::new(), Vec::new(), Vec::new());
        {
            let bytes = read.slice(..).get_mapped_range().unwrap();
            for pair in bytes.chunks_exact(32).skip(WARMUP) {
                let s0 = u64::from_le_bytes(pair[0..8].try_into().unwrap());
                let s1 = u64::from_le_bytes(pair[8..16].try_into().unwrap());
                let g0 = u64::from_le_bytes(pair[16..24].try_into().unwrap());
                let g1 = u64::from_le_bytes(pair[24..32].try_into().unwrap());
                let sm = (s1.saturating_sub(s0)) as f64 * period_ms;
                let gm = (g1.saturating_sub(g0)) as f64 * period_ms;
                solid_ms.push(sm);
                glass_ms.push(gm);
                delta_ms.push(gm - sm);
            }
        }
        read.unmap();

        solid_ms.sort_by(f64::total_cmp);
        glass_ms.sort_by(f64::total_cmp);
        delta_ms.sort_by(f64::total_cmp);
        let med = |v: &[f64]| v[v.len() / 2];
        let p95 = |v: &[f64]| v[(v.len() * 95) / 100];
        let max = |v: &[f64]| *v.last().unwrap();
        let d_med = med(&delta_ms);
        let per_surface = d_med / surfaces as f64;
        println!(
            "  {name:<26} surfaces={surfaces:>2} epochs={:>2} py={:.2}MB | solid={:.3}ms glass={:.3}ms | delta med={:.3}ms ({:.3}ms/surf) p95={:.3}ms max={:.3}ms",
            stats.epochs,
            stats.pyramid_bytes as f64 / (1024.0 * 1024.0),
            med(&solid_ms),
            med(&glass_ms),
            d_med,
            per_surface,
            p95(&delta_ms),
            max(&delta_ms),
        );
    }
}
