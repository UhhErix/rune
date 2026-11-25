use rune::render::{Gpu, KIND_LAYER, KIND_LAYER_END, OFFSCREEN_FORMAT, Offscreen, Renderer};
use rune::{Color, Cx, Env, GlassPolicy, LiquidGlass, Runtime, col, el, memo};

#[test]
fn hoisted_glass_keeps_sibling_layers_balanced_across_paint_keep() {
    let gpu = Gpu::headless();
    let (w, h) = (320u32, 240u32);
    let mut renderer = Renderer::new(&gpu.device, OFFSCREEN_FORMAT);

    #[derive(Clone, Default)]
    struct S {
        tick: u32,
        opacity: f32,
    }

    fn view(s: &S, _: &mut Cx<S>) -> rune::El {
        let tick = s.tick;
        let op = s.opacity;
        let glass_panel = el()
            .key("glass-wrap")
            .size(160.0, 60.0)
            .opacity(op)
            .fill(Color { r: 0.05, g: 0.05, b: 0.05, a: 1.0 })
            .child(
                el().key("glass-pill")
                    .size(140.0, 44.0)
                    .radius(12.0)
                    .liquid_glass(LiquidGlass::panel())
                    .child(
                        el().size(12.0 + (tick % 4) as f32, 12.0)
                            .fill(Color::WHITE),
                    ),
            );
        let static_sibling = memo("static-layer-sib", 0u8, || {
            el().key("clipped-card")
                .size(200.0, 90.0)
                .radius(14.0)
                .clip_rounded(14.0)
                .fill(Color { r: 0.12, g: 0.14, b: 0.18, a: 1.0 })
                .child(
                    el().size(180.0, 40.0)
                        .radius(8.0)
                        .clip_rounded(8.0)
                        .fill(Color { r: 0.2, g: 0.3, b: 0.5, a: 1.0 })
                        .child(
                            el().size(60.0, 20.0)
                                .fill(Color { r: 0.4, g: 0.7, b: 1.0, a: 1.0 }),
                        ),
                )
        });
        col()
            .w_full()
            .h_full()
            .fill(Color::BLACK)
            .child(glass_panel)
            .child(static_sibling)
    }

    let mut rt = Runtime::new(
        S { tick: 0, opacity: 0.65 },
        view,
        (w as f32, h as f32),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);

    for step in 0..4u32 {
        rt.update(move |s, _| {
            s.tick = step;
            s.opacity = if step % 2 == 0 { 0.65 } else { 0.72 };
        });
        rt.frame(step as f64 / 60.0);
        let target = Offscreen::new(&gpu.device, w, h);
        renderer.prepare(&gpu.device, &gpu.queue, &mut rt.text.atlas, &rt.instances, (w, h));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        renderer.draw(&mut encoder, &target.view, [0.0, 0.0, 0.0, 1.0]);
        target.copy(&mut encoder);
        gpu.queue.submit([encoder.finish()]);
        let _ = target.read(&gpu.device);
        let mut depth = 0isize;
        for inst in &rt.instances {
            if inst.params[1] == KIND_LAYER {
                depth += 1;
            } else if inst.params[1] == KIND_LAYER_END {
                assert!(depth > 0, "step {step}: unbalanced layer end in rt.instances");
                depth -= 1;
            }
        }
        assert_eq!(depth, 0, "step {step}: unclosed layers in rt.instances");
    }
}
