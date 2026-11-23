use super::*;
use crate::{Cx, Env, Key, KeyEvent, Mods, Runtime};

struct OtpApp {
    otp: OtpState,
    changes: Vec<String>,
    completed: Vec<String>,
}

fn otp_view(s: &OtpApp, _: &mut Cx<OtpApp>) -> El {
    OtpInput::new(&s.otp)
        .key("code")
        .on_state(|s: &mut OtpApp| &mut s.otp)
        .on_change(|s: &mut OtpApp, v, _| s.changes.push(v.to_owned()))
        .on_complete(|s: &mut OtpApp, v, _| s.completed.push(v.to_owned()))
        .build()
}

fn click_key<S: 'static>(rt: &mut Runtime<S>, key: &str) {
    let rect = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing element {key}"))
        .rect;
    let cx = rect[0] + rect[2] * 0.5;
    let cy = rect[1] + rect[3] * 0.5;
    rt.pointer_down(cx, cy);
    rt.pointer_up(cx, cy);
    rt.frame(0.016);
}

#[test]
fn otp_typing_advances_backspace_retreats_and_completes() {
    let app = OtpApp {
        otp: OtpState::new(4),
        changes: Vec::new(),
        completed: Vec::new(),
    };
    let mut rt = Runtime::new(app, otp_view, (300.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);

    click_key(&mut rt, "code-0");
    // Non-digit is ignored in Digits mode.
    rt.key(KeyEvent {
        key: Key::Text("x".into()),
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.otp.value(), "");

    for digit in ["1", "2", "3"] {
        rt.key(KeyEvent {
            key: Key::Text(digit.into()),
            mods: Mods::default(),
        });
        rt.frame(0.016);
    }
    assert_eq!(rt.state.otp.value(), "123");
    assert!(!rt.state.otp.is_complete());

    // Box 3 is now focused and empty; Backspace steps back to box 2 and clears "3".
    rt.key(KeyEvent {
        key: Key::Backspace,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.otp.value(), "12");

    // Type remaining two digits -> completes and fires on_complete.
    rt.key(KeyEvent {
        key: Key::Text("9".into()),
        mods: Mods::default(),
    });
    rt.frame(0.016);
    rt.key(KeyEvent {
        key: Key::Text("8".into()),
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.otp.value(), "1298");
    assert!(rt.state.otp.is_complete());
    assert_eq!(rt.state.completed, vec!["1298"]);
}

#[test]
fn otp_paste_fills_all_boxes_and_strips_separators() {
    let app = OtpApp {
        otp: OtpState::new(6),
        changes: Vec::new(),
        completed: Vec::new(),
    };
    let mut rt = Runtime::new(app, otp_view, (400.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);

    // Focus middle box (index 2) and paste a full 6-digit code with dash & spaces.
    click_key(&mut rt, "code-2");
    rt.paste(" 482-915 ");
    rt.frame(0.016);
    assert_eq!(rt.state.otp.value(), "482915");
    assert!(rt.state.otp.is_complete());
    assert_eq!(rt.state.completed, vec!["482915"]);
    for (i, expected) in ['4', '8', '2', '9', '1', '5'].into_iter().enumerate() {
        assert_eq!(rt.state.otp.slot(i), Some(expected));
    }
}

#[test]
fn otp_alphanumeric_upper_handles_steam_guard_codes_and_arrow_keys() {
    let app = OtpApp {
        otp: OtpState::new(5).kind(OtpKind::AlphanumericUpper),
        changes: Vec::new(),
        completed: Vec::new(),
    };
    let mut rt = Runtime::new(app, otp_view, (360.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);

    click_key(&mut rt, "code-0");
    rt.paste("k7p2x");
    rt.frame(0.016);
    assert_eq!(rt.state.otp.value(), "K7P2X");
    assert_eq!(rt.state.completed, vec!["K7P2X"]);

    // Left arrow moves to slot 3 and overwrites '2' with 'q' -> 'Q'.
    rt.key(KeyEvent {
        key: Key::Left,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    rt.key(KeyEvent {
        key: Key::Text("q".into()),
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.otp.value(), "K7PQX");
}
