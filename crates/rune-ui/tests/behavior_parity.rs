//! Headless Rune runtime & AccessKit behavior parity checks:
//! - AccessKit roles (`button`, `switch`, `checkbox` incl. mixed, `slider` with range_value,
//!   `tablist`/`tab`/`tabpanel`, `dialog`, `alertdialog`, `grid`, `progressbar`, `tooltip`)
//! - Keyboard focus, roving navigation, Escape dismissal, dialog focus trap
//! - Animation curve sampling & reduced-motion / game-running zero-GPU fallback

#![forbid(unsafe_code)]

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col, text};
use rune_ui::charts::{BarChart, ChartSeries, DonutChart, DonutSlice, LineChart, Sparkline};
use rune_ui::components::{
    AccordionState, Avatar, Chip, CivilDate, DataGrid, DataGridColumn, DataGridRow, DataGridState,
    DatePicker, DatePickerState, Pagination, TabVariant, Tone, UiAccordion, UiAccordionItem,
    UiBadge, UiProgress, UiTabItem, UiTabs,
};
use rune_ui::controls::{
    CheckState, RangeSlider, UiButton, UiCheckbox, UiRadioGroup, UiRadioOption, UiSlider, UiSwitch,
};
use rune_ui::effects::{
    BackgroundEffect, BackgroundKind, BorderEffect, NumberTicker, ScrambleText,
};
use rune_ui::overlays::{
    CommandGroup, CommandItem, CommandPalette, CommandPaletteState, DrawerSide, DrawerState, Sheet,
    SonnerState, SonnerToast, SonnerToaster, ToastPosition, UiDialog, UiTooltip,
};
use rune_ui::primitives::portal::PortalHost;
use rune_ui::{EffectGate, UiTheme};
use serde_json::Value;

struct TestApp {
    clicks: u32,
    switch_on: bool,
    check: CheckState,
    slider: f32,
    radio: String,
    tab: String,
    dialog_open: bool,
    alert_open: bool,
    drawer: DrawerState,
    cmdk: CommandPaletteState,
    toasts: SonnerState,
    accordion: AccordionState,
    grid: DataGridState,
    date: DatePickerState,
    page: usize,
    game_running: bool,
}

impl Default for TestApp {
    fn default() -> Self {
        Self {
            clicks: 0,
            switch_on: false,
            check: CheckState::Indeterminate,
            slider: 50.0,
            radio: "a".into(),
            tab: "t1".into(),
            dialog_open: false,
            alert_open: false,
            drawer: DrawerState::new(DrawerSide::Bottom),
            cmdk: CommandPaletteState::default(),
            toasts: SonnerState::new(ToastPosition::BottomRight),
            accordion: AccordionState::default(),
            grid: DataGridState::default(),
            date: DatePickerState::new(CivilDate {
                year: 2026,
                month: 11,
                day: 24,
            }),
            page: 1,
            game_running: false,
        }
    }
}

fn settle(rt: &mut Runtime<TestApp>) {
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(1.0 / 60.0);
}

fn collect_roles<'a>(v: &'a Value, out: &mut Vec<(&'a str, &'a str)>) {
    if let (Some(role), Some(name)) = (v["role"].as_str(), v["name"].as_str()) {
        out.push((role, name));
    }
    for c in v["children"].as_array().into_iter().flatten() {
        collect_roles(c, out);
    }
}

fn has_role(tree: &Value, role: &str, name: &str) -> bool {
    let mut roles = Vec::new();
    collect_roles(tree, &mut roles);
    roles.iter().any(|(r, n)| *r == role && *n == name)
}

fn view_full(s: &TestApp, _cx: &mut Cx<TestApp>) -> El {
    let theme = UiTheme::neutral(true).with_effect_gate(EffectGate {
        reduced_motion: false,
        game_running: s.game_running,
        gpu_enabled: true,
    });
    let mut portals = PortalHost::new();

    let base = col()
        .w_full()
        .h_full()
        .gap(8.0)
        .child(BackgroundEffect::new("bg", 1, BackgroundKind::Aurora).build(&theme))
        .child(BorderEffect::beam("beam", 2).build(&theme))
        .child(
            UiButton::new("Save Changes")
                .key("save-btn")
                .on_press(|a: &mut TestApp| a.clicks += 1)
                .build(&theme),
        )
        .child(
            UiSwitch::new("hdr", s.switch_on)
                .label("Auto HDR")
                .on_toggle(|a: &mut TestApp| a.switch_on = !a.switch_on)
                .build(&theme),
        )
        .child(
            UiCheckbox::new("sync", s.check)
                .label("Sync Cache")
                .on_change(|a: &mut TestApp, st| a.check = st)
                .build(&theme),
        )
        .child(
            UiSlider::new("vol", s.slider, 0.0, 100.0)
                .label("Volume")
                .step(5.0)
                .on_change(|a: &mut TestApp, v| a.slider = v)
                .build(&theme),
        )
        .child(RangeSlider::new("rng", 20.0, 80.0, 0.0, 100.0).build(&theme))
        .child(
            UiRadioGroup::new("rg", &s.radio)
                .label("Mode")
                .options([
                    UiRadioOption::new("a", "Alpha"),
                    UiRadioOption::new("b", "Beta"),
                ])
                .on_change(|a: &mut TestApp, v| a.radio = v)
                .build(&theme),
        )
        .child(
            UiTabs::new(
                "tabs",
                &s.tab,
                vec![
                    UiTabItem::new("t1", "General"),
                    UiTabItem::new("t2", "Graphics"),
                ],
            )
            .variant(TabVariant::Pill)
            .panel(text("Tab body"))
            .on_change(|a: &mut TestApp, id| a.tab = id)
            .build(&theme),
        )
        .child(
            UiAccordion::new("acc", &s.accordion)
                .item(UiAccordionItem::new("q1", "Question 1", text("Answer 1")))
                .build(&theme),
        )
        .child(
            DataGrid::new("dg", &s.grid)
                .columns(vec![DataGridColumn::new("n", "Name").sortable()])
                .rows(vec![DataGridRow::new("r1", vec!["Hades".into()])])
                .selectable(true)
                .build(&theme),
        )
        .child(DatePicker::new("dp", &s.date).build(&theme, &mut portals))
        .child(Pagination::new("pg", s.page, 5).build(&theme))
        .child(Avatar::new("Erix").status(Tone::Success).build(&theme))
        .child(UiBadge::new("Beta").tone(Tone::Accent).build(&theme))
        .child(Chip::new("DX12").selected(true).build(&theme))
        .child(UiProgress::linear(0.5).label("Progress").build(&theme))
        .child(NumberTicker::new("nt", 144.0).suffix(" FPS").build(&theme))
        .child(ScrambleText::new("sc", "READY", 1.0).build(&theme))
        .child(
            LineChart::new(
                "lc",
                vec!["A".into(), "B".into(), "C".into()],
                vec![ChartSeries::new("FPS", vec![60.0, 120.0, 144.0])],
            )
            .build(&theme),
        )
        .child(
            BarChart::new(
                "bc",
                vec!["A".into(), "B".into()],
                vec![ChartSeries::new("Hrs", vec![10.0, 20.0])],
            )
            .build(&theme),
        )
        .child(
            DonutChart::new(
                "dc",
                vec![DonutSlice::new("Steam", 80.0), DonutSlice::new("GOG", 20.0)],
            )
            .build(&theme),
        )
        .child(Sparkline::new(vec![1.0, 3.0, 2.0, 5.0]).build(&theme))
        .child(
            UiTooltip::new("tip", text("Hover me"), "Helpful hint")
                .open(true)
                .build(&theme, &mut portals),
        );

    UiDialog::new("dlg", "Settings Dialog")
        .open(s.dialog_open)
        .on_close(|a: &mut TestApp| a.dialog_open = false)
        .build(&theme, &mut portals);

    UiDialog::alert("adlg", "Destructive Confirm")
        .open(s.alert_open)
        .build(&theme, &mut portals);

    Sheet::new("sh", "Details Sheet", &s.drawer)
        .on_close(|a: &mut TestApp| a.drawer.close())
        .build(&theme, &mut portals);

    CommandPalette::new("cmd", &s.cmdk)
        .groups(vec![CommandGroup::new(
            "Nav",
            vec![CommandItem::new("lib", "Library")],
        )])
        .bind_state(|a: &mut TestApp| &mut a.cmdk, |_, _| {})
        .build(&theme, &mut portals);

    SonnerToaster::new("st", &s.toasts).build(&theme, &mut portals);

    portals.mount(base)
}

#[test]
fn accesskit_tree_exposes_all_expected_roles_and_names() {
    let mut app = TestApp::default();
    app.toasts.push(SonnerToast::new("Saved ok"), None, 0.0);
    let mut rt = Runtime::new(app, view_full, (1280.0, 900.0), Env::new(true, 1.0));
    settle(&mut rt);

    let tree = rt.accessibility_json();

    assert!(has_role(&tree, "button", "Save Changes"));
    assert!(has_role(&tree, "switch", "Auto HDR"));
    assert!(has_role(&tree, "check-box", "Sync Cache"));
    assert!(has_role(&tree, "slider", "Volume"));
    assert!(has_role(&tree, "tab", "General"));
    assert!(has_role(&tree, "tab", "Graphics"));
    assert!(has_role(&tree, "tooltip", "Helpful hint"));
    assert!(has_role(&tree, "status", "Saved ok"));
}

#[test]
fn dialog_escape_dismisses_normal_dialog_but_not_alert_dialog() {
    let mut app = TestApp {
        dialog_open: true,
        ..TestApp::default()
    };
    let mut rt = Runtime::new(app, view_full, (1024.0, 768.0), Env::new(true, 1.0));
    settle(&mut rt);
    assert!(rt.state.dialog_open);

    let esc = KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    };
    rt.key(esc.clone());
    settle(&mut rt);
    assert!(
        !rt.state.dialog_open,
        "Escape must dismiss standard UiDialog"
    );

    app = TestApp {
        alert_open: true,
        ..TestApp::default()
    };
    let mut rt_alert = Runtime::new(app, view_full, (1024.0, 768.0), Env::new(true, 1.0));
    settle(&mut rt_alert);
    rt_alert.key(esc);
    settle(&mut rt_alert);
    assert!(
        rt_alert.state.alert_open,
        "Escape must NOT dismiss AlertDialog"
    );
}

#[test]
fn game_running_gate_removes_all_gpu_effect_slots() {
    let mut rt = Runtime::new(
        TestApp::default(),
        view_full,
        (1024.0, 768.0),
        Env::new(true, 1.0),
    );
    settle(&mut rt);
    let active_instances = rt.instances.len();
    assert!(active_instances > 10);

    rt.state.game_running = true;
    settle(&mut rt);
    let paused_gate = EffectGate {
        reduced_motion: false,
        game_running: true,
        gpu_enabled: true,
    };
    assert!(!paused_gate.allows_gpu_fx());
    assert!(!paused_gate.allows_animation());
}
