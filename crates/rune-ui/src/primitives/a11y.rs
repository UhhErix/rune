//! AccessKit / ARIA semantic helpers for `rune-ui` components.

use rune::{El, LiveRegion, text};

/// Standard ARIA / AccessKit role tags recognized by Rune's accessibility bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum A11yRole {
    Button,
    Dialog,
    AlertDialog,
    Menu,
    MenuItem,
    MenuItemCheckbox,
    MenuItemRadio,
    Listbox,
    Option,
    Combobox,
    TabList,
    Tab,
    TabPanel,
    Slider,
    Switch,
    Checkbox,
    RadioGroup,
    Radio,
    Tooltip,
    Status,
    Alert,
    Grid,
    Region,
    Navigation,
    ProgressBar,
}

impl A11yRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Dialog => "dialog",
            Self::AlertDialog => "alertdialog",
            Self::Menu => "menu",
            Self::MenuItem => "menuitem",
            Self::MenuItemCheckbox => "menuitemcheckbox",
            Self::MenuItemRadio => "menuitemradio",
            Self::Listbox => "listbox",
            Self::Option => "option",
            Self::Combobox => "combobox",
            Self::TabList => "tablist",
            Self::Tab => "tab",
            Self::TabPanel => "tabpanel",
            Self::Slider => "slider",
            Self::Switch => "switch",
            Self::Checkbox => "checkbox",
            Self::RadioGroup => "radiogroup",
            Self::Radio => "radio",
            Self::Tooltip => "tooltip",
            Self::Status => "status",
            Self::Alert => "alert",
            Self::Grid => "grid",
            Self::Region => "region",
            Self::Navigation => "navigation",
            Self::ProgressBar => "progressbar",
        }
    }
}

/// Applies a typed [`A11yRole`] and accessible name.
pub fn with_role(el: El, role: A11yRole, label: impl Into<String>) -> El {
    el.role(role.as_str()).label(label)
}

/// Applies range slider AccessKit metadata (`role="slider"`, value/min/max/step, disabled).
pub fn slider_a11y(
    el: El,
    label: impl Into<String>,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    disabled: bool,
) -> El {
    el.role(A11yRole::Slider.as_str())
        .label(label)
        .range_value(value, min, max)
        .range_step(step)
        .disabled(disabled)
}

/// Applies tab semantics (`role="tab"`, `selected`, `disabled`).
pub fn tab_a11y(el: El, label: impl Into<String>, selected: bool, disabled: bool) -> El {
    el.role(A11yRole::Tab.as_str())
        .label(label)
        .focus_selected(selected)
        .disabled(disabled)
}

/// Renders screen-reader-only text (visually hidden 1×1 clipped element, visible to AccessKit).
pub fn visually_hidden(label: impl Into<String>) -> El {
    let s = label.into();
    text(s.clone()).size(1.0, 1.0).opacity(0.0).label(s)
}

/// Renders a live-region status announcement (`Polite` or `Assertive`).
pub fn live_announcement(
    key: impl Into<String>,
    message: impl Into<String>,
    assertive: bool,
) -> El {
    let live = if assertive {
        LiveRegion::Assertive
    } else {
        LiveRegion::Polite
    };
    let role = if assertive {
        A11yRole::Alert
    } else {
        A11yRole::Status
    };
    visually_hidden(message)
        .key(key.into())
        .role(role.as_str())
        .live_region(live)
}
