//! Radix / React Aria behavior primitives: focus management, roving tabindex, typeahead,
//! LIFO dismiss layers, anchored positioning with flip & shift, ordered portals, AccessKit
//! helpers, and unified keyboard + controller navigation.

pub mod a11y;
pub mod controller;
pub mod dismiss;
pub mod focus;
pub mod portal;
pub mod position;
pub mod typeahead;

pub use a11y::{A11yRole, live_announcement, slider_a11y, tab_a11y, visually_hidden, with_role};
pub use controller::{GamepadButton, NavCommand, from_gamepad, from_key_event};
pub use dismiss::{DismissEntry, DismissPolicy, DismissStack};
pub use focus::{FocusScope, RovingOrientation, RovingState, is_activation_key};
pub use portal::{LayerSlot, PortalHost};
pub use position::{ComputedPosition, PositionSpec, Rect, compute_position};
pub use typeahead::TypeaheadBuffer;
