//! In-window Liquid Glass renderer (`El::liquid_glass`, `KIND_LIQUID_GLASS`).
//!
//! Shares one pooled down-only frost pyramid (`F0..F6`, `Rgba16Float`) across every capture epoch
//! in a frame, evaluates up to 32 smooth-unioned shapes per container draw, runs asynchronous
//! non-blocking luminance probes for `Cx::glass_is_dark`, and degrades cleanly to still mode or
//! `Kind::Rect` when capture or pipeline creation is unavailable.

pub mod apple;
pub(crate) mod composite;
mod plan;
pub(crate) mod probe;
pub(crate) mod pyramid;

use std::sync::Mutex;

use super::{ActivePass, Backdrop, Instance, Kind, Renderer, dxbc, layers};
use composite::{
    EpochUniform, build_epoch_uniform, composite_from_wgsl, composite_wgsl, make_composite_pipeline,
};
pub use composite::{
    FLAG_CURSOR_LIGHT, FLAG_HAS_TINT, FLAG_INCREASED_CONTRAST, FLAG_LARGE,
    FLAG_REDUCED_TRANSPARENCY, GlassShapeGpu, MAX_SHAPES_PER_DRAW, MODE_CLEAR, MODE_CUSTOM,
    MODE_REGULAR,
};
use plan::{EpochPlan, RecordPlan, plan_epochs};
pub use probe::GlassProbeRequest;
use probe::ProbeRing;
use pyramid::{
    DownUniform, FrostPyramid, MIPS, SLOT, SLOTS_PER_EPOCH, STEP, create_pyramid, mip_scissor,
    write_epoch_down_uniforms,
};

/// Downsample + probe WGSL source (`liquid_glass_down.wgsl`).
pub(crate) const DOWN_WGSL: &str = include_str!("../shaders/liquid_glass_down.wgsl");

/// Composite WGSL source (`clip.wgsl` + `liquid_glass.wgsl`).
pub(crate) fn composite_shader_wgsl() -> String {
    composite_wgsl()
}

/// Summary statistics for the last prepared Liquid Glass frame (for benchmarks and tests).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiquidGlassStats {
    pub epochs: usize,
    pub records: usize,
    pub shapes: usize,
    pub pyramid_bytes: usize,
}

/// Per-frame Liquid Glass side-table inputs handed from `Runtime` to `Renderer::set_liquid_glass`.
#[derive(Clone, Debug, Default)]
pub struct LiquidGlassFrame {
    pub shapes: Vec<GlassShapeGpu>,
    pub probes: Vec<GlassProbeRequest>,
    /// Live pointer in device pixels `[x, y, active_mix (0..1), 0]`.
    pub pointer: [f32; 4],
    /// Global pass-level backdrop saturation multiplier (default 1.0).
    pub saturation_scale: f32,
}

impl LiquidGlassFrame {
    pub fn new() -> Self {
        Self {
            shapes: Vec::new(),
            probes: Vec::new(),
            pointer: [0.0; 4],
            saturation_scale: 1.0,
        }
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
        self.probes.clear();
        self.pointer = [0.0; 4];
        self.saturation_scale = 1.0;
    }
}

pub(super) struct LiquidGlassPass {
    down_pipeline: wgpu::RenderPipeline,
    down_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::PipelineLayout,
    result_layout: wgpu::BindGroupLayout,
    composite_shaders: dxbc::Shaders,
    composite_precompiled: bool,
    composites: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    sampler: wgpu::Sampler,
    dummy: wgpu::TextureView,
    down_uniforms: wgpu::Buffer,
    down_capacity_epochs: usize,
    epoch_uniforms: wgpu::Buffer,
    epoch_capacity: usize,
    shapes_buf: wgpu::Buffer,
    shapes_capacity: usize,
    pyramid: Option<FrostPyramid>,
    /// Down pass 0 bind group per backdrop format.
    first_down: Vec<(wgpu::TextureFormat, wgpu::BindGroup)>,
    /// Composite bind group per backdrop format (reads `pyramid.full_view` + `backdrop.view` + `shapes_buf` + `epoch_uniforms`).
    composite_binds: Vec<(wgpu::TextureFormat, wgpu::BindGroup)>,
    /// Still-mode fallback composite bind group (reads `dummy` for both textures).
    still_bind: Option<wgpu::BindGroup>,
    /// Probe bind group reading `pyramid.full_view` + `probe.uniform_buf`.
    probe_bind: Option<wgpu::BindGroup>,
    probe: Mutex<ProbeRing>,
    epochs: Vec<EpochPlan>,
    records: Vec<RecordPlan>,
    viewport: (u32, u32),
    /// True if composite pipeline failed on a broken driver (breaker fallback rewrites records to `Kind::Rect`).
    broken: bool,
    stats: LiquidGlassStats,
}

impl LiquidGlassPass {
    pub(super) fn new(device: &wgpu::Device, uber: &wgpu::BindGroupLayout) -> Self {
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let smp_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let dyn_uniform = |binding, size: u64| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(size),
            },
            count: None,
        };
        let down_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("liquid glass down"),
            entries: &[
                tex_entry(0),
                smp_entry(1),
                dyn_uniform(2, std::mem::size_of::<DownUniform>() as u64),
            ],
        });
        let result_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("liquid glass composite group 1"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                smp_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<GlassShapeGpu>() as u64,
                        ),
                    },
                    count: None,
                },
                dyn_uniform(4, std::mem::size_of::<EpochUniform>() as u64),
            ],
        });
        let down_pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("liquid glass down"),
            bind_group_layouts: &[Some(&down_layout)],
            immediate_size: 0,
        });
        let make_down = |vs: &wgpu::ShaderModule, fs: &wgpu::ShaderModule| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("liquid glass down"),
                layout: Some(&down_pipe_layout),
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
                    entry_point: Some("fs_down"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: layers::FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        #[cfg(target_os = "windows")]
        let precompiled_down = dxbc::liquid_glass_down(device, DOWN_WGSL, |m| {
            (
                make_down(&m[0], &m[1]),
                ProbeRing::new(device, &down_layout, &m[0], &m[2]),
            )
        });
        #[cfg(not(target_os = "windows"))]
        let precompiled_down = None;
        let (down_pipeline, probe) = precompiled_down.unwrap_or_else(|| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("liquid glass down"),
                source: wgpu::ShaderSource::Wgsl(DOWN_WGSL.into()),
            });
            (
                make_down(&shader, &shader),
                ProbeRing::new(device, &down_layout, &shader, &shader),
            )
        });

        let comp_wgsl = composite_wgsl();
        #[cfg(target_os = "windows")]
        let comp_pre = dxbc::liquid_glass_composite(device, &comp_wgsl);
        #[cfg(not(target_os = "windows"))]
        let comp_pre = None;
        let composite_precompiled = comp_pre.is_some();
        let composite_shaders = comp_pre.unwrap_or_else(|| composite_from_wgsl(device, &comp_wgsl));

        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("liquid glass composite"),
            bind_group_layouts: &[Some(uber), Some(&result_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("liquid glass"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let dummy = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("liquid glass dummy"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: layers::FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());

        let init_epochs = 4usize;
        let init_shapes = 32usize;
        let down_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid glass down uniforms"),
            size: (init_epochs * SLOTS_PER_EPOCH) as u64 * SLOT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // +1 slot at end reserved for still-mode fallback uniform.
        let epoch_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid glass epoch uniforms"),
            size: (init_epochs + 1) as u64 * SLOT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shapes_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("liquid glass shapes"),
            size: (init_shapes * std::mem::size_of::<GlassShapeGpu>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            down_pipeline,
            down_layout,
            composite_layout,
            result_layout,
            composite_shaders,
            composite_precompiled,
            composites: Vec::new(),
            sampler,
            dummy,
            down_uniforms,
            down_capacity_epochs: init_epochs,
            epoch_uniforms,
            epoch_capacity: init_epochs,
            shapes_buf,
            shapes_capacity: init_shapes,
            pyramid: None,
            first_down: Vec::new(),
            composite_binds: Vec::new(),
            still_bind: None,
            probe_bind: None,
            probe: Mutex::new(probe),
            epochs: Vec::new(),
            records: Vec::new(),
            viewport: (0, 0),
            broken: false,
            stats: LiquidGlassStats::default(),
        }
    }

    pub(super) fn give_back(&mut self) {
        self.pyramid = None;
        self.first_down.clear();
        self.composite_binds.clear();
        self.still_bind = None;
        self.probe_bind = None;
        self.stats.pyramid_bytes = 0;
    }

    pub(super) fn bind_backdrop(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        view: &wgpu::TextureView,
    ) {
        let down_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("liquid glass down 0"),
            layout: &self.down_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.down_uniforms,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<DownUniform>() as u64),
                    }),
                },
            ],
        });
        match self.first_down.iter_mut().find(|(f, _)| *f == format) {
            Some(slot) => slot.1 = down_bind,
            None => self.first_down.push((format, down_bind)),
        }
        if let Some(pyramid) = &self.pyramid {
            let comp_bind = self.make_composite_bind(device, &pyramid.full_view, view);
            match self.composite_binds.iter_mut().find(|(f, _)| *f == format) {
                Some(slot) => slot.1 = comp_bind,
                None => self.composite_binds.push((format, comp_bind)),
            }
        }
    }

    fn make_composite_bind(
        &self,
        device: &wgpu::Device,
        frost_view: &wgpu::TextureView,
        backdrop_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("liquid glass composite"),
            layout: &self.result_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(frost_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(backdrop_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.shapes_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.epoch_uniforms,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<EpochUniform>() as u64),
                    }),
                },
            ],
        })
    }

    fn rebuild_binds(&mut self, device: &wgpu::Device, backdrops: &[Backdrop]) {
        self.first_down.clear();
        self.composite_binds.clear();
        self.still_bind = Some(self.make_composite_bind(device, &self.dummy, &self.dummy));
        if let Some(pyramid) = &self.pyramid {
            let probe = self.probe.lock().unwrap();
            self.probe_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("liquid glass probe src"),
                layout: &self.down_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&pyramid.full_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &probe.uniform_buf,
                            offset: 0,
                            size: wgpu::BufferSize::new(std::mem::size_of::<DownUniform>() as u64),
                        }),
                    },
                ],
            }));
        } else {
            self.probe_bind = None;
        }
        for bd in backdrops {
            self.bind_backdrop(device, bd.format, &bd.view);
        }
    }

    pub(super) fn ensure_composite(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if self.broken || self.composites.iter().any(|(f, _)| *f == format) {
            return;
        }
        #[cfg(target_os = "windows")]
        if self.composite_precompiled {
            if let Some(pipeline) =
                dxbc::checked(device, "liquid glass composite", "compiling WGSL", || {
                    make_composite_pipeline(
                        device,
                        &self.composite_layout,
                        &self.composite_shaders,
                        format,
                    )
                })
            {
                self.composites.push((format, pipeline));
                return;
            }
            self.composite_shaders = composite_from_wgsl(device, &composite_wgsl());
            self.composite_precompiled = false;
        }
        let pipeline = make_composite_pipeline(
            device,
            &self.composite_layout,
            &self.composite_shaders,
            format,
        );
        self.composites.push((format, pipeline));
    }

    fn ensure_buffers(
        &mut self,
        device: &wgpu::Device,
        epochs: usize,
        shapes: usize,
        backdrops: &[Backdrop],
    ) {
        let mut rebuilt = false;
        let need_ep = epochs.max(1);
        if need_ep > self.down_capacity_epochs {
            let cap = need_ep.next_power_of_two();
            self.down_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("liquid glass down uniforms"),
                size: (cap * SLOTS_PER_EPOCH) as u64 * SLOT,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.down_capacity_epochs = cap;
            self.pyramid = None;
            rebuilt = true;
        }
        if need_ep > self.epoch_capacity {
            let cap = need_ep.next_power_of_two();
            self.epoch_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("liquid glass epoch uniforms"),
                size: (cap + 1) as u64 * SLOT,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.epoch_capacity = cap;
            rebuilt = true;
        }
        let need_sh = shapes.max(1);
        if need_sh > self.shapes_capacity {
            let cap = need_sh.next_power_of_two();
            self.shapes_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("liquid glass shapes"),
                size: (cap * std::mem::size_of::<GlassShapeGpu>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.shapes_capacity = cap;
            rebuilt = true;
        }
        if rebuilt || self.still_bind.is_none() {
            self.rebuild_binds(device, backdrops);
        }
    }

    fn ensure_pyramid(
        &mut self,
        device: &wgpu::Device,
        need: [u32; 2],
        cap: [u32; 2],
        backdrops: &[Backdrop],
    ) {
        let have = self.pyramid.as_ref().map_or([0; 2], |p| p.size);
        let size = [0, 1].map(|k| have[k].min(cap[k]).max(need[k]));
        if size == have && self.pyramid.is_some() {
            return;
        }
        self.pyramid = Some(create_pyramid(
            device,
            size,
            &self.down_layout,
            &self.sampler,
            &self.down_uniforms,
        ));
        self.rebuild_binds(device, backdrops);
    }

    /// Encodes the down-only frost pyramid (F0..F_max) for `epoch_idx`, plus the luminance probe
    /// pass on epoch 0 when any `glass_id` probes are queued.
    fn encode_epoch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        format: wgpu::TextureFormat,
        epoch_idx: usize,
        timestamps: Option<&wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let (Some(pyramid), Some((_, first))) = (
            &self.pyramid,
            self.first_down.iter().find(|(f, _)| *f == format),
        ) else {
            return;
        };
        let ep = &self.epochs[epoch_idx];
        let base_slot = epoch_idx * SLOTS_PER_EPOCH;
        let max_mip = ep.max_mip.min(MIPS as usize - 1);
        for m in 0..=max_mip {
            let (x, y, r, b) = mip_scissor(ep.region, ep.origin, pyramid.size, m);
            let ts = timestamps.and_then(|t| {
                let begin = (m == 0)
                    .then_some(t.beginning_of_pass_write_index)
                    .flatten();
                let end = (m == max_mip)
                    .then_some(t.end_of_pass_write_index)
                    .flatten();
                (begin.is_some() || end.is_some()).then_some(wgpu::RenderPassTimestampWrites {
                    query_set: t.query_set,
                    beginning_of_pass_write_index: begin,
                    end_of_pass_write_index: end,
                })
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("liquid glass down"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &pyramid.mip_views[m],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: ts,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(x, y, r - x, b - y);
            pass.set_pipeline(&self.down_pipeline);
            let bind = if m == 0 {
                first
            } else {
                &pyramid.down_binds[m - 1]
            };
            let dyn_off = ((base_slot + m) as u64 * SLOT) as u32;
            pass.set_bind_group(0, bind, &[dyn_off]);
            pass.draw(0..3, 0..1);
        }
        if epoch_idx == 0
            && let Some(probe_bind) = &self.probe_bind
            && let Ok(mut probe) = self.probe.lock()
        {
            probe.encode(encoder, probe_bind);
        }
    }
}

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

    pub(super) fn has_liquid_glass(&self) -> bool {
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

    pub(super) fn prepare_liquid_glass(
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
    pub(super) fn draw_liquid_glass(
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
                Some(index) => &self.layers.targets[index].as_ref().unwrap().base,
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

#[cfg(test)]
mod tests;
