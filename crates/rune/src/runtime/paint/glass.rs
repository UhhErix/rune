//! Paint-pass assembly for Liquid Glass (`El::liquid_glass`, `El::glass_container`, `El::glass_group`,
//! `El::glass_id`, `El::glass_union`): builds the per-frame `GlassShapeGpu` side table, groups
//! container members into single `KIND_LIQUID_GLASS` records placed before their first member's
//! content, hoists glass out of ancestor layers (alpha fade, `El::cache`, `El::blur`, blend) so it
//! samples the real backdrop, handles solid / contrast fallbacks, and applies interactive press/hover
//! springs, gel stretch, drag follow, touch glow and materialize ramps.

use std::collections::HashMap;

use super::{intersect, premul};
use crate::color::Color;
use crate::el::{GlassPolicy, GlassVariant, LiquidGlass};
use crate::render::liquid_glass::{
    FLAG_CURSOR_LIGHT, FLAG_HAS_TINT, FLAG_INCREASED_CONTRAST, FLAG_LARGE,
    FLAG_REDUCED_TRANSPARENCY, GlassProbeRequest, GlassShapeGpu, LiquidGlassFrame,
    MAX_SHAPES_PER_DRAW, MODE_CLEAR, MODE_CUSTOM, MODE_REGULAR, apple,
};
use crate::render::{Instance, KIND_LIQUID_GLASS, NO_CLIP};
use crate::runtime::RuntimeCore;
use crate::runtime::motion::props::{
    P_GLASS_GLOW, P_GLASS_HOVER_X, P_GLASS_HOVER_Y, P_GLASS_MATERIALIZE, P_GLASS_PRESS, P_HOVER,
};
use crate::runtime::types::Node;
use rune_motion::NodeId;

/// Deferred insertion of one or more `KIND_LIQUID_GLASS` records before `insert_before` in
/// `RuntimeCore::instances` (used for hoisted glass and multi-chunk containers).
struct GlassSplice {
    insert_before: usize,
    instances: Vec<Instance>,
}

/// Active `El::glass_container` / `El::glass_group` open on the paint stack.
struct OpenContainer {
    node_index: usize,
    spacing_pt: f32,
    group_id: u32,
    /// Where the first member of this container painted (in `instances`), or `None` if no member yet.
    placeholder_at: Option<usize>,
    /// If the container's first member was inside an ancestor layer, the outermost layer's `KIND_LAYER` index.
    hoist_before: Option<usize>,
    shapes: Vec<GlassShapeGpu>,
    union_ids: HashMap<String, u32>,
    still_rgba: [f32; 4],
    opacity: f32,
    clip: [f32; 4],
    fade: [u32; 4],
    k_px: f32,
    max_sigma_px: f32,
    max_refract_px: f32,
    bounds: [f32; 4],
    has_bounds: bool,
}

/// Per-frame Liquid Glass paint state owned by `PaintState`.
#[derive(Default)]
pub(in crate::runtime) struct GlassPaintState {
    pub(in crate::runtime) frame: LiquidGlassFrame,
    containers: Vec<OpenContainer>,
    pending_splices: Vec<GlassSplice>,
    next_group_id: u32,
    /// Nodes seen with `liquid_glass` in the previous frame (for unmatched materialize detection).
    seen_ids: HashMap<NodeId, bool>,
    /// Nodes seen in the current frame.
    curr_ids: HashMap<NodeId, bool>,
    /// Last pointer contact point (points, window space) per interactive glass node for touch glow.
    pub(in crate::runtime) touch_points: HashMap<NodeId, [f32; 2]>,
    /// Debug once-warning flag for elements carrying both `backdrop_blur` and `liquid_glass`.
    warned_both: bool,
}

impl GlassPaintState {
    pub(in crate::runtime) fn new() -> Self {
        Self {
            frame: LiquidGlassFrame::new(),
            ..Self::default()
        }
    }

    pub(in crate::runtime) fn begin_frame(&mut self, pointer_px: [f32; 4]) {
        self.frame.clear();
        self.frame.pointer = pointer_px;
        self.containers.clear();
        self.pending_splices.clear();
        self.next_group_id = 0;
        std::mem::swap(&mut self.seen_ids, &mut self.curr_ids);
        self.curr_ids.clear();
    }

    pub(in crate::runtime) fn open_container(&mut self, node_index: usize, spacing_pt: f32) {
        self.next_group_id += 1;
        self.containers.push(OpenContainer {
            node_index,
            spacing_pt,
            group_id: self.next_group_id,
            placeholder_at: None,
            hoist_before: None,
            shapes: Vec::new(),
            union_ids: HashMap::new(),
            still_rgba: [0.0; 4],
            opacity: 1.0,
            clip: NO_CLIP,
            fade: [0; 4],
            k_px: 1.0,
            max_sigma_px: 0.0,
            max_refract_px: 0.0,
            bounds: [0.0; 4],
            has_bounds: false,
        });
    }

    pub(in crate::runtime) fn close_containers_to_parent(
        &mut self,
        nodes: &[Node],
        instances: &mut [Instance],
        node_parent: Option<usize>,
    ) {
        while let Some(top) = self.containers.last() {
            let ancestor = top.node_index;
            let mut parent = node_parent;
            while let Some(p) = parent
                && p != ancestor
            {
                parent = nodes[p].parent;
            }
            if parent == Some(ancestor) {
                break;
            }
            if let Some(c) = self.containers.pop() {
                self.finish_container(c, instances);
            }
        }
    }

    pub(in crate::runtime) fn close_all_containers(&mut self, instances: &mut [Instance]) {
        while let Some(c) = self.containers.pop() {
            self.finish_container(c, instances);
        }
    }

    fn finish_container(&mut self, c: OpenContainer, instances: &mut [Instance]) {
        if c.shapes.is_empty() {
            return;
        }
        let spacing_px = c.spacing_pt * c.k_px;
        let mut chunk_records: Vec<Instance> = Vec::new();
        for chunk in c.shapes.chunks(MAX_SHAPES_PER_DRAW) {
            let first_shape = self.frame.shapes.len();
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            let mut max_sigma = 0.0f32;
            let mut max_refract = 0.0f32;
            for sh in chunk {
                let pad = sh.extra[0] * 1.6 + sh.extra[1].abs() + spacing_px + 2.0;
                bounds[0] = bounds[0].min(sh.rect[0] - pad);
                bounds[1] = bounds[1].min(sh.rect[1] - pad);
                bounds[2] = bounds[2].max(sh.rect[0] + sh.rect[2] + pad);
                bounds[3] = bounds[3].max(sh.rect[1] + sh.rect[3] + pad);
                max_sigma = max_sigma.max(sh.tone[3]);
                max_refract = max_refract.max(sh.bend[1]);
                self.frame.shapes.push(*sh);
            }
            let rect = [
                bounds[0],
                bounds[1],
                (bounds[2] - bounds[0]).max(0.0),
                (bounds[3] - bounds[1]).max(0.0),
            ];
            let primary_radii = chunk[0].radii;
            chunk_records.push(Instance {
                rect,
                radii: primary_radii,
                fill: [c.still_rgba[0], c.still_rgba[1], c.still_rgba[2], c.opacity],
                border: c.still_rgba,
                params: [max_sigma, KIND_LIQUID_GLASS, first_shape as f32, c.k_px],
                uv: [
                    chunk.len() as f32,
                    spacing_px,
                    max_refract,
                    c.group_id as f32,
                ],
                clip: c.clip,
                fade: c.fade,
            });
        }
        if let Some(hoist_idx) = c.hoist_before {
            self.pending_splices.push(GlassSplice {
                insert_before: hoist_idx,
                instances: chunk_records,
            });
        } else if let Some(slot) = c.placeholder_at {
            let mut iter = chunk_records.into_iter();
            if let Some(first) = iter.next() {
                instances[slot] = first;
            }
            let rest: Vec<Instance> = iter.collect();
            if !rest.is_empty() {
                self.pending_splices.push(GlassSplice {
                    insert_before: slot + 1,
                    instances: rest,
                });
            }
        }
    }

    /// Applies all deferred hoisted / overflow glass record insertions in a single linear pass,
    /// keeping `surfaces` and `marks` indices consistent.
    pub(in crate::runtime) fn flush_splices(
        &mut self,
        instances: &mut Vec<Instance>,
        surfaces: &mut [crate::native_surface::SurfaceDraw],
        marks: &mut Vec<u32>,
        track_marks: bool,
    ) {
        if self.pending_splices.is_empty() {
            return;
        }
        self.pending_splices.sort_by_key(|s| s.insert_before);
        let extra: usize = self.pending_splices.iter().map(|s| s.instances.len()).sum();
        let old_len = instances.len();
        let mut out = Vec::with_capacity(old_len + extra);
        let mut out_marks = if track_marks {
            Vec::with_capacity(old_len + extra)
        } else {
            Vec::new()
        };
        let mut shifts: Vec<(usize, usize)> = Vec::with_capacity(self.pending_splices.len());
        let mut cursor = 0usize;
        for splice in self.pending_splices.drain(..) {
            let at = splice.insert_before.min(old_len);
            if at > cursor {
                out.extend_from_slice(&instances[cursor..at]);
                if track_marks {
                    let end = at.min(marks.len());
                    if end > cursor {
                        out_marks.extend_from_slice(&marks[cursor..end]);
                    }
                    out_marks.resize(out.len(), 0);
                }
                cursor = at;
            }
            let added = splice.instances.len();
            shifts.push((at, added));
            out.extend(splice.instances);
            if track_marks {
                out_marks.resize(out.len(), 0);
            }
        }
        if cursor < old_len {
            out.extend_from_slice(&instances[cursor..]);
            if track_marks {
                let end = old_len.min(marks.len());
                if end > cursor {
                    out_marks.extend_from_slice(&marks[cursor..end]);
                }
                out_marks.resize(out.len(), 0);
            }
        }
        // Adjust every native surface's `before_instance` by all splices inserted at or before it.
        for surf in surfaces.iter_mut() {
            let mut delta = 0u32;
            for &(at, added) in &shifts {
                if (at as u32) <= surf.before_instance {
                    delta += added as u32;
                }
            }
            surf.before_instance += delta;
        }
        *instances = out;
        if track_marks {
            *marks = out_marks;
        }
    }
}

#[inline]
fn to_srgb_rgb(c: Color) -> [f32; 3] {
    [
        crate::color::srgb_encode(c.r.clamp(0.0, 1.0)),
        crate::color::srgb_encode(c.g.clamp(0.0, 1.0)),
        crate::color::srgb_encode(c.b.clamp(0.0, 1.0)),
    ]
}

#[inline]
fn to_srgb_premul(c: Color) -> [f32; 4] {
    let rgb = to_srgb_rgb(c);
    let a = c.a.clamp(0.0, 1.0);
    [rgb[0] * a, rgb[1] * a, rgb[2] * a, a]
}

#[inline]
fn to_srgb_straight(c: Color) -> [f32; 4] {
    let rgb = to_srgb_rgb(c);
    [rgb[0], rgb[1], rgb[2], c.a.clamp(0.0, 1.0)]
}

/// Decides whether a glass element renders as live `KIND_LIQUID_GLASS` or falls back to a solid
/// uber `Rect` (+ contrast rim under a system contrast theme).
#[derive(Clone, Copy)]
enum GlassResolution {
    /// Render live glass, optionally with §0 accessibility parameter overrides.
    Live {
        reduced_transparency: bool,
        increased_contrast: bool,
    },
    /// Render solid fallback fill + optional rim colour (zero GPU glass pass).
    Solid { fill: Color, rim: Option<Color> },
}

fn resolve_glass(
    glass: &LiquidGlass,
    policy: GlassPolicy,
    sys_reduced_transparency: bool,
    sys_increased_contrast: bool,
    sys_contrast: Option<crate::SystemContrast>,
) -> GlassResolution {
    if let Some(contrast) = sys_contrast {
        return GlassResolution::Solid {
            fill: contrast.window,
            rim: Some(contrast.text),
        };
    }
    match policy {
        GlassPolicy::Solid => GlassResolution::Solid {
            fill: glass.still,
            rim: None,
        },
        GlassPolicy::Glass => GlassResolution::Live {
            reduced_transparency: false,
            increased_contrast: false,
        },
        GlassPolicy::FollowSystem => {
            let rt = sys_reduced_transparency;
            let ic = sys_increased_contrast;
            if glass.variant == GlassVariant::Custom && (rt || ic) {
                GlassResolution::Solid {
                    fill: glass.still,
                    rim: None,
                }
            } else {
                GlassResolution::Live {
                    reduced_transparency: rt,
                    increased_contrast: ic,
                }
            }
        }
    }
}

impl RuntimeCore {
    /// Emits a Liquid Glass shape (or solid fallback) for node `i`, handling containers, hoisting
    /// out of ancestor layers, interactive spring transforms/glow, materialize ramps, and luminance
    /// probe registration. Returns the signed corner radius for `blur_hole` (so outer shadows skip
    /// the glass interior).
    #[allow(clippy::too_many_arguments)]
    pub(in crate::runtime) fn paint_liquid_glass(
        &mut self,
        i: usize,
        glass: LiquidGlass,
        (rect, radii, continuous): ([f32; 4], [f32; 4], bool),
        (clip, fade, fade_origin): ([f32; 4], [f32; 4], [f32; 4]),
        rounded: Option<([f32; 4], [f32; 4])>,
        k_px: f32,
        opacity: f32,
        effective_opacity: f32,
        layers: &[(usize, usize, usize)],
    ) -> Option<f32> {
        if glass.is_identity() {
            return None;
        }
        let n = &self.tree.nodes[i];
        if cfg!(debug_assertions) && n.vis.backdrop_blur > 0.0 && !self.paint.glass.warned_both {
            self.paint.glass.warned_both = true;
            eprintln!(
                "rune: element {:?} sets both backdrop_blur and liquid_glass; using liquid_glass",
                n.path
            );
        }

        let node_id = n.id;
        let glass_id = n
            .vis
            .liquid_glass
            .as_deref()
            .and_then(|gn| gn.glass_id.clone());
        let glass_union = n
            .vis
            .liquid_glass
            .as_deref()
            .and_then(|gn| gn.glass_union.clone());
        let is_apple_capsule_default =
            matches!(glass.variant, GlassVariant::Regular | GlassVariant::Clear)
                && radii == [0.0; 4]
                && !continuous;
        let effective_radii = if is_apple_capsule_default {
            let cap = rect[2].min(rect[3]) * 0.5;
            [cap; 4]
        } else {
            radii
        };
        let signed_radii = if continuous {
            effective_radii.map(|v| -v)
        } else {
            effective_radii
        };

        let resolution = resolve_glass(
            &glass,
            self.host.glass_policy,
            self.host.system_reduced_transparency,
            self.host.system_increased_contrast,
            self.host.system_contrast,
        );

        let (clip, round) = match rounded {
            Some((mask, cr)) if intersect(clip, mask) == mask => (mask, Some(cr)),
            Some((mask, _)) => (intersect(clip, mask), None),
            None => (clip, None),
        };
        let packed_fade = crate::render::pack_fade(fade, fade_origin, clip);

        // Solid / high-contrast fallback: ordinary uber Rect (+ rim border), zero GPU glass pass.
        if let GlassResolution::Solid { fill, rim } = resolution {
            let srgb = self.paint.srgb_blend;
            let f_col = premul(fill, opacity, srgb);
            let (b_col, bw) = match rim {
                Some(rc) => (premul(rc, opacity, srgb), k_px.max(1.0)),
                None if glass.rim[0] > 0.0 => (
                    premul(glass.rim_color.alpha(glass.rim[0]), opacity, srgb),
                    k_px.max(1.0),
                ),
                None => ([0.0; 4], 0.0),
            };
            let mut inst = Instance {
                rect,
                radii: signed_radii,
                fill: f_col,
                border: b_col,
                params: [bw, crate::render::KIND_RECT, 0.0, opacity],
                uv: [0.0; 4],
                clip,
                fade: packed_fade,
            };
            if let Some(cr) = round {
                inst.round_clip_corners(cr);
            }
            self.instances.push(inst);
            let hole = crate::system_material::clamp_radius(effective_radii, rect[2], rect[3]);
            return Some(if continuous { -hole } else { hole });
        }

        let GlassResolution::Live {
            reduced_transparency,
            increased_contrast,
        } = resolution
        else {
            unreachable!();
        };

        // Track appearance for unmatched materialize ramp (frame 1 lands settled at 1.0; newly
        // mounted unmatched glass on later animated commits ramps 0 -> 1).
        let newly_mounted = !self.paint.glass.seen_ids.contains_key(&node_id)
            && self.commit.commits > 1
            && !self.motion.flying.iter().any(|f| f.0 == node_id)
            && !self.scene.store().has(node_id, P_GLASS_MATERIALIZE);
        if newly_mounted
            && !self.reduced_motion
            && !self.frame.pending.commit.snaps()
            && !self.frame.pending.unprompted
        {
            self.scene.set(node_id, P_GLASS_MATERIALIZE, 0.0);
            let m_spring = apple::materialize_spring(self.reduced_motion);
            self.scene.animate(
                node_id,
                P_GLASS_MATERIALIZE,
                1.0,
                rune_motion::Anim::from(m_spring),
            );
        }
        self.paint.glass.curr_ids.insert(node_id, true);
        let materialize = self
            .scene
            .get(node_id, P_GLASS_MATERIALIZE)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        // Interactive physics: press spring, hover swell springs, gel stretch, drag follow, touch glow.
        let mut shape_rect = rect;
        let mut hover_amt = 0.0f32;
        let mut glow_vec = [0.0f32; 4];
        if glass.interactive {
            let press_t = self.scene.get(node_id, P_GLASS_PRESS).unwrap_or(0.0);
            let hover_x = self.scene.get(node_id, P_GLASS_HOVER_X).unwrap_or(1.0);
            let hover_y = self.scene.get(node_id, P_GLASS_HOVER_Y).unwrap_or(1.0);
            hover_amt = self
                .scene
                .get(node_id, P_HOVER)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let glow_t = self
                .scene
                .get(node_id, P_GLASS_GLOW)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);

            let base_press_scale = 1.0 + (apple::PRESS_SCALE - 1.0) * press_t;
            let mut sx = base_press_scale * hover_x;
            let mut sy = base_press_scale * hover_y;
            let mut follow_x = 0.0f32;
            let mut follow_y = 0.0f32;

            let cx = rect[0] + rect[2] * 0.5;
            let cy = rect[1] + rect[3] * 0.5;
            let s = self.env.scale;
            if let Some((px_pt, py_pt)) = self.pointer.pos {
                let px = px_pt * s;
                let py = py_pt * s;
                if press_t > 0.005 {
                    self.paint.glass.touch_points.insert(node_id, [px, py]);
                    if !self.reduced_motion {
                        let dx = px - cx;
                        let dy = py - cy;
                        let pt_c = press_t.clamp(0.0, 1.0);
                        follow_x = (dx * apple::DRAG_FOLLOW_RATIO).clamp(
                            -rect[2] * apple::DRAG_FOLLOW_CAP,
                            rect[2] * apple::DRAG_FOLLOW_CAP,
                        ) * pt_c;
                        follow_y = (dy * apple::DRAG_FOLLOW_RATIO).clamp(
                            -rect[3] * apple::DRAG_FOLLOW_CAP,
                            rect[3] * apple::DRAG_FOLLOW_CAP,
                        ) * pt_c;
                        // Gel stretch along the drag direction up to PRESS_STRETCH_MAX (1.18).
                        let extra_stretch = apple::PRESS_STRETCH_MAX - apple::PRESS_SCALE;
                        let ax = (dx.abs() / rect[2].max(1.0)).clamp(0.0, 1.0);
                        let ay = (dy.abs() / rect[3].max(1.0)).clamp(0.0, 1.0);
                        sx += extra_stretch * ax * pt_c;
                        sy += extra_stretch * ay * pt_c;
                    }
                }
            }
            let nw = (rect[2] * sx).max(1.0);
            let nh = (rect[3] * sy).max(1.0);
            shape_rect = [cx + follow_x - nw * 0.5, cy + follow_y - nh * 0.5, nw, nh];

            if glow_t > 0.001 {
                let [gx, gy] = self
                    .paint
                    .glass
                    .touch_points
                    .get(&node_id)
                    .copied()
                    .unwrap_or([cx, cy]);
                let diag = (nw * nw + nh * nh).sqrt();
                glow_vec = [gx, gy, diag * (0.25 + 0.75 * glow_t), glow_t];
            }
        }

        // Compute size-derived optics in points, then convert to device pixels.
        let w_pt = (shape_rect[2] / k_px).max(1.0);
        let h_pt = (shape_rect[3] / k_px).max(1.0);
        let base_sigma_pt = match (glass.variant, glass.frost_sigma) {
            (_, Some(s)) => s,
            (GlassVariant::Regular, None) => apple::regular_frost_pt(w_pt, h_pt),
            (GlassVariant::Clear, None) => apple::CLEAR_SIGMA_PT,
            (GlassVariant::Custom, None) => 10.0,
            (GlassVariant::Identity, _) => 0.0,
        };
        let sigma_pt = if reduced_transparency {
            apple::reduced_transparency_sigma_pt(base_sigma_pt)
        } else {
            base_sigma_pt
        };
        let sigma_px = sigma_pt * k_px;

        let bevel_pt = apple::bevel_pt(w_pt, h_pt, glass.bevel);
        let bevel_px = bevel_pt * k_px;
        let refract_scale = if reduced_transparency {
            glass.refraction * 0.35
        } else {
            glass.refraction
        };
        let max_refract_px =
            apple::refraction_max_pt(w_pt, h_pt, glass.bevel, refract_scale) * k_px;

        let (shadow_blur_px, shadow_y_px) = match glass.variant {
            GlassVariant::Regular | GlassVariant::Clear => (
                apple::shadow_blur_pt(w_pt, h_pt) * k_px,
                apple::SHADOW_OFFSET_Y_PT * k_px,
            ),
            GlassVariant::Custom | GlassVariant::Identity => (0.0, 0.0),
        };

        let mut mode_flags = match glass.variant {
            GlassVariant::Regular => MODE_REGULAR,
            GlassVariant::Clear => MODE_CLEAR,
            GlassVariant::Custom => MODE_CUSTOM,
            GlassVariant::Identity => MODE_REGULAR,
        };
        if w_pt >= apple::LARGE_ELEMENT_PT && h_pt >= apple::LARGE_ELEMENT_PT {
            mode_flags |= FLAG_LARGE;
        }
        if glass.cursor_light && !self.reduced_motion {
            mode_flags |= FLAG_CURSOR_LIGHT;
        }
        if reduced_transparency {
            mode_flags |= FLAG_REDUCED_TRANSPARENCY;
        }
        if increased_contrast {
            mode_flags |= FLAG_INCREASED_CONTRAST;
        }
        if glass.has_tint {
            mode_flags |= FLAG_HAS_TINT;
        }

        // Tint encoding: Apple maps straight tint RGB to luminance at TINT_ALPHA (or tint.a if set);
        // Custom uses premultiplied tint overlay matching CSS background-color.
        let tint_gpu = if glass.variant == GlassVariant::Custom {
            to_srgb_premul(glass.tint)
        } else if glass.has_tint {
            let rgb = to_srgb_rgb(glass.tint);
            let alpha = if glass.tint.a < 0.999 {
                glass.tint.a
            } else {
                apple::TINT_ALPHA
            };
            [rgb[0], rgb[1], rgb[2], alpha]
        } else {
            [0.0; 4]
        };

        let rim_w_px = if glass.rim_width > 0.0 {
            (glass.rim_width * k_px).max(1.0)
        } else {
            k_px.max(1.0)
        };
        let rim_rgb = to_srgb_rgb(glass.rim_color);
        let inner_c = to_srgb_premul(glass.inner_shadow_color);
        let still_rgba = to_srgb_straight(glass.still);

        let union_id_f = if let (Some(uid), Some(container)) = (
            glass_union.as_deref(),
            self.paint.glass.containers.last_mut(),
        ) {
            let next_id = container.union_ids.len() as u32 + 1;
            *container.union_ids.entry(uid.to_owned()).or_insert(next_id) as f32
        } else {
            0.0
        };

        // Light angle: fixed top-left under Reduce Motion, otherwise subtly responsive to element position.
        let light_angle = if self.reduced_motion {
            glass.specular[1]
        } else {
            let vp_w = (self.viewport.0 * self.env.scale).max(1.0);
            let nx = ((shape_rect[0] + shape_rect[2] * 0.5) / vp_w - 0.5).clamp(-0.5, 0.5);
            glass.specular[1] + nx * 0.28
        };

        // Re-scale radii if shape grew with press/hover (capsule keeps pill curvature).
        let scaled_radii = if is_apple_capsule_default {
            let cap = shape_rect[2].min(shape_rect[3]) * 0.5;
            [cap; 4]
        } else {
            signed_radii
        };

        // Register luminance probe request if tagged with `El::glass_id`.
        if let Some(id) = glass_id {
            self.paint.glass.frame.probes.push(GlassProbeRequest {
                id: id.to_string(),
                center_px: [
                    shape_rect[0] + shape_rect[2] * 0.5,
                    shape_rect[1] + shape_rect[3] * 0.5,
                ],
            });
        }

        // Determine hoisting out of ancestor layers (alpha fade, cache, blur, blend — Q3).
        let (hoist_before, record_opacity, record_clip, record_fade) =
            if let Some(&(_, outer_layer_idx, _)) = layers.first() {
                let outer = &self.instances[outer_layer_idx];
                (
                    Some(outer_layer_idx),
                    effective_opacity.clamp(0.0, 1.0),
                    outer.clip,
                    outer.fade,
                )
            } else {
                let mut tmp_inst = Instance {
                    clip,
                    fade: packed_fade,
                    ..bytemuck::Zeroable::zeroed()
                };
                if let Some(cr) = round {
                    tmp_inst.round_clip_corners(cr);
                }
                (None, opacity.clamp(0.0, 1.0), tmp_inst.clip, tmp_inst.fade)
            };

        let shape = GlassShapeGpu {
            rect: shape_rect,
            radii: scaled_radii,
            tone: [glass.brightness, glass.contrast, glass.saturation, sigma_px],
            tint: tint_gpu,
            rim: [glass.rim[0], glass.rim[1], glass.rim[2], rim_w_px],
            rim_c: [rim_rgb[0], rim_rgb[1], rim_rgb[2], glass.specular[0]],
            inner: [
                glass.inner_shadow[0] * k_px,
                glass.inner_shadow[1] * k_px,
                glass.inner_shadow[2] * k_px,
                glass.sheen[0],
            ],
            inner_c,
            bend: [bevel_px, max_refract_px, glass.dispersion, glass.sheen[1]],
            light: [light_angle, mode_flags as f32, union_id_f, 1.0],
            glow: glow_vec,
            extra: [shadow_blur_px, shadow_y_px, hover_amt, materialize],
        };

        if let Some(container) = self.paint.glass.containers.last_mut() {
            if container.shapes.is_empty() {
                container.still_rgba = still_rgba;
                container.opacity = record_opacity;
                container.clip = record_clip;
                container.fade = record_fade;
                container.k_px = k_px;
                if let Some(hoist_idx) = hoist_before {
                    container.hoist_before = Some(hoist_idx);
                } else {
                    container.placeholder_at = Some(self.instances.len());
                    self.instances.push(Instance {
                        params: [0.0, KIND_LIQUID_GLASS, 0.0, k_px],
                        clip: record_clip,
                        fade: record_fade,
                        ..bytemuck::Zeroable::zeroed()
                    });
                }
            }
            let pad = shadow_blur_px * 1.6 + shadow_y_px.abs() + container.spacing_pt * k_px + 2.0;
            let b = [
                shape_rect[0] - pad,
                shape_rect[1] - pad,
                shape_rect[0] + shape_rect[2] + pad,
                shape_rect[1] + shape_rect[3] + pad,
            ];
            container.bounds = if container.has_bounds {
                [
                    container.bounds[0].min(b[0]),
                    container.bounds[1].min(b[1]),
                    container.bounds[2].max(b[2]),
                    container.bounds[3].max(b[3]),
                ]
            } else {
                container.has_bounds = true;
                b
            };
            container.max_sigma_px = container.max_sigma_px.max(sigma_px);
            container.max_refract_px = container.max_refract_px.max(max_refract_px);
            container.shapes.push(shape);
        } else {
            // Lone glass element: container of one.
            let first_shape = self.paint.glass.frame.shapes.len();
            self.paint.glass.frame.shapes.push(shape);
            let pad = shadow_blur_px * 1.6 + shadow_y_px.abs() + 2.0;
            let quad_rect = [
                shape_rect[0] - pad,
                shape_rect[1] - pad,
                shape_rect[2] + 2.0 * pad,
                shape_rect[3] + 2.0 * pad,
            ];
            let rec = Instance {
                rect: quad_rect,
                radii: scaled_radii,
                fill: [still_rgba[0], still_rgba[1], still_rgba[2], record_opacity],
                border: still_rgba,
                params: [sigma_px, KIND_LIQUID_GLASS, first_shape as f32, k_px],
                uv: [1.0, 0.0, max_refract_px, 0.0],
                clip: record_clip,
                fade: record_fade,
            };
            if let Some(hoist_idx) = hoist_before {
                self.paint.glass.pending_splices.push(GlassSplice {
                    insert_before: hoist_idx,
                    instances: vec![rec],
                });
            } else {
                self.instances.push(rec);
            }
        }

        let hole = crate::system_material::clamp_radius(effective_radii, rect[2], rect[3]);
        Some(if continuous { -hole } else { hole })
    }
}
