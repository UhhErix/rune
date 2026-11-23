//! Per-epoch capture planner for Liquid Glass (`El::liquid_glass`).
//!
//! Walks the frame's `KIND_LIQUID_GLASS` records in stream order and packs non-overlapping records
//! (and members of the same `El::glass_container` / `El::glass_group`) into shared capture epochs.
//! Each epoch pays one pass split, one scissored region copy and one down-only frost pyramid;
//! every glass record in that epoch draws with a single composite draw call.

use super::pyramid::{MIPS, STEP};
use crate::render::{Instance, Kind};

/// One planned capture epoch covering a contiguous slice of glass records.
#[derive(Clone, Debug)]
pub(super) struct EpochPlan {
    /// Window device-pixel union of every record's sample region in this epoch: `[x0, y0, x1, y1]`.
    pub region: [f32; 4],
    /// Pyramid window origin in device pixels, aligned to the coarsest grid (`2 << (MIPS - 1)`)
    /// so scrolling content behind glass never crawls or shimmers.
    pub origin: [f32; 2],
    /// Minimum level-0 half-res pyramid dimensions in texels (multiples of `STEP`).
    pub size: [u32; 2],
    /// Highest mip level needed by any record in this epoch (`0..MIPS`).
    pub max_mip: usize,
    /// Optional explicit container/group id when records belong to a split large container.
    pub group_id: u32,
}

/// One planned `KIND_LIQUID_GLASS` draw record linked to its epoch.
#[derive(Clone, Copy, Debug)]
pub(super) struct RecordPlan {
    /// Instance stream index in `Renderer::instances`.
    pub at: u32,
    /// Epoch index in `LiquidGlassPass::epochs`.
    pub epoch: usize,
    /// True if this record triggers the epoch's backdrop capture & down pyramid before drawing.
    pub starts_epoch: bool,
}

#[inline]
fn rects_overlap(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}

#[inline]
fn union_rect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

#[inline]
fn is_empty_rect(r: [f32; 4]) -> bool {
    r[2] <= r[0] || r[3] <= r[1]
}

/// Sample region of a glass record in device pixels: rect inflated by blur reach + refraction
/// reach, clipped to the viewport.
pub(super) fn sample_region(inst: &Instance, viewport: (u32, u32)) -> Option<[f32; 4]> {
    let sigma_px = inst.params[0].max(0.0);
    let refract_px = inst.uv[2].max(0.0);
    let lod = super::pyramid::sigma_max_mip(sigma_px);
    let bspline_pad = (2u32 << (lod + 1).min(MIPS as usize)) as f32;
    let reach = (sigma_px * 3.0 + bspline_pad).min(256.0) + refract_px + 4.0;
    let (vw, vh) = (viewport.0 as f32, viewport.1 as f32);
    let r = [
        (inst.rect[0] - reach).max(0.0),
        (inst.rect[1] - reach).max(0.0),
        (inst.rect[0] + inst.rect[2] + reach).min(vw),
        (inst.rect[1] + inst.rect[3] + reach).min(vh),
    ];
    if is_empty_rect(r) { None } else { Some(r) }
}

/// Computes aligned pyramid origin and level-0 half-res size for `region`.
fn align_region(region: [f32; 4]) -> ([f32; 2], [u32; 2]) {
    // Align to the coarsest mip's device-pixel grid (2 * STEP = 128 px) so scrolling never crawls.
    let align = (STEP * 2) as f32;
    let origin = [
        (region[0] / align).floor() * align,
        (region[1] / align).floor() * align,
    ];
    let size = [0, 1].map(|k| {
        ((((region[k + 2] - origin[k]) * 0.5).ceil() as u32).next_multiple_of(STEP)).max(STEP)
    });
    (origin, size)
}

/// Plans capture epochs across all `KIND_LIQUID_GLASS` records in `instances`.
/// Returns `(epochs, records, required_pyramid_size)`.
pub(super) fn plan_epochs(
    instances: &[Instance],
    glass_indices: &[u32],
    viewport: (u32, u32),
) -> (Vec<EpochPlan>, Vec<RecordPlan>, [u32; 2]) {
    let mut epochs: Vec<EpochPlan> = Vec::new();
    let mut records: Vec<RecordPlan> = Vec::with_capacity(glass_indices.len());
    let mut dirty_since_capture: Option<[f32; 4]> = None;
    let mut prev_at: usize = 0;

    for &at_u32 in glass_indices {
        let at = at_u32 as usize;
        let inst = &instances[at];
        let Some(region) = sample_region(inst, viewport) else {
            continue;
        };
        let group_id = inst.uv[3].max(0.0) as u32;
        // Always build through mip 5 so coarse luminance `L` (F5) and detail (`|F3 - F5|`) are valid.
        let rec_mip = super::pyramid::sigma_max_mip(inst.params[0])
            .max(5)
            .min(MIPS as usize - 1);

        // Accumulate device-pixel bounds of everything drawn since the previous glass record.
        if !epochs.is_empty() && at > prev_at {
            for drawn in &instances[prev_at..at] {
                if matches!(drawn.kind(), Some(Kind::Layer | Kind::LayerEnd)) {
                    continue;
                }
                let b = drawn.layer_bounds();
                if !is_empty_rect(b) {
                    dirty_since_capture = Some(match dirty_since_capture {
                        Some(d) => union_rect(d, b),
                        None => b,
                    });
                }
            }
        }

        let joins_current = if let Some(cur) = epochs.last() {
            let same_group = group_id != 0 && cur.group_id == group_id;
            same_group || dirty_since_capture.is_none_or(|dirty| !rects_overlap(region, dirty))
        } else {
            false
        };

        if joins_current {
            let epoch_idx = epochs.len() - 1;
            let cur = &mut epochs[epoch_idx];
            cur.region = union_rect(cur.region, region);
            cur.max_mip = cur.max_mip.max(rec_mip);
            records.push(RecordPlan {
                at: at_u32,
                epoch: epoch_idx,
                starts_epoch: false,
            });
        } else {
            let epoch_idx = epochs.len();
            epochs.push(EpochPlan {
                region,
                origin: [0.0; 2],
                size: [STEP; 2],
                max_mip: rec_mip,
                group_id,
            });
            dirty_since_capture = None;
            records.push(RecordPlan {
                at: at_u32,
                epoch: epoch_idx,
                starts_epoch: true,
            });
        }

        // This glass record itself dirties its rect for any *later* ungrouped record.
        let self_b = inst.layer_bounds();
        if !is_empty_rect(self_b) {
            dirty_since_capture = Some(match dirty_since_capture {
                Some(d) => union_rect(d, self_b),
                None => self_b,
            });
        }
        prev_at = at + 1;
    }

    let mut need = [STEP; 2];
    for ep in &mut epochs {
        let (origin, size) = align_region(ep.region);
        ep.origin = origin;
        ep.size = size;
        need[0] = need[0].max(size[0]);
        need[1] = need[1].max(size[1]);
    }
    (epochs, records, need)
}
