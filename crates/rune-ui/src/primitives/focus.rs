//! Focus management, focus scopes, and roving tabindex (Radix / React Aria semantics).

use rune::{El, Key, KeyEvent};

use crate::primitives::controller::{GamepadButton, NavCommand, from_gamepad, from_key_event};

/// Axis along which roving keyboard/controller navigation moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RovingOrientation {
    #[default]
    Horizontal,
    Vertical,
    Both,
}

/// Roving tabindex state machine matching Radix `RovingFocusGroup` and React Aria semantics.
///
/// Exactly one enabled item holds tab-stop focus (`active`), arrow keys move within the group
/// skipping disabled items, `Home`/`End` jump to first/last enabled item, and optional wrapping
/// loops seamlessly across ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RovingState {
    pub active: usize,
    pub orientation: RovingOrientation,
    pub loop_nav: bool,
}

impl Default for RovingState {
    fn default() -> Self {
        Self {
            active: 0,
            orientation: RovingOrientation::Horizontal,
            loop_nav: true,
        }
    }
}

impl RovingState {
    pub fn new(orientation: RovingOrientation) -> Self {
        Self {
            active: 0,
            orientation,
            loop_nav: true,
        }
    }

    pub fn loop_nav(mut self, enabled: bool) -> Self {
        self.loop_nav = enabled;
        self
    }

    /// Ensures `active` points to an enabled index if one exists.
    pub fn clamp(&mut self, disabled: &[bool]) -> Option<usize> {
        let len = disabled.len();
        if len == 0 {
            self.active = 0;
            return None;
        }
        if self.active < len && !disabled[self.active] {
            return Some(self.active);
        }
        if let Some(first) = disabled.iter().position(|d| !*d) {
            self.active = first;
            Some(first)
        } else {
            self.active = 0;
            None
        }
    }

    /// Returns the first enabled item index.
    pub fn first_enabled(disabled: &[bool]) -> Option<usize> {
        disabled.iter().position(|d| !*d)
    }

    /// Returns the last enabled item index.
    pub fn last_enabled(disabled: &[bool]) -> Option<usize> {
        disabled.iter().rposition(|d| !*d)
    }

    /// Steps forward (`delta = 1`) or backward (`delta = -1`), skipping disabled indices.
    pub fn step(&mut self, delta: isize, disabled: &[bool]) -> Option<usize> {
        let len = disabled.len();
        if len == 0 || disabled.iter().all(|d| *d) {
            return None;
        }
        let start = self.active.min(len - 1);
        let mut cur = start as isize;
        for _ in 0..len {
            cur += delta;
            if self.loop_nav {
                cur = cur.rem_euclid(len as isize);
            } else if cur < 0 || cur >= len as isize {
                return Some(start);
            }
            let idx = cur as usize;
            if !disabled[idx] {
                self.active = idx;
                return Some(idx);
            }
        }
        Some(start)
    }

    /// Applies a unified [`NavCommand`] (from keyboard or controller) and returns the new active
    /// index if navigation consumed the command.
    pub fn handle_command(&mut self, cmd: NavCommand, disabled: &[bool]) -> Option<usize> {
        match (self.orientation, cmd) {
            (RovingOrientation::Horizontal | RovingOrientation::Both, NavCommand::Right)
            | (RovingOrientation::Vertical | RovingOrientation::Both, NavCommand::Down) => {
                self.step(1, disabled)
            }
            (RovingOrientation::Horizontal | RovingOrientation::Both, NavCommand::Left)
            | (RovingOrientation::Vertical | RovingOrientation::Both, NavCommand::Up) => {
                self.step(-1, disabled)
            }
            (_, NavCommand::Home) => {
                let idx = Self::first_enabled(disabled)?;
                self.active = idx;
                Some(idx)
            }
            (_, NavCommand::End) => {
                let idx = Self::last_enabled(disabled)?;
                self.active = idx;
                Some(idx)
            }
            _ => None,
        }
    }

    /// Handles a Rune [`KeyEvent`] directly.
    pub fn handle_key(&mut self, ev: &KeyEvent, disabled: &[bool]) -> Option<usize> {
        let cmd = from_key_event(ev)?;
        self.handle_command(cmd, disabled)
    }

    /// Handles a [`GamepadButton`] directly.
    pub fn handle_gamepad(&mut self, btn: GamepadButton, disabled: &[bool]) -> Option<usize> {
        let cmd = from_gamepad(btn)?;
        self.handle_command(cmd, disabled)
    }

    /// Returns whether item `index` should be the active tab stop (`tabIndex = 0` vs `-1`).
    pub fn is_tab_stop(&self, index: usize, disabled: bool) -> bool {
        !disabled && self.active == index
    }
}

/// Focus scope configuration matching Radix `FocusScope` (`trapped`, `return_to`, `autofocus`).
#[derive(Clone, Debug, Default)]
pub struct FocusScope {
    pub trapped: bool,
    pub click_outside: bool,
    pub return_to: Option<String>,
    pub autofocus: bool,
}

impl FocusScope {
    pub fn modal() -> Self {
        Self {
            trapped: true,
            click_outside: true,
            return_to: None,
            autofocus: true,
        }
    }

    pub fn return_to(mut self, trigger_key: impl Into<String>) -> Self {
        self.return_to = Some(trigger_key.into());
        self
    }

    pub fn click_outside(mut self, allow: bool) -> Self {
        self.click_outside = allow;
        self
    }

    pub fn autofocus(mut self, enabled: bool) -> Self {
        self.autofocus = enabled;
        self
    }

    /// Applies focus scope semantics to an element without a dismiss callback (pure focus trap).
    pub fn apply_trap(self, mut el: El) -> El {
        if self.autofocus {
            el = el.autofocus();
        }
        if let Some(trigger) = self.return_to {
            el = el.focus_return(trigger);
        }
        if self.trapped {
            el = el.modal_trap();
        }
        el
    }

    /// Applies focus scope semantics with a dismiss callback (Escape + optional outside click).
    pub fn apply_modal<S: 'static>(self, mut el: El, on_dismiss: impl Fn(&mut S) + 'static) -> El {
        if self.autofocus {
            el = el.autofocus();
        }
        if let Some(trigger) = self.return_to {
            el = el.focus_return(trigger);
        }
        if self.trapped {
            el = el.modal(self.click_outside, on_dismiss);
        }
        el
    }
}

/// Checks if a key event is an activation key (`Enter` or `Space`).
pub fn is_activation_key(ev: &KeyEvent) -> bool {
    if ev.mods.ctrl || ev.mods.alt || ev.mods.cmd || ev.mods.logo {
        return false;
    }
    matches!(&ev.key, Key::Enter) || matches!(&ev.key, Key::Text(t) if t == " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roving_skips_disabled_and_wraps() {
        let disabled = [false, true, false, true, false];
        let mut r = RovingState::new(RovingOrientation::Horizontal);
        assert_eq!(r.active, 0);
        assert_eq!(r.handle_command(NavCommand::Right, &disabled), Some(2));
        assert_eq!(r.handle_command(NavCommand::Right, &disabled), Some(4));
        assert_eq!(r.handle_command(NavCommand::Right, &disabled), Some(0));
        assert_eq!(r.handle_command(NavCommand::Left, &disabled), Some(4));
        assert_eq!(r.handle_command(NavCommand::Home, &disabled), Some(0));
        assert_eq!(r.handle_command(NavCommand::End, &disabled), Some(4));
    }

    #[test]
    fn roving_non_wrapping_stops_at_ends() {
        let disabled = [true, false, false, true];
        let mut r = RovingState::new(RovingOrientation::Vertical).loop_nav(false);
        assert_eq!(r.clamp(&disabled), Some(1));
        assert_eq!(r.handle_command(NavCommand::Up, &disabled), Some(1));
        assert_eq!(r.handle_command(NavCommand::Down, &disabled), Some(2));
        assert_eq!(r.handle_command(NavCommand::Down, &disabled), Some(2));
    }

    #[test]
    fn roving_ignores_cross_axis_and_handles_all_disabled() {
        let mut r = RovingState::new(RovingOrientation::Horizontal);
        assert_eq!(r.handle_command(NavCommand::Down, &[false, false]), None);
        assert_eq!(r.handle_command(NavCommand::Right, &[true, true]), None);
    }

    #[test]
    fn roving_gamepad_dpad_matches_arrows() {
        let disabled = [false, false, false];
        let mut r = RovingState::new(RovingOrientation::Vertical);
        assert_eq!(
            r.handle_gamepad(GamepadButton::DpadDown, &disabled),
            Some(1)
        );
        assert_eq!(r.handle_gamepad(GamepadButton::DpadUp, &disabled), Some(0));
    }
}
