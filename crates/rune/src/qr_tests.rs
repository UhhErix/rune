use super::*;
use crate::{Cx, Env, Pixels, Runtime, SurfaceFit, el, row};

struct QrApp {
    url: String,
    png_pixels: Pixels,
}

fn qr_view(s: &QrApp, _: &mut Cx<QrApp>) -> El {
    let code = QrCode::encode(&s.url).expect("encode qr");
    row()
        .w_full()
        .h_full()
        .child(
            QrView::new(&code)
                .size(160.0)
                .label("Steam sign-in code")
                .build()
                .key("steam-qr"),
        )
        .child(
            el().key("runtime-png")
                .size(160.0, 160.0)
                .pixels_fit(&s.png_pixels, SurfaceFit::Fit),
        )
}

#[test]
fn qr_code_and_runtime_png_pixels_mount_and_size_correctly() {
    let url = "https://s.team/q/1/987654321".to_owned();
    let png = QrCode::encode(&url).unwrap().to_png(4, 4).unwrap();
    let png_pixels = Pixels::from_png(&png).unwrap();

    let mut rt = Runtime::new(
        QrApp { url, png_pixels },
        qr_view,
        (400.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);

    let qr_el = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("steam-qr"))
        .expect("steam-qr mounted");
    assert_eq!(qr_el.role, "image");
    assert_eq!(qr_el.label.as_deref(), Some("Steam sign-in code"));
    assert!((qr_el.rect[2] - 160.0).abs() < 0.5);
    assert!((qr_el.rect[3] - 160.0).abs() < 0.5);

    let png_el = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("runtime-png"))
        .expect("runtime-png mounted");
    assert!((png_el.rect[2] - 160.0).abs() < 0.5);
    assert!((png_el.rect[3] - 160.0).abs() < 0.5);

    // Shorthand helper also works directly.
    let quick = qr_code("https://s.team/q/1/test", 120.0).expect("quick qr_code");
    assert_eq!(quick.key, None);
}
