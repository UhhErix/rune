# Component Inventory: React Ecosystem -> Rune / `rune-kit` / `rune-ui` / `exo-kit`

This document maps every component, behavior primitive, motion pattern, visual effect, and chart
from the reference React libraries to what Rune, `rune-kit`, `rune-fx`, `rune-motion`, and
`exo-kit` already provide, what `rune-ui` adds, and where Exo Launcher's rooms use them.

## 1. Behavior Primitives (Radix / React Aria)

| Primitive | Reference | Rune / `rune-kit` Today | `rune-ui` Addition | Exo Rooms Needed |
| --- | --- | --- | --- | --- |
| Focus trap & return | Radix FocusScope | `El::modal`, `El::focus_return`, `El::autofocus` | `FocusScope` builder + modal focus helpers | Dialogs, Command Palette, Drawers, Settings |
| Roving tabindex | Radix RovingFocusGroup | `El::focus_group`, `list_nav` (internal) | Public `RovingState` + `RovingOrientation` (Horizontal/Vertical/Both, wrap, skip disabled) | Tabs, RadioGroup, Menus, Toolbars, Gamepad nav |
| Typeahead | Radix / React Aria | `list_nav::TypeAhead` (crate-private) | Public `TypeaheadBuffer` (multi-char prefix + same-char cycling) | Select, DropdownMenu, ContextMenu, List |
| Dismiss layers | Radix DismissableLayer | `El::modal(click_outside, on_dismiss)` | `DismissStack` (LIFO topmost-only Escape & outside press, nested overlays) | Popover, Menu, Dialog, Sheet, Command Palette |
| Anchored positioning | Radix Popper / Floating UI | `El::anchored`, `El::anchored_fit`, `popover::anchor_rect` | `compute_position` with full flip + cross-axis shift + collision padding + arrow offset + available box | Popover, Select, Combobox, Tooltip, ContextMenu |
| Portals & z-layers | Radix Portal | `rune_kit::root(base, overlays)` | `PortalHost` with typed `LayerSlot` z-order (`Dropdown`, `Sticky`, `Modal`, `Popover`, `Toast`, `Tooltip`) | Shell overlays, Bar menus, Toasts, Tooltips |
| Accessibility | AccessKit / React Aria | `El::role`, `label`, `checked`, `range_value`, `live_region`, `active_descendant` | Typed `a11y` helpers (`A11yRole`, `visually_hidden`, `announce`, slider/tab/dialog semantics) | Every room (Windows UIA / Narrator / NVDA) |
| Keyboard + Controller | Exo Gamepad + React Aria | `exo-ui/src/gamepad.rs` (room-local) | `NavCommand` + `ControllerMap` unifying Keyboard & Gamepad D-pad/A/B/LB/RB | Library, Store, Game Page, Shell Bar |

## 2. Core Controls & Forms

| Component | Reference | Rune / `rune-kit` Today | `rune-ui` Addition | Exo Rooms Needed |
| --- | --- | --- | --- | --- |
| Button | HeroUI, shadcn, Mantine | `rune_kit::Button`, `exo_kit::primary_button` / `glass_button` | `UiButton` (6 variants, 4 sizes, loading spinner, leading/trailing icons, press scale 0.97) | All rooms |
| Input / TextArea | HeroUI, shadcn, Mantine | `rune_kit::TextInput`, `TextArea`, `TextField` | `UiInput` / `UiTextArea` (label, description, error, addons, clear button, password eye, char count) | Search, Discord chat, Settings, Auth, Store |
| Select | Radix, shadcn, HeroUI | `rune_kit::Select`, `exo_kit::select` | `UiSelect` (grouped options, typeahead, flip+shift viewport, check indicator, disabled items) | Settings, Game Properties, PC Display/GPU |
| Combobox | shadcn, Mantine, HeroUI | `rune_kit::Combobox` (single-select) | `UiCombobox` (single + multi-select chips, fuzzy rank, empty/creatable hooks) | Library tags/folders, Discord pickers, PC |
| Menu / Dropdown | Radix DropdownMenu | `rune_kit::Menu`, `exo_kit::menu` | `DropdownMenu` (submenus, checkbox/radio items, shortcuts, typeahead, roving arrows) | Bar, Game cards, Discord messages, PC rows |
| Context Menu | Radix ContextMenu | `rune_kit::context_menu`, `exo_kit::menu` | `ContextMenu` (pointer-anchored flip+shift, right-click trigger, nested submenus) | Library game tiles, Discord channels/msgs, Captures |
| Dialog / AlertDialog | Radix Dialog, HeroUI | `rune_kit::Dialog` | `UiDialog` + `AlertDialog` (scrim/glass, header/body/footer, focus trap, non-dismissable confirm) | Uninstall confirm, Key redemption, Add game, Anti-cheat warning |
| Sheet / Drawer | Vaul, shadcn Sheet | None | `Sheet` / `Drawer` (4 edges, snap points, 1:1 drag, velocity fling, rubber-band overdrag, bg scale) | Game Properties, What's New, Mobile/Compact panels, Notifications |
| Popover | Radix Popover | `rune_kit::Popover` | `UiPopover` (flip+shift, optional arrow, controlled/uncontrolled dismiss) | Quick filters, Color picker, Audio mixer, Game info |
| Tooltip | Radix Tooltip, HeroUI | `rune_kit::Tooltip` | `UiTooltip` + `TooltipGroup` (hover delay + skip-delay across siblings, flip+shift, a11y) | Bar icons, Device metrics, Store badges |
| Tabs | Radix Tabs, HeroUI, Framer Motion | `rune_kit::TabStrip` (editor tabs) | `UiTabs` (horizontal/vertical, animated `El::matched()` indicator, underline/pill/segmented, manual/auto) | Library views, PC subpages, Settings sections, Store shelves |
| Accordion | Radix Accordion | `rune_kit::Accordion` | `UiAccordion` (`Single { collapsible }` / `Multiple`, roving arrows, smooth spring height) | Settings groups, Game FAQ/specs, PC diagnostics |
| Slider | Radix Slider, HeroUI | `exo_kit::slider_track` (visual only) | `UiSlider` + `RangeSlider` (single & dual thumb, ticks/marks, 1:1 drag, keyboard step/page/home/end, AccessKit numeric value) | Audio volume, GPU power/fan/clock, UI scale, Frame cap |
| Switch | Radix Switch, HeroUI | `rune_kit::Switch`, `exo_kit::switch_pill` | `UiSwitch` (spring thumb travel, sizes, label/description, Space/Enter, AccessKit `switch`) | Settings, PC HDR/Upscalers, Game launch opts |
| Checkbox | Radix Checkbox | `rune_kit::Checkbox` | `UiCheckbox` (tri-state `Unchecked`/`Checked`/`Indeterminate`, group support, AccessKit `checked_mixed`) | Batch selection, Filters, DataGrid select-all |
| Radio | Radix RadioGroup | `rune_kit::RadioGroup` | `UiRadioGroup` (roving arrows, classic & card-tile variants, orientation) | Music provider, Upscaler preset, Theme/Glass mode |
| Toast Stack | Sonner | `rune_kit::ToastStack` | `SonnerStack` (7 variants, collapsed peek/scale, hover expand, swipe-to-dismiss physics, action/cancel, Alt+T) | Download finished, Settings saved, Errors, Clipboard |
| Command Palette | cmdk | `exo-ui/src/palette.rs` (app-specific) | `CommandPalette` (cmdk `command-score` fuzzy rank, dynamic groups, nested pages + Backspace pop, shortcut hints) | Global Ctrl+K Command Palette |
| Table / Data Grid | Mantine, shadcn, TanStack | `rune_kit::Table` (virtualized sortable) | `DataGrid` (virtual rows + filter + pagination + multi-select + density + loading skeleton + empty state) | PC Apps/Storage/Runtimes, Captures, Game files |
| Date Picker | Mantine, shadcn, React Aria | None | `DatePicker` + `DateRangePicker` (pure-Rust civil calendar, leap years, min/max, keyboard grid nav, range preview) | Captures filter, Playtime history, Release filters |
| Avatar | HeroUI, Radix Avatar | None (ad-hoc circles) | `Avatar` + `AvatarGroup` (image/pixels, initials fallback, status indicator, `+N` overflow stack) | Discord friends/servers, Profile capsule |
| Badge / Chip | HeroUI, Mantine | `rune_kit::Badge`, `exo_kit::status_badge` | `UiBadge` (solid/soft/outline/dot) + `Chip` (selectable filter & removable tag with Backspace) | Store discounts, Library genre/tag filters, Device status |
| Skeleton | HeroUI, shadcn | None | `Skeleton` (rect/circle/text-lines, shimmer animation + zero-GPU static fallback) | Store loading, Discord hydration, Captures grid |
| Progress | HeroUI, Radix Progress | `rune_kit::Progress`, `ProgressCircle` | `UiProgress` (determinate spring linear/ring + indeterminate sweep + label) | Downloads, Shader compile, Disk usage |
| Pagination | HeroUI, Mantine | None | `Pagination` (smart ellipsis window, siblings/boundaries, prev/next/first/last) | DataGrid, Store browse, Captures library |
| Carousel | shadcn/Embla, Mantine | None | `Carousel` (drag/fling snap points, loop mode, prev/next arrows, dot indicators, keyboard) | Store hero banners, Game screenshots, Media gallery |

## 3. Motion (Framer Motion Patterns on `rune-motion` + `El::matched()`)

| Pattern | Reference | Rune / `rune-motion` Today | `rune-ui` Addition | Exo Rooms Needed |
| --- | --- | --- | --- | --- |
| Layout animations | Framer Motion `layout` | `El::motion(spring)`, `LayoutAnim` | `MotionBox`, `MotionPreset` (`Fade`, `Scale`, `Slide*`, `BlurFade`, `Pop`) | Cards, Expandable rows, Filters, Grid reflow |
| Shared elements | Framer Motion `layoutId` | `El::matched(id)` | `SharedElement` + `SharedIndicator` (tabs pill, selected capsule, hero tile morph) | Bar active pill, Tabs, Library -> Game Page hero |
| Presence & stagger | `AnimatePresence` | `El::transition(...)`, `rune_motion::Stagger` | `PresenceGroup`, `StaggerList` (respecting `reduced_motion`) | Room enter, Shelf cards, Search results |
| Gestures | `drag`, `dragElastic`, `onDragEnd` | `El::on_drag`, `VelocityTracker`, `rubber_band`, `project` | `DragGesture` / `SwipeState` (1:1 drag + rubber-band bounds + fling snap) | Vaul Drawer, Sonner Toast swipe, Carousel, Sliders |

## 4. GPU Effects (wgpu WGSL Shaders — Clean-Room Original)

Every effect checks `EffectGate { reduced_motion, game_running, gpu_enabled }`:
- Zero-GPU-cost static fallback when `!gpu_enabled || reduced_motion`.
- Pauses completely (`animating() == false`, zero GPU passes / redraws) while `game_running == true`.

| Effect | Visual Reference | License Status | `rune-ui::effects` Implementation |
| --- | --- | --- | --- |
| Aurora background | Magic UI / React Bits visual | Clean-room WGSL | `BackgroundEffect::Aurora` (multi-band smoothstep aurora shader) |
| Particles background | Magic UI / React Bits visual | Clean-room WGSL | `BackgroundEffect::Particles` (procedural hash-grid bokeh/stars + pointer parallax) |
| Waves background | React Bits visual | Clean-room WGSL | `BackgroundEffect::Waves` (layered sinusoidal ribbon waves shader) |
| Grid background | Magic UI RetroGrid/DotPattern | Clean-room WGSL | `BackgroundEffect::Grid` (animated dot/line grid with radial spotlight pulse) |
| Blur-in text | Magic UI BlurFade / React Bits | Clean-room Rust | `BlurInText` (staggered blur + opacity + y-lift entrance) |
| Split text | React Bits SplitText visual | Clean-room Rust | `SplitText` (per-char/word spring stagger reveal) |
| Scramble text | React Bits DecryptedText visual | Clean-room Rust | `ScrambleText` (deterministic glyph decode progression) |
| Gradient text | Magic UI AnimatedGradientText | Clean-room Rust | `GradientText` (animated stops via Rune `El::text_gradient`) |
| Shiny text | Magic UI ShinyText / React Bits | Clean-room Rust | `ShinyText` (sweeping specular band via `El::text_gradient`) |
| Border beam | Magic UI BorderBeam | Clean-room WGSL | `BorderEffect::Beam` (SDF rounded-rect perimeter light beam) |
| Glow border | Magic UI ShineBorder / React Bits | Clean-room WGSL | `BorderEffect::Glow` (SDF ambient/pointer-tracking edge glow) |
| Number ticker | Magic UI NumberTicker | Clean-room Rust | `NumberTicker` (spring odometer over `RollingText` + formatter) |
| Marquee | Magic UI Marquee | Clean-room Rust | `Marquee` (seamless loop, pause-on-hover, edge fade, game-pause aware) |

## 5. Charts (Tremor / Recharts-Level)

Rendered via Rune native vector `Shape` / `Path` / `canvas` with interactive hover tooltips and spring animation:

| Chart | Reference | `rune-ui::charts` Implementation | Exo Rooms Needed |
| --- | --- | --- | --- |
| Line Chart | Recharts / Tremor | `LineChart` (multi-series linear or Fritsch-Carlson monotone cubic spline, crosshair, tooltip, legend) | PC Performance (FPS/frametime/temp history) |
| Area Chart | Recharts / Tremor | `AreaChart` (stacked or overlapping translucent fills + crisp stroke + hover tooltip) | PC GPU/CPU/VRAM telemetry, Download speed |
| Bar Chart | Recharts / Tremor | `BarChart` (grouped or stacked, vertical/horizontal, rounded caps, hover tooltip) | Library playtime by week/game, Storage breakdown |
| Donut Chart | Tremor DonutChart / Recharts Pie | `DonutChart` (configurable inner radius, hover pop-out, center KPI readout, legend) | PC Storage by drive/category, Library by store |
| Sparkline | Tremor SparkChart | `Sparkline` (compact inline line/area/bar micro-chart with zero chrome) | PC live sensor rows, Game card playtime trend |
