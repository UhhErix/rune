//! Headless 1920x1080 GPU frame-timing benchmark for Liquid Glass.
//!
//! Run with `--profile perf`:
//!   cargo run --locked --profile perf -p rune --example liquid_glass_bench
//!
//! Compares paired, alternating `GlassPolicy::Solid` vs `GlassPolicy::Glass` (and `BlurOnly`) GPU
//! render passes via `wgpu::Features::TIMESTAMP_QUERY` (or wall-clock `submit + poll` when an
//! adapter such as WARP lacks timestamp queries), reporting median, p95, and max (keeping outliers).
//! Note: headless benchmarks omit host CPU-to-GPU upload overhead and swapchain presentation.

use std::time::Instant;

use rune::render::{Gpu, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::{Color, Cx, El, Env, GlassPolicy, GlassTier, LiquidGlass, Runtime, el};

fn probe_auto_tier(info: &wgpu::AdapterInfo, on_battery: bool) -> GlassTier {
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| el().size(16.0, 16.0),
        (16.0, 16.0),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Auto);
    rt.set_glass_adapter_info(info, on_battery);
    rt.glass_tier()
}

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
            // 4 glass layers on screen at 1920x1080: top nav bar, side rail, main card, dock.
            root = root
                .child(
                    el().key("layer-nav")
                        .absolute(24.0, 20.0)
                        .size(1872.0, 56.0)
                        .radius(18.0)
                        .liquid_glass(LiquidGlass::control()),
                )
                .child(
                    el().key("layer-rail")
                        .absolute(24.0, 96.0)
                        .size(280.0, 860.0)
                        .radius(22.0)
                        .liquid_glass(LiquidGlass::pane()),
                )
                .child(
                    el().key("layer-card")
                        .absolute(340.0, 140.0)
                        .size(920.0, 560.0)
                        .radius(28.0)
                        .liquid_glass(LiquidGlass::panel()),
                )
                .child(
                    el().key("layer-dock")
                        .absolute(520.0, 968.0)
                        .size(880.0, 84.0)
                        .radius(28.0)
                        .liquid_glass(LiquidGlass::regular()),
                );
        }
        4 => {
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
        5 => {
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

fn prep_renderer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    kind: usize,
    policy: GlassPolicy,
) -> Renderer {
    let mut rt = Runtime::new(
        kind,
        |&k: &usize, _: &mut Cx<usize>| build_scene(k),
        (W as f32, H as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(policy);
    rt.frame(0.0);
    let mut ren = Renderer::new(device, OFFSCREEN_FORMAT);
    ren.set_liquid_glass(rt.liquid_glass_frame());
    ren.prepare(device, queue, &mut rt.text.atlas, &rt.instances, (W, H));
    ren
}

fn run_pair_timestamp(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &Offscreen,
    queries: &wgpu::QuerySet,
    resolve: &wgpu::Buffer,
    read: &wgpu::Buffer,
    period_ms: f64,
    ren_solid: &mut Renderer,
    ren_test: &mut Renderer,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let total_queries = (TOTAL * 4) as u32;
    let resolve_bytes = u64::from(total_queries) * 8;
    for iter in 0..TOTAL {
        let base_q = (iter * 4) as u32;
        let order: [(u32, bool); 2] = if iter % 2 == 0 {
            [(0, false), (2, true)]
        } else {
            [(2, true), (0, false)]
        };
        for (slot, is_test) in order {
            let ren = if is_test {
                &mut *ren_test
            } else {
                &mut *ren_solid
            };
            let mut enc = device.create_command_encoder(&Default::default());
            ren.draw_timed(
                &mut enc,
                &target.view,
                [0.0; 4],
                Some(wgpu::RenderPassTimestampWrites {
                    query_set: queries,
                    beginning_of_pass_write_index: Some(base_q + slot),
                    end_of_pass_write_index: Some(base_q + slot + 1),
                }),
            );
            ren.submit(queue, enc);
            drop(device.poll(wgpu::PollType::wait_indefinitely()));
        }
    }

    let mut enc = device.create_command_encoder(&Default::default());
    enc.resolve_query_set(queries, 0..total_queries, resolve, 0);
    enc.copy_buffer_to_buffer(resolve, 0, read, 0, resolve_bytes);
    queue.submit([enc.finish()]);
    read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    drop(device.poll(wgpu::PollType::wait_indefinitely()));

    let (mut solid_ms, mut test_ms, mut delta_ms) = (Vec::new(), Vec::new(), Vec::new());
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
            test_ms.push(gm);
            delta_ms.push(gm - sm);
        }
    }
    read.unmap();
    (solid_ms, test_ms, delta_ms)
}

fn run_pair_wall(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &Offscreen,
    warmup: usize,
    samples: usize,
    ren_solid: &mut Renderer,
    ren_test: &mut Renderer,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let (mut solid_ms, mut test_ms, mut delta_ms) = (Vec::new(), Vec::new(), Vec::new());
    for iter in 0..(warmup + samples) {
        let mut s_val = 0.0;
        let mut t_val = 0.0;
        let order = if iter % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        };
        for is_test in order {
            let ren = if is_test {
                &mut *ren_test
            } else {
                &mut *ren_solid
            };
            let t0 = Instant::now();
            let mut enc = device.create_command_encoder(&Default::default());
            ren.draw(&mut enc, &target.view, [0.0; 4]);
            ren.submit(queue, enc);
            drop(device.poll(wgpu::PollType::wait_indefinitely()));
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            if is_test {
                t_val = ms;
            } else {
                s_val = ms;
            }
        }
        if iter >= warmup {
            solid_ms.push(s_val);
            test_ms.push(t_val);
            delta_ms.push(t_val - s_val);
        }
    }
    (solid_ms, test_ms, delta_ms)
}

fn print_row(
    name: &str,
    surfaces: usize,
    stats: rune::render::LiquidGlassStats,
    mut solid_ms: Vec<f64>,
    mut test_ms: Vec<f64>,
    mut delta_ms: Vec<f64>,
) {
    solid_ms.sort_by(f64::total_cmp);
    test_ms.sort_by(f64::total_cmp);
    delta_ms.sort_by(f64::total_cmp);
    let med = |v: &[f64]| v[v.len() / 2];
    let p95 = |v: &[f64]| v[((v.len() * 95) / 100).min(v.len() - 1)];
    let max = |v: &[f64]| *v.last().unwrap();
    let d_med = med(&delta_ms);
    let per_surface = d_med / surfaces as f64;
    println!(
        "  {name:<30} surfaces={surfaces:>2} epochs={:>2} py={:.2}MB | solid={:.3}ms test={:.3}ms | delta med={:.3}ms ({:.3}ms/surf) p95={:.3}ms max={:.3}ms",
        stats.epochs,
        stats.pyramid_bytes as f64 / (1024.0 * 1024.0),
        med(&solid_ms),
        med(&test_ms),
        d_med,
        per_surface,
        p95(&delta_ms),
        max(&delta_ms),
    );
}

fn bench_secondary_adapter(adapter: &wgpu::Adapter) {
    let info = adapter.get_info();
    let auto_ac = probe_auto_tier(&info, false);
    let auto_bat = probe_auto_tier(&info, true);
    let is_software = info.device_type == wgpu::DeviceType::Cpu
        || info
            .name
            .to_ascii_lowercase()
            .contains("basic render driver");
    let req_features = if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        wgpu::Features::TIMESTAMP_QUERY
    } else {
        wgpu::Features::empty()
    };
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lg secondary bench"),
        required_features: req_features,
        required_limits: adapter.limits(),
        memory_hints: Default::default(),
        trace: Default::default(),
        experimental_features: Default::default(),
    })) else {
        println!(
            "\nSecondary adapter {} ({:?}): failed to open device",
            info.name, info.backend
        );
        return;
    };
    let has_ts = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    let method = if has_ts {
        "wall submit+poll (TIMESTAMP_QUERY available)"
    } else {
        "wall submit+poll (no TIMESTAMP_QUERY)"
    };
    println!(
        "\nSecondary adapter 1920x1080 bench ({method}; omits upload & swapchain present): {} ({:?}, type={:?}, auto_ac={:?}, auto_battery={:?})",
        info.name, info.backend, info.device_type, auto_ac, auto_bat,
    );
    let target = Offscreen::new(&device, W, H);
    let (warmup, samples) = if is_software { (2, 8) } else { (6, 24) };
    for &(label, policy) in &[
        ("four_layers_1080p [Full]", GlassPolicy::Glass),
        ("four_layers_1080p [BlurOnly]", GlassPolicy::BlurOnly),
    ] {
        let mut ren_solid = prep_renderer(&device, &queue, 3, GlassPolicy::Solid);
        let mut ren_test = prep_renderer(&device, &queue, 3, policy);
        let stats = ren_test.liquid_glass_stats();
        let (s, t, d) = run_pair_wall(
            &device,
            &queue,
            &target,
            warmup,
            samples,
            &mut ren_solid,
            &mut ren_test,
        );
        print_row(label, 4, stats, s, t, d);
    }
}

fn main() {
    let gpu = Gpu::headless();
    let info = gpu.adapter.get_info();
    let auto_tier = probe_auto_tier(&info, false);
    assert!(
        gpu.device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY),
        "adapter {:?} lacks TIMESTAMP_QUERY",
        info
    );
    let period_ms = f64::from(gpu.queue.get_timestamp_period()) / 1e6;
    println!(
        "Liquid Glass 1920x1080 GPU bench (headless TIMESTAMP_QUERY; omits upload & swapchain present): {} ({:?}, type={:?}, auto_tier={:?})",
        info.name, info.backend, info.device_type, auto_tier,
    );

    let target = Offscreen::new(&gpu.device, W, H);
    let scenarios: &[(&str, usize, usize, GlassPolicy)] = &[
        ("single_regular_480x160", 0, 1, GlassPolicy::Glass),
        ("single_control_480x160", 1, 1, GlassPolicy::Glass),
        ("single_panel_960x540", 2, 1, GlassPolicy::Glass),
        (
            "single_panel_960x540 [BlurOnly]",
            2,
            1,
            GlassPolicy::BlurOnly,
        ),
        ("four_layers_1080p [Full]", 3, 4, GlassPolicy::Glass),
        ("four_layers_1080p [BlurOnly]", 3, 4, GlassPolicy::BlurOnly),
        ("exo_16_surfaces_3_epochs", 4, 16, GlassPolicy::Glass),
        ("stress_64_grouped", 5, 64, GlassPolicy::Glass),
        ("ungrouped_16_overlapping", 6, 16, GlassPolicy::Glass),
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

    for &(name, kind, surfaces, policy) in scenarios {
        let mut ren_solid = prep_renderer(&gpu.device, &gpu.queue, kind, GlassPolicy::Solid);
        let mut ren_test = prep_renderer(&gpu.device, &gpu.queue, kind, policy);
        let stats = ren_test.liquid_glass_stats();
        let (s, t, d) = run_pair_timestamp(
            &gpu.device,
            &gpu.queue,
            &target,
            &queries,
            &resolve,
            &read,
            period_ms,
            &mut ren_solid,
            &mut ren_test,
        );
        print_row(name, surfaces, stats, s, t, d);
    }

    // Enumerate DX12 adapters to also measure an iGPU or Microsoft Basic Render Driver (WARP).
    let adapters = pollster::block_on(gpu.instance.enumerate_adapters(wgpu::Backends::DX12));
    for adapter in adapters {
        let ainfo = adapter.get_info();
        if ainfo.name == info.name && ainfo.device_type == info.device_type {
            continue;
        }
        if matches!(
            ainfo.device_type,
            wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::Cpu
        ) || ainfo
            .name
            .to_ascii_lowercase()
            .contains("basic render driver")
        {
            bench_secondary_adapter(&adapter);
        }
    }
}
