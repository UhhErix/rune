//! A titlebar the app draws itself (`App::titlebar`), and where the window's own controls sit
//! over it (`Cx::chrome_insets`).
//!
//! The app declares how tall its titlebar is; Rune takes the native one away and keeps the
//! window's controls over the app's: AppKit's traffic lights at the leading edge on macOS, and
//! the minimize, maximize and close buttons at the trailing edge on Windows, which Rune draws
//! as elements over the view. The app keeps clear of [`ChromeInsets`] and marks the parts
//! that move the window with `.window_drag()`; hit targets inside them keep their presses.

use crate::{Chrome, Color, El};

mod caption;
#[cfg(test)]
mod tests;

pub(crate) use caption::{BUTTONS_KEY, FRAME_KEY, frame};

/// Which glyph a Windows caption button draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionGlyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Visual and layout styling for the Windows caption buttons drawn over a custom
/// [`Titlebar`] (`Titlebar::caption_style`).
///
/// Any field left unset keeps Windows 11's defaults: 46 pt wide, full titlebar height, square
/// corners, 10 px one-device-pixel symbols, translucent washes on hover and press, and Windows'
/// red on close. Hit-testing (`HTMINBUTTON`, `HTMAXBUTTON` for Snap Layouts, `HTCLOSE`),
/// accessibility invoke actions, right-to-left mirroring and zoom invariance all follow the
/// configured geometry.
#[derive(Clone, Copy, Debug)]
pub struct CaptionStyle {
    pub(super) button_width: f32,
    pub(super) button_height: Option<f32>,
    pub(super) gap: f32,
    pub(super) trailing_inset: f32,
    pub(super) top_inset: Option<f32>,
    pub(super) radius: f32,
    pub(super) hover_fill: Option<Color>,
    pub(super) press_fill: Option<Color>,
    pub(super) hover_symbol: Option<Color>,
    pub(super) press_symbol: Option<Color>,
    pub(super) close_hover_fill: Option<Color>,
    pub(super) close_press_fill: Option<Color>,
    pub(super) close_hover_symbol: Option<Color>,
    pub(super) close_press_symbol: Option<Color>,
    pub(super) glyph: Option<fn(CaptionGlyph, Color) -> El>,
    pub(super) hide_glyphs: bool,
}

impl CaptionStyle {
    /// Windows 11's default caption button geometry and styling.
    pub const fn new() -> Self {
        Self {
            button_width: Titlebar::CAPTION_BUTTON_WIDTH,
            button_height: None,
            gap: 0.0,
            trailing_inset: 0.0,
            top_inset: None,
            radius: 0.0,
            hover_fill: None,
            press_fill: None,
            hover_symbol: None,
            press_symbol: None,
            close_hover_fill: None,
            close_press_fill: None,
            close_hover_symbol: None,
            close_press_symbol: None,
            glyph: None,
            hide_glyphs: false,
        }
    }

    /// Width of each caption button in window points. Defaults to [`Titlebar::CAPTION_BUTTON_WIDTH`] (46).
    pub const fn button_width(mut self, points: f32) -> Self {
        self.button_width = points;
        self
    }

    /// Height of each caption button in window points. When unset, buttons fill the titlebar band.
    pub const fn button_height(mut self, points: f32) -> Self {
        self.button_height = Some(points);
        self
    }

    /// Horizontal gap between adjacent caption buttons in window points. Defaults to 0.
    pub const fn gap(mut self, points: f32) -> Self {
        self.gap = points;
        self
    }

    /// Inset from the window's trailing edge (leading edge in RTL) in window points. Defaults to 0.
    pub const fn trailing_inset(mut self, points: f32) -> Self {
        self.trailing_inset = points;
        self
    }

    /// Inset from the window's top edge in window points. When unset and [`Self::button_height`]
    /// is set, buttons are centred vertically inside the titlebar band; otherwise 0.
    pub const fn top_inset(mut self, points: f32) -> Self {
        self.top_inset = Some(points);
        self
    }

    /// Corner radius of each caption button's hover and press fill in window points. Defaults to 0.
    pub const fn radius(mut self, points: f32) -> Self {
        self.radius = points;
        self
    }

    /// Background fill for minimize and maximize/restore while hovered.
    pub const fn hover_fill(mut self, color: Color) -> Self {
        self.hover_fill = Some(color);
        self
    }

    /// Background fill for minimize and maximize/restore while pressed.
    pub const fn press_fill(mut self, color: Color) -> Self {
        self.press_fill = Some(color);
        self
    }

    /// Symbol colour for minimize and maximize/restore while hovered.
    pub const fn hover_symbol(mut self, color: Color) -> Self {
        self.hover_symbol = Some(color);
        self
    }

    /// Symbol colour for minimize and maximize/restore while pressed. Falls back to
    /// [`Self::hover_symbol`] when unset.
    pub const fn press_symbol(mut self, color: Color) -> Self {
        self.press_symbol = Some(color);
        self
    }

    /// Background fill for the close button while hovered. Defaults to Windows' red.
    pub const fn close_hover_fill(mut self, color: Color) -> Self {
        self.close_hover_fill = Some(color);
        self
    }

    /// Background fill for the close button while pressed.
    pub const fn close_press_fill(mut self, color: Color) -> Self {
        self.close_press_fill = Some(color);
        self
    }

    /// Symbol colour for the close button while hovered. Defaults to white.
    pub const fn close_hover_symbol(mut self, color: Color) -> Self {
        self.close_hover_symbol = Some(color);
        self
    }

    /// Symbol colour for the close button while pressed. Falls back to [`Self::close_hover_symbol`].
    pub const fn close_press_symbol(mut self, color: Color) -> Self {
        self.close_press_symbol = Some(color);
        self
    }

    /// App-drawn glyph builder, centred inside each caption button in place of Rune's default
    /// Windows 11 10 px symbols. Receives the resolved symbol colour for the button's current
    /// rest, hover, press or disabled state.
    pub const fn glyph(mut self, draw: fn(CaptionGlyph, Color) -> El) -> Self {
        self.glyph = Some(draw);
        self
    }

    /// When `true`, Rune skips painting caption button fills and glyphs while keeping the
    /// non-client hit-test rectangles (`HTMINBUTTON`, `HTMAXBUTTON`, `HTCLOSE`) and accessibility
    /// buttons, so the app can draw its own controls underneath.
    pub const fn hide_glyphs(mut self, hide: bool) -> Self {
        self.hide_glyphs = hide;
        self
    }
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for CaptionStyle {
    fn eq(&self, other: &Self) -> bool {
        self.button_width == other.button_width
            && self.button_height == other.button_height
            && self.gap == other.gap
            && self.trailing_inset == other.trailing_inset
            && self.top_inset == other.top_inset
            && self.radius == other.radius
            && self.hover_fill == other.hover_fill
            && self.press_fill == other.press_fill
            && self.hover_symbol == other.hover_symbol
            && self.press_symbol == other.press_symbol
            && self.close_hover_fill == other.close_hover_fill
            && self.close_press_fill == other.close_press_fill
            && self.close_hover_symbol == other.close_hover_symbol
            && self.close_press_symbol == other.close_press_symbol
            && match (self.glyph, other.glyph) {
                (Some(a), Some(b)) => std::ptr::fn_addr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && self.hide_glyphs == other.hide_glyphs
    }
}

/// The app draws its own titlebar across the top of the window, `height` points tall
/// (`App::titlebar`, `WindowSpec::titlebar`). Plain data, the same on every platform:
///
/// - **macOS:** a full-size content view under a transparent titlebar, `.window_drag()`
///   regions only (`Chrome::drag_regions`), and the traffic lights centred in the band unless
///   `Chrome::traffic_lights` places them.
/// - **Windows:** no native title bar. The window keeps its resize edges, Aero Snap, the snap
///   layouts flyout over maximize (Windows 11), the window menu (Alt+Space, or a right-click on
///   a `.window_drag()` region) and its animations; Rune draws the three caption buttons over
///   the trailing edge at the band's height (or as styled by [`Titlebar::caption_style`]), as
///   Electron's `titleBarOverlay` does.
/// - **Elsewhere:** the native frame stays and [`ChromeInsets`] is zero.
///
/// `Cx::titlebar_height` changes the height while the app runs, as a UI zoom does, and
/// `Cx::set_titlebar` the whole titlebar, as a switch between layouts does. A borderless window
/// has no titlebar to replace, and ignores it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Titlebar {
    height: f32,
    traffic_lights: Option<(f32, f32)>,
    controls: bool,
    caption: CaptionStyle,
}

impl Titlebar {
    /// A caption button's width in points: Windows' and Electron's 46.
    pub const CAPTION_BUTTON_WIDTH: f32 = 46.0;

    pub const fn new(height: f32) -> Self {
        Self { height, traffic_lights: None, controls: true, caption: CaptionStyle::new() }
    }

    /// macOS: the close button's top-left, in points from the window's top-left, for this
    /// titlebar (Electron's `trafficLightPosition`), in place of `Chrome::traffic_lights` and
    /// the band's middle. Ignored elsewhere and in full screen.
    pub const fn traffic_lights(mut self, x: f32, y: f32) -> Self {
        self.traffic_lights = Some((x, y));
        self
    }

    /// Whether the window shows its own caption controls over the titlebar: traffic lights on
    /// macOS and the minimize/maximize/close buttons on Windows. Defaults to `true`.
    ///
    /// When `false`, Windows still installs the custom frame (keeping invisible outer side and
    /// bottom resize borders, the top inside resize edge, Aero Snap, Alt+Space and right-click
    /// window menu, and `Tx::minimize()`/`Tx::toggle_maximized()`), macOS hides its traffic
    /// lights while keeping the full-size content view and `.window_drag()` regions, and
    /// [`ChromeInsets`] reports zero `leading` and `trailing` insets. Use this when the app
    /// draws both its own titlebar and its own window controls.
    pub const fn controls(mut self, shown: bool) -> Self {
        self.controls = shown;
        self
    }

    /// Windows: visual and layout styling for the caption buttons (size, insets, corner radius,
    /// hover and press fills, custom glyphs or hidden chrome). Ignored elsewhere.
    pub const fn caption_style(mut self, style: CaptionStyle) -> Self {
        self.caption = style;
        self
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    /// Where [`Self::traffic_lights`] put the close button, if it did.
    pub fn traffic_lights_origin(&self) -> Option<(f32, f32)> {
        self.traffic_lights
    }

    /// Whether the window shows its own caption controls over the titlebar (`true` by default).
    pub const fn has_controls(&self) -> bool {
        self.controls
    }

    /// The Windows caption button style set by [`Self::caption_style`], or Windows 11 defaults.
    pub fn caption(&self) -> CaptionStyle {
        self.caption
    }

    /// The frame macOS draws for this titlebar over `chrome`: full-size content, drag regions
    /// only, and the traffic lights where this titlebar or the app placed them, else in the
    /// band's middle (`None` when [`Self::controls`] is `false`).
    pub(crate) fn mac_chrome(&self, chrome: Chrome) -> Chrome {
        if !self.controls {
            return Chrome { full_size_content: true, drag_regions: true, traffic_lights: None };
        }
        let y = ((self.height - Chrome::TRAFFIC_LIGHT) / 2.0).max(0.0).round();
        let lights = self.traffic_lights.or(chrome.traffic_lights).or(Some((16.0, y)));
        Chrome { full_size_content: true, drag_regions: true, traffic_lights: lights }
    }
}

/// What the view set of the window's titlebar on its last run: `Cx::set_titlebar`, then
/// `Cx::titlebar_height` over it. Empty keeps `App::titlebar`'s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TitlebarSet {
    pub titlebar: Option<Titlebar>,
    pub height: Option<f32>,
}

impl TitlebarSet {
    /// The titlebar the window draws in place of the app's `base`.
    pub(crate) fn over(self, base: Titlebar) -> Titlebar {
        let titlebar = self.titlebar.unwrap_or(base);
        self.height.map_or(titlebar, |height| Titlebar { height, ..titlebar })
    }

    /// Whether the window draws another titlebar for this set than it did for `before`, over
    /// the app's `base`. A view that sets the titlebar the window opened with changes nothing,
    /// and without `App::titlebar` nothing the view sets applies.
    #[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
    pub(crate) fn changes(self, before: TitlebarSet, base: Option<Titlebar>) -> bool {
        base.is_some_and(|base| self.over(base) != before.over(base))
    }
}

#[cfg(test)]
mod set_tests {
    use super::*;

    #[test]
    fn a_view_that_sets_the_titlebar_the_window_opened_with_changes_nothing() {
        let base = Titlebar::new(36.0).traffic_lights(14.0, 15.0);
        let unset = TitlebarSet::default();
        let same = TitlebarSet { titlebar: Some(base), height: None };
        assert!(!same.changes(unset, Some(base)));
        assert!(!TitlebarSet { titlebar: None, height: Some(36.0) }.changes(unset, Some(base)));
        assert!(!unset.changes(unset, Some(base)));
    }

    #[test]
    fn another_height_or_other_traffic_lights_change_it() {
        let base = Titlebar::new(48.0).traffic_lights(20.0, 25.0);
        let unset = TitlebarSet::default();
        assert!(TitlebarSet { titlebar: Some(Titlebar::new(36.0).traffic_lights(14.0, 15.0)), height: None }.changes(unset, Some(base)));
        assert!(TitlebarSet { titlebar: Some(Titlebar::new(48.0).traffic_lights(14.0, 15.0)), height: None }.changes(unset, Some(base)));
        assert!(TitlebarSet { titlebar: None, height: Some(52.0) }.changes(unset, Some(base)));
        // Lights the titlebar leaves to `App::chrome` against lights it places itself.
        assert!(TitlebarSet { titlebar: Some(Titlebar::new(48.0)), height: None }.changes(unset, Some(base)));
    }

    #[test]
    fn a_window_without_a_titlebar_of_its_own_never_changes() {
        let set = TitlebarSet { titlebar: Some(Titlebar::new(36.0)), height: Some(40.0) };
        assert!(!set.changes(TitlebarSet::default(), None));
    }

    #[test]
    fn controls_builder_defaults_true_and_is_tracked_by_titlebar_set() {
        let base = Titlebar::new(36.0);
        assert!(base.has_controls());
        let hidden = base.controls(false);
        assert!(!hidden.has_controls());
        assert!(hidden.controls(true).has_controls());

        let unset = TitlebarSet::default();
        let hide_set = TitlebarSet { titlebar: Some(hidden), height: None };
        assert!(hide_set.changes(unset, Some(base)));
        assert!(!hide_set.changes(unset, Some(hidden)));
        assert!(!TitlebarSet { titlebar: None, height: Some(44.0) }.over(hidden).has_controls());
        assert_eq!(hidden.mac_chrome(Chrome::DEFAULT), Chrome { full_size_content: true, drag_regions: true, traffic_lights: None });
    }
}

/// What the window's own controls take of the app's titlebar, in points
/// (`Cx::chrome_insets`). The view re-runs when it changes: entering or leaving full screen,
/// a scale change, `Cx::titlebar_height`, `Cx::set_titlebar`. Memos rebuild with it too, so a
/// `memo` closure that reads it needs nothing in its version.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ChromeInsets {
    /// From the window's left edge to the end of the controls there: the macOS traffic lights
    /// (`Cx::titlebar_inset`), or the Windows caption buttons when the app's language reads right
    /// to left (`labels::set_direction`), where the traffic lights move to the right instead. 0 in full
    /// screen and elsewhere.
    pub leading: f32,
    /// From the window's right edge to the start of the controls there: the Windows caption
    /// buttons, unless they're mirrored to the left, or the macOS traffic lights in a right-to-left
    /// app. 0 in full screen and elsewhere.
    pub trailing: f32,
    /// The titlebar's height as the window draws it: [`Titlebar::height`] where Rune replaced
    /// the native titlebar, 0 where the native frame stays (no `App::titlebar`, Linux, a
    /// borderless window).
    pub height: f32,
}

impl ChromeInsets {
    pub(crate) fn new(leading: f32, trailing: f32, height: f32) -> Self {
        Self { leading, trailing, height }
    }

    /// The insets, in window points, in the points of a view zoomed `k` times (`Cx::zoom`).
    pub(crate) fn zoomed(self, k: f32) -> Self {
        Self { leading: self.leading / k, trailing: self.trailing / k, height: self.height / k }
    }

    /// The bits a memo scope hashes, so memos rebuild when the insets change.
    pub(crate) fn bits(&self) -> [u32; 3] {
        [self.leading, self.trailing, self.height].map(f32::to_bits)
    }
}

/// One of the Windows caption buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}

impl CaptionButton {
    const ALL: [CaptionButton; 3] = [CaptionButton::Minimize, CaptionButton::Maximize, CaptionButton::Close];
}

/// Where the pointer is with the caption buttons. The system answers presses on them as
/// non-client hits, so the buttons see neither hover nor press as pointer events: the window
/// procedure keeps this, and the buttons draw from it.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CaptionState {
    #[default]
    Idle,
    /// Over `button` with no press.
    Hovered(CaptionButton),
    /// A press began on `button`; `over` while the pointer is still on it. Releasing there acts.
    Pressed { button: CaptionButton, over: bool },
}

/// The caption buttons Rune draws over a custom titlebar on Windows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Caption {
    pub height: f32,
    pub style: CaptionStyle,
    pub state: CaptionState,
    pub maximized: bool,
    /// The window has a minimize box (`WS_MINIMIZEBOX`); without one, the button is dimmed.
    pub minimizable: bool,
    /// The window has a maximize box (`WS_MAXIMIZEBOX`); a window without one, a window that
    /// isn't resizable, never maximizes, and the button is dimmed.
    pub maximizable: bool,
    /// Full screen: the window draws no buttons, but keeps the frame that hosts them.
    pub hidden: bool,
    /// The app's language reads right to left (`labels::set_direction`): the buttons sit at the
    /// window's top-left, Close first, as Windows mirrors a right-to-left window's caption and
    /// Chrome and Edge draw theirs in Arabic or Hebrew.
    pub mirrored: bool,
}

impl Caption {
    #[cfg(test)]
    pub(crate) fn new(height: f32) -> Self {
        Self::with_style(height, CaptionStyle::new())
    }

    pub(crate) fn with_style(height: f32, style: CaptionStyle) -> Self {
        Self {
            height,
            style,
            state: CaptionState::Idle,
            maximized: false,
            minimizable: true,
            maximizable: true,
            hidden: false,
            mirrored: crate::labels::rtl(),
        }
    }

    /// The buttons drawn, left to right. A window that can neither minimize nor maximize shows
    /// only Close, as Windows draws its native caption.
    pub(crate) fn buttons(&self) -> &'static [CaptionButton] {
        match (self.hidden, self.minimizable || self.maximizable) {
            (true, _) => &[],
            (false, true) => &CaptionButton::ALL,
            (false, false) => &[CaptionButton::Close],
        }
    }

    /// Whether `button` does anything: a press on a dimmed button doesn't.
    pub(crate) fn enabled(&self, button: CaptionButton) -> bool {
        match button {
            CaptionButton::Minimize => self.minimizable,
            CaptionButton::Maximize => self.maximizable,
            CaptionButton::Close => true,
        }
    }

    /// The buttons' rects, in [`Caption::buttons`]' order, in points from the window's top-left for
    /// a window `width` points wide at `scale`: each a whole number of pixels, inset from the
    /// trailing edge (or leading edge when mirrored, Close leftmost) by `style.trailing_inset`.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn rects(&self, width: f32, scale: f32) -> impl Iterator<Item = (CaptionButton, [f32; 4])> + use<> {
        let lay = caption::layout(self.height, self.style, scale);
        let buttons = self.buttons();
        let (n, mirrored) = (buttons.len(), self.mirrored);
        let step = lay.w + lay.gap;
        buttons.iter().enumerate().map(move |(i, &b)| {
            let from_edge = lay.trailing + (n - 1 - i) as f32 * step;
            let x = if mirrored { from_edge } else { width - from_edge - lay.w };
            (b, [x, lay.top, lay.w, lay.h])
        })
    }

    /// The enabled button at (x, y), in points from the window's top-left, in a window `width`
    /// points wide at `scale`.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn hit(&self, width: f32, scale: f32, (x, y): (f32, f32)) -> Option<CaptionButton> {
        self.rects(width, scale)
            .find(|(_, [rx, ry, rw, rh])| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
            .map(|(button, _)| button)
            .filter(|&b| self.enabled(b))
    }

    /// Points the buttons take from their edge at `scale` (including `style.trailing_inset` and
    /// any gaps between buttons): the right, or the left mirrored.
    pub(crate) fn width(&self, scale: f32) -> f32 {
        let n = self.buttons().len();
        if n == 0 {
            return 0.0;
        }
        let lay = caption::layout(self.height, self.style, scale);
        lay.trailing + lay.w * n as f32 + lay.gap * (n - 1) as f32
    }
}
