use crate::{
    Color, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Span, TextEdit, TextStyle, auto_grow, col,
    edit_access, text_input,
};

struct EditorApp {
    edit: TextEdit,
    submitted: Vec<String>,
    mark_typos: bool,
}

fn markdown_spans(src: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for line in src.split_inclusive('\n') {
        if line.starts_with("# ") {
            out.push(Span::new(offset, offset + line.trim_end_matches('\n').len()).weight(700.0));
        }
        offset += line.len();
    }
    out
}

fn typo_ranges(src: &str) -> Vec<(usize, usize)> {
    let needle = "teh";
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(pos) = src[at..].find(needle) {
        let start = at + pos;
        out.push((start, start + needle.len()));
        at = start + needle.len();
    }
    out
}

fn composer_view(s: &EditorApp, _: &mut Cx<EditorApp>) -> El {
    let style = TextStyle::new(14.0, 400.0).lh(20.0);
    let mut field = text_input(&s.edit, style)
        .key("composer")
        .w_full()
        .color(Color::WHITE)
        .spans_with(markdown_spans)
        .on_edit(edit_access(|s: &mut EditorApp| &mut s.edit))
        .on_submit(|s: &mut EditorApp, _| {
            let msg = std::mem::take(&mut s.edit.text);
            s.edit.anchor = 0;
            s.edit.focus = 0;
            s.edit.sync_invariants();
            s.submitted.push(msg);
        });
    if s.mark_typos {
        field = field.spellcheck_with(typo_ranges);
    }
    col().w(240.0).h(160.0).child(auto_grow(field, 1, 4))
}

fn focus_key<S: 'static>(rt: &mut Runtime<S>, key: &str) {
    let rect = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .expect("element mounted")
        .rect;
    rt.pointer_down(rect[0] + 6.0, rect[1] + 6.0);
    rt.pointer_up(rect[0] + 6.0, rect[1] + 6.0);
    rt.frame(0.016);
}

#[test]
fn multiline_discord_composer_handles_shift_enter_submit_selection_undo_nav_clipboard_and_scroll() {
    let app = EditorApp {
        edit: TextEdit::new(""),
        submitted: Vec::new(),
        mark_typos: false,
    };
    let mut rt = Runtime::new(app, composer_view, (260.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    focus_key(&mut rt, "composer");

    // Type line 1, Shift+Enter inserts newline (does NOT submit), type line 2.
    rt.paste("# Hello world");
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods {
            shift: true,
            ..Default::default()
        },
    });
    rt.paste("second line");
    rt.frame(0.016);
    assert_eq!(rt.state.edit.text, "# Hello world\nsecond line");
    assert!(
        rt.state.submitted.is_empty(),
        "Shift+Enter must insert newline without submitting"
    );

    // Word navigation + selection (Ctrl+Shift+Left selects "line") + copy.
    let word_mod = if cfg!(target_os = "macos") {
        Mods {
            alt: true,
            shift: true,
            ..Default::default()
        }
    } else {
        Mods {
            ctrl: true,
            shift: true,
            ..Default::default()
        }
    };
    let primary = if cfg!(target_os = "macos") {
        Mods {
            cmd: true,
            ..Default::default()
        }
    } else {
        Mods {
            ctrl: true,
            ..Default::default()
        }
    };
    rt.key(KeyEvent {
        key: Key::Left,
        mods: word_mod,
    });
    rt.key(KeyEvent {
        key: Key::Command('c'),
        mods: primary,
    });
    rt.frame(0.016);
    assert_eq!(
        rt.copy_request.as_ref().map(|c| c.content().text()),
        Some("line")
    );

    // Undo and redo restore edits cleanly.
    rt.state.edit.break_history_group();
    rt.key(KeyEvent {
        key: Key::Text("!".into()),
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.edit.text, "# Hello world\nsecond !");
    rt.key(KeyEvent {
        key: Key::Command('z'),
        mods: primary,
    });
    rt.frame(0.016);
    assert_eq!(rt.state.edit.text, "# Hello world\nsecond line");
    let redo_mods = if cfg!(target_os = "macos") {
        Mods {
            cmd: true,
            shift: true,
            ..Default::default()
        }
    } else {
        Mods {
            ctrl: true,
            shift: true,
            ..Default::default()
        }
    };
    rt.key(KeyEvent {
        key: Key::Command('z'),
        mods: redo_mods,
    });
    rt.frame(0.016);
    assert_eq!(rt.state.edit.text, "# Hello world\nsecond !");

    // Soft wrap + auto_grow clamps scroll viewport at max_lines (4 * 20pt = 80pt) while content overflows.
    rt.paste("\nline three\nline four\nline five\nline six with enough words to soft wrap neatly");
    rt.frame(0.016);
    rt.frame(1.0);
    let scroll_el = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("scroll"))
        .expect("auto_grow scroll mounted");
    let field_el = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("composer"))
        .expect("composer mounted");
    assert!(
        scroll_el.rect[3] <= 80.5,
        "scroll viewport caps at 4 lines (80pt): {:?}",
        scroll_el.rect
    );
    assert!(
        field_el.rect[3] > 80.0,
        "multi-line soft-wrapped content overflows viewport: {:?}",
        field_el.rect
    );

    // Plain Enter submits the full multi-line message (Discord composer behaviour).
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert_eq!(rt.state.submitted.len(), 1);
    assert!(rt.state.submitted[0].contains('\n'));
    assert!(rt.state.edit.text.is_empty());
}

#[test]
fn spans_with_and_spellcheck_with_decorate_multiline_editor() {
    let app = EditorApp {
        edit: TextEdit::new("# Title\nFix teh typo"),
        submitted: Vec::new(),
        mark_typos: false,
    };
    let mut rt = Runtime::new(app, composer_view, (260.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);
    let without_spell = rt.instances.len();

    rt.update(|s, _| s.mark_typos = true);
    rt.frame(0.016);
    let with_spell = rt.instances.len();
    assert!(
        with_spell > without_spell,
        "spellcheck_with must emit native spelling underline instances ({without_spell} -> {with_spell})"
    );
}
