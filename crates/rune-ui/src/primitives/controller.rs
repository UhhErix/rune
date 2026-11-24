//! Unified keyboard and game-controller navigation mapping.
//!
//! Components decode either a Rune [`KeyEvent`] or a [`GamepadButton`] into a semantic
//! [`NavCommand`], so menus, tabs, sliders, drawers, and command palettes work identically with
//! keyboard or controller (D-pad / A / B / LB / RB) without duplicating focus logic.

use rune::{Key, KeyEvent};

/// Standard game controller inputs mapped to UI navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    Start,
    Select,
}

/// Semantic navigation command shared by keyboard and controller input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavCommand {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Confirm,
    Cancel,
    NextSection,
    PrevSection,
    ContextAction,
    FocusNext,
    FocusPrev,
}

/// Maps a Rune [`KeyEvent`] to a semantic [`NavCommand`].
pub fn from_key_event(ev: &KeyEvent) -> Option<NavCommand> {
    let m = ev.mods;
    if m.alt || m.logo {
        return None;
    }
    if m.ctrl && !m.shift && !m.cmd {
        return match &ev.key {
            Key::Text(t) if t == "n" => Some(NavCommand::Down),
            Key::Text(t) if t == "p" => Some(NavCommand::Up),
            Key::Command('n') => Some(NavCommand::Down),
            Key::Command('p') => Some(NavCommand::Up),
            _ => None,
        };
    }
    if m.ctrl || m.cmd {
        return None;
    }
    match &ev.key {
        Key::Up => Some(NavCommand::Up),
        Key::Down => Some(NavCommand::Down),
        Key::Left => Some(NavCommand::Left),
        Key::Right => Some(NavCommand::Right),
        Key::PageUp => Some(NavCommand::PageUp),
        Key::PageDown => Some(NavCommand::PageDown),
        Key::Home => Some(NavCommand::Home),
        Key::End => Some(NavCommand::End),
        Key::Enter => Some(NavCommand::Confirm),
        Key::Escape => Some(NavCommand::Cancel),
        Key::Tab if m.shift => Some(NavCommand::FocusPrev),
        Key::Tab => Some(NavCommand::FocusNext),
        _ => None,
    }
}

/// Maps a [`GamepadButton`] to a semantic [`NavCommand`].
pub fn from_gamepad(btn: GamepadButton) -> Option<NavCommand> {
    match btn {
        GamepadButton::DpadUp => Some(NavCommand::Up),
        GamepadButton::DpadDown => Some(NavCommand::Down),
        GamepadButton::DpadLeft => Some(NavCommand::Left),
        GamepadButton::DpadRight => Some(NavCommand::Right),
        GamepadButton::South => Some(NavCommand::Confirm),
        GamepadButton::East => Some(NavCommand::Cancel),
        GamepadButton::West => Some(NavCommand::ContextAction),
        GamepadButton::LeftBumper => Some(NavCommand::PrevSection),
        GamepadButton::RightBumper => Some(NavCommand::NextSection),
        GamepadButton::North | GamepadButton::Start | GamepadButton::Select => None,
    }
}
