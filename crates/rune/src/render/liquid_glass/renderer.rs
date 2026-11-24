//! `Renderer` entry points for Liquid Glass frame upload, pyramid preparation, draw and probe polling.

use super::composite::{EpochUniform, build_epoch_uniform};
use super::plan::plan_epochs;
use super::pyramid::{SLOT, SLOTS_PER_EPOCH, STEP, write_epoch_down_uniforms};
use super::{CachedEpochKey, LiquidGlassFrame, LiquidGlassPass, LiquidGlassStats};
use crate::render::pass::ActivePass;
use crate::render::{Instance, Kind, Renderer, layers};

impl Renderer {
    /// Hands this frame's Liquid Glass shape table, luminance probe requests and pointer state
    /// from the runtime to the renderer (call before `prepare` / `prepare_ui`).
    pub fn set_liquid_glass(&mut self, frame: &LiquidGlassFrame) {
        self.liquid_glass_frame.shapes.clear();
        self.liquid_glass_frame
            .shapes
            .extend_from_slice(&frame.shapes);
        self.liquid_glass_frame.probes.clear();
        self.liquid_glass_frame
            .probes
            .extend_from_slice(&frame.probes);
        self.liquid_glass_frame.pointer = frame.pointer;
        self.liquid_glass_frame.saturation_scale = if frame.saturation_scale.is_finite() {
            frame.saturation_scale.max(0.0)
        } else {
            1.0
        };
    }

    /// Sets the pass-level backdrop saturation multiplier for Liquid Glass (default `1.0`).
    pub fn set_liquid_glass_saturation(&mut self, saturation: f32) {
        self.liquid_glass_frame.saturation_scale = if saturation.is_finite() {
            saturation.max(0.0)
        } else {
            1.0
        };
    }

    pub(in crate::render) fn has_liquid_glass(&self) -> bool {
        self.liquid_glass
            .as_ref()
            .is_some_and(|lg| !lg.records.is_empty())
    }

    #[cfg(any(target_os = "windows", target_arch = "wasm32"))]
    pub(in crate::render) fn liquid_glass_signature(&self) -> Option<u64> {
        let lg = self.liquid_glass.as_ref().filter(|lg| !lg.records.is_empty())?;
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |bytes: &[u8]| {
            for &b in bytes {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        mix(&self.liquid_glass_frame.saturation_scale.to_bits().to_le_bytes());
        mix(bytemuck::cast_slice(&self.liquid_glass_frame.shapes));
        let cursor_lit = self
            .liquid_glass_frame
            .shapes
            .iter()
            .any(|s| ((s.light[1] as u32) & super::FLAG_CURSOR_LIGHT) != 0);
        if cursor_lit {
            mix(bytemuck::cast_slice(&self.liquid_glass_frame.pointer));
        }
        for ep in &lg.epochs {
            mix(bytemuck::cast_slice(&ep.region));
            mix(bytemuck::cast_slice(&ep.origin));
            mix(bytemuck::cast_slice(&ep.size));
            mix(&(ep.max_mip as u64).to_le_bytes());
        }
        for rec in &lg.records {
            mix(&rec.at.to_le_bytes());
            mix(&(rec.epoch as u64).to_le_bytes());
        }
        Some(h)
    }

    #[cfg(any(target_os = "windows", target_arch = "wasm32"))]
    pub(in crate::render) fn liquid_glass_disjoint(&self, rects: &[[u32; 4]]) -> bool {
        let Some(lg) = self.liquid_glass.as_ref().filter(|lg| !lg.records.is_empty()) else {
            return true;
        };
        for &[rx, ry, rw, rh] in rects {
            let (rx0, ry0, rx1, ry1) = (rx as f32, ry as f32, (rx + rw) as f32, (ry + rh) as f32);
            for ep in &lg.epochs {
                let r = ep.region;
                if rx0 < r[2] + 1.0 && rx1 > r[0] - 1.0 && ry0 < r[3] + 1.0 && ry1 > r[1] - 1.0 {
                    return false;
                }
            }
            for &(_, ref inst) in &self.effects {
                if inst.kind() != Some(Kind::LiquidGlass) {
                    continue;
                }
                let r = inst.rect;
                if rx0 < r[0] + r[2] + 1.0
                    && rx1 > r[0] - 1.0
                    && ry0 < r[1] + r[3] + 1.0
                    && ry1 > r[1] - 1.0
                {
                    return false;
                }
            }
        }
        true
    }

    /// Stats from the most recently prepared Liquid Glass frame (epochs, records, shapes, pyramid bytes).
    pub fn liquid_glass_stats(&self) -> LiquidGlassStats {
        self.liquid_glass.as_ref().map_or_default(|lg| lg.stats)
    }

    /// Non-blocking poll of asynchronous GPU luminance probes for `El::glass_id`. Returns any
    /// `(glass_id, luminance)` samples that completed since the previous call; never blocks.
    pub fn poll_glass_probes(&self, device: &wgpu::Device) -> Vec<(String, f32)> {
        let Some(lg) = &self.liquid_glass else {
            return Vec::new();
        };
        lg.probe
            .lock()
            .map(|mut p| p.poll_results(device))
            .unwrap_or_default()
    }

    /// Counter of blocking polls performed by the luminance probe (always 0).
    #[cfg(test)]
    pub(crate) fn glass_probe_blocking_polls(&self) -> u64 {
        self.liquid_glass
            .as_ref()
            .and_then(|lg| lg.probe.lock().ok().map(|p| p.blocking_polls))
            .unwrap_or(0)
    }

    /// Makes and drops the Liquid Glass down, probe and composite pipelines for `regen_dxbc` and `first_use_ms`.
    #[doc(hidden)]
    pub fn make_liquid_glass(&self, device: &wgpu::Device) {
        let mut lg = LiquidGlassPass::new(device, &self.bind_layout);
        lg.ensure_composite(device, self.format);
    }

    #[cfg(test)]
    pub(crate) fn glass_epoch0_reuses(&self) -> u64 {
        self.liquid_glass
            .as_ref()
            .map_or(0, |lg| lg.epoch0_reuses.load(std::sync::atomic::Ordering::Relaxed))
    }

    pub(in crate::render) fn prepare_liquid_glass(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[Instance],
        (vw, vh): (u32, u32),
        backdrop_deps_clean: bool,
    ) {
        let glass_indices: Vec<u32> = self
            .effects
            .iter()
            .filter(|(_, i)| i.kind() == Some(Kind::LiquidGlass))
            .map(|(at, _)| *at)
            .collect();
        if let Some(lg) = &mut self.liquid_glass {
            lg.records.clear();
            lg.epochs.clear();
            lg.stats = LiquidGlassStats::default();
            lg.epoch0_backdrop_unchanged = false;
        }
        if glass_indices.is_empty()
            || self.liquid_glass_frame.shapes.is_empty()
            || vw == 0
            || vh == 0
        {
            if let Some(lg) = &mut self.liquid_glass {
                lg.invalidate_cache();
            }
            return;
        }
        let staging = self.uploads.staging(device, queue);
        if self.liquid_glass.is_none() {
            let mut lg = LiquidGlassPass::new(device, &self.bind_layout);
            for bd in &self.backdrops {
                lg.bind_backdrop(device, bd.format, &bd.view);
            }
            self.liquid_glass = Some(lg);
        }
        let Some(lg) = &mut self.liquid_glass else {
            return;
        };
        lg.ensure_composite(device, self.format);
        for (format, _) in &self.extra {
            lg.ensure_composite(device, *format);
        }
        if self.layers.targets.iter().any(Option::is_some) {
            lg.ensure_composite(device, layers::FORMAT);
        }
        lg.viewport = (vw, vh);

        let (epochs, records, need) = plan_epochs(instances, &glass_indices, (vw, vh));
        if epochs.is_empty() || records.is_empty() {
            return;
        }
        let cap = [vw, vh].map(|v| (v.div_ceil(2) + STEP).next_multiple_of(STEP));
        lg.ensure_buffers(
            device,
            epochs.len(),
            self.liquid_glass_frame.shapes.len(),
            &self.backdrops,
        );
        lg.ensure_pyramid(device, need, cap, &self.backdrops);
        let Some(pyramid) = &lg.pyramid else { return };
        let pyramid_size = pyramid.size;
        let pyramid_bytes = pyramid.byte_size();

        // Upload shapes storage buffer.
        staging.write_buffer(
            &lg.shapes_buf,
            0,
            bytemuck::cast_slice(&self.liquid_glass_frame.shapes),
        );

        // Upload down-chain uniforms and per-epoch composite uniforms.
        let encode_srgb = self.format.is_srgb();
        let mut down_bytes = vec![0u8; epochs.len() * SLOTS_PER_EPOCH * SLOT as usize];
        let mut epoch_bytes = vec![0u8; (epochs.len() + 1) * SLOT as usize];
        for (idx, ep) in epochs.iter().enumerate() {
            let d_off = idx * SLOTS_PER_EPOCH * SLOT as usize;
            write_epoch_down_uniforms(
                &mut down_bytes[d_off..d_off + SLOTS_PER_EPOCH * SLOT as usize],
                ep,
                pyramid_size,
                (vw, vh),
                encode_srgb,
            );
            let eu = build_epoch_uniform(
                ep,
                pyramid_size,
                (vw, vh),
                self.liquid_glass_frame.pointer,
                false,
                encode_srgb,
                self.liquid_glass_frame.saturation_scale,
            );
            let e_off = idx * SLOT as usize;
            epoch_bytes[e_off..e_off + std::mem::size_of::<EpochUniform>()]
                .copy_from_slice(bytemuck::bytes_of(&eu));
        }
        // Final slot: still-mode fallback uniform (used if ActivePass::split returns None).
        let still_u = build_epoch_uniform(
            &epochs[0],
            pyramid_size,
            (vw, vh),
            self.liquid_glass_frame.pointer,
            true,
            encode_srgb,
            self.liquid_glass_frame.saturation_scale,
        );
        let still_off = epochs.len() * SLOT as usize;
        epoch_bytes[still_off..still_off + std::mem::size_of::<EpochUniform>()]
            .copy_from_slice(bytemuck::bytes_of(&still_u));

        staging.write_buffer(&lg.down_uniforms, 0, &down_bytes);
        staging.write_buffer(&lg.epoch_uniforms, 0, &epoch_bytes);

        if let Ok(mut probe) = lg.probe.lock() {
            probe.prepare(
                queue,
                &self.liquid_glass_frame.probes,
                epochs[0].origin,
                pyramid_size,
                epochs[0].region,
            );
        }

        let only_liquid_glass = self
            .effects
            .iter()
            .all(|(_, i)| i.kind() == Some(Kind::LiquidGlass));
        let epoch0_unchanged = backdrop_deps_clean
            && only_liquid_glass
            && epochs.len() == 1
            && self.springs.is_none()
            && self.surfaces.as_ref().is_none_or(|s| s.draws_nothing())
            && self.liquid_glass_frame.probes.is_empty()
            && records.first().is_some_and(|r| {
                r.starts_epoch
                    && r.epoch == 0
                    && (self.uploads.first_changed >= r.at as usize
                        || (self.layers.in_place()
                            && self.uploads.changed_bounds.is_some_and(|b| {
                                let ep = &epochs[0].region;
                                b[2] <= b[0]
                                    || b[3] <= b[1]
                                    || b[2] <= ep[0] - 2.0
                                    || b[0] >= ep[2] + 2.0
                                    || b[3] <= ep[1] - 2.0
                                    || b[1] >= ep[3] + 2.0
                            })))
            });

        lg.stats = LiquidGlassStats {
            epochs: epochs.len(),
            records: records.len(),
            shapes: self.liquid_glass_frame.shapes.len(),
            pyramid_bytes,
        };
        lg.epochs = epochs;
        lg.records = records;
        lg.epoch0_backdrop_unchanged = epoch0_unchanged;
        if !epoch0_unchanged {
            if let Ok(mut g) = lg.cached_epoch.lock() {
                *g = None;
            }
        }
    }

    /// Draws one `KIND_LIQUID_GLASS` record at stream index `at`. On an epoch start, splits the
    /// active pass to copy the epoch's union region and build the down-only frost pyramid once;
    /// every subsequent record in the same epoch draws directly from that pyramid without splitting.
    pub(in crate::render) fn draw_liquid_glass(
        &self,
        pass: &mut ActivePass<'_, '_>,
        format: wgpu::TextureFormat,
        at: u32,
        instance: &Instance,
        target: Option<usize>,
    ) {
        let Some(lg) = &self.liquid_glass else { return };
        let Ok(k) = lg.records.binary_search_by_key(&at, |r| r.at) else {
            return;
        };
        let rec = lg.records[k];
        let Some((_, pipeline)) = lg.composites.iter().find(|(f, _)| *f == format) else {
            return;
        };
        let (origin, size) = self.target_frame(target);

        let (r, c) = (instance.rect, instance.clip);
        let edge = |v: f32, o: f32, max: u32| (v - o).clamp(0.0, max as f32) as u32;
        let (x0, y0) = (
            edge(r[0].max(c[0]).floor(), origin[0], size[0]),
            edge(r[1].max(c[1]).floor(), origin[1], size[1]),
        );
        let (x1, y1) = (
            edge((r[0] + r[2]).min(c[2]).ceil(), origin[0], size[0]),
            edge((r[1] + r[3]).min(c[3]).ceil(), origin[1], size[1]),
        );
        if let Some([bx, by, bw, bh]) = pass.bound() {
            let (ix0, iy0) = (x0.max(bx), y0.max(by));
            let (ix1, iy1) = (x1.min(bx + bw), y1.min(by + bh));
            if ix1 <= ix0 || iy1 <= iy0 {
                return;
            }
        }

        let mut use_still = false;
        if rec.starts_epoch {
            let key = (rec.epoch == 0 && target.is_none() && lg.epochs.len() == 1)
                .then(|| pass.clear_bits())
                .flatten()
                .map(|clear| {
                    let ep = &lg.epochs[0];
                    let pyramid_size = lg.pyramid.as_ref().map_or([0; 2], |p| p.size);
                    CachedEpochKey {
                        format,
                        viewport: lg.viewport,
                        clear,
                        region: ep.region.map(f32::to_bits),
                        origin: ep.origin.map(f32::to_bits),
                        size: ep.size,
                        max_mip: ep.max_mip,
                        pyramid_size,
                    }
                });
            let reused = key.is_some_and(|k| {
                lg.epoch0_backdrop_unchanged
                    && pass.is_owned_splittable()
                    && self.backdrops.iter().any(|b| b.format == format)
                    && lg.cached_epoch.lock().ok().is_some_and(|g| *g == Some(k))
            });
            if reused {
                #[cfg(test)]
                lg.epoch0_reuses
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                let ep = &lg.epochs[rec.epoch];
                let split = pass.split(
                    self,
                    format,
                    origin,
                    size,
                    target.is_some(),
                    Some(ep.region),
                    |encoder, bd| {
                        lg.encode_epoch(encoder, bd.format, rec.epoch, None);
                    },
                );
                if split.is_none() {
                    use_still = true;
                    if let Ok(mut g) = lg.cached_epoch.lock() {
                        *g = None;
                    }
                } else if let Ok(mut g) = lg.cached_epoch.lock() {
                    *g = key;
                }
            }
        } else if !pass.is_owned_splittable() {
            use_still = true;
        }

        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let scope = pass.scissor();
        let [sx, sy, sw, sh] = scope.unwrap_or([0, 0, size[0], size[1]]);
        let (x0, y0, x1, y1) = (x0.max(sx), y0.max(sy), x1.min(sx + sw), y1.min(sy + sh));
        if x1 > x0 && y1 > y0 {
            let (bind1, dyn_off) = if use_still {
                let Some(sb) = &lg.still_bind else { return };
                (sb, (lg.epochs.len() as u64 * SLOT) as u32)
            } else {
                let Some((_, cb)) = lg.composite_binds.iter().find(|(f, _)| *f == format) else {
                    return;
                };
                (cb, (rec.epoch as u64 * SLOT) as u32)
            };
            let bind0 = match target {
                Some(index) => {
                    let Some(layer) = self.layers.targets[index].as_ref() else {
                        return;
                    };
                    &layer.base
                }
                None => &self.bind_group,
            };
            pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind0, &[]);
            pass.set_bind_group(1, bind1, &[dyn_off]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..4, at..at + 1);
        }
        pass.set_scissor(scope, size);
    }
}
