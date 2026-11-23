//! In-app pointer drag-and-drop state machine and drop target routing (`El::draggable`,
//! `El::drag_preview`, `El::on_drop`, `El::accepts_drop`, `El::on_drop_target_hover`,
//! `El::on_drop_target_move`).

use super::*;
use crate::dnd::{ActiveDrag, DND_PREVIEW_KEY, DragPayload};
use crate::el::dnd::{DndHoverFn, DndMoveFn, DndPreviewFn};
use crate::el::{DragEndFn, PointerEvent};

/// Target currently hovered by an in-app pointer drag: `(path, accepted)`.
type DndAim = (String, bool);

/// Data carried by an active drag session while the pointer moves, or moved into `DndEvent::End`
/// when the drag finishes or cancels.
pub(in crate::runtime) struct DraggingData {
    source_path: String,
    source_key: Option<String>,
    payload: DragPayload,
    preview: Option<DndPreviewFn>,
    preview_offset: Option<[f32; 2]>,
    origin: (f32, f32),
    grab_offset: (f32, f32),
    pos: (f32, f32),
    hovered: Option<(DndAim, Option<DndHoverFn>)>,
    on_end: Option<DragEndFn>,
}

/// One step of an in-app pointer drag queued onto the frame's action queue.
pub(in crate::runtime) enum DndEvent {
    Start(String),
    Move(f32, f32),
    End {
        drag: Box<DraggingData>,
        drop_at: Option<(f32, f32)>,
    },
}

/// Explicit state machine for an in-app pointer drag session, owned inside `PointerState`.
#[derive(Default)]
pub(in crate::runtime) enum PointerDrag {
    #[default]
    Idle,
    Armed {
        source_path: String,
        origin: (f32, f32),
        grab_offset: (f32, f32),
        threshold: f32,
    },
    Dragging(Box<DraggingData>),
}

impl PointerDrag {
    #[inline]
    pub(in crate::runtime) fn is_dragging(&self) -> bool {
        matches!(self, Self::Dragging(_))
    }

    #[inline]
    pub(in crate::runtime) fn is_armed(&self) -> bool {
        matches!(self, Self::Armed { .. })
    }

    #[inline]
    pub(in crate::runtime) fn active(&self) -> Option<ActiveDrag> {
        match self {
            Self::Dragging(d) => Some(ActiveDrag::new(
                d.payload.clone(),
                d.source_key.clone(),
                [d.pos.0, d.pos.1],
                [d.origin.0, d.origin.1],
                [d.grab_offset.0, d.grab_offset.1],
            )),
            _ => None,
        }
    }

    pub(in crate::runtime) fn hovered_key<'a>(
        &self,
        node: impl Fn(&str) -> Option<&'a Node>,
        accepted: bool,
    ) -> Option<String> {
        let Self::Dragging(d) = self else { return None };
        let Some(((path, a), _)) = d.hovered.as_ref() else {
            return None;
        };
        if *a != accepted {
            return None;
        }
        Some(
            node(path)
                .and_then(|n| n.key.clone())
                .unwrap_or_else(|| path.clone()),
        )
    }

    pub(in crate::runtime) fn forget_targets(&mut self) {
        if let Self::Dragging(d) = self {
            d.hovered = None;
        }
    }

    /// Builds the positioned floating drag preview element, if the active drag declared one.
    pub(in crate::runtime) fn preview_el(
        &self,
        states: crate::states::StatesRef<'_>,
    ) -> Option<El> {
        let Self::Dragging(d) = self else { return None };
        let build = d.preview.as_ref()?;
        let [ox, oy] = d
            .preview_offset
            .unwrap_or([-d.grab_offset.0, -d.grab_offset.1]);
        let (px, py) = (d.pos.0 + ox, d.pos.1 + oy);
        let content = build(states, &d.payload);
        Some(
            crate::el::el()
                .key(DND_PREVIEW_KEY)
                .absolute(px, py)
                .raise()
                .paint_order(i16::MAX)
                .instant_changes()
                .inert()
                .child(content),
        )
    }
}

impl RuntimeCore {
    /// Arms an in-app pointer drag on primary press down over a `draggable` element.
    pub(in crate::runtime) fn arm_dnd(
        &mut self,
        path: String,
        origin: (f32, f32),
        grab_offset: (f32, f32),
        threshold: f32,
    ) {
        self.pointer.dnd = PointerDrag::Armed {
            source_path: path,
            origin,
            grab_offset,
            threshold,
        };
    }

    /// Disarms any pending `Armed` in-app drag (leaves active drags intact).
    pub(in crate::runtime) fn disarm_dnd(&mut self) {
        if self.pointer.dnd.is_armed() {
            self.pointer.dnd = PointerDrag::Idle;
        }
    }

    /// Advances an armed or active in-app drag on pointer move; returns `true` when the in-app
    /// drag handled the move (suppressing normal `on_drag` / press movement).
    pub(in crate::runtime) fn dnd_pointer_move(
        &mut self,
        app: &dyn ViewDriver,
        x: f32,
        y: f32,
    ) -> bool {
        match &mut self.pointer.dnd {
            PointerDrag::Idle => false,
            PointerDrag::Armed {
                source_path,
                origin,
                grab_offset,
                threshold,
            } => {
                let (ox, oy) = *origin;
                let (gx, gy) = *grab_offset;
                if (x - ox).hypot(y - oy) < *threshold {
                    return false;
                }
                let source_path = source_path.clone();
                let Some(i) = self.live_index(&source_path) else {
                    self.pointer.dnd = PointerDrag::Idle;
                    return false;
                };
                let (source_key, src, on_end) = {
                    let n = &self.tree.nodes[i];
                    let Some(src) = n.handlers.dnd_source.as_deref().cloned() else {
                        self.pointer.dnd = PointerDrag::Idle;
                        return false;
                    };
                    (n.key.clone(), src, n.handlers.on_drag_end.clone())
                };
                let Some(payload) = src.payload.as_ref().and_then(|f| f(app.states_ref())) else {
                    self.pointer.dnd = PointerDrag::Idle;
                    return false;
                };
                // Drop the held press so release never fires `on_press` once dragging starts.
                self.drop_press();
                self.pointer.dnd = PointerDrag::Dragging(Box::new(DraggingData {
                    source_path: source_path.clone(),
                    source_key,
                    payload,
                    preview: src.preview,
                    preview_offset: src.preview_offset,
                    origin: (ox, oy),
                    grab_offset: (gx, gy),
                    pos: (x, y),
                    hovered: None,
                    on_end,
                }));
                self.frame
                    .queue
                    .push(Action::Dnd(DndEvent::Start(source_path)));
                self.frame.queue.push(Action::Dnd(DndEvent::Move(x, y)));
                true
            }
            PointerDrag::Dragging(d) => {
                d.pos = (x, y);
                self.frame.queue.push(Action::Dnd(DndEvent::Move(x, y)));
                true
            }
        }
    }

    /// Completes an active in-app drag on primary pointer release; returns `true` when consumed.
    pub(in crate::runtime) fn dnd_pointer_up(&mut self, x: f32, y: f32) -> bool {
        match std::mem::take(&mut self.pointer.dnd) {
            PointerDrag::Idle | PointerDrag::Armed { .. } => false,
            PointerDrag::Dragging(mut drag) => {
                drag.pos = (x, y);
                self.frame.queue.push(Action::Dnd(DndEvent::End {
                    drag,
                    drop_at: Some((x, y)),
                }));
                true
            }
        }
    }

    /// Cancels an active in-app drag (e.g., on Escape, blur, or modal open); returns `true` if an
    /// active drag was cancelled.
    pub(in crate::runtime) fn cancel_dnd(&mut self) -> bool {
        match std::mem::take(&mut self.pointer.dnd) {
            PointerDrag::Idle | PointerDrag::Armed { .. } => false,
            PointerDrag::Dragging(drag) => {
                self.frame.queue.push(Action::Dnd(DndEvent::End {
                    drag,
                    drop_at: None,
                }));
                true
            }
        }
    }

    /// Whether node `i` is the floating drag preview or sits inside it.
    pub(in crate::runtime) fn is_dnd_preview(&self, i: usize) -> bool {
        std::iter::successors(Some(i), |&j| self.tree.nodes[j].parent)
            .any(|j| self.tree.nodes[j].key.as_deref() == Some(DND_PREVIEW_KEY))
    }

    /// Whether an in-app pointer drag is currently active.
    #[inline]
    pub fn is_dragging_payload(&self) -> bool {
        self.pointer.dnd.is_dragging()
    }

    /// Key (else path) of the accepted drop target currently hovered by an in-app pointer drag.
    pub fn dnd_hovered(&self) -> Option<String> {
        self.pointer.dnd.hovered_key(|p| self.node(p), true)
    }

    /// Key (else path) of the rejected drop target currently hovered by an in-app pointer drag.
    pub fn dnd_rejected_hovered(&self) -> Option<String> {
        self.pointer.dnd.hovered_key(|p| self.node(p), false)
    }

    /// Current layout/target rect `[x0, y0, x1, y1]` of the floating drag preview, if mounted.
    pub fn dnd_preview_rect(&self) -> Option<[f32; 4]> {
        self.tree
            .nodes
            .iter()
            .enumerate()
            .find(|&(i, n)| !self.subtree_exiting(i) && n.key.as_deref() == Some(DND_PREVIEW_KEY))
            .map(|(_, n)| n.target)
    }

    /// Finds the innermost drop target under `(x, y)` that accepts or rejects `payload`.
    fn dnd_target(
        &self,
        states: crate::states::StatesRef<'_>,
        payload: &DragPayload,
        at: (f32, f32),
    ) -> Option<(usize, bool)> {
        let (x, y) = at;
        if !self.in_viewport(x, y) {
            return None;
        }
        let ok = |id: NodeId| {
            self.tree.node_index.get(&id).copied().filter(|&i| {
                i < self.tree.nodes.len()
                    && !self.subtree_exiting(i)
                    && self.allowed(i)
                    && !self.is_dnd_preview(i)
            })
        };
        let top = self
            .hit
            .top_drop_or_hit(|id, r| self.hit_contains(id, r, x, y), ok);
        if top.is_none() && self.active_modal().is_some() {
            return None;
        }
        let root = self.active_modal().unwrap_or_else(|| self.app_root());
        std::iter::successors(top, |&i| self.tree.nodes[i].parent)
            .chain(std::iter::once(root).filter(|&r| r < self.tree.nodes.len()))
            .filter(|&i| self.allowed(i) && !self.subtree_exiting(i) && !self.is_dnd_preview(i))
            .find_map(|i| {
                let target = self.tree.nodes[i].handlers.dnd_target.as_ref()?;
                if target.accepts_payload(states, payload) {
                    Some((i, true))
                } else if target.claims_rejected(payload) {
                    Some((i, false))
                } else {
                    None
                }
            })
    }

    fn dnd_hover_fn(&self, (path, accepted): &DndAim) -> Option<DndHoverFn> {
        self.node(path)
            .and_then(|n| n.handlers.dnd_target.as_ref()?.hover_fn(*accepted))
    }

    fn dnd_move_fn(&self, (path, accepted): &DndAim) -> Option<DndMoveFn> {
        self.node(path)
            .filter(|_| *accepted)
            .and_then(|n| n.handlers.dnd_target.as_ref()?.on_move.clone())
    }

    fn dnd_point(&self, path: &str, at: (f32, f32)) -> Option<PointerEvent> {
        let i = self.live_index(path)?;
        let [lx, ly] = self.to_local(i, at.0, at.1);
        Some(PointerEvent {
            x: lx,
            y: ly,
            window_x: at.0,
            window_y: at.1,
        })
    }

    fn set_dnd_hover(
        &mut self,
        app: &mut dyn AppDriver,
        target: Option<DndAim>,
        payload: &DragPayload,
        at: Option<(f32, f32)>,
        tx: &mut Tx,
    ) -> bool {
        let same = match &self.pointer.dnd {
            PointerDrag::Dragging(d) => d.hovered.as_ref().map(|(aim, _)| aim) == target.as_ref(),
            _ => target.is_none(),
        };
        if same {
            return false;
        }
        let new = target.map(|aim| {
            let f = self.dnd_hover_fn(&aim);
            (aim, f)
        });
        let old = match &mut self.pointer.dnd {
            PointerDrag::Dragging(d) => std::mem::replace(&mut d.hovered, new.clone()),
            _ => None,
        };
        let mut ran = false;
        if let Some((aim, kept)) = old
            && let Some(f) = self.dnd_hover_fn(&aim).or(kept)
        {
            tx.drop_point = None;
            f(app.states(), None, tx);
            ran = true;
        }
        if let Some((aim, kept)) = new
            && let Some(f) = self.dnd_hover_fn(&aim).or(kept)
        {
            tx.drop_point = at.and_then(|p| self.dnd_point(&aim.0, p));
            f(app.states(), Some(payload), tx);
            tx.drop_point = None;
            ran = true;
        }
        ran
    }

    fn finish_dnd(
        &mut self,
        app: &mut dyn AppDriver,
        drag: DraggingData,
        drop_at: Option<(f32, f32)>,
        tx: &mut Tx,
    ) -> bool {
        let DraggingData {
            source_path,
            payload,
            hovered,
            on_end,
            ..
        } = drag;
        let target = drop_at.and_then(|at| {
            self.dnd_target(app.states_ref(), &payload, at)
                .and_then(|(i, accepted)| {
                    accepted.then(|| (i, self.tree.nodes[i].path.clone(), at))
                })
        });
        // If hovered over a different target than the drop target (or cancelling), leave it first.
        let mut hovered = hovered;
        if let Some(((h_path, _), _)) = hovered.as_ref()
            && target
                .as_ref()
                .is_none_or(|(_, t_path, _)| t_path != h_path)
            && let Some((aim, kept)) = hovered.take()
            && let Some(f) = self.dnd_hover_fn(&aim).or(kept)
        {
            tx.drop_point = None;
            f(app.states(), None, tx);
        }
        let mut outcome = crate::DragOutcome::Cancelled;
        if let Some((i, path, at)) = target {
            if let Some(f) = self.tree.nodes[i]
                .handlers
                .dnd_target
                .as_ref()
                .and_then(|t| t.on_drop.clone())
            {
                tx.drop_point = self.dnd_point(&path, at);
                f(app.states(), &payload, tx);
                tx.drop_point = None;
                outcome = crate::DragOutcome::Moved;
            }
        }
        // Leave the drop target's hover state after delivering `on_drop`, so drop handlers
        // (such as `DropReorder::finish`) still observe the active hover placement.
        if let Some((aim, kept)) = hovered
            && let Some(f) = self.dnd_hover_fn(&aim).or(kept)
        {
            tx.drop_point = None;
            f(app.states(), None, tx);
        }
        let end_fn = self
            .node(&source_path)
            .and_then(|n| n.handlers.on_drag_end.clone())
            .or(on_end);
        if let Some(f) = end_fn {
            f(app.states(), outcome, tx);
        }
        // Always commit so the floating drag preview unmounts and `Cx::active_drag` clears.
        true
    }

    pub(in crate::runtime) fn run_dnd(
        &mut self,
        app: &mut dyn AppDriver,
        event: DndEvent,
        tx: &mut Tx,
    ) -> bool {
        match event {
            DndEvent::Start(path) => {
                tx.commit = tx.commit.instant();
                if let Some(f) = self
                    .node(&path)
                    .and_then(|n| n.handlers.on_drag_start.clone())
                {
                    f(app.states(), tx);
                }
                true
            }
            DndEvent::Move(x, y) => {
                let payload = match &self.pointer.dnd {
                    PointerDrag::Dragging(d) => d.payload.clone(),
                    _ => return false,
                };
                tx.commit = tx.commit.instant();
                let aim = self
                    .dnd_target(app.states_ref(), &payload, (x, y))
                    .map(|(i, accepted)| (self.tree.nodes[i].path.clone(), accepted));
                let _ = self.set_dnd_hover(app, aim.clone(), &payload, Some((x, y)), tx);
                if let Some(aim) = aim
                    && let Some(f) = self.dnd_move_fn(&aim)
                    && let Some(p) = self.dnd_point(&aim.0, (x, y))
                {
                    tx.drop_point = Some(p);
                    f(app.states(), &payload, p, tx);
                    tx.drop_point = None;
                }
                true
            }
            DndEvent::End { drag, drop_at } => self.finish_dnd(app, *drag, drop_at, tx),
        }
    }
}
