use super::*;
use crate::{Cx, Env, Ime, Key, KeyEvent, Mods, Runtime};

struct PwApp {
    pw: TextEdit,
    revealed: bool,
    toggles: Vec<bool>,
}

fn pw_view(s: &PwApp, _: &mut Cx<PwApp>) -> El {
    PasswordInput::new(&s.pw, TextStyle::new(14.0, 400.0).lh(20.0))
        .key("pw")
        .revealed(s.revealed)
        .on_edit(|s: &mut PwApp| &mut s.pw)
        .on_reveal_toggle(|s: &mut PwApp, next, _| {
            s.revealed = next;
            s.pw.set_revealed(next);
            s.toggles.push(next);
        })
        .build()
        .w(260.0)
}

#[test]
fn password_input_masks_blocks_copy_and_ime_and_fires_reveal_hook() {
    let mut pw = TextEdit::new("s3cr3t");
    pw.anchor = 0;
    pw.focus = pw.text.len();
    let app = PwApp {
        pw,
        revealed: false,
        toggles: Vec::new(),
    };
    let mut rt = Runtime::new(app, pw_view, (300.0, 100.0), Env::new(true, 1.0));
    rt.frame(0.0);

    // Focus the inner input and verify secure flag is active.
    let input_rect = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("pw-input"))
        .expect("pw-input mounted")
        .rect;
    rt.pointer_down(input_rect[0] + 10.0, input_rect[1] + 10.0);
    rt.pointer_up(input_rect[0] + 10.0, input_rect[1] + 10.0);
    rt.frame(0.016);
    assert!(rt.has_secure_focus());

    // Copy-out must be blocked while hidden AND while revealed.
    rt.key(KeyEvent {
        key: Key::Command('a'),
        mods: Mods {
            ctrl: true,
            ..Default::default()
        },
    });
    rt.key(KeyEvent {
        key: Key::Command('c'),
        mods: Mods {
            ctrl: true,
            ..Default::default()
        },
    });
    rt.frame(0.016);
    assert!(
        rt.copy_request.is_none(),
        "secure password must block copy-out"
    );

    // IME preedit must be rejected while secure.
    rt.ime(Ime::Preedit("abc".to_owned(), Some((0, 3)), Vec::new()));
    rt.frame(0.016);
    assert!(
        rt.state.pw.preedit().is_none(),
        "IME composition must stay off on password fields"
    );

    // Click reveal toggle button -> hook fires with true, focus stays on input, copy still blocked.
    let btn_rect = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("pw-reveal"))
        .expect("pw-reveal mounted")
        .rect;
    let bx = btn_rect[0] + btn_rect[2] * 0.5;
    let by = btn_rect[1] + btn_rect[3] * 0.5;
    rt.pointer_down(bx, by);
    rt.pointer_up(bx, by);
    rt.frame(0.016);
    assert!(rt.state.revealed);
    assert_eq!(rt.state.toggles, vec![true]);
    assert!(
        rt.has_secure_focus(),
        "reveal toggle preserves focus on password field"
    );

    rt.key(KeyEvent {
        key: Key::Command('c'),
        mods: Mods {
            ctrl: true,
            ..Default::default()
        },
    });
    rt.frame(0.016);
    assert!(
        rt.copy_request.is_none(),
        "revealed password must still block copy-out"
    );

    // Helper toggle_revealed works on TextEdit directly too.
    assert!(!rt.state.pw.toggle_revealed());
    assert!(rt.state.pw.toggle_revealed());
}
