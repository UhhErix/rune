//! `rune-ui`: upstream-friendly, token-themable UI primitives, components, motion patterns,
//! clean-room GPU shader effects, and interactive vector charts for Rune.
//!
//! Components consume [`UiTheme`] (wrapping [`rune_kit::Kit`]) rather than hardcoded colors or
//! fonts, allowing downstream applications to layer bespoke themes (such as Void/Liquid Glass
//! or custom brand palettes) cleanly on top.

#![forbid(unsafe_code)]

pub mod charts;
pub mod components;
pub mod controls;
pub mod effects;
pub mod motion;
pub mod overlays;
pub mod primitives;
pub mod theme;

pub use rune_kit::{Kit, Placement, State};
pub use theme::{
    ChartPalette, EffectGate, FocusRingPolicy, GlassTokens, UiRadii, UiSprings, UiTheme,
};
