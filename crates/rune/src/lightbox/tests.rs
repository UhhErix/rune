use crate::{
    El, Env, Key, KeyEvent, Lightbox, LightboxItem, LightboxState, MagnifyEvent, Mods, Pixels,
    Runtime, WheelEvent, col, el,
};

struct App {
    lb: LightboxState,
    items: Vec<LightboxItem>,
    closed_count: usize,
    nav_log: Vec<usize>,
}

fn solid_pixels(r: u8, g: u8, b: u8) -> Pixels {
    let p = Pixels::new();
    let mut data = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        data.extend_from_slice(&[r, g, b, 255]);
    }
    p.set(16, 16, data);
    p
}

fn view(s: &App, _: &mut crate::Cx<App>) -> El {
    col()
        .w_full()
        .h_full()
        .child(el().key("behind").w(120.0).h(40.0).focusable(true))
        .children(
            Lightbox::new("lb", s.items.clone(), |s: &App| &s.lb, |s| &mut s.lb)
                .on_close(|s, _| s.closed_count += 1)
                .on_index_change(|s, i, _| s.nav_log.push(i))
                .build(s),
        )
}

fn center_of(rt: &Runtime<App>, key: &str) -> (f32, f32) {
    let info = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing element {key:?}"));
    (
        info.rect[0] + info.rect[2] * 0.5,
        info.rect[1] + info.rect[3] * 0.5,
    )
}

const DT: f64 = 1.0 / 60.0;

fn click(rt: &mut Runtime<App>, x: f32, y: f32) {
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.frame(DT);
    rt.pointer_up(x, y);
    rt.frame(DT);
}

fn press_key(rt: &mut Runtime<App>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
}

#[test]
fn lightbox_navigation_zoom_pan_and_escape_lifecycle() {
    let items = vec![
        LightboxItem::new(solid_pixels(220, 40, 40))
            .caption("Shot 1")
            .alt("Red shot"),
        LightboxItem::new(solid_pixels(40, 220, 40)).caption("Shot 2"),
        LightboxItem::new(solid_pixels(40, 40, 220)).caption("Shot 3"),
    ];
    let mut rt = Runtime::new(
        App {
            lb: LightboxState::new(),
            items,
            closed_count: 0,
            nav_log: Vec::new(),
        },
        view,
        (800.0, 600.0),
        Env::default(),
    );
    rt.frame(0.0);
    assert!(!rt.state.lb.is_open());
    assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("lb")));

    // Open at slide 0.
    rt.update(|s, _| s.lb.open(0));
    rt.frame(DT);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("lb-image"))
    );

    // Click Next button -> index 1.
    let (nx, ny) = center_of(&rt, "lb-next");
    click(&mut rt, nx, ny);
    assert_eq!(rt.state.lb.index(), Some(1));
    assert_eq!(rt.state.nav_log, vec![1]);

    // Arrow Right -> index 2; another Right clamps (wrap is false).
    press_key(&mut rt, Key::Right);
    rt.frame(DT);
    assert_eq!(rt.state.lb.index(), Some(2));
    press_key(&mut rt, Key::Right);
    rt.frame(DT);
    assert_eq!(rt.state.lb.index(), Some(2));
    assert_eq!(rt.state.nav_log, vec![1, 2]);

    // Click Zoom In button -> 1.5x, then pinch zoom + Ctrl+wheel zoom.
    let (zx, zy) = center_of(&rt, "lb-zoom-in");
    click(&mut rt, zx, zy);
    assert!((rt.state.lb.zoom() - 1.5).abs() < 0.01);

    let (sx, sy) = center_of(&rt, "lb-stage");
    rt.magnify_event(MagnifyEvent::new(0.25).at(sx, sy));
    rt.frame(DT);
    assert!(rt.state.lb.zoom() > 1.8);

    rt.wheel_event(WheelEvent::new(0.0, 40.0).at(sx, sy).mods(Mods {
        ctrl: true,
        ..Mods::default()
    }));
    rt.frame(DT);
    assert!(rt.state.lb.zoom() > 2.1);

    // Drag to pan while zoomed.
    rt.pointer_move(sx, sy);
    rt.pointer_down(sx, sy);
    rt.frame(DT);
    rt.pointer_move(sx + 60.0, sy - 40.0);
    rt.frame(DT);
    rt.pointer_up(sx + 60.0, sy - 40.0);
    rt.frame(DT);
    let pan = rt.state.lb.pan();
    assert!(
        (pan[0] - 60.0).abs() < 1.0 && (pan[1] + 40.0).abs() < 1.0,
        "expected pan near [60, -40], got {pan:?}"
    );

    // Slide change via Arrow Left resets zoom and pan back to fit.
    press_key(&mut rt, Key::Left);
    rt.frame(DT);
    assert_eq!(rt.state.lb.index(), Some(1));
    assert!(!rt.state.lb.is_zoomed());
    assert_eq!(rt.state.lb.pan(), [0.0, 0.0]);

    // Double-click on the stage toggles 2.0x zoom; first Escape resets zoom without closing;
    // second Escape closes the lightbox and fires on_close.
    click(&mut rt, sx, sy);
    click(&mut rt, sx, sy);
    assert!(
        rt.state.lb.is_zoomed(),
        "double-click should toggle zoom on"
    );
    assert!((rt.state.lb.zoom() - 2.0).abs() < 0.01);

    press_key(&mut rt, Key::Escape);
    rt.frame(DT);
    assert!(
        rt.state.lb.is_open(),
        "first Escape when zoomed resets zoom without closing"
    );
    assert!(!rt.state.lb.is_zoomed());
    assert_eq!(rt.state.closed_count, 0);

    press_key(&mut rt, Key::Escape);
    rt.frame(DT);
    assert!(
        !rt.state.lb.is_open(),
        "second Escape at fit zoom closes the lightbox"
    );
    assert_eq!(rt.state.closed_count, 1);
}

#[test]
fn lightbox_wrap_mode_and_backdrop_close() {
    let items = vec![
        LightboxItem::new(solid_pixels(10, 20, 30)),
        LightboxItem::new(solid_pixels(40, 50, 60)),
    ];
    let mut lb = LightboxState::new().with_wrap(true);
    lb.open(0);
    assert!(lb.prev(2));
    assert_eq!(lb.index(), Some(1));
    assert!(lb.next(2));
    assert_eq!(lb.index(), Some(0));

    let mut rt = Runtime::new(
        App {
            lb,
            items,
            closed_count: 0,
            nav_log: Vec::new(),
        },
        view,
        (800.0, 600.0),
        Env::default(),
    );
    rt.frame(0.0);
    assert!(rt.state.lb.is_open());

    // Clicking the bottom margin (over the backdrop, outside the stage/buttons) closes it.
    click(&mut rt, 400.0, 594.0);
    assert!(
        !rt.state.lb.is_open(),
        "clicking dimmed backdrop should close lightbox"
    );
    assert_eq!(rt.state.closed_count, 1);
}
