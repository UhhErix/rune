//! `Renderer` entry points for Liquid Glass frame upload, pyramid preparation, draw and probe polling.

use super::composite::{EpochUniform, build_epoch_uniform};
use super::plan::plan_epochs;
use super::pyramid::{SLOT, SLOTS_PER_EPOCH, STEP, write_epoch_down_uniforms};
use super::{LiquidGlassFrame, LiquidGlassPass, LiquidGlassStats};
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

    pub(in crate::render) fn prepare_liquid_glass(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[Instance],
        (vw, vh): (u32, u32),
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
        }
        if glass_indices.is_empty()
            || self.liquid_glass_frame.shapes.is_empty()
            || vw == 0
            || vh == 0
        {
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

        lg.stats = LiquidGlassStats {
            epochs: epochs.len(),
            records: records.len(),
            shapes: self.liquid_glass_frame.shapes.len(),
            pyramid_bytes,
        };
        lg.epochs = epochs;
        lg.records = records;
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

        let mut use_still = false;
        if rec.starts_epoch {
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
            }
        } else if !pass.is_owned_splittable() {
            use_still = true;
        }

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
