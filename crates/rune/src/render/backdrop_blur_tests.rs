use super::{SIGMAS, levels};

#[test]
fn levels_cover_the_range_continuously() {
    let top = SIGMAS.len() - 1;
    assert_eq!(levels(0.0, top), (1, 0.0));
    assert_eq!(levels(SIGMAS[top] * 4.0, top), (top, 1.0));
    let mut last = 0.0;
    for k in 1..400 {
        let sigma = k as f32 * 0.5;
        let (n, share) = levels(sigma, top);
        let (a, b) = (SIGMAS[n - 1], SIGMAS[n]);
        let got = (a * a + share * (b * b - a * a)).sqrt();
        assert!((got - sigma.clamp(SIGMAS[0], SIGMAS[top])).abs() < 1e-3, "sigma {sigma}: {got}");
        assert!(got >= last);
        last = got;
    }
}

#[test]
fn custom_max_backdrop_blurs_renders_past_default_and_grows_capacity() {
    let gpu = crate::render::Gpu::headless();
    let (device, queue) = (&gpu.device, &gpu.queue);
    let mut renderer = crate::render::Renderer::new(device, crate::render::OFFSCREEN_FORMAT);
    let mut text = crate::text::TextSystem::new(&[], ("Inter", "Inter"));
    assert_eq!(renderer.max_backdrop_blurs(), usize::MAX);

    let w = 320u32;
    let h = 48u32;
    let mut instances = Vec::new();
    for k in 0..10 {
        let x0 = (k * 32) as f32;
        instances.push(crate::render::Instance {
            rect: [x0, 0.0, 16.0, h as f32],
            fill: [1.0, 0.0, 0.0, 1.0],
            clip: crate::render::NO_CLIP,
            ..bytemuck::Zeroable::zeroed()
        });
        instances.push(crate::render::Instance {
            rect: [x0 + 16.0, 0.0, 16.0, h as f32],
            fill: [0.0, 0.0, 1.0, 1.0],
            clip: crate::render::NO_CLIP,
            ..bytemuck::Zeroable::zeroed()
        });
        let mut blur = crate::render::Instance {
            rect: [x0 + 4.0, 4.0, 24.0, 40.0],
            fill: [0.0, 0.0, 0.0, 1.0],
            clip: crate::render::NO_CLIP,
            ..bytemuck::Zeroable::zeroed()
        };
        blur.params = [8.0, crate::render::KIND_BACKDROP_BLUR, 0.0, 1.0];
        instances.push(blur);
    }

    renderer.set_max_backdrop_blurs(4);
    renderer.prepare(device, queue, &mut text.atlas, &instances, (w, h));
    assert_eq!(renderer.backdrop_blur.as_ref().unwrap().records.len(), 4);

    renderer.set_max_backdrop_blurs(16);
    renderer.prepare(device, queue, &mut text.atlas, &instances, (w, h));
    assert_eq!(renderer.backdrop_blur.as_ref().unwrap().records.len(), 10);

    let target = crate::render::Offscreen::new(device, w, h);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.draw(&mut encoder, &target.view, [0.0, 0.0, 0.0, 1.0]);
    target.copy(&mut encoder);
    queue.submit([encoder.finish()]);
    let pixels = target.read(device);

    let sample_x = 9 * 32 + 14;
    let sample_y = 24;
    let at = ((sample_y * w + sample_x) * 4) as usize;
    let (r, b) = (pixels[at], pixels[at + 2]);
    assert!(r > 30 && b > 30, "10th blurred tile must blur both red and blue across seam, got rgba={:?}", &pixels[at..at + 4]);
}

#[test]
fn overlapping_backdrop_blurs_sample_fresh_rendered_underlayer() {
    let gpu = crate::render::Gpu::headless();
    let (device, queue) = (&gpu.device, &gpu.queue);
    let mut renderer = crate::render::Renderer::new(device, crate::render::OFFSCREEN_FORMAT);
    renderer.set_max_backdrop_blurs(8);
    let mut text = crate::text::TextSystem::new(&[], ("Inter", "Inter"));
    let (w, h) = (96u32, 96u32);

    let bg = crate::render::Instance {
        rect: [0.0, 0.0, 96.0, 96.0],
        fill: [1.0, 0.0, 0.0, 1.0],
        clip: crate::render::NO_CLIP,
        ..bytemuck::Zeroable::zeroed()
    };
    let mut lower_blur = crate::render::Instance {
        rect: [8.0, 8.0, 80.0, 80.0],
        fill: [0.0, 0.0, 0.0, 1.0],
        clip: crate::render::NO_CLIP,
        ..bytemuck::Zeroable::zeroed()
    };
    lower_blur.params = [6.0, crate::render::KIND_BACKDROP_BLUR, 0.0, 1.0];
    let lower_fill = crate::render::Instance {
        rect: [8.0, 8.0, 80.0, 80.0],
        fill: [0.0, 1.0, 0.0, 1.0],
        clip: crate::render::NO_CLIP,
        ..bytemuck::Zeroable::zeroed()
    };
    let mut upper_blur = crate::render::Instance {
        rect: [24.0, 24.0, 48.0, 48.0],
        fill: [0.0, 0.0, 0.0, 1.0],
        clip: crate::render::NO_CLIP,
        ..bytemuck::Zeroable::zeroed()
    };
    upper_blur.params = [6.0, crate::render::KIND_BACKDROP_BLUR, 0.0, 1.0];

    let instances = [bg, lower_blur, lower_fill, upper_blur];
    renderer.prepare(device, queue, &mut text.atlas, &instances, (w, h));
    let target = crate::render::Offscreen::new(device, w, h);
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.draw(&mut encoder, &target.view, [0.0, 0.0, 0.0, 1.0]);
    target.copy(&mut encoder);
    queue.submit([encoder.finish()]);
    let pixels = target.read(device);

    let at = ((48 * w + 48) * 4) as usize;
    let [r, g, b, _] = [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]];
    assert!(g > 200 && r < 40 && b < 40, "overlapping upper blur must sample fresh green underlayer, not stale red; got rgba={:?}", &pixels[at..at + 4]);
}

#[test]
fn runtime_emits_backdrop_blurs_up_to_configured_cap() {
    use crate::{Cx, Env, Runtime, el, oklch};
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            let mut root = el().size(400.0, 100.0);
            for i in 0..12 {
                root = root.child(
                    el().key(format!("b-{i}"))
                        .size(24.0, 24.0)
                        .backdrop_blur(8.0)
                        .fill(oklch(0.3, 0.0, 0.0).alpha(0.5)),
                );
            }
            root
        },
        (400.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);
    let default_count = rt.instances.iter().filter(|i| i.params[1] == crate::render::KIND_BACKDROP_BLUR).count();
    assert_eq!(default_count, 12);

    rt.set_max_backdrop_blurs(4);
    rt.frame(0.0);
    let capped_count = rt.instances.iter().filter(|i| i.params[1] == crate::render::KIND_BACKDROP_BLUR).count();
    assert_eq!(capped_count, 4);

    rt.set_max_backdrop_blurs(16);
    rt.frame(0.0);
    let raised_count = rt.instances.iter().filter(|i| i.params[1] == crate::render::KIND_BACKDROP_BLUR).count();
    assert_eq!(raised_count, 12);
}
