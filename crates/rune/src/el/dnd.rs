//! In-app pointer drag-and-drop element builders: `El::draggable`, `El::drag_preview`,
//! `El::on_drop`, `El::accepts_drop`, `El::on_drop_target_hover`, `El::on_drop_target_move`
//! and `El::on_drop_target_rejected_hover`.

use std::any::TypeId;
use std::rc::Rc;

use crate::dnd::DragPayload;
use crate::el::{El, PointerEvent, Tx};
use crate::states::downcast;

pub(crate) type DndPayloadFn = Rc<dyn Fn(crate::states::StatesRef<'_>) -> Option<DragPayload>>;
pub(crate) type DndPreviewFn = Rc<dyn Fn(crate::states::StatesRef<'_>, &DragPayload) -> El>;
pub(crate) type DndAcceptsFn = Rc<dyn Fn(crate::states::StatesRef<'_>, &DragPayload) -> bool>;
pub(crate) type DndDropFn = Rc<dyn Fn(crate::states::States<'_>, &DragPayload, &mut Tx)>;
pub(crate) type DndHoverFn = Rc<dyn Fn(crate::states::States<'_>, Option<&DragPayload>, &mut Tx)>;
pub(crate) type DndMoveFn =
    Rc<dyn Fn(crate::states::States<'_>, &DragPayload, PointerEvent, &mut Tx)>;

/// In-app drag source specification attached by [`El::draggable`] and [`El::drag_preview`].
#[derive(Clone, Default)]
pub(crate) struct DndSourceSpec {
    pub payload: Option<DndPayloadFn>,
    pub preview: Option<DndPreviewFn>,
    pub preview_offset: Option<[f32; 2]>,
}

/// In-app drop target specification attached by [`El::on_drop`] and friends.
#[derive(Clone, Default)]
pub(crate) struct DndTargetSpec {
    pub payload_type: Option<TypeId>,
    pub on_drop: Option<DndDropFn>,
    pub accepts: Option<DndAcceptsFn>,
    pub on_hover: Option<DndHoverFn>,
    pub on_move: Option<DndMoveFn>,
    pub on_reject_hover: Option<DndHoverFn>,
}

impl DndTargetSpec {
    /// Whether this target matches the payload's type and passes `accepts_drop`.
    pub(crate) fn accepts_payload(
        &self,
        states: crate::states::StatesRef<'_>,
        payload: &DragPayload,
    ) -> bool {
        if self.on_drop.is_none() && self.on_hover.is_none() && self.on_move.is_none() {
            return false;
        }
        if self.payload_type.is_some_and(|t| t != payload.type_id()) {
            return false;
        }
        self.accepts.as_ref().is_none_or(|f| f(states, payload))
    }

    /// Whether this target matches the payload's type and wants to claim rejected drags.
    pub(crate) fn claims_rejected(&self, payload: &DragPayload) -> bool {
        self.on_reject_hover.is_some() && self.payload_type.is_none_or(|t| t == payload.type_id())
    }

    pub(crate) fn hover_fn(&self, accepted: bool) -> Option<DndHoverFn> {
        if accepted {
            self.on_hover.clone()
        } else {
            self.on_reject_hover.clone()
        }
    }
}

fn state_ref<S: 'static>(s: crate::states::StatesRef<'_>) -> &S {
    s.cast::<S>()
        .expect("handler registered for a different state type than the app's")
}

impl El {
    /// Make this element an in-app pointer drag source carrying a typed payload `T`.
    ///
    /// Once the pointer moves past [`El::drag_threshold`] (default [`crate::DND_THRESHOLD`], 4 pt)
    /// from the press, `payload` runs: returning `Some(value)` starts the drag (suppressing
    /// `on_press` on release), while `None` leaves the press as a normal click.
    ///
    /// Pair with [`El::drag_preview`] for a floating preview that follows the pointer,
    /// [`El::on_drag_start`] / [`El::on_drag_end`] for lifecycle callbacks, and [`El::on_drop`]
    /// on drop targets. Pressing Escape cancels an active drag.
    pub fn draggable<S: 'static, T: 'static>(
        mut self,
        payload: impl Fn(&S) -> Option<T> + 'static,
    ) -> Self {
        let src = self.handlers.dnd_source.get_or_insert_default();
        src.payload = Some(Rc::new(move |s| {
            payload(state_ref::<S>(s)).map(DragPayload::new)
        }));
        self
    }

    /// Floating preview element drawn above the view while this [`El::draggable`] element is
    /// dragged. Positioned to follow the pointer at its grab offset (or [`El::drag_preview_offset`])
    /// without blocking hit-testing on drop targets underneath.
    pub fn drag_preview<S: 'static>(mut self, build: impl Fn(&S) -> El + 'static) -> Self {
        let src = self.handlers.dnd_source.get_or_insert_default();
        src.preview = Some(Rc::new(move |s, _| build(state_ref::<S>(s))));
        self
    }

    /// Like [`El::drag_preview`], passing the active typed payload `&T` alongside state `&S`.
    pub fn drag_preview_with<S: 'static, T: 'static>(
        mut self,
        build: impl Fn(&S, &T) -> El + 'static,
    ) -> Self {
        let src = self.handlers.dnd_source.get_or_insert_default();
        src.preview = Some(Rc::new(move |s, p| {
            let value = p
                .downcast_ref::<T>()
                .expect("drag_preview_with payload type must match draggable");
            build(state_ref::<S>(s), value)
        }));
        self
    }

    /// Overrides the drag preview's offset from the pointer in points (`[dx, dy]`), instead of
    /// anchoring at the local point where the pointer grabbed the source element.
    pub fn drag_preview_offset(mut self, offset: [f32; 2]) -> Self {
        let src = self.handlers.dnd_source.get_or_insert_default();
        src.preview_offset = Some(offset);
        self
    }

    /// Accept in-app pointer drags carrying payload `T` dropped on this element.
    ///
    /// The innermost target under the pointer that matches `T` and passes [`El::accepts_drop`]
    /// receives the drop. Inside `f`, [`Tx::drop_point`] reports the local and window drop point.
    pub fn on_drop<S: 'static, T: 'static>(
        mut self,
        f: impl Fn(&mut S, &T, &mut Tx) + 'static,
    ) -> Self {
        let target = self.handlers.dnd_target.get_or_insert_default();
        target.payload_type = Some(TypeId::of::<T>());
        target.on_drop = Some(Rc::new(move |s, p, tx| {
            if let Some(v) = p.downcast_ref::<T>() {
                f(downcast::<S>(s), v, tx);
            }
        }));
        self
    }

    /// Predicate deciding whether this drop target accepts an in-app drag carrying `T`.
    /// Returning `false` passes the drag through to an accepting ancestor unless
    /// [`El::on_drop_target_rejected_hover`] is set.
    pub fn accepts_drop<S: 'static, T: 'static>(
        mut self,
        accepts: impl Fn(&S, &T) -> bool + 'static,
    ) -> Self {
        let target = self.handlers.dnd_target.get_or_insert_default();
        target.payload_type = Some(TypeId::of::<T>());
        target.accepts = Some(Rc::new(move |s, p| {
            p.downcast_ref::<T>()
                .is_some_and(|v| accepts(state_ref::<S>(s), v))
        }));
        self
    }

    /// Called with `Some(&payload)` when an accepted in-app drag enters this target, and `None`
    /// when it leaves, drops, is cancelled by Escape, or unmounts. Inside `Some`, [`Tx::drop_point`]
    /// gives the pointer's entry point on the element.
    pub fn on_drop_target_hover<S: 'static, T: 'static>(
        mut self,
        f: impl Fn(&mut S, Option<&T>, &mut Tx) + 'static,
    ) -> Self {
        let target = self.handlers.dnd_target.get_or_insert_default();
        target.payload_type = Some(TypeId::of::<T>());
        target.on_hover = Some(Rc::new(move |s, p, tx| {
            f(
                downcast::<S>(s),
                p.and_then(DragPayload::downcast_ref::<T>),
                tx,
            );
        }));
        self
    }

    /// Called as an accepted in-app drag moves across this target, receiving the payload and local
    /// [`PointerEvent`] (use with [`crate::DropReorder::hover_point`] for live insertion indicators).
    pub fn on_drop_target_move<S: 'static, T: 'static>(
        mut self,
        f: impl Fn(&mut S, &T, PointerEvent, &mut Tx) + 'static,
    ) -> Self {
        let target = self.handlers.dnd_target.get_or_insert_default();
        target.payload_type = Some(TypeId::of::<T>());
        target.on_move = Some(Rc::new(move |s, p, ev, tx| {
            if let Some(v) = p.downcast_ref::<T>() {
                f(downcast::<S>(s), v, ev, tx);
            }
        }));
        self
    }

    /// Called with `Some(&payload)` when a drag carrying `T` is rejected by [`El::accepts_drop`]
    /// over this target (stopping it from falling through to ancestors), and `None` when it leaves,
    /// drops, or cancels.
    pub fn on_drop_target_rejected_hover<S: 'static, T: 'static>(
        mut self,
        f: impl Fn(&mut S, Option<&T>, &mut Tx) + 'static,
    ) -> Self {
        let target = self.handlers.dnd_target.get_or_insert_default();
        target.payload_type = Some(TypeId::of::<T>());
        target.on_reject_hover = Some(Rc::new(move |s, p, tx| {
            f(
                downcast::<S>(s),
                p.and_then(DragPayload::downcast_ref::<T>),
                tx,
            );
        }));
        self
    }
}
