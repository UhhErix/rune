use super::*;
use crate::{Cx, Env, Key, KeyEvent, Mods, Runtime};

fn solid_pixels(w: u32, h: u32, rgba: [u8; 4]) -> Pixels {
    let px = Pixels::new();
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        data.extend_from_slice(&rgba);
    }
    px.set(w, h, data);
    px
}

struct SnipApp {
    canvas: ImageCanvasState,
    pixels: Pixels,
    changes: usize,
}

fn snip_view(s: &SnipApp, _: &mut Cx<SnipApp>) -> El {
    ImageCanvas::new(&s.canvas, &s.pixels)
        .key("snip")
        .size(200.0, 100.0)
        .on_state(|s: &mut SnipApp| &mut s.canvas)
        .on_change(|s: &mut SnipApp, _| s.changes += 1)
        .build()
}

#[test]
fn image_canvas_draws_stroke_arrow_rect_crops_undoes_and_exports_png() {
    let pixels = solid_pixels(200, 100, [20, 40, 80, 255]);
    let mut canvas = ImageCanvasState::new(200, 100);
    canvas.set_color(Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    });
    canvas.set_stroke_width(6.0);

    let mut rt = Runtime::new(
        SnipApp {
            canvas,
            pixels: pixels.clone(),
            changes: 0,
        },
        snip_view,
        (200.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);

    // 1. Freehand stroke across (20, 20) -> (60, 20).
    rt.pointer_down(20.0, 20.0);
    rt.pointer_move(40.0, 20.0);
    rt.pointer_move(60.0, 20.0);
    rt.pointer_up(60.0, 20.0);
    rt.frame(0.016);
    assert_eq!(rt.state.canvas.annotations().len(), 1);
    assert!(matches!(
        rt.state.canvas.annotations()[0],
        CanvasAnnotation::Stroke { .. }
    ));

    // 2. Arrow tool (80, 50) -> (140, 50) in green.
    rt.update(|s, _| {
        s.canvas.set_tool(CanvasTool::Arrow);
        s.canvas.set_color(Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        });
    });
    rt.frame(0.016);
    rt.pointer_down(80.0, 50.0);
    rt.pointer_move(140.0, 50.0);
    rt.pointer_up(140.0, 50.0);
    rt.frame(0.016);
    assert_eq!(rt.state.canvas.annotations().len(), 2);
    assert!(matches!(
        rt.state.canvas.annotations()[1],
        CanvasAnnotation::Arrow { .. }
    ));

    // 3. Rect tool (100, 20) -> (150, 40) with blue fill.
    rt.update(|s, _| {
        s.canvas.set_tool(CanvasTool::Rect);
        s.canvas.set_color(Color::WHITE);
        s.canvas.set_rect_fill(Some(Color {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        }));
    });
    rt.frame(0.016);
    rt.pointer_down(100.0, 20.0);
    rt.pointer_move(150.0, 40.0);
    rt.pointer_up(150.0, 40.0);
    rt.frame(0.016);
    assert_eq!(rt.state.canvas.annotations().len(), 3);
    assert!(matches!(
        rt.state.canvas.annotations()[2],
        CanvasAnnotation::Rect { .. }
    ));
    assert_eq!(rt.state.changes, 3);

    // 4. Undo removes rect; redo brings it back.
    assert!(rt.state.canvas.undo());
    assert_eq!(rt.state.canvas.annotations().len(), 2);
    assert!(rt.state.canvas.redo());
    assert_eq!(rt.state.canvas.annotations().len(), 3);

    // 5. Crop tool: drag top-left handle from (0, 0) -> (50, 10), bottom-right from (200, 100) -> (170, 80).
    rt.update(|s, _| s.canvas.set_tool(CanvasTool::Crop));
    rt.frame(0.016);
    rt.pointer_down(2.0, 2.0);
    rt.pointer_move(52.0, 12.0);
    rt.pointer_up(52.0, 12.0);
    rt.frame(0.016);
    let crop1 = rt.state.canvas.crop().expect("crop set");
    assert!(
        (crop1.x - 50.0).abs() < 1.0 && (crop1.y - 10.0).abs() < 1.0,
        "got {crop1:?}"
    );

    // Escape during crop handle drag restores previous crop.
    rt.pointer_down(crop1.x + crop1.w - 2.0, crop1.y + crop1.h - 2.0);
    rt.pointer_move(120.0, 60.0);
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.canvas.crop(), Some(crop1));

    // Export cropped + annotated PNG and verify decoded dimensions and painted pixels!
    let png = rt.state.canvas.export_png(&pixels).expect("export PNG");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoded = Pixels::from_png(&png).expect("decode exported PNG");
    let (ew, eh, rgba) = decoded.to_rgba8();
    assert_eq!(
        (ew, eh),
        (150, 90),
        "cropped 200x100 from (50,10) -> 150x90"
    );

    // Green arrow passes through image (110, 50) -> cropped (60, 40).
    let arrow_px = ((40 * ew + 60) * 4) as usize;
    assert!(
        rgba[arrow_px + 1] > 200 && rgba[arrow_px] < 30,
        "arrow pixel should be green: {:?}",
        &rgba[arrow_px..arrow_px + 4]
    );

    // Blue filled rect covers image (125, 30) -> cropped (75, 20).
    let rect_px = ((20 * ew + 75) * 4) as usize;
    assert!(
        rgba[rect_px + 2] > 200 && rgba[rect_px] < 30,
        "rect fill pixel should be blue: {:?}",
        &rgba[rect_px..rect_px + 4]
    );

    // Unpainted corner image (55, 85) -> cropped (5, 75) retains original dark blue background.
    let bg_px = ((75 * ew + 5) * 4) as usize;
    assert_eq!(&rgba[bg_px..bg_px + 4], &[20, 40, 80, 255]);
}
