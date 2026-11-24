//! Framer-Motion-style layout animations, shared-element transitions (`El::matched`),
//! staggered presence lists, and 1:1 drag/fling gestures on `rune-motion`.

pub mod gesture;
pub mod layout;
pub mod shared;

pub use gesture::{DragAxis, DragBounds, DragGestureState, FlingResult};
pub use layout::{MotionBox, MotionPreset, StaggerList};
pub use shared::{SharedElement, shared_indicator};
