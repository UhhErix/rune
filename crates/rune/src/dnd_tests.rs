use super::*;
use crate::{Cx, DragOutcome, Env, Key, KeyEvent, Mods, Runtime, col, row};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemPayload(usize);

#[derive(Default)]
struct DndApp {
    pressed: usize,
    starts: usize,
    ends: Vec<DragOutcome>,
    hovered: Option<usize>,
    rejected: bool,
    dropped: Vec<(usize, [f32; 2])>,
    accept_id: Option<usize>,
}

fn dnd_view(_s: &DndApp, cx: &mut Cx<DndApp>) -> El {
    let active_tag = cx
        .active_drag()
        .and_then(|d| d.downcast_ref::<ItemPayload>())
        .map(|p| format!("dragging-{}", p.0));
    row()
        .w_full()
        .h_full()
        .children(active_tag.map(|k| el().key(k).absolute(0.0, 0.0).size(1.0, 1.0)))
        .child(
            el().key("source")
                .size(80.0, 40.0)
                .on_press(|s: &mut DndApp| s.pressed += 1)
                .draggable(|_: &DndApp| Some(ItemPayload(7)))
                .drag_preview_with(|_: &DndApp, _p: &ItemPayload| {
                    el().key("ghost").size(60.0, 30.0).fill(Color::WHITE).hit()
                })
                .on_drag_start(|s: &mut DndApp, _| s.starts += 1)
                .on_drag_end(|s: &mut DndApp, outcome, _| s.ends.push(outcome)),
        )
        .child(
            el().key("target")
                .size(120.0, 80.0)
                .accepts_drop(|s: &DndApp, p: &ItemPayload| s.accept_id.is_none_or(|id| id == p.0))
                .on_drop_target_hover(|s: &mut DndApp, p: Option<&ItemPayload>, _| {
                    s.hovered = p.map(|x| x.0);
                })
                .on_drop_target_rejected_hover(|s: &mut DndApp, p: Option<&ItemPayload>, _| {
                    s.rejected = p.is_some();
                })
                .on_drop(|s: &mut DndApp, p: &ItemPayload, tx| {
                    let pt = tx.drop_point().map(|e| [e.x, e.y]).unwrap_or([0.0, 0.0]);
                    s.dropped.push((p.0, pt));
                }),
        )
}

#[test]
fn click_within_threshold_fires_press_without_starting_drag() {
    let mut rt = Runtime::new(
        DndApp::default(),
        dnd_view,
        (300.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);
    rt.pointer_down(20.0, 20.0);
    rt.pointer_move(21.0, 21.0);
    rt.pointer_up(21.0, 21.0);
    rt.frame(0.016);
    assert_eq!(rt.state.pressed, 1);
    assert_eq!(rt.state.starts, 0);
    assert!(rt.state.ends.is_empty());
    assert!(!rt.is_dragging_payload());
}

#[test]
fn drag_preview_follows_pointer_and_drops_payload_on_target() {
    let mut rt = Runtime::new(
        DndApp::default(),
        dnd_view,
        (300.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);

    // Grab at (20, 20) inside source (0..80, 0..40) -> grab_offset = (20, 20).
    rt.pointer_down(20.0, 20.0);
    rt.frame(0.016);

    // Move across threshold into target (80..200, 0..80).
    rt.pointer_move(110.0, 30.0);
    rt.frame(0.016);
    assert!(rt.is_dragging_payload());
    assert_eq!(rt.state.starts, 1);
    assert_eq!(rt.state.hovered, Some(7));
    assert_eq!(rt.dnd_hovered().as_deref(), Some("target"));
    assert_eq!(rt.cursor(), crate::Cursor::Grabbing);

    // Preview sits at pointer - grab_offset = (110 - 20, 30 - 20) = (90, 10).
    let rect = rt.dnd_preview_rect().expect("preview mounted");
    assert!(
        (rect[0] - 90.0).abs() < 0.5 && (rect[1] - 10.0).abs() < 0.5,
        "got {rect:?}"
    );

    // Move further inside target — preview must follow and must NOT block target hit-testing.
    rt.pointer_move(140.0, 50.0);
    rt.frame(0.016);
    let rect2 = rt.dnd_preview_rect().expect("preview follows pointer");
    assert!(
        (rect2[0] - 120.0).abs() < 0.5 && (rect2[1] - 30.0).abs() < 0.5,
        "got {rect2:?}"
    );
    assert_eq!(rt.state.hovered, Some(7));

    // Drop on target at (140, 50) -> local (60, 50) on target (x=80..200).
    rt.pointer_up(140.0, 50.0);
    rt.frame(0.016);
    assert!(!rt.is_dragging_payload());
    assert!(
        rt.dnd_preview_rect().is_none(),
        "preview unmounts after drop"
    );
    assert_eq!(rt.state.hovered, None, "hover clears on drop");
    assert_eq!(rt.state.pressed, 0, "dragging suppresses click press");
    assert_eq!(rt.state.ends, vec![DragOutcome::Moved]);
    assert_eq!(rt.state.dropped.len(), 1);
    assert_eq!(rt.state.dropped[0].0, 7);
    assert!((rt.state.dropped[0].1[0] - 60.0).abs() < 0.5);
    assert!((rt.state.dropped[0].1[1] - 50.0).abs() < 0.5);
}

#[test]
fn escape_cancels_active_drag_and_clears_hover_and_preview() {
    let mut rt = Runtime::new(
        DndApp::default(),
        dnd_view,
        (300.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);

    rt.pointer_down(20.0, 20.0);
    rt.pointer_move(120.0, 30.0);
    rt.frame(0.016);
    assert!(rt.is_dragging_payload());
    assert_eq!(rt.state.hovered, Some(7));
    assert!(rt.dnd_preview_rect().is_some());

    // Pressing Escape cancels immediately and consumes the key.
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.016);
    assert!(!rt.is_dragging_payload());
    assert!(rt.dnd_preview_rect().is_none());
    assert_eq!(rt.state.hovered, None);
    assert!(rt.state.dropped.is_empty());
    assert_eq!(rt.state.ends, vec![DragOutcome::Cancelled]);

    // Subsequent pointer release does nothing.
    rt.pointer_up(120.0, 30.0);
    rt.frame(0.016);
    assert!(rt.state.dropped.is_empty());
    assert_eq!(rt.state.ends, vec![DragOutcome::Cancelled]);
}

#[test]
fn accepts_drop_rejects_mismatched_payload_and_reports_rejected_hover() {
    let mut rt = Runtime::new(
        DndApp {
            accept_id: Some(99),
            ..Default::default()
        },
        dnd_view,
        (300.0, 200.0),
        Env::new(true, 1.0),
    );
    rt.frame(0.0);

    rt.pointer_down(20.0, 20.0);
    rt.pointer_move(120.0, 30.0);
    rt.frame(0.016);
    assert_eq!(rt.state.hovered, None);
    assert!(rt.state.rejected);
    assert_eq!(rt.dnd_rejected_hovered().as_deref(), Some("target"));

    rt.pointer_up(120.0, 30.0);
    rt.frame(0.016);
    assert!(!rt.state.rejected);
    assert!(rt.state.dropped.is_empty());
    assert_eq!(rt.state.ends, vec![DragOutcome::Cancelled]);
}

#[test]
fn drop_reorder_helper_handles_lists_grids_and_folders() {
    let mut r = DropReorder::new(ReorderAxis::Vertical);
    r.start(0);
    r.hover_item(2, 35.0, 40.0, false);
    assert_eq!(r.placement(), Some(DropPlacement::After(2)));
    assert_eq!(r.insertion_index(), Some(3));
    assert_eq!(r.finish(), Some(ReorderAction::Move { from: 0, to: 2 }));

    let mut items = vec!["a", "b", "c", "d"];
    DropReorder::apply(&mut items, 0, 2);
    assert_eq!(items, ["b", "c", "a", "d"]);

    // Grid with folder drop-into zone: middle 50% drops into folder, edges reorder before/after.
    let mut g = DropReorder::new(ReorderAxis::Grid)
        .allow_into(true)
        .edge_fraction(0.25);
    g.start(1);
    g.hover_item(0, 50.0, 100.0, true);
    assert_eq!(g.placement(), Some(DropPlacement::Into(0)));
    assert_eq!(g.into_target(), Some(0));
    assert_eq!(g.insertion_index(), None);
    assert_eq!(
        g.finish(),
        Some(ReorderAction::DropInto { from: 1, target: 0 })
    );

    g.start(3);
    g.hover_item(0, 10.0, 100.0, true);
    assert_eq!(g.placement(), Some(DropPlacement::Before(0)));
    assert_eq!(g.finish(), Some(ReorderAction::Move { from: 3, to: 0 }));
    DropReorder::apply(&mut items, 3, 0);
    assert_eq!(items, ["d", "b", "c", "a"]);
}

#[derive(Clone, Copy)]
struct LibraryDrag(usize);

struct LibraryApp {
    items: Vec<(&'static str, bool)>,
    folders: Vec<(String, String)>,
    reorder: DropReorder,
}

fn library_view(s: &LibraryApp, _: &mut Cx<LibraryApp>) -> El {
    let mut list = col().w_full().h_full();
    for (i, &(name, is_folder)) in s.items.iter().enumerate() {
        let row_el = el()
            .key(name)
            .size(160.0, 40.0)
            .draggable(move |_: &LibraryApp| Some(LibraryDrag(i)))
            .on_drag_start(move |s: &mut LibraryApp, _| s.reorder.start(i))
            .on_drag_end(|s: &mut LibraryApp, outcome, _| {
                if outcome == DragOutcome::Cancelled {
                    s.reorder.cancel();
                }
            })
            .on_drop_target_move(move |s: &mut LibraryApp, _: &LibraryDrag, ev, _| {
                s.reorder.hover_point(i, ev, [160.0, 40.0], is_folder);
            })
            .on_drop_target_hover(move |s: &mut LibraryApp, over: Option<&LibraryDrag>, _| {
                if over.is_none() {
                    s.reorder.leave_item(i);
                }
            })
            .on_drop(move |s: &mut LibraryApp, drag: &LibraryDrag, _| {
                assert_eq!(s.reorder.dragging(), Some(drag.0));
                if let Some(action) = s.reorder.finish() {
                    match action {
                        ReorderAction::Move { from, to } => {
                            DropReorder::apply(&mut s.items, from, to)
                        }
                        ReorderAction::DropInto { from, target } => {
                            let item = s.items[from].0.to_owned();
                            let folder = s.items[target].0.to_owned();
                            s.folders.push((folder, item));
                        }
                    }
                }
            });
        list = list.child(s.reorder.wrap_item(i, row_el, Color::WHITE));
    }
    list
}

#[test]
fn library_list_end_to_end_reorder_and_drop_into_folder() {
    let app = LibraryApp {
        items: vec![("Folder", true), ("GameA", false), ("GameB", false)],
        folders: Vec::new(),
        reorder: DropReorder::new(ReorderAxis::Vertical).allow_into(true),
    };
    let mut rt = Runtime::new(app, library_view, (200.0, 200.0), Env::new(true, 1.0));
    rt.frame(0.0);

    // Drag GameB (y=80..120) to top edge of GameA (y=40..80, local y=4 -> Before(1)).
    rt.pointer_down(40.0, 100.0);
    rt.pointer_move(40.0, 44.0);
    rt.frame(0.016);
    assert_eq!(rt.state.reorder.placement(), Some(DropPlacement::Before(1)));
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.key.as_deref() == Some("__rune_dnd_indicator"))
    );

    rt.pointer_up(40.0, 44.0);
    rt.frame(0.016);
    let names: Vec<_> = rt.state.items.iter().map(|x| x.0).collect();
    assert_eq!(names, ["Folder", "GameB", "GameA"]);

    // Let reorder springs settle so GameA sits at y=80..120.
    rt.frame(1.0);

    // Drag GameA (now index 2, y=80..120) into middle of Folder (y=0..40, local y=20 -> Into(0)).
    rt.pointer_down(40.0, 100.0);
    rt.pointer_move(40.0, 20.0);
    rt.frame(0.016);
    assert_eq!(rt.state.reorder.placement(), Some(DropPlacement::Into(0)));

    rt.pointer_up(40.0, 20.0);
    rt.frame(0.016);
    assert_eq!(
        rt.state.folders,
        vec![("Folder".to_owned(), "GameA".to_owned())]
    );
}
