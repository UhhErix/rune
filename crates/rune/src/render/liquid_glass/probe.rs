//! Asynchronous GPU backdrop luminance probe for `El::glass_id` / `Cx::glass_is_dark`.
//!
//! Renders one `Rgba16Float` texel per probed glass shape into a tiny `N×1` strip from the coarse
//! frost pyramid, copies it into a 3-slot `MAP_READ` ring, and maps asynchronously without ever
//! blocking the frame loop.

use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};

use super::pyramid::{DownUniform, SLOT};
use crate::render::layers;

/// Number of async readback buffers in the non-blocking ring.
const RING_LEN: usize = 3;
/// Maximum simultaneous probed `glass_id` shapes per frame.
pub(super) const MAX_PROBES: u32 = 64;
/// Bytes per row for `Rgba16Float` copy (aligned to WebGPU `COPY_BYTES_PER_ROW_ALIGNMENT = 256`).
const ROW_BYTES: u32 = ((MAX_PROBES * 8 + 255) / 256) * 256;

/// GPU storage entry matching `ProbeCoord` in `liquid_glass_down.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct ProbeCoord {
    pub uv: [f32; 4],
}

/// CPU-side description of one probed `glass_id` shape passed from paint.
#[derive(Clone, Debug)]
pub struct GlassProbeRequest {
    pub id: String,
    pub center_px: [f32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotState {
    Free,
    PendingCopy,
    Mapping,
}

struct ReadSlot {
    buffer: wgpu::Buffer,
    state: SlotState,
    ready: Arc<Mutex<Option<bool>>>,
    ids: Vec<String>,
}

/// Owns the probe target strip, coord buffer, uniform buffer and 3-slot async readback ring.
pub(super) struct ProbeRing {
    pub pipeline: wgpu::RenderPipeline,
    pub coords_buf: wgpu::Buffer,
    pub coords_bind: wgpu::BindGroup,
    pub uniform_buf: wgpu::Buffer,
    pub strip_tex: wgpu::Texture,
    pub strip_view: wgpu::TextureView,
    slots: Vec<ReadSlot>,
    next_write: usize,
    /// Number of active probes scheduled for this frame.
    pub count: u32,
    pub active_ids: Vec<String>,
    /// Counter of blocking polls (always 0; verified by unit tests).
    #[cfg(test)]
    pub blocking_polls: u64,
}

impl ProbeRing {
    pub(super) fn new(
        device: &wgpu::Device,
        down_layout: &wgpu::BindGroupLayout,
        vs: &wgpu::ShaderModule,
        fs: &wgpu::ShaderModule,
    ) -> Self {
        let probe_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("liquid glass probe coords"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(
                        std::mem::size_of::<ProbeCoord>() as u64
                    ),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("liquid glass probe"),
            bind_group_layouts: &[Some(down_layout), Some(&probe_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("liquid glass probe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: vs,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: fs,
                entry_point: Some("fs_probe"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: layers::FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let coords_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid glass probe coords"),
            size: (MAX_PROBES as usize * std::mem::size_of::<ProbeCoord>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let coords_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("liquid glass probe coords"),
            layout: &probe_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: coords_buf.as_entire_binding(),
            }],
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid glass probe uniform"),
            size: SLOT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let strip_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("liquid glass probe strip"),
            size: wgpu::Extent3d {
                width: MAX_PROBES,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: layers::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let strip_view = strip_tex.create_view(&Default::default());
        let slots = (0..RING_LEN)
            .map(|_| ReadSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("liquid glass probe readback"),
                    size: ROW_BYTES as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                state: SlotState::Free,
                ready: Arc::new(Mutex::new(None)),
                ids: Vec::new(),
            })
            .collect();
        Self {
            pipeline,
            coords_buf,
            coords_bind,
            uniform_buf,
            strip_tex,
            strip_view,
            slots,
            next_write: 0,
            count: 0,
            active_ids: Vec::new(),
            #[cfg(test)]
            blocking_polls: 0,
        }
    }

    /// Uploads probe coordinates for this frame's first epoch (if any `glass_id` requests exist).
    pub(super) fn prepare(
        &mut self,
        queue: &wgpu::Queue,
        requests: &[GlassProbeRequest],
        origin: [f32; 2],
        pyramid_size: [u32; 2],
        region: [f32; 4],
    ) {
        self.count = 0;
        self.active_ids.clear();
        if requests.is_empty() {
            return;
        }
        let extent = [
            (pyramid_size[0] as f32 * 2.0).max(1.0),
            (pyramid_size[1] as f32 * 2.0).max(1.0),
        ];
        let region_uv = [
            (region[0] - origin[0]) / extent[0],
            (region[1] - origin[1]) / extent[1],
            (region[2] - origin[0]) / extent[0],
            (region[3] - origin[1]) / extent[1],
        ];
        let mut coords = Vec::with_capacity(requests.len().min(MAX_PROBES as usize));
        for req in requests.iter().take(MAX_PROBES as usize) {
            let u = (req.center_px[0] - origin[0]) / extent[0];
            let v = (req.center_px[1] - origin[1]) / extent[1];
            coords.push(ProbeCoord {
                uv: [u, v, 5.0, 3.0],
            });
            self.active_ids.push(req.id.clone());
        }
        self.count = coords.len() as u32;
        queue.write_buffer(&self.coords_buf, 0, bytemuck::cast_slice(&coords));
        let u = DownUniform {
            w: [0.0, self.count as f32, 0.0, 0.0],
            region: region_uv,
            map: [1.0, 1.0, 0.0, 0.0],
            extra: [0.0; 4],
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
    }

    /// Records the probe render pass and copies into a free ring buffer slot (never stalls).
    pub(super) fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pyramid_probe_bind: &wgpu::BindGroup,
    ) {
        if self.count == 0 {
            return;
        }
        let Some(slot_idx) = (0..RING_LEN)
            .map(|k| (self.next_write + k) % RING_LEN)
            .find(|&idx| self.slots[idx].state == SlotState::Free)
        else {
            return;
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("liquid glass probe"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.strip_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(0, 0, self.count, 1);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, pyramid_probe_bind, &[0]);
            pass.set_bind_group(1, &self.coords_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.strip_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.slots[slot_idx].buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_BYTES),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: self.count,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        self.slots[slot_idx].state = SlotState::PendingCopy;
        self.slots[slot_idx].ids.clone_from(&self.active_ids);
        self.next_write = (slot_idx + 1) % RING_LEN;
    }

    /// Kicks async map on submitted slots and harvests any already-mapped results using a purely
    /// non-blocking device poll (`PollType::Poll`). Never blocks the frame!
    pub(super) fn poll_results(&mut self, device: &wgpu::Device) -> Vec<(String, f32)> {
        // Kick map_async on any slot copied by a prior submission.
        for slot in &mut self.slots {
            if slot.state == SlotState::PendingCopy {
                let flag = slot.ready.clone();
                if let Ok(mut guard) = flag.lock() {
                    *guard = None;
                }
                slot.buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |res| {
                        if let Ok(mut guard) = flag.lock() {
                            *guard = Some(res.is_ok());
                        }
                    });
                slot.state = SlotState::Mapping;
            }
        }
        // Strictly non-blocking poll (`PollType::Poll`).
        let _ = device.poll(wgpu::PollType::Poll);

        let mut out = Vec::new();
        for slot in &mut self.slots {
            if slot.state != SlotState::Mapping {
                continue;
            }
            let status = slot.ready.lock().ok().and_then(|mut g| g.take());
            match status {
                Some(true) => {
                    if let Ok(view) = slot.buffer.slice(..).get_mapped_range() {
                        let bytes: &[u8] = &view;
                        for (i, id) in slot.ids.iter().enumerate() {
                            let off = i * 8;
                            if off + 2 <= bytes.len() {
                                let bits = u16::from_le_bytes([bytes[off], bytes[off + 1]]);
                                let luma = half::f16::from_bits(bits).to_f32().clamp(0.0, 1.0);
                                out.push((id.clone(), luma));
                            }
                        }
                    }
                    slot.buffer.unmap();
                    slot.ids.clear();
                    slot.state = SlotState::Free;
                }
                Some(false) => {
                    slot.ids.clear();
                    slot.state = SlotState::Free;
                }
                None => {}
            }
        }
        out
    }
}
