//! `ui-gallery`: native Rune showcase for every `rune-ui` component family, state, motion
//! pattern, clean-room wgpu shader effect, and vector chart.
//!
//! ```sh
//! cargo run -p ui-gallery
//! cargo run -p ui-gallery -- --scene overview --snap overview.png --size 1920x1080
//! cargo run -p ui-gallery -- --scene controls --snap controls.png --size 1440x900
//! cargo run -p ui-gallery -- --scene effects --snap effects.png --size 1440x900
//! cargo run -p ui-gallery -- --scene charts --snap charts.png --size 1440x900
//! ```

#![forbid(unsafe_code)]

use rune::{
    AppSpec, Color, Cx, El, Key, KeyEvent, TextEdit, TextStyle, Tx, col, el, row, spacer, text,
};
use rune_kit::{Kit, State as VisualState};
use rune_ui::charts::{
    AreaChart, BarChart, ChartSeries, DonutChart, DonutSlice, LineChart, Sparkline, SparklineKind,
};
use rune_ui::components::{
    AccordionMode, AccordionState, Avatar, AvatarGroup, Carousel, CarouselState, Chip, CivilDate,
    DataGrid, DataGridColumn, DataGridRow, DataGridState, DatePicker, DatePickerState, Pagination,
    Skeleton, TabVariant, Tone, UiAccordion, UiAccordionItem, UiBadge, UiBadgeVariant, UiProgress,
    UiTabItem, UiTabs,
};
use rune_ui::controls::{
    CheckState, RangeSlider, SliderMark, UiButton, UiButtonVariant, UiCheckbox, UiInput,
    UiRadioGroup, UiRadioOption, UiSize, UiSlider, UiSwitch, UiTextArea,
};
use rune_ui::effects::{
    BackgroundEffect, BackgroundKind, BlurInText, BorderEffect, EffectPalette, GradientText,
    Marquee, NumberTicker, ScrambleText, ShinyText, SplitText, UiEffectsCompositor,
};
use rune_ui::motion::{MotionBox, MotionPreset};
use rune_ui::overlays::{
    CommandGroup, CommandItem, CommandPalette, CommandPaletteState, DrawerSide, DrawerState,
    DropdownMenu, Sheet, SonnerState, SonnerToast, SonnerToaster, ToastPosition, ToastVariant,
    UiCombobox, UiComboboxOption, UiComboboxState, UiDialog, UiMenuEntry, UiMenuItem, UiPopover,
    UiSelect, UiSelectOption, UiSelectState, UiTooltip,
};
use rune_ui::primitives::portal::PortalHost;
use rune_ui::{EffectGate, FocusRingPolicy, UiTheme};

struct Gallery {
    tab: String,
    reduced_motion: bool,
    game_running: bool,
    gpu_enabled: bool,
    name_input: TextEdit,
    pass_input: TextEdit,
    pass_visible: bool,
    notes_input: TextEdit,
    slider_val: f32,
    range_val: (f32, f32),
    switch_on: bool,
    check_state: CheckState,
    radio_val: String,
    select_val: Option<String>,
    select_state: UiSelectState,
    combo_state: UiComboboxState,
    menu_open: bool,
    popover_open: bool,
    tooltip_open: bool,
    dialog_open: bool,
    drawer_state: DrawerState,
    toasts: SonnerState,
    cmdk: CommandPaletteState,
    accordion: AccordionState,
    grid_state: DataGridState,
    date_state: DatePickerState,
    page: usize,
    carousel: CarouselState,
    ticker_val: f64,
    line_hover: Option<usize>,
    bar_hover: Option<usize>,
    donut_hover: Option<usize>,
    time_secs: f32,
}

impl Gallery {
    fn new() -> Self {
        let args = rune::extra_args();
        let tab = scene_arg(args).unwrap_or("overview").to_owned();
        let reduced_motion = args.iter().any(|a| a == "--reduced-motion");
        let game_running = args.iter().any(|a| a == "--game-running");
        let gpu_enabled = !args.iter().any(|a| a == "--no-gpu-fx");

        let name_input = TextEdit::new("Exo Native");
        let pass_input = TextEdit::new("hunter2").secure();
        let notes_input = TextEdit::new("Zero WebView2. 100% native Rust + wgpu DX12.");

        let mut combo_state = UiComboboxState::multi();
        combo_state.selected = vec!["dlss".into(), "hdr".into()];

        let mut toasts = SonnerState::new(ToastPosition::BottomRight);
        if tab == "overlays" {
            toasts.push(
                SonnerToast::new("Shaders compiled")
                    .description("12 pipelines cached in 14 ms")
                    .variant(ToastVariant::Success),
                None,
                0.0,
            );
            toasts.push(
                SonnerToast::new("Controller connected")
                    .description("DualSense Edge · USB")
                    .variant(ToastVariant::Info)
                    .action("Configure"),
                None,
                0.1,
            );
        }

        let mut accordion = AccordionState::new(AccordionMode::Single { collapsible: true });
        accordion.toggle("perf");

        let mut drawer_state =
            DrawerState::new(DrawerSide::Right).with_snap_points(vec![0.45, 0.85]);
        if tab == "overlays" {
            drawer_state.open = false;
        }

        let mut cmdk = CommandPaletteState::default();
        if tab == "cmdk" {
            cmdk.open_root();
        }

        let tooltip_open = tab == "overlays";

        Self {
            tab,
            reduced_motion,
            game_running,
            gpu_enabled,
            name_input,
            pass_input,
            pass_visible: false,
            notes_input,
            slider_val: 72.0,
            range_val: (24.0, 144.0),
            switch_on: true,
            check_state: CheckState::Indeterminate,
            radio_val: "balanced".into(),
            select_val: Some("dx12".into()),
            select_state: UiSelectState::new(),
            combo_state,
            menu_open: false,
            popover_open: false,
            tooltip_open,
            dialog_open: false,
            drawer_state,
            toasts,
            cmdk,
            accordion,
            grid_state: DataGridState {
                page_size: 4,
                ..DataGridState::default()
            },
            date_state: DatePickerState::new(CivilDate {
                year: 2026,
                month: 11,
                day: 24,
            }),
            page: 3,
            carousel: CarouselState::new(3),
            ticker_val: 240.0,
            line_hover: Some(3),
            bar_hover: Some(2),
            donut_hover: Some(0),
            time_secs: 1.25,
        }
    }

    fn gate(&self) -> EffectGate {
        EffectGate {
            reduced_motion: self.reduced_motion,
            game_running: self.game_running,
            gpu_enabled: self.gpu_enabled,
        }
    }

    fn theme(&self) -> UiTheme {
        UiTheme::from_kit(Kit::neutral(true))
            .with_focus_ring(FocusRingPolicy::SubtleFill {
                fill: Color::WHITE.alpha(0.08),
                border: Color::WHITE.alpha(0.18),
            })
            .with_effect_gate(self.gate())
    }
}

fn scene_arg(args: &[String]) -> Option<&str> {
    args.windows(2)
        .find(|w| w[0] == "--scene")
        .map(|w| w[1].as_str())
}

fn section_card(title: &str, body: El, theme: &UiTheme) -> El {
    let k = &theme.kit;
    col()
        .w_full()
        .padding(16.0)
        .gap(12.0)
        .radius(theme.radii.card)
        .fill(Color {
            r: 0.055,
            g: 0.058,
            b: 0.066,
            a: 1.0,
        })
        .border(1.0, theme.float_line)
        .child(
            text(title.to_owned())
                .font(k.ty.ui_medium)
                .color(k.c.text_base),
        )
        .child(body)
}

fn controls_section(s: &Gallery, theme: &UiTheme) -> El {
    let buttons = row()
        .w_full()
        .gap(8.0)
        .wrap()
        .center_y()
        .child(
            UiButton::new("Primary")
                .variant(UiButtonVariant::Primary)
                .on_press(|g: &mut Gallery| g.ticker_val += 15.0)
                .build(theme),
        )
        .child(
            UiButton::new("Secondary")
                .variant(UiButtonVariant::Secondary)
                .build(theme),
        )
        .child(
            UiButton::new("Outline")
                .variant(UiButtonVariant::Outline)
                .build(theme),
        )
        .child(
            UiButton::new("Ghost")
                .variant(UiButtonVariant::Ghost)
                .build(theme),
        )
        .child(
            UiButton::new("Danger")
                .variant(UiButtonVariant::Danger)
                .build(theme),
        )
        .child(
            UiButton::new("Loading")
                .variant(UiButtonVariant::Secondary)
                .loading(true)
                .build(theme),
        )
        .child(
            UiButton::new("Disabled")
                .size(UiSize::Sm)
                .disabled(true)
                .build(theme),
        )
        .child(
            UiButton::new("Hover")
                .variant(UiButtonVariant::Secondary)
                .state(VisualState::Hover)
                .build(theme),
        );

    let inputs = row()
        .gap(12.0)
        .w_full()
        .child(
            col()
                .w(260.0)
                .gap(10.0)
                .child(
                    UiInput::new("name", &s.name_input)
                        .label("Display name")
                        .placeholder("Enter name")
                        .clearable(true)
                        .max_len(32)
                        .on_edit(|g: &mut Gallery| &mut g.name_input)
                        .on_clear(|g: &mut Gallery| g.name_input.clear())
                        .build(theme),
                )
                .child(
                    UiInput::new("pass", &s.pass_input)
                        .label("API token")
                        .password(true, s.pass_visible)
                        .description("Stored only in native host keychain")
                        .on_edit(|g: &mut Gallery| &mut g.pass_input)
                        .on_toggle_password(|g: &mut Gallery| g.pass_visible = !g.pass_visible)
                        .build(theme),
                ),
        )
        .child(
            col().w(280.0).child(
                UiTextArea::new("notes", &s.notes_input)
                    .label("Release notes")
                    .max_len(140)
                    .on_edit(|g: &mut Gallery| &mut g.notes_input)
                    .build(theme),
            ),
        );

    let sliders = row()
        .gap(20.0)
        .child(
            UiSlider::new("vol", s.slider_val, 0.0, 100.0)
                .label("Master volume")
                .step(1.0)
                .show_value("%")
                .marks([
                    SliderMark::new(0.0).label("0"),
                    SliderMark::new(50.0).label("50"),
                    SliderMark::new(100.0).label("100"),
                ])
                .on_change(|g: &mut Gallery, v| g.slider_val = v)
                .build(theme),
        )
        .child(
            RangeSlider::new("fps-range", s.range_val.0, s.range_val.1, 0.0, 240.0)
                .label("Frame rate window (FPS)")
                .step(4.0)
                .min_gap(16.0)
                .on_change(|g: &mut Gallery, r| g.range_val = r)
                .build(theme),
        );

    let toggles = row()
        .w_full()
        .gap(18.0)
        .wrap()
        .items_start()
        .child(
            col()
                .gap(12.0)
                .child(
                    UiSwitch::new("hdr-sw", s.switch_on)
                        .label("Auto HDR")
                        .description("10-bit scRGB output on supported displays")
                        .on_toggle(|g: &mut Gallery| g.switch_on = !g.switch_on)
                        .build(theme),
                )
                .child(
                    UiCheckbox::new("tri-cb", s.check_state)
                        .label("Sync shader cache")
                        .description("Tri-state checkbox (indeterminate -> checked -> unchecked)")
                        .on_change(|g: &mut Gallery, next| g.check_state = next)
                        .build(theme),
                ),
        )
        .child(
            UiRadioGroup::new("power-mode", &s.radio_val)
                .label("Power profile")
                .horizontal()
                .card_style(true)
                .options([
                    UiRadioOption::new("quiet", "Quiet").description("Lower fan curve"),
                    UiRadioOption::new("balanced", "Balanced").description("Adaptive boost"),
                    UiRadioOption::new("turbo", "Max FPS").description("Unlocked TDP"),
                ])
                .on_change(|g: &mut Gallery, v| g.radio_val = v)
                .build(theme),
        );

    col()
        .gap(14.0)
        .child(section_card(
            "Buttons (HeroUI / shadcn variants & states)",
            buttons,
            theme,
        ))
        .child(section_card("Inputs & TextArea", inputs, theme))
        .child(section_card(
            "Sliders (Radix single & dual-thumb range)",
            sliders,
            theme,
        ))
        .child(section_card(
            "Switch, Tri-State Checkbox & RadioGroup",
            toggles,
            theme,
        ))
}

fn overlays_section(s: &Gallery, theme: &UiTheme, portals: &mut PortalHost) -> El {
    let select_el = UiSelect::new("backend-sel", s.select_val.clone())
        .options(vec![
            UiSelectOption::new("dx12", "Direct3D 12").group("Windows"),
            UiSelectOption::new("vulkan", "Vulkan 1.3").group("Cross-platform"),
            UiSelectOption::new("warp", "WARP Software")
                .group("Fallback")
                .disabled(true),
        ])
        .open(&s.select_state)
        .on_toggle(|g: &mut Gallery| g.select_state.open = !g.select_state.open)
        .on_close(|g: &mut Gallery| g.select_state.close())
        .on_select(|g: &mut Gallery, v| {
            g.select_val = Some(v);
            g.select_state.close();
        })
        .build(theme, portals);

    let combo_el = UiCombobox::new("feat-combo", &s.combo_state)
        .options(vec![
            UiComboboxOption::new("dlss", "DLSS 4"),
            UiComboboxOption::new("hdr", "HDR10"),
            UiComboboxOption::new("reflex", "NVIDIA Reflex"),
            UiComboboxOption::new("vrr", "G-SYNC / VRR"),
        ])
        .placeholder("Filter graphics features...")
        .bind_state(|g: &mut Gallery| &mut g.combo_state)
        .build(theme, portals);

    let menu_trigger = UiButton::new("Dropdown Menu")
        .variant(UiButtonVariant::Secondary)
        .on_press(|g: &mut Gallery| g.menu_open = !g.menu_open)
        .build(theme);
    let menu_el = DropdownMenu::new("actions-menu", menu_trigger)
        .open(s.menu_open)
        .entries(vec![
            UiMenuEntry::Label("Game actions".into()),
            UiMenuEntry::item(
                UiMenuItem::new("Play now")
                    .shortcut("Enter")
                    .on_select(|g: &mut Gallery| g.menu_open = false),
            ),
            UiMenuEntry::item(
                UiMenuItem::new("Pin to top")
                    .checked(true)
                    .on_select(|g: &mut Gallery| g.menu_open = false),
            ),
            UiMenuEntry::Separator,
            UiMenuEntry::item(
                UiMenuItem::new("Uninstall")
                    .danger(true)
                    .shortcut("Del")
                    .on_select(|g: &mut Gallery| g.menu_open = false),
            ),
        ])
        .on_close(|g: &mut Gallery| g.menu_open = false)
        .build(theme, portals);

    let pop_trigger = UiButton::new("Popover")
        .variant(UiButtonVariant::Outline)
        .on_press(|g: &mut Gallery| g.popover_open = !g.popover_open)
        .build(theme);
    let pop_el = UiPopover::new(
        "info-pop",
        pop_trigger,
        col()
            .w(220.0)
            .gap(6.0)
            .child(
                text("Anchored popover")
                    .font(theme.kit.ty.ui_medium)
                    .color(theme.kit.c.text_base),
            )
            .child(
                text("Flips and shifts automatically inside viewport bounds.")
                    .font(theme.kit.ty.small)
                    .color(theme.kit.c.text_muted)
                    .wrap(),
            ),
    )
    .open(s.popover_open)
    .on_close(|g: &mut Gallery| g.popover_open = false)
    .build(theme, portals);

    let tip_trigger = UiButton::new("Tooltip")
        .variant(UiButtonVariant::Ghost)
        .on_press(|g: &mut Gallery| g.tooltip_open = !g.tooltip_open)
        .build(theme);
    let tip_el = UiTooltip::new("demo-tip", tip_trigger, "Keyboard shortcut: Ctrl+K")
        .open(s.tooltip_open)
        .build(theme, portals);

    let triggers = row()
        .w_full()
        .gap(10.0)
        .wrap()
        .center_y()
        .child(select_el)
        .child(combo_el)
        .child(menu_el)
        .child(pop_el)
        .child(tip_el)
        .child(
            UiButton::new("Open Dialog")
                .variant(UiButtonVariant::Primary)
                .on_press(|g: &mut Gallery| g.dialog_open = true)
                .build(theme),
        )
        .child(
            UiButton::new("Open Vaul Sheet")
                .variant(UiButtonVariant::Secondary)
                .on_press(|g: &mut Gallery| g.drawer_state.open_at(0))
                .build(theme),
        )
        .child(
            UiButton::new("Command Palette (Ctrl+K)")
                .variant(UiButtonVariant::Outline)
                .on_press(|g: &mut Gallery| g.cmdk.open_root())
                .build(theme),
        )
        .child(
            UiButton::new("Push Sonner Toast")
                .variant(UiButtonVariant::Secondary)
                .on_press_tx(|g: &mut Gallery, tx: &mut Tx| {
                    g.toasts.push(
                        SonnerToast::new("Profile synced")
                            .description("Saved in 2 ms over named pipe")
                            .variant(ToastVariant::Success),
                        Some(4.0),
                        tx.clock(),
                    );
                })
                .build(theme),
        );

    UiDialog::new("demo-dlg", "Confirm shader reset")
        .description("Rebuilding the pipeline cache clears compiled DX12 PSO entries.")
        .open(s.dialog_open)
        .on_close(|g: &mut Gallery| g.dialog_open = false)
        .child(
            text("This takes under 2 seconds on the next launch.")
                .font(theme.kit.ty.ui)
                .color(theme.kit.c.text_muted),
        )
        .footer_action(
            UiButton::new("Cancel")
                .variant(UiButtonVariant::Ghost)
                .on_press(|g: &mut Gallery| g.dialog_open = false)
                .build(theme),
        )
        .footer_action(
            UiButton::new("Reset cache")
                .variant(UiButtonVariant::Danger)
                .on_press(|g: &mut Gallery| g.dialog_open = false)
                .build(theme),
        )
        .build(theme, portals);

    Sheet::new(
        "demo-sheet",
        "Game Properties (Vaul physics)",
        &s.drawer_state,
    )
    .description("Drag handle to snap between 45% and 85% or fling to dismiss.")
    .on_close(|g: &mut Gallery| g.drawer_state.close())
    .bind_drag(|g: &mut Gallery| &mut g.drawer_state, 900.0)
    .child(
        text("Launch arguments, anti-cheat status, and upscaler overrides live here.")
            .font(theme.kit.ty.ui)
            .color(theme.kit.c.text_muted)
            .wrap(),
    )
    .footer(
        UiButton::new("Done")
            .variant(UiButtonVariant::Primary)
            .on_press(|g: &mut Gallery| g.drawer_state.close())
            .build(theme),
    )
    .build(theme, portals);

    CommandPalette::new("demo-cmdk", &s.cmdk)
        .groups(vec![
            CommandGroup::new(
                "Navigation",
                vec![
                    CommandItem::new("library", "Go to Library").shortcut("G L"),
                    CommandItem::new("store", "Browse Store").shortcut("G S"),
                    CommandItem::new("pc", "Open PC Performance").shortcut("G P"),
                ],
            ),
            CommandGroup::new(
                "Quick Actions",
                vec![
                    CommandItem::new("hdr", "Toggle Auto HDR").keywords(["display", "scrgb"]),
                    CommandItem::new("game-mode", "Toggle Game-Running Pause")
                        .keywords(["fx", "gpu"]),
                ],
            ),
        ])
        .bind_state(
            |g: &mut Gallery| &mut g.cmdk,
            |g: &mut Gallery, id| {
                if id == "game-mode" {
                    g.game_running = !g.game_running;
                }
            },
        )
        .build(theme, portals);

    SonnerToaster::new("demo-sonner", &s.toasts)
        .bind_state(|g: &mut Gallery| &mut g.toasts, |_, _| {})
        .build(theme, portals);

    section_card(
        "Overlays, Menus, Vaul Sheet, Sonner Stack & cmdk Palette",
        triggers,
        theme,
    )
}

fn components_section(s: &Gallery, theme: &UiTheme, portals: &mut PortalHost) -> El {
    let badges_avatars = row()
        .gap(18.0)
        .wrap()
        .center_y()
        .child(
            Avatar::new("Erix Dev")
                .size(40.0)
                .status(Tone::Success)
                .build(theme),
        )
        .child(
            AvatarGroup::new(vec![
                Avatar::new("Alex K"),
                Avatar::new("Sam R"),
                Avatar::new("Maya L"),
                Avatar::new("Chris P"),
                Avatar::new("Jordan T"),
            ])
            .max(3)
            .size(34.0)
            .build(theme),
        )
        .child(
            UiBadge::new("Native DX12")
                .tone(Tone::Accent)
                .variant(UiBadgeVariant::Soft)
                .build(theme),
        )
        .child(
            UiBadge::new("Online")
                .tone(Tone::Success)
                .variant(UiBadgeVariant::Dot)
                .build(theme),
        )
        .child(
            UiBadge::new("240 Hz")
                .tone(Tone::Info)
                .variant(UiBadgeVariant::Outline)
                .build(theme),
        )
        .child(
            Chip::new("Steam")
                .selected(true)
                .tone(Tone::Accent)
                .build(theme),
        )
        .child(
            Chip::new("Ray Tracing")
                .on_remove(|_: &mut Gallery| {})
                .build(theme),
        )
        .child(
            UiProgress::circular(48.0, 0.78)
                .tone(Tone::Success)
                .build(theme),
        )
        .child(
            UiProgress::linear(0.64)
                .width(160.0)
                .label("Shader cache")
                .build(theme),
        )
        .child(Skeleton::text_lines(140.0, 2).build(theme));

    let grid = DataGrid::new("games-grid", &s.grid_state)
        .columns(vec![
            DataGridColumn::new("title", "Title").sortable(),
            DataGridColumn::new("store", "Store")
                .width(110.0)
                .sortable(),
            DataGridColumn::new("hours", "Hours").width(90.0).sortable(),
            DataGridColumn::new("fps", "Avg FPS").width(90.0).sortable(),
        ])
        .rows(vec![
            DataGridRow::new(
                "er",
                vec![
                    "Elden Ring".into(),
                    "Steam".into(),
                    "142".into(),
                    "120".into(),
                ],
            ),
            DataGridRow::new(
                "cp",
                vec![
                    "Cyberpunk 2077".into(),
                    "GOG".into(),
                    "96".into(),
                    "138".into(),
                ],
            ),
            DataGridRow::new(
                "ha",
                vec!["Hades II".into(), "Steam".into(), "64".into(), "240".into()],
            ),
            DataGridRow::new(
                "ba",
                vec!["Balatro".into(), "Steam".into(), "88".into(), "240".into()],
            ),
        ])
        .selectable(true)
        .striped(true)
        .bind_state(|g: &mut Gallery| &mut g.grid_state)
        .build(theme);

    let acc = UiAccordion::new("demo-acc", &s.accordion)
        .item(UiAccordionItem::new(
            "perf",
            "Zero-WebView architecture",
            text("Direct wgpu DX12 rendering keeps idle memory under 60 MB and frame times under 1 ms.")
                .font(theme.kit.ty.small)
                .color(theme.kit.c.text_muted)
                .w_full()
                .wrap(),
        ))
        .item(UiAccordionItem::new(
            "a11y",
            "AccessKit & Controller support",
            text("Every control exposes UIA roles and shares Keyboard + Gamepad navigation.")
                .font(theme.kit.ty.small)
                .color(theme.kit.c.text_muted)
                .w_full()
                .wrap(),
        ))
        .bind_state(|g: &mut Gallery| &mut g.accordion)
        .build(theme);

    let date_el = DatePicker::new("capture-date", &s.date_state)
        .bind_state(|g: &mut Gallery| &mut g.date_state)
        .build(theme, portals);

    let pager = Pagination::new("demo-pager", s.page, 12)
        .on_change(|g: &mut Gallery, p| g.page = p)
        .build(theme);

    let carousel = Carousel::new(
        "hero-carousel",
        &s.carousel,
        vec![
            el().w_full()
                .h_full()
                .center()
                .fill(Color {
                    r: 0.08,
                    g: 0.14,
                    b: 0.24,
                    a: 1.0,
                })
                .child(
                    text("Slide 1 · Featured Release")
                        .font(theme.kit.ty.ui_medium)
                        .color(Color::WHITE),
                ),
            el().w_full()
                .h_full()
                .center()
                .fill(Color {
                    r: 0.14,
                    g: 0.09,
                    b: 0.24,
                    a: 1.0,
                })
                .child(
                    text("Slide 2 · Liquid Glass Showcase")
                        .font(theme.kit.ty.ui_medium)
                        .color(Color::WHITE),
                ),
            el().w_full()
                .h_full()
                .center()
                .fill(Color {
                    r: 0.07,
                    g: 0.19,
                    b: 0.17,
                    a: 1.0,
                })
                .child(
                    text("Slide 3 · Telemetry & Charts")
                        .font(theme.kit.ty.ui_medium)
                        .color(Color::WHITE),
                ),
        ],
    )
    .size(380.0, 130.0)
    .bind_state(|g: &mut Gallery| &mut g.carousel)
    .build(theme);

    col()
        .gap(14.0)
        .child(section_card(
            "Avatars, Badges, Chips, Progress & Skeletons",
            badges_avatars,
            theme,
        ))
        .child(
            row()
                .gap(14.0)
                .w_full()
                .items_start()
                .child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .gap(10.0)
                        .child(section_card(
                            "DataGrid (sortable, selectable, striped)",
                            grid,
                            theme,
                        ))
                        .child(row().gap(14.0).center_y().child(date_el).child(pager)),
                )
                .child(
                    col()
                        .w(420.0)
                        .gap(14.0)
                        .child(section_card("Accordion", acc, theme))
                        .child(section_card(
                            "Carousel (drag / fling / loop)",
                            carousel,
                            theme,
                        )),
                ),
        )
}

fn effects_section(s: &Gallery, theme: &UiTheme, time_secs: f32) -> El {
    let fx_card = |id: u16, label: &str, kind: BackgroundKind| {
        el().key(format!("fx-bg-{id}"))
            .size(210.0, 110.0)
            .radius(theme.radii.card)
            .fill(Color {
                r: 0.03,
                g: 0.035,
                b: 0.05,
                a: 1.0,
            })
            .border(1.0, theme.float_line)
            .child(
                BackgroundEffect::new(format!("bg-slot-{id}"), id, kind)
                    .radius(theme.radii.card)
                    .build(theme),
            )
            .child(
                el().absolute(12.0, 10.0).child(
                    text(label.to_owned())
                        .font(theme.kit.ty.small_medium)
                        .color(Color::WHITE),
                ),
            )
    };

    let bgs = row()
        .w_full()
        .gap(12.0)
        .wrap()
        .child(fx_card(1, "Aurora (wgpu)", BackgroundKind::Aurora))
        .child(fx_card(2, "Particles (wgpu)", BackgroundKind::Particles))
        .child(fx_card(3, "Waves (wgpu)", BackgroundKind::Waves))
        .child(fx_card(4, "Grid + Pulse (wgpu)", BackgroundKind::Grid))
        .child(fx_card(
            7,
            "Iridescence (wgpu)",
            BackgroundKind::Iridescence,
        ))
        .child(
            el().key("fx-beam-card")
                .size(210.0, 110.0)
                .radius(theme.radii.card)
                .fill(Color {
                    r: 0.04,
                    g: 0.045,
                    b: 0.055,
                    a: 1.0,
                })
                .center()
                .child(
                    BorderEffect::beam("beam-slot", 5)
                        .radius(theme.radii.card)
                        .build(theme),
                )
                .child(
                    text("BorderBeam (wgpu SDF)")
                        .font(theme.kit.ty.small_medium)
                        .color(Color::WHITE),
                ),
        )
        .child(
            el().key("fx-glow-card")
                .size(210.0, 110.0)
                .radius(theme.radii.card)
                .fill(Color {
                    r: 0.04,
                    g: 0.045,
                    b: 0.055,
                    a: 1.0,
                })
                .center()
                .child(
                    BorderEffect::glow("glow-slot", 6)
                        .radius(theme.radii.card)
                        .build(theme),
                )
                .child(
                    text("GlowBorder (wgpu SDF)")
                        .font(theme.kit.ty.small_medium)
                        .color(Color::WHITE),
                ),
        )
        .child(
            el().key("fx-spot-card")
                .size(210.0, 110.0)
                .radius(theme.radii.card)
                .fill(Color {
                    r: 0.04,
                    g: 0.045,
                    b: 0.055,
                    a: 1.0,
                })
                .center()
                .child(
                    BorderEffect::spotlight("spot-slot", 8)
                        .radius(theme.radii.card)
                        .build(theme),
                )
                .child(
                    text("SpotlightCard Rim (wgpu)")
                        .font(theme.kit.ty.small_medium)
                        .color(Color::WHITE),
                ),
        );

    let title_style = TextStyle::new(20.0, 650.0);
    let text_fx = row()
        .gap(20.0)
        .wrap()
        .center_y()
        .child(
            BlurInText::new("blur-in", "Blur-In Reveal")
                .font(title_style)
                .build(theme),
        )
        .child(
            SplitText::new("split-txt", "Split Reveal")
                .by_word()
                .font(title_style)
                .build(theme),
        )
        .child(
            ScrambleText::new("scram", "DECRYPTED 240HZ", (time_secs * 0.4).min(1.0))
                .font(title_style)
                .build(theme),
        )
        .child(
            GradientText::new(
                "Gradient Text",
                vec![
                    Color {
                        r: 0.35,
                        g: 0.70,
                        b: 1.0,
                        a: 1.0,
                    },
                    Color {
                        r: 0.72,
                        g: 0.42,
                        b: 1.0,
                        a: 1.0,
                    },
                    Color {
                        r: 0.25,
                        g: 0.92,
                        b: 0.80,
                        a: 1.0,
                    },
                ],
            )
            .font(title_style)
            .phase(time_secs)
            .build(theme),
        )
        .child(
            ShinyText::new("Shiny Specular Text", time_secs)
                .font(title_style)
                .build(theme),
        )
        .child(
            NumberTicker::new("fps-tick", s.ticker_val)
                .suffix(" FPS")
                .font(title_style)
                .build(theme),
        );

    let marquee = Marquee::new("demo-marquee", 880.0, 620.0, time_secs, || {
        [
            "HeroUI",
            "shadcn/ui",
            "Radix",
            "cmdk",
            "Sonner",
            "Vaul",
            "Mantine",
            "Tremor",
            "Recharts",
            "Motion",
        ]
        .into_iter()
        .map(|name| {
            UiBadge::new(name)
                .tone(Tone::Neutral)
                .variant(UiBadgeVariant::Outline)
                .build(theme)
        })
        .collect()
    })
    .build(theme);

    let gate_bar = row()
        .gap(16.0)
        .center_y()
        .child(
            UiSwitch::new("game-pause", s.game_running)
                .label("Game running (pauses all GPU fx)")
                .on_toggle(|g: &mut Gallery| g.game_running = !g.game_running)
                .build(theme),
        )
        .child(
            UiSwitch::new("red-motion", s.reduced_motion)
                .label("Reduce motion (static fallback)")
                .on_toggle(|g: &mut Gallery| g.reduced_motion = !g.reduced_motion)
                .build(theme),
        );

    col()
        .gap(14.0)
        .child(section_card(
            "GPU Shader Gate (zero-GPU fallback & game-running pause)",
            gate_bar,
            theme,
        ))
        .child(section_card(
            "Clean-Room wgpu Backgrounds & SDF Borders",
            bgs,
            theme,
        ))
        .child(section_card("Text Effects & Number Ticker", text_fx, theme))
        .child(section_card("Infinite Marquee", marquee, theme))
}

fn charts_section(s: &Gallery, theme: &UiTheme) -> El {
    let cats = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

    let line = LineChart::new(
        "fps-line",
        cats.clone(),
        vec![
            ChartSeries::new(
                "Avg FPS",
                vec![118.0, 126.0, 134.0, 142.0, 139.0, 155.0, 162.0],
            ),
            ChartSeries::new("1% Low", vec![82.0, 88.0, 91.0, 98.0, 94.0, 108.0, 114.0]),
        ],
    )
    .size(480.0, 210.0)
    .unit(" fps")
    .hovered(s.line_hover)
    .on_hover(|g: &mut Gallery, i| g.line_hover = i)
    .build(theme);

    let area = AreaChart::area_chart(
        "vram-area",
        cats.clone(),
        vec![
            ChartSeries::new("Textures", vec![4.2, 4.8, 5.1, 5.6, 5.4, 6.2, 6.0]),
            ChartSeries::new("RT BVH", vec![1.4, 1.6, 1.8, 2.0, 1.9, 2.3, 2.2]),
        ],
    )
    .stacked(true)
    .size(480.0, 210.0)
    .unit(" GB")
    .build(theme);

    let bar = BarChart::new(
        "playtime-bar",
        cats,
        vec![
            ChartSeries::new("Steam", vec![2.4, 3.1, 1.8, 4.2, 5.0, 6.4, 5.2]),
            ChartSeries::new("GOG", vec![0.8, 1.2, 0.5, 1.1, 1.8, 2.2, 1.6]),
        ],
    )
    .size(480.0, 210.0)
    .unit("h")
    .hovered(s.bar_hover)
    .on_hover(|g: &mut Gallery, i| g.bar_hover = i)
    .build(theme);

    let donut = DonutChart::new(
        "storage-donut",
        vec![
            DonutSlice::new("Steam Library", 640.0),
            DonutSlice::new("Epic Games", 210.0),
            DonutSlice::new("GOG & Local", 145.0),
            DonutSlice::new("Shader Cache", 38.0),
        ],
    )
    .diameter(172.0)
    .center_kpi("1.03 TB", "Used")
    .unit(" GB")
    .hovered(s.donut_hover)
    .on_hover(|g: &mut Gallery, i| g.donut_hover = i)
    .build(theme);

    let sparks = row()
        .gap(24.0)
        .center_y()
        .child(
            col()
                .gap(4.0)
                .child(
                    text("GPU Temp (Area)")
                        .font(theme.kit.ty.small)
                        .color(theme.kit.c.text_muted),
                )
                .child(
                    Sparkline::new(vec![54.0, 58.0, 62.0, 65.0, 64.0, 67.0, 63.0, 61.0])
                        .kind(SparklineKind::Area)
                        .build(theme),
                ),
        )
        .child(
            col()
                .gap(4.0)
                .child(
                    text("Frame Time (Line)")
                        .font(theme.kit.ty.small)
                        .color(theme.kit.c.text_muted),
                )
                .child(
                    Sparkline::new(vec![7.2, 6.8, 6.9, 6.5, 6.4, 6.7, 6.3, 6.2])
                        .kind(SparklineKind::Line)
                        .build(theme),
                ),
        )
        .child(
            col()
                .gap(4.0)
                .child(
                    text("Download MB/s (Bar)")
                        .font(theme.kit.ty.small)
                        .color(theme.kit.c.text_muted),
                )
                .child(
                    Sparkline::new(vec![42.0, 68.0, 95.0, 112.0, 108.0, 118.0, 94.0, 115.0])
                        .kind(SparklineKind::Bar)
                        .build(theme),
                ),
        );

    col()
        .gap(14.0)
        .child(
            row()
                .gap(14.0)
                .wrap()
                .child(section_card(
                    "LineChart (Monotone Cubic + Tooltip)",
                    line,
                    theme,
                ))
                .child(section_card("AreaChart (Stacked Telemetry)", area, theme)),
        )
        .child(
            row()
                .gap(14.0)
                .wrap()
                .child(section_card("BarChart (Grouped Playtime)", bar, theme))
                .child(section_card(
                    "DonutChart (Center KPI + Interactive Legend)",
                    donut,
                    theme,
                )),
        )
        .child(section_card("Sparklines (Area, Line & Bar)", sparks, theme))
}

fn view(s: &Gallery, _cx: &mut Cx<Gallery>) -> El {
    let theme = s.theme();
    let k = &theme.kit;
    let mut portals = PortalHost::new();
    let t = s.time_secs;

    let tabs = UiTabs::new(
        "gallery-nav",
        &s.tab,
        vec![
            UiTabItem::new("overview", "Overview"),
            UiTabItem::new("controls", "Controls"),
            UiTabItem::new("overlays", "Overlays"),
            UiTabItem::new("components", "Components"),
            UiTabItem::new("effects", "Effects"),
            UiTabItem::new("charts", "Charts"),
        ],
    )
    .variant(TabVariant::Segmented)
    .on_change(|g: &mut Gallery, id| g.tab = id)
    .build(&theme);

    let header = row()
        .w_full()
        .justify_between()
        .center_y()
        .child(
            row()
                .gap(10.0)
                .center_y()
                .child(
                    text("rune-ui Gallery")
                        .font(TextStyle::new(20.0, 650.0))
                        .color(Color::WHITE),
                )
                .child(
                    UiBadge::new("Native Rust + wgpu")
                        .tone(Tone::Accent)
                        .build(&theme),
                ),
        )
        .child(tabs)
        .child(spacer());

    let content = match s.tab.as_str() {
        "controls" => controls_section(s, &theme),
        "overlays" | "cmdk" => overlays_section(s, &theme, &mut portals),
        "components" => components_section(s, &theme, &mut portals),
        "effects" => effects_section(s, &theme, t),
        "charts" => charts_section(s, &theme),
        _ => col().gap(14.0).child(
            row()
                .gap(14.0)
                .items_start()
                .child(col().w(600.0).gap(14.0).child(controls_section(s, &theme)))
                .child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .gap(14.0)
                        .child(overlays_section(s, &theme, &mut portals))
                        .child(effects_section(s, &theme, t))
                        .child(charts_section(s, &theme)),
                ),
        ),
    };

    let page = col()
        .w_full()
        .h_full()
        .padding(20.0)
        .gap(16.0)
        .fill(Color::BLACK)
        .child(header)
        .child(
            MotionBox::new(format!("scene-{}", s.tab), content)
                .preset(MotionPreset::Fade)
                .build(&theme),
        );

    let _ = k;
    portals.mount(page)
}

fn on_key(s: &mut Gallery, ev: &KeyEvent, _tx: &mut Tx) -> bool {
    if (ev.mods.ctrl || ev.mods.cmd)
        && matches!(&ev.key, Key::Text(t) if t.eq_ignore_ascii_case("k"))
    {
        if s.cmdk.open {
            s.cmdk.close();
        } else {
            s.cmdk.open_root();
        }
        return true;
    }
    false
}

type SceneEntry = (&'static str, fn(&mut Gallery));

const SCENES: &[SceneEntry] = &[
    ("overview", |s| {
        s.tab = "overview".into();
        s.game_running = false;
    }),
    ("controls", |s| {
        s.tab = "controls".into();
    }),
    ("overlays", |s| {
        s.tab = "overlays".into();
        s.tooltip_open = true;
        s.toasts.push(
            SonnerToast::new("Shaders compiled")
                .description("12 pipelines cached in 14 ms")
                .variant(ToastVariant::Success),
            None,
            0.0,
        );
        s.toasts.push(
            SonnerToast::new("Controller connected")
                .description("DualSense Edge · USB")
                .variant(ToastVariant::Info)
                .action("Configure"),
            None,
            0.1,
        );
    }),
    ("components", |s| {
        s.tab = "components".into();
    }),
    ("effects", |s| {
        s.tab = "effects".into();
        s.game_running = false;
        s.reduced_motion = false;
    }),
    ("effects-paused", |s| {
        s.tab = "effects".into();
        s.game_running = true;
    }),
    ("charts", |s| {
        s.tab = "charts".into();
    }),
];

fn main() {
    let comp =
        UiEffectsCompositor::new(|s: &Gallery| s.gate()).with_palette(EffectPalette::default());
    let mut spec = AppSpec::new("rune-ui Gallery", (1440.0, 900.0), Gallery::new, view);
    spec.on_key = Some(on_key);
    spec.scenes = SCENES;
    rune::run_with(spec, comp);
}
