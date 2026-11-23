use crate::el::{Cursor, El};
use crate::{Cx, Env, Key, KeyEvent, Mods, Runtime, col, el, row};

/// A bar with a live `tab` and an ordinary `plain` beside it, a menu under it (with two focusable
/// items) that names `tab` as live, a submenu over the menu that names it too, and a dialog that names
/// nothing.
#[derive(Default)]
struct App {
    open: bool,
    sub: bool,
    dialog: bool,
    down: bool,
    log: Vec<&'static str>,
}

fn view(s: &App, _: &mut Cx<App>) -> El {
    let tab = el()
        .key("tab")
        .size(100.0, 40.0)
        .cursor(Cursor::Pointer)
        .on_press(|s: &mut App| s.log.push("tab"));
    let tab = if s.down { tab.press_on_down() } else { tab };
    let bar = row().key("bar").size(300.0, 40.0).child(tab).child(
        el().key("plain")
            .size(100.0, 40.0)
            .cursor(Cursor::Pointer)
            .on_press(|s: &mut App| s.log.push("plain")),
    );
    let mut root = col().key("root").size(300.0, 300.0).child(bar);
    if s.open {
        root = root.child(
            col()
                .key("menu")
                .absolute(0.0, 100.0)
                .size(100.0, 100.0)
                .modal(true, |s: &mut App| {
                    s.open = false;
                    s.log.push("dismiss");
                })
                .modal_live(["tab"])
                .child(el().key("item-a").size(100.0, 40.0).focusable(true))
                .child(el().key("item-b").size(100.0, 40.0).focusable(true)),
        );
    }
    if s.open && s.sub {
        root = root.child(
            col()
                .key("sub")
                .absolute(50.0, 150.0)
                .size(100.0, 100.0)
                .modal(true, |s: &mut App| {
                    s.sub = false;
                    s.log.push("sub");
                })
                .modal_live(["tab"])
                .child(el().key("sub-item").size(100.0, 40.0).focusable(true)),
        );
    }
    if s.dialog {
        root = root.child(
            col()
                .key("dialog")
                .absolute(200.0, 200.0)
                .size(90.0, 90.0)
                .modal(true, |s: &mut App| {
                    s.dialog = false;
                    s.log.push("dialog");
                })
                .child(el().key("dialog-item").size(90.0, 40.0).focusable(true)),
        );
    }
    root
}

fn runtime(open: bool) -> Runtime<App> {
    let mut rt = Runtime::new(App::default(), view, (300.0, 300.0), Env::new(true, 1.0));
    rt.frame(0.0);
    if open {
        rt.update(|s, _| s.open = true);
        rt.frame(0.0);
    }
    rt
}

fn click(rt: &mut Runtime<App>, (x, y): (f32, f32)) {
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn focused(rt: &Runtime<App>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

#[test]
fn a_live_node_keeps_its_hover_cursor_while_a_modal_is_open() {
    let mut rt = runtime(true);
    rt.pointer_move(20.0, 20.0);
    assert_eq!(
        rt.cursor(),
        Cursor::Pointer,
        "the live tab shows the pointer"
    );
    rt.pointer_move(150.0, 20.0);
    assert_eq!(
        rt.cursor(),
        Cursor::Default,
        "the ordinary node beside it is inert behind the modal"
    );
    rt.update(|s, _| s.open = false);
    rt.frame(0.0);
    rt.pointer_move(150.0, 20.0);
    assert_eq!(
        rt.cursor(),
        Cursor::Pointer,
        "with no modal open the bar is as before"
    );
}

#[test]
fn a_press_on_a_live_node_dismisses_then_activates_in_one_frame() {
    let mut rt = runtime(true);
    rt.pointer_move(20.0, 20.0);
    rt.pointer_down(20.0, 20.0);
    assert!(rt.state.open, "the press is not delivered until release");
    rt.pointer_up(20.0, 20.0);
    rt.frame(0.0);
    assert_eq!(rt.state.log, ["dismiss", "tab"]);
    assert!(!rt.state.open);
}

#[test]
fn a_press_on_a_live_node_that_delivers_on_down_dismisses_then_activates() {
    let mut rt = runtime(false);
    rt.update(|s, _| {
        s.down = true;
        s.open = true;
    });
    rt.frame(0.0);
    rt.pointer_move(20.0, 20.0);
    rt.pointer_down(20.0, 20.0);
    rt.frame(0.0);
    assert_eq!(rt.state.log, ["dismiss", "tab"]);
    rt.pointer_up(20.0, 20.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.log,
        ["dismiss", "tab"],
        "the release delivers nothing again"
    );
}

#[test]
fn a_press_on_an_ordinary_node_behind_the_modal_only_dismisses() {
    let mut rt = runtime(true);
    click(&mut rt, (150.0, 20.0));
    assert_eq!(rt.state.log, ["dismiss"]);
    assert!(!rt.state.open);
}

#[test]
fn tab_stays_trapped_in_the_modal_and_never_reaches_a_live_node() {
    let mut rt = runtime(true);
    let mut seen = Vec::new();
    for _ in 0..4 {
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        rt.frame(0.0);
        seen.push(focused(&rt));
    }
    assert!(
        seen.iter()
            .all(|f| matches!(f.as_deref(), Some("item-a" | "item-b"))),
        "focus stayed in the menu: {seen:?}"
    );
    assert!(rt.state.log.is_empty());
}

#[test]
fn a_submenu_over_a_menu_keeps_the_bar_live_and_dismisses_innermost_first() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.sub = true);
    rt.frame(0.0);
    rt.pointer_move(20.0, 20.0);
    assert_eq!(
        rt.cursor(),
        Cursor::Pointer,
        "the live tab still shows the pointer under two modals"
    );
    click(&mut rt, (20.0, 20.0));
    assert_eq!(rt.state.log, ["sub", "dismiss", "tab"]);
    assert!(
        !rt.state.open && !rt.state.sub,
        "the press closed both menus"
    );
}

#[test]
fn a_dialog_over_a_menu_makes_the_live_bar_inert_again() {
    let mut rt = runtime(true);
    rt.update(|s, _| s.dialog = true);
    rt.frame(0.0);
    rt.pointer_move(20.0, 20.0);
    assert_eq!(
        rt.cursor(),
        Cursor::Default,
        "a modal that names nothing keeps the bar inert"
    );
    click(&mut rt, (20.0, 20.0));
    assert_eq!(
        rt.state.log,
        ["dialog"],
        "the press only dismisses the dialog on top"
    );
    assert!(rt.state.open, "the menu stays open under it");
}
