use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use crate::color::Color;
use crate::el::{Direction, text};
use crate::render::{Gpu, KIND_GLYPH, Offscreen, Renderer, cpu_boot};
use crate::runtime::{Cx, Env, Runtime};
use crate::text::Span;
use crate::tok::TextStyle;

const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const GREEN: Color = Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 };
const BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

fn render_pair<S: 'static>(
    rt: &mut Runtime<S>,
    w: u32,
    h: u32,
) -> (Vec<u8>, Vec<u8>) {
    rt.frame(0.0);
    let gpu = Gpu::headless();
    let mut renderer = Renderer::new(&gpu.device, wgpu::TextureFormat::Bgra8Unorm);
    renderer.set_srgb_blend(true);
    let target = Offscreen::with_format(&gpu.device, w, h, wgpu::TextureFormat::Bgra8Unorm);
    renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, (w, h));
    renderer.upload_images(&gpu.device, &gpu.queue, &mut rt.images);
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    renderer.draw(&mut enc, &target.view, [0.0, 0.0, 0.0, 1.0]);
    target.copy(&mut enc);
    gpu.queue.submit([enc.finish()]);
    let gpu_bgra = target.read(&gpu.device);

    let mut cpu_bgra = vec![0u8; (w * h * 4) as usize];
    let scene = cpu_boot::Scene::new(&rt.instances, &rt.text.atlas, &rt.images, (w, h), [0.0, 0.0, 0.0, 1.0]).srgb_blend(true);
    cpu_boot::paint(&scene, &mut cpu_boot::Target::new(&mut cpu_bgra, w, h), 1).unwrap();
    (gpu_bgra, cpu_bgra)
}

/// Average RGB of lit (non-background) pixels in `[x0..x1, y0..y1]` of a BGRA8 image.
fn lit_mean_rgb(bgra: &[u8], w: u32, x0: u32, x1: u32, y0: u32, y1: u32) -> [f32; 3] {
    let mut sum = [0.0f64; 3];
    let mut count = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let off = ((y * w + x) * 4) as usize;
            let (r, g, b) = (bgra[off + 2], bgra[off + 1], bgra[off]);
            if r.max(g).max(b) > 80 {
                sum[0] += r as f64;
                sum[1] += g as f64;
                sum[2] += b as f64;
                count += 1;
            }
        }
    }
    assert!(count > 0, "no lit glyph pixels in [{x0}..{x1}, {y0}..{y1}]");
    [
        (sum[0] / count as f64) as f32,
        (sum[1] / count as f64) as f32,
        (sum[2] / count as f64) as f32,
    ]
}

fn assert_gpu_matches_cpu(gpu: &[u8], cpu: &[u8], label: &str) {
    assert_eq!(gpu.len(), cpu.len());
    let mut max_diff = 0u8;
    for (&a, &b) in gpu.iter().zip(cpu) {
        max_diff = max_diff.max(a.abs_diff(b));
    }
    assert!(max_diff <= 2, "{label}: GPU and cpu_boot diverged by {max_diff}");
}

#[test]
fn gpu_and_cpu_boot_fill_horizontal_vertical_and_multistop_text_gradients() {
    // 1. Horizontal 2-stop gradient (left red -> right blue) across a multi-glyph line.
    let (w, h) = (260u32, 64u32);
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            text("MMMMMMMM")
                .font(TextStyle::new(36.0, 700.0))
                .text_gradient(FRAC_PI_2, [(0.0, RED), (1.0, BLUE)])
                .w(260.0)
                .h(64.0)
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    let (gpu, cpu) = render_pair(&mut rt, w, h);
    assert_gpu_matches_cpu(&gpu, &cpu, "horizontal 2-stop");
    let left = lit_mean_rgb(&gpu, w, 0, 50, 0, h);
    let right = lit_mean_rgb(&gpu, w, 170, w, 0, h);
    assert!(left[0] > left[2] + 80.0, "left glyphs should be red-dominant: {left:?}");
    assert!(right[2] > right[0] + 80.0, "right glyphs should be blue-dominant: {right:?}");

    // 2. Vertical 2-stop gradient (top red -> bottom blue, CSS 180deg = PI).
    let (vw, vh) = (120u32, 64u32);
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            text("HHH")
                .font(TextStyle::new(48.0, 700.0))
                .text_gradient(PI, [(0.0, RED), (1.0, BLUE)])
                .w(120.0)
                .h(64.0)
        },
        (vw as f32, vh as f32),
        Env::new(true, 1.0),
    );
    let (gpu, cpu) = render_pair(&mut rt, vw, vh);
    assert_gpu_matches_cpu(&gpu, &cpu, "vertical 2-stop");
    let top = lit_mean_rgb(&gpu, vw, 0, vw, 0, 26);
    let bot = lit_mean_rgb(&gpu, vw, 0, vw, 34, vh);
    assert!(top[0] > top[2] + 40.0, "top of glyphs should be red-dominant: {top:?}");
    assert!(bot[2] > bot[0] + 40.0, "bottom of glyphs should be blue-dominant: {bot:?}");

    // 3. Multi-stop 3-stop horizontal gradient (0% red -> 50% green -> 100% blue).
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            text("MMMMMMMM")
                .font(TextStyle::new(36.0, 700.0))
                .text_gradient(FRAC_PI_2, [(0.0, RED), (0.5, GREEN), (1.0, BLUE)])
                .w(260.0)
                .h(64.0)
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    let (gpu, cpu) = render_pair(&mut rt, w, h);
    assert_gpu_matches_cpu(&gpu, &cpu, "horizontal 3-stop");
    let left = lit_mean_rgb(&gpu, w, 0, 45, 0, h);
    let mid = lit_mean_rgb(&gpu, w, 100, 160, 0, h);
    let right = lit_mean_rgb(&gpu, w, 195, w, 0, h);
    assert!(left[0] > left[1] && left[0] > left[2], "left should be red: {left:?}");
    assert!(mid[1] > mid[0] && mid[1] > mid[2], "middle should be green: {mid:?}");
    assert!(right[2] > right[0] && right[2] > right[1], "right should be blue: {right:?}");

    // 4. Angled gradient (45deg, bottom-left red -> top-right blue).
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            text("MMMMMM")
                .font(TextStyle::new(36.0, 700.0))
                .text_gradient(FRAC_PI_4, [(0.0, RED), (1.0, BLUE)])
                .w(260.0)
                .h(64.0)
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    let (gpu, cpu) = render_pair(&mut rt, w, h);
    assert_gpu_matches_cpu(&gpu, &cpu, "angled 45deg");
    let left = lit_mean_rgb(&gpu, w, 0, 50, 0, h);
    let right = lit_mean_rgb(&gpu, w, 130, w, 0, h);
    assert!(left[0] > right[0] + 40.0 && right[2] > left[2] + 40.0, "angled ramp: left={left:?} right={right:?}");
}

#[test]
fn text_gradient_span_override_rtl_mirroring_and_opacity_fold() {
    // Explicit Span::color overrides the gradient on its glyphs while other glyphs keep the gradient.
    let (w, h) = (220u32, 56u32);
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            text("MM MM")
                .font(TextStyle::new(32.0, 700.0))
                .text_gradient(FRAC_PI_2, [(0.0, RED), (1.0, BLUE)])
                .spans(vec![Span::new(0, 2).color(GREEN)])
                .w(220.0)
                .h(56.0)
        },
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    let (gpu, cpu) = render_pair(&mut rt, w, h);
    assert_gpu_matches_cpu(&gpu, &cpu, "span override");
    let first = lit_mean_rgb(&gpu, w, 0, 55, 0, h);
    assert!(first[1] > first[0] + 100.0 && first[1] > first[2] + 100.0, "span override should be green: {first:?}");
    assert!(
        rt.instances.iter().any(|i| i.params[1] == KIND_GLYPH && (i.radii[1] != 0.0 || i.radii[2] != 0.0)),
        "un-overridden glyphs should still carry gradient parameters"
    );

    // RTL flips the horizontal gradient direction (right-to-left: right red, left blue).
    let mut rt_rtl = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            text("MMMMMMMM")
                .dir(Direction::Rtl)
                .font(TextStyle::new(32.0, 700.0))
                .text_gradient(FRAC_PI_2, [(0.0, RED), (1.0, BLUE)])
                .w(260.0)
                .h(64.0)
        },
        (260.0, 64.0),
        Env::new(true, 1.0),
    );
    let (gpu_rtl, cpu_rtl) = render_pair(&mut rt_rtl, 260, 64);
    assert_gpu_matches_cpu(&gpu_rtl, &cpu_rtl, "rtl mirror");
    let left_rtl = lit_mean_rgb(&gpu_rtl, 260, 0, 60, 0, 64);
    let right_rtl = lit_mean_rgb(&gpu_rtl, 260, 190, 260, 0, 64);
    assert!(left_rtl[2] > left_rtl[0] + 60.0, "RTL left should be blue: {left_rtl:?}");
    assert!(right_rtl[0] > right_rtl[2] + 60.0, "RTL right should be red: {right_rtl:?}");

    // Single-glyph layer opacity folding (`end_layer`) scales both `fill` and `border` on gradient glyphs.
    let mut list = vec![
        crate::render::Instance::begin_layer([0.0, 0.0, 20.0, 20.0], crate::render::NO_CLIP, crate::render::LayerSpec { opacity: 0.5, ..Default::default() }),
        crate::render::Instance {
            rect: [0.0, 0.0, 20.0, 20.0],
            radii: [0.0, 0.05, 0.0, 0.0],
            fill: [1.0, 0.0, 0.0, 1.0],
            border: [0.0, 0.0, 1.0, 1.0],
            params: [0.0, KIND_GLYPH, 0.0, 1.0],
            uv: [0.0, 0.0, 0.0, 0.0],
            clip: crate::render::NO_CLIP,
            fade: [0; 4],
        },
    ];
    let mut marks = Vec::new();
    crate::render::end_layer(&mut list, &mut [], &mut marks, 0, 0);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].fill, [0.5, 0.0, 0.0, 0.5]);
    assert_eq!(list[0].border, [0.0, 0.0, 0.5, 0.5]);
}
