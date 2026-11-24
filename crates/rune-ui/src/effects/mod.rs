//! Clean-room GPU shader backgrounds & borders (`wgpu` WGSL), text animations, number ticker,
//! and marquee — all with zero-GPU-cost fallbacks and automatic pause while a game is running.

pub mod gpu;
pub mod text;
pub mod ticker;

pub use gpu::{
    BackgroundEffect, BackgroundKind, BorderEffect, BorderKind, EFFECT_MARK_BASE, EffectPalette,
    UiEffectsCompositor,
};
pub use text::{BlurInText, GradientText, ScrambleText, ShinyText, SplitText, scramble_frame};
pub use ticker::{ClickSpark, Marquee, NumberTicker};
