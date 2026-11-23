use std::cell::Cell;
use std::rc::Rc;

use crate::{Cx, El, Env, Runtime, col, el, row};

struct StripState {
    count: usize,
    built: Rc<Cell<usize>>,
    visible: Vec<std::ops::Range<usize>>,
}

fn strip_view(s: &StripState, _: &mut Cx<StripState>) -> El {
    let built = s.built.clone();
    col().w_full().h_full().child(
        row().key("scroller").w_full().h(120.0).scroll_x().child(
            row()
                .key("strip")
                .gap(10.0)
                .h(120.0)
                .on_visible_cols(|st: &mut StripState, range| {
                    st.visible.push(range);
                    false
                })
                .virtual_cols(
                    s.count,
                    100.0,
                    |_: &StripState, i| format!("shot-{i}").into(),
                    move |_: &StripState, i| {
                        built.set(built.get() + 1);
                        el().key(format!("shot-{i}")).w(100.0).h(100.0)
                    },
                ),
        ),
    )
}

#[test]
fn virtual_cols_mounts_only_window_and_updates_on_horizontal_scroll() {
    let built = Rc::new(Cell::new(0));
    let mut rt = Runtime::new(
        StripState {
            count: 200,
            built: built.clone(),
            visible: Vec::new(),
        },
        strip_view,
        (400.0, 200.0),
        Env::default(),
    );
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);

    let initial_range = rt.commit.virtuals["#0/scroller/strip"].range.clone();
    assert!(
        initial_range.len() < 40,
        "200-item horizontal strip should only mount viewport + overscan, got {initial_range:?}"
    );
    assert!(
        built.get() < 60,
        "offscreen horizontal strip items must not be built eagerly, built {}",
        built.get()
    );
    assert!(
        rt.state
            .visible
            .last()
            .is_some_and(|r| r.start == 0 && r.end >= 4 && r.end <= 6),
        "visible cols callback should report initial visible items, got {:?}",
        rt.state.visible
    );

    // Jump far down the strip via Tx::scroll_to_x and verify the window shifts and culls early items.
    rt.update(|_, tx| tx.scroll_to_x("scroller", 5500.0));
    for k in 2..10 {
        rt.frame(k as f64 / 60.0);
    }

    let shifted_range = rt.commit.virtuals["#0/scroller/strip"].range.clone();
    assert!(
        shifted_range.start >= 30 && shifted_range.contains(&50),
        "scrolling horizontally should move the mounted window to cover item 50, got {shifted_range:?}"
    );
    assert!(
        rt.commit.virtuals["#0/scroller/strip"].before > 2000.0,
        "culled items before the window should contribute left padding"
    );
    assert!(
        rt.state
            .visible
            .last()
            .is_some_and(|r| r.start >= 48 && r.end <= 56),
        "visible cols callback should track horizontal scroll, got {:?}",
        rt.state.visible.last()
    );
}

#[test]
fn virtual_cols_scroll_into_view_reaches_unmounted_item_and_handles_variable_widths() {
    fn var_view(s: &usize, _: &mut Cx<usize>) -> El {
        col()
            .w_full()
            .h_full()
            .child(row().key("scroller").w(400.0).h(120.0).scroll_x().child(
                row().key("strip").gap(8.0).h(120.0).virtual_cols(
                    *s,
                    120.0,
                    |_: &usize, i| format!("card-{i}").into(),
                    |_: &usize, i| {
                        let w = if i % 2 == 0 { 160.0 } else { 90.0 };
                        el().key(format!("card-{i}")).w(w).h(100.0)
                    },
                ),
            ))
    }

    let mut rt = Runtime::new(100usize, var_view, (400.0, 200.0), Env::default());
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
    assert!(!rt.commit.virtuals["#0/scroller/strip"].range.contains(&60));

    rt.update(|_, tx| {
        tx.instant();
        tx.scroll_into_view("card-60");
    });
    for k in 2..8 {
        rt.frame(k as f64 / 60.0);
    }

    assert!(
        rt.commit.virtuals["#0/scroller/strip"].range.contains(&60),
        "Tx::scroll_into_view must mount and reveal an unmounted horizontal virtual item, range={:?}",
        rt.commit.virtuals["#0/scroller/strip"].range
    );
    let card = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("card-60"))
        .expect("card-60 mounted");
    assert!(
        (card.rect[2] - 160.0).abs() < 0.5,
        "mounted variable-width card should lay out at its measured 160pt width, got {:?}",
        card.rect
    );
}
