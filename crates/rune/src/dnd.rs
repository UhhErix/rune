//! In-app pointer drag and drop: typed payloads, active drag state, and drop-to-reorder helpers
//! for vertical lists, horizontal strips and grids (including drop-into-folder targets).

use std::any::{Any, TypeId};
use std::rc::Rc;

use crate::color::Color;
use crate::el::{El, PointerEvent, el};

/// Key of the floating preview element the runtime mounts while an in-app pointer drag runs.
pub(crate) const DND_PREVIEW_KEY: &str = "__rune_dnd_preview";

/// Default pointer travel (points) before a press on an [`El::draggable`] element becomes a drag.
pub const DND_THRESHOLD: f32 = 4.0;

/// Type-erased, cheaply cloned payload carried by an in-app pointer drag ([`El::draggable`]).
#[derive(Clone)]
pub struct DragPayload {
    value: Rc<dyn Any>,
    type_id: TypeId,
}

impl DragPayload {
    /// Wraps `value` as a drag payload.
    pub fn new<T: 'static>(value: T) -> Self {
        Self {
            value: Rc::new(value),
            type_id: TypeId::of::<T>(),
        }
    }

    /// Borrows the payload as `T` when it carries that type.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }

    /// Whether the payload carries type `T`.
    pub fn is<T: 'static>(&self) -> bool {
        self.type_id == TypeId::of::<T>()
    }

    /// The `TypeId` of the value inside the payload.
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }
}

impl std::fmt::Debug for DragPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DragPayload")
            .field("type_id", &self.type_id)
            .finish()
    }
}

/// Snapshot of an active in-app pointer drag, readable during view builds via [`Cx::active_drag`](crate::Cx::active_drag).
#[derive(Clone, Debug)]
pub struct ActiveDrag {
    payload: DragPayload,
    source_key: Option<String>,
    position: [f32; 2],
    origin: [f32; 2],
    grab_offset: [f32; 2],
}

impl ActiveDrag {
    pub(crate) fn new(
        payload: DragPayload,
        source_key: Option<String>,
        position: [f32; 2],
        origin: [f32; 2],
        grab_offset: [f32; 2],
    ) -> Self {
        Self {
            payload,
            source_key,
            position,
            origin,
            grab_offset,
        }
    }

    /// The carried payload.
    pub fn payload(&self) -> &DragPayload {
        &self.payload
    }

    /// Borrows the carried payload as `T` when it matches.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.payload.downcast_ref::<T>()
    }

    /// Key of the source element that started the drag, when keyed.
    pub fn source_key(&self) -> Option<&str> {
        self.source_key.as_deref()
    }

    /// Current pointer position in logical window points.
    pub fn position(&self) -> [f32; 2] {
        self.position
    }

    /// Pointer position in logical window points where the press began.
    pub fn origin(&self) -> [f32; 2] {
        self.origin
    }

    /// Pointer offset from the source element's top-left corner at press time, in points.
    pub fn grab_offset(&self) -> [f32; 2] {
        self.grab_offset
    }
}

/// Axis along which [`DropReorder`] splits an item into before/after insertion slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderAxis {
    /// Stacked rows: top half inserts before, bottom half inserts after; horizontal indicator bar.
    Vertical,
    /// Horizontal strip: left half inserts before, right half inserts after; vertical indicator bar.
    Horizontal,
    /// Wrapping grid of cards: left edge inserts before, right edge inserts after; vertical indicator bar.
    Grid,
}

impl ReorderAxis {
    /// Whether this axis splits items horizontally (left/right edges) rather than vertically.
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Horizontal | Self::Grid)
    }
}

/// Where an active reorder drag would land relative to a hovered item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPlacement {
    /// Insert immediately before item `usize`.
    Before(usize),
    /// Insert immediately after item `usize`.
    After(usize),
    /// Drop into container/folder item `usize`.
    Into(usize),
}

/// Action returned by [`DropReorder::finish`] when a reorder drag is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderAction {
    /// Move item at `from` to destination index `to` (post-removal index for [`DropReorder::apply`]).
    Move { from: usize, to: usize },
    /// Drop item at `from` into container/folder item at `target`.
    DropInto { from: usize, target: usize },
}

/// App-owned helper for drop-to-reorder lists and grids, with optional drop-into-folder zones
/// and insertion indicator rendering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropReorder {
    axis: ReorderAxis,
    allow_into: bool,
    edge_fraction: f32,
    dragging: Option<usize>,
    placement: Option<DropPlacement>,
}

impl DropReorder {
    /// Creates a new reorder helper for `axis`.
    pub const fn new(axis: ReorderAxis) -> Self {
        Self {
            axis,
            allow_into: false,
            edge_fraction: 0.25,
            dragging: None,
            placement: None,
        }
    }

    /// Allow dropping onto the centre zone of items marked `can_drop_into` (such as folders).
    pub const fn allow_into(mut self, allow: bool) -> Self {
        self.allow_into = allow;
        self
    }

    /// Fraction (clamped to `0.1..=0.45`, default `0.25`) of an item's extent on each edge that
    /// resolves to `Before` / `After` when `allow_into` is enabled and the item accepts drops into it.
    pub fn edge_fraction(mut self, fraction: f32) -> Self {
        self.edge_fraction = if fraction.is_finite() {
            fraction.clamp(0.1, 0.45)
        } else {
            0.25
        };
        self
    }

    /// Begins tracking a drag of the item at `from`.
    pub fn start(&mut self, from: usize) {
        self.dragging = Some(from);
        self.placement = None;
    }

    /// Updates hover placement from a pointer coordinate along the reorder axis within item `index`.
    pub fn hover_item(
        &mut self,
        index: usize,
        local_pos: f32,
        item_extent: f32,
        can_drop_into: bool,
    ) {
        let Some(from) = self.dragging else { return };
        if index == from {
            self.placement = None;
            return;
        }
        let extent = item_extent.max(1.0);
        let t = (local_pos / extent).clamp(0.0, 1.0);
        let placement = if self.allow_into && can_drop_into {
            if t < self.edge_fraction {
                DropPlacement::Before(index)
            } else if t > 1.0 - self.edge_fraction {
                DropPlacement::After(index)
            } else {
                DropPlacement::Into(index)
            }
        } else if t < 0.5 {
            DropPlacement::Before(index)
        } else {
            DropPlacement::After(index)
        };
        self.placement = Some(placement);
    }

    /// Updates hover placement from a [`PointerEvent`] and item layout size `[w, h]`.
    pub fn hover_point(
        &mut self,
        index: usize,
        ev: PointerEvent,
        size: [f32; 2],
        can_drop_into: bool,
    ) {
        let (pos, extent) = if self.axis.is_horizontal() {
            (ev.x, size[0])
        } else {
            (ev.y, size[1])
        };
        self.hover_item(index, pos, extent, can_drop_into);
    }

    /// Clears placement when the pointer leaves item `index`.
    pub fn leave_item(&mut self, index: usize) {
        let matches = match self.placement {
            Some(DropPlacement::Before(i) | DropPlacement::After(i) | DropPlacement::Into(i)) => {
                i == index
            }
            None => false,
        };
        if matches {
            self.placement = None;
        }
    }

    /// Cancels the active drag and clears any indicator.
    pub fn cancel(&mut self) {
        self.dragging = None;
        self.placement = None;
    }

    /// Finishes the active drag and returns the resolved [`ReorderAction`], if it changes order
    /// or drops into a container.
    pub fn finish(&mut self) -> Option<ReorderAction> {
        let from = self.dragging.take()?;
        let placement = self.placement.take()?;
        match placement {
            DropPlacement::Into(target) if target != from => {
                Some(ReorderAction::DropInto { from, target })
            }
            DropPlacement::Before(index) => {
                let to = if from < index {
                    index.saturating_sub(1)
                } else {
                    index
                };
                (to != from).then_some(ReorderAction::Move { from, to })
            }
            DropPlacement::After(index) => {
                let to = if from < index { index } else { index + 1 };
                (to != from).then_some(ReorderAction::Move { from, to })
            }
            DropPlacement::Into(_) => None,
        }
    }

    /// Index of the item currently being dragged, if any.
    pub const fn dragging(&self) -> Option<usize> {
        self.dragging
    }

    /// Current resolved drop placement, if hovering a valid slot.
    pub const fn placement(&self) -> Option<DropPlacement> {
        self.placement
    }

    /// Insertion gap index (`0..=len`) when hovering a `Before` or `After` slot.
    pub const fn insertion_index(&self) -> Option<usize> {
        match self.placement {
            Some(DropPlacement::Before(i)) => Some(i),
            Some(DropPlacement::After(i)) => Some(i + 1),
            Some(DropPlacement::Into(_)) | None => None,
        }
    }

    /// Target container index when hovering a `DropPlacement::Into` zone.
    pub const fn into_target(&self) -> Option<usize> {
        match self.placement {
            Some(DropPlacement::Into(i)) => Some(i),
            _ => None,
        }
    }

    /// Axis of this helper.
    pub const fn axis(&self) -> ReorderAxis {
        self.axis
    }

    /// Moves element at `from` to post-removal index `to` in `items` (matching [`ReorderAction::Move`]).
    pub fn apply<T>(items: &mut Vec<T>, from: usize, to: usize) {
        if from >= items.len() || to >= items.len() || from == to {
            return;
        }
        let item = items.remove(from);
        items.insert(to, item);
    }

    /// Returns a positioned insertion indicator overlay for item `index` when `placement` targets
    /// its leading or trailing edge.
    pub fn indicator(&self, index: usize, color: Color) -> Option<El> {
        let before = match self.placement {
            Some(DropPlacement::Before(i)) if i == index => true,
            Some(DropPlacement::After(i)) if i == index => false,
            _ => return None,
        };
        let bar = reorder_indicator(self.axis, color);
        Some(match (self.axis.is_horizontal(), before) {
            (false, true) => bar.pin(Some(0.0), Some(-1.5), Some(0.0), None),
            (false, false) => bar.pin(Some(0.0), None, Some(0.0), Some(-1.5)),
            (true, true) => bar.pin(Some(-1.5), Some(0.0), None, Some(0.0)),
            (true, false) => bar.pin(None, Some(0.0), Some(-1.5), Some(0.0)),
        })
    }

    /// Wraps `item` at `index` with visual drop feedback: an insertion bar on its edge for
    /// `Before`/`After`, or an outline highlight when hovering `Into(index)`.
    pub fn wrap_item(&self, index: usize, item: El, accent: Color) -> El {
        let item = if self.into_target() == Some(index) {
            item.outline(2.0, 2.0, accent)
        } else {
            item
        };
        match self.indicator(index, accent) {
            Some(bar) => item.child(bar),
            None => item,
        }
    }
}

/// Builds a crisp rounded insertion indicator bar for `axis` in `color`.
pub fn reorder_indicator(axis: ReorderAxis, color: Color) -> El {
    let base = el()
        .key("__rune_dnd_indicator")
        .fill(color)
        .radius(2.0)
        .inert()
        .instant_changes();
    if axis.is_horizontal() {
        base.w(3.0).h_full()
    } else {
        base.h(3.0).w_full()
    }
}

#[cfg(test)]
#[path = "dnd_tests.rs"]
mod tests;
