use super::*;
use crate::{Chrome, Color, Env, Runtime, el};

fn pill_capsule_style() -> CaptionStyle {
    CaptionStyle::new()
        .button_width(34.0)
        .button_height(30.0)
        .gap(2.0)
        .trailing_inset(13.0)
        .radius(999.0)
        .hover_fill(Color::WHITE.alpha(0.10))
        .press_fill(Color::WHITE.alpha(0.133))
        .hover_symbol(Color::WHITE)
        .close_hover_fill(Color::WHITE.alpha(0.10))
        .close_press_fill(Color::WHITE.alpha(0.133))
        .close_hover_symbol(Color { r: 1.0, g: 0.2, b: 0.2, a: 1.0 })
}

fn custom_glyph(glyph: CaptionGlyph, ink: Color) -> El {
    let key = match glyph {
        CaptionGlyph::Minimize => "custom-min",
        CaptionGlyph::Maximize => "custom-max",
        CaptionGlyph::Restore => "custom-restore",
        CaptionGlyph::Close => "custom-close",
    };
    el().key(key).size(16.0, 16.0).fill(ink)
}

fn alt_glyph(_: CaptionGlyph, ink: Color) -> El {
    el().key("alt").size(12.0, 12.0).fill(ink)
}

#[test]
fn default_style_keeps_windows_11_geometry() {
    let cap = Caption::new(52.0);
    assert_eq!(cap.width(1.0), 46.0 * 3.0);
    let rects: Vec<_> = cap.rects(1000.0, 1.0).collect();
    assert_eq!(rects, vec![
        (CaptionButton::Minimize, [1000.0 - 138.0, 0.0, 46.0, 52.0]),
        (CaptionButton::Maximize, [1000.0 - 92.0, 0.0, 46.0, 52.0]),
        (CaptionButton::Close, [1000.0 - 46.0, 0.0, 46.0, 52.0]),
    ]);
    assert_eq!(cap.hit(1000.0, 1.0, (1000.0 - 70.0, 26.0)), Some(CaptionButton::Maximize));
    assert_eq!(cap.hit(1000.0, 1.0, (1000.0 - 140.0, 26.0)), None);
}

#[test]
fn custom_geometry_centers_vertically_and_hit_tests_gaps_and_insets() {
    let cap = Caption::with_style(52.0, pill_capsule_style());
    // trailing 13 + 3 * 34 + 2 * 2 = 119
    assert_eq!(cap.width(1.0), 119.0);
    let rects: Vec<_> = cap.rects(1920.0, 1.0).collect();
    assert_eq!(rects, vec![
        (CaptionButton::Minimize, [1801.0, 11.0, 34.0, 30.0]),
        (CaptionButton::Maximize, [1837.0, 11.0, 34.0, 30.0]),
        (CaptionButton::Close, [1873.0, 11.0, 34.0, 30.0]),
    ]);
    // Inside each button
    assert_eq!(cap.hit(1920.0, 1.0, (1818.0, 26.0)), Some(CaptionButton::Minimize));
    assert_eq!(cap.hit(1920.0, 1.0, (1854.0, 26.0)), Some(CaptionButton::Maximize));
    assert_eq!(cap.hit(1920.0, 1.0, (1890.0, 26.0)), Some(CaptionButton::Close));
    // Gaps, top margin, bottom margin, and trailing inset do not hit
    assert_eq!(cap.hit(1920.0, 1.0, (1835.5, 26.0)), None);
    assert_eq!(cap.hit(1920.0, 1.0, (1871.5, 26.0)), None);
    assert_eq!(cap.hit(1920.0, 1.0, (1854.0, 10.0)), None);
    assert_eq!(cap.hit(1920.0, 1.0, (1854.0, 41.0)), None);
    assert_eq!(cap.hit(1920.0, 1.0, (1910.0, 26.0)), None);

    // Explicit top_inset overrides vertical centring
    let pinned = Caption::with_style(52.0, pill_capsule_style().top_inset(6.0));
    let rects: Vec<_> = pinned.rects(1920.0, 1.0).collect();
    assert_eq!(rects[0].1[1], 6.0);
}

#[test]
fn rtl_mirroring_and_single_button_and_hidden_geometry() {
    let mut cap = Caption::with_style(52.0, pill_capsule_style());
    cap.mirrored = true;
    let rects: Vec<_> = cap.rects(1920.0, 1.0).collect();
    assert_eq!(rects, vec![
        (CaptionButton::Minimize, [85.0, 11.0, 34.0, 30.0]),
        (CaptionButton::Maximize, [49.0, 11.0, 34.0, 30.0]),
        (CaptionButton::Close, [13.0, 11.0, 34.0, 30.0]),
    ]);
    assert_eq!(cap.hit(1920.0, 1.0, (20.0, 26.0)), Some(CaptionButton::Close));
    assert_eq!(cap.hit(1920.0, 1.0, (60.0, 26.0)), Some(CaptionButton::Maximize));

    // Single-button window (Close only)
    let mut close_only = Caption::with_style(52.0, pill_capsule_style());
    close_only.minimizable = false;
    close_only.maximizable = false;
    assert_eq!(close_only.width(1.0), 13.0 + 34.0);
    let rects: Vec<_> = close_only.rects(1920.0, 1.0).collect();
    assert_eq!(rects, vec![(CaptionButton::Close, [1873.0, 11.0, 34.0, 30.0])]);

    // Full screen (hidden)
    let mut hidden = Caption::with_style(52.0, pill_capsule_style());
    hidden.hidden = true;
    assert_eq!(hidden.width(1.0), 0.0);
    assert_eq!(hidden.rects(1920.0, 1.0).count(), 0);
}

#[test]
fn rendered_buttons_match_hit_rects_and_apply_style_colors_and_glyphs() {
    let style = pill_capsule_style().glyph(custom_glyph);
    let mut rt = Runtime::new(0u32, |_, cx| {
        cx.caption_color(Color::WHITE.alpha(0.7));
        el().w_full().h_full()
    }, (1920.0, 1080.0), Env::new(true, 1.0));
    let mut cap = Caption::with_style(52.0, style);
    cap.state = CaptionState::Hovered(CaptionButton::Close);
    cap.maximized = true;
    rt.set_caption(Some(cap));
    rt.frame(0.0);

    let (min_r, _) = rt.presented("rune-caption-minimize").expect("minimize");
    let (max_r, _) = rt.presented("rune-caption-maximize").expect("maximize");
    let (cls_r, _) = rt.presented("rune-caption-close").expect("close");
    let expected: Vec<_> = cap.rects(1920.0, 1.0).collect();
    assert_eq!(min_r, expected[0].1);
    assert_eq!(max_r, expected[1].1);
    assert_eq!(cls_r, expected[2].1);

    // Custom glyphs mounted, with Restore when maximized
    assert!(rt.presented("custom-min").is_some());
    assert!(rt.presented("custom-restore").is_some());
    assert!(rt.presented("custom-max").is_none());
    assert!(rt.presented("custom-close").is_some());

    // Verify resolved fill, radius, and close hover symbol colour in the rendered element tree
    let root = frame(el().w_full().h_full(), &cap, Color::WHITE.alpha(0.7), (1.0, 1.0), false);
    let strip = &root.children[1];
    let close_el = &strip.children[2];
    assert_eq!(close_el.vis.fill, Color::WHITE.alpha(0.10));
    assert_eq!(close_el.vis.radius, 999.0);
    assert_eq!(close_el.children[0].vis.fill, Color { r: 1.0, g: 0.2, b: 0.2, a: 1.0 });

    // Hovering Maximize picks hover_fill and hover_symbol
    cap.state = CaptionState::Hovered(CaptionButton::Maximize);
    let root = frame(el().w_full().h_full(), &cap, Color::WHITE.alpha(0.7), (1.0, 1.0), false);
    let max_el = &root.children[1].children[1];
    assert_eq!(max_el.vis.fill, Color::WHITE.alpha(0.10));
    assert_eq!(max_el.children[0].vis.fill, Color::WHITE);

    // Pressing Maximize picks press_fill
    cap.state = CaptionState::Pressed { button: CaptionButton::Maximize, over: true };
    let root = frame(el().w_full().h_full(), &cap, Color::WHITE.alpha(0.7), (1.0, 1.0), false);
    let max_el = &root.children[1].children[1];
    assert_eq!(max_el.vis.fill, Color::WHITE.alpha(0.133));
}

#[test]
fn hide_glyphs_skips_painting_while_keeping_hit_rects_and_a11y_buttons() {
    let style = pill_capsule_style().hide_glyphs(true);
    let mut cap = Caption::with_style(52.0, style);
    cap.state = CaptionState::Hovered(CaptionButton::Close);
    assert_eq!(cap.hit(1920.0, 1.0, (1890.0, 26.0)), Some(CaptionButton::Close));

    let root = frame(el().w_full().h_full(), &cap, Color::WHITE, (1.0, 1.0), false);
    let strip = &root.children[1];
    assert_eq!(strip.children.len(), 3);
    for btn in &strip.children {
        assert_eq!(btn.vis.fill, Color::TRANSPARENT);
        assert!(btn.children.is_empty());
        assert!(btn.handlers.on_press.is_some());
    }
}

#[test]
fn titlebar_caption_style_propagates_to_headless_chrome_insets() {
    let tb = Titlebar::new(52.0).caption_style(pill_capsule_style());
    let mut rt = Runtime::new(0u32, |_, _| el().w_full().h_full(), (1920.0, 1080.0), Env::new(true, 1.0));
    rt.core_mut().headless_chrome(Chrome::default(), Some(tb), false);
    let insets = rt.core().chrome_insets();
    if cfg!(target_os = "windows") {
        assert_eq!(insets.trailing, 119.0);
        assert_eq!(insets.height, 52.0);
    } else {
        assert_eq!(insets.trailing, 0.0);
    }
}

#[test]
fn caption_style_equality_compares_glyph_callbacks() {
    let a = pill_capsule_style().glyph(custom_glyph);
    let b = pill_capsule_style().glyph(custom_glyph);
    let c = pill_capsule_style().glyph(alt_glyph);
    let d = pill_capsule_style();
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_ne!(a, d);
}

#[test]
fn non_integer_dpi_keeps_hit_rects_and_presented_buttons_aligned() {
    let style = pill_capsule_style().glyph(custom_glyph);
    for scale in [1.25_f32, 1.5, 2.0] {
        let mut rt = Runtime::new(0u32, |_, _| el().w_full().h_full(), (1280.0, 720.0), Env::new(true, scale));
        let cap = Caption::with_style(52.0, style);
        rt.set_caption(Some(cap));
        rt.frame(0.0);

        let expected: Vec<_> = cap.rects(1280.0, scale).collect();
        for (i, key) in ["rune-caption-minimize", "rune-caption-maximize", "rune-caption-close"].iter().enumerate() {
            let (rect, _) = rt.presented(key).unwrap_or_else(|| panic!("{key} at scale {scale}"));
            for c in 0..4 {
                assert!((rect[c] - expected[i].1[c]).abs() < 1e-3, "scale={scale} {key}[{c}]: got {} expected {}", rect[c], expected[i].1[c]);
            }
        }
    }
}

#[test]
fn set_titlebar_updates_caption_style_on_same_commit() {
    let mut rt = Runtime::new(false, |use_custom, cx| {
        if *use_custom {
            cx.set_titlebar(Titlebar::new(52.0).caption_style(pill_capsule_style().glyph(custom_glyph)));
        }
        el().w_full().h_full()
    }, (1920.0, 1080.0), Env::new(true, 1.0));
    rt.set_caption(Some(Caption::new(52.0)));
    rt.frame(0.0);
    assert!(rt.presented("custom-close").is_none());
    let (cls_default, _) = rt.presented("rune-caption-close").expect("default close");
    assert_eq!(cls_default[2], 46.0);

    rt.update(|use_custom, tx| { tx.instant(); *use_custom = true; });
    rt.frame(1.0 / 60.0);
    assert!(rt.presented("custom-close").is_some());
    let (cls_custom, _) = rt.presented("rune-caption-close").expect("custom close");
    assert_eq!(cls_custom, [1873.0, 11.0, 34.0, 30.0]);
}
