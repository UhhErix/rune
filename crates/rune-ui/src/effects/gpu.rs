//! Clean-room wgpu shader compositor (`UiEffectsCompositor`) and declarative background / border
//! effect builders (`BackgroundEffect`, `BorderEffect`).
//!
//! Every effect automatically degrades to a **zero-GPU-cost static fallback** when
//! `!theme.effect_gate.allows_gpu_fx()` (GPU disabled, reduced motion, or a game is running),
//! and `UiEffectsCompositor::animating()` returns `false` while paused so idle CPU/GPU is 0%.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::{Color, El, el};
use wgpu::util::DeviceExt;

use crate::theme::{EffectGate, UiTheme};

/// Reserved `El::effect(mark)` base tag for `rune-ui` GPU effects.
pub const EFFECT_MARK_BASE: u32 = 0x5255_0000;
const EFFECT_MARK_MASK: u32 = 0xFFFF_0000;

/// Procedural GPU background shader kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BackgroundKind {
    Aurora = 0,
    Particles = 1,
    Waves = 2,
    Grid = 3,
    Iridescence = 6,
}

/// Procedural GPU border shader kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BorderKind {
    Beam = 4,
    Glow = 5,
    Spotlight = 7,
}

/// Declarative animated background layer with automatic zero-GPU static fallback.
pub struct BackgroundEffect {
    key: String,
    kind: BackgroundKind,
    speed: f32,
    intensity: f32,
    radius: f32,
    fallback_color: Option<Color>,
    slot_id: u16,
}

impl BackgroundEffect {
    pub fn new(key: impl Into<String>, slot_id: u16, kind: BackgroundKind) -> Self {
        Self {
            key: key.into(),
            kind,
            speed: 1.0,
            intensity: 0.85,
            radius: 14.0,
            fallback_color: None,
            slot_id,
        }
    }

    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.0);
        self
    }

    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity.clamp(0.0, 1.5);
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius.max(0.0);
        self
    }

    pub fn fallback_color(mut self, color: Color) -> Self {
        self.fallback_color = Some(color);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        let fallback = self
            .fallback_color
            .unwrap_or_else(|| theme.kit.c.bg_accent.alpha(0.08));
        if !theme.effect_gate.allows_gpu_fx() {
            return el()
                .key(self.key)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .radius(self.radius)
                .fill(fallback)
                .a11y_hidden();
        }
        let mark = EFFECT_MARK_BASE | u32::from(self.slot_id);
        el().key(self.key)
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .radius(self.radius)
            .effect(mark)
            .effect_data([
                self.kind as u32 as f32,
                self.speed,
                self.intensity,
                self.radius,
            ])
            .a11y_hidden()
    }
}

/// Declarative animated border overlay (`BorderBeam` or `GlowBorder`) with zero-GPU fallback.
pub struct BorderEffect {
    key: String,
    kind: BorderKind,
    speed: f32,
    intensity: f32,
    radius: f32,
    slot_id: u16,
}

impl BorderEffect {
    pub fn beam(key: impl Into<String>, slot_id: u16) -> Self {
        Self {
            key: key.into(),
            kind: BorderKind::Beam,
            speed: 1.0,
            intensity: 0.95,
            radius: 14.0,
            slot_id,
        }
    }

    pub fn glow(key: impl Into<String>, slot_id: u16) -> Self {
        Self {
            key: key.into(),
            kind: BorderKind::Glow,
            speed: 1.0,
            intensity: 0.9,
            radius: 14.0,
            slot_id,
        }
    }

    pub fn spotlight(key: impl Into<String>, slot_id: u16) -> Self {
        Self {
            key: key.into(),
            kind: BorderKind::Spotlight,
            speed: 1.0,
            intensity: 0.95,
            radius: 14.0,
            slot_id,
        }
    }

    pub fn speed(mut self, s: f32) -> Self {
        self.speed = s.max(0.0);
        self
    }

    pub fn intensity(mut self, i: f32) -> Self {
        self.intensity = i.clamp(0.0, 1.5);
        self
    }

    pub fn radius(mut self, r: f32) -> Self {
        self.radius = r.max(0.0);
        self
    }

    pub fn build(self, theme: &UiTheme) -> El {
        if !theme.effect_gate.allows_gpu_fx() {
            return el()
                .key(self.key)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .radius(self.radius)
                .border(1.0, theme.float_line)
                .a11y_hidden();
        }
        let mark = EFFECT_MARK_BASE | u32::from(self.slot_id);
        el().key(self.key)
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .radius(self.radius)
            .effect(mark)
            .effect_bleed(if self.kind == BorderKind::Glow {
                12.0
            } else {
                2.0
            })
            .effect_data([
                self.kind as u32 as f32,
                self.speed,
                self.intensity,
                self.radius,
            ])
            .a11y_hidden()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EffectUniforms {
    rect: [f32; 4],
    target: [f32; 4],
    params: [f32; 4],
    extra: [f32; 4],
    color_a: [f32; 4],
    color_b: [f32; 4],
    color_c: [f32; 4],
}

struct GpuState {
    device_key: DeviceKey,
    shader: wgpu::ShaderModule,
    bgl: wgpu::BindGroupLayout,
    pipe_layout: wgpu::PipelineLayout,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl GpuState {
    fn new(device: &wgpu::Device, device_key: DeviceKey) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rune-ui-effects-wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rune-ui-effects-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rune-ui-effects-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        Self {
            device_key,
            shader,
            bgl,
            pipe_layout,
            pipelines: HashMap::new(),
        }
    }

    fn pipeline_for(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rune-ui-effects-pipeline"),
                layout: Some(&self.pipe_layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

/// Palette passed to [`UiEffectsCompositor`].
#[derive(Clone, Copy, Debug)]
pub struct EffectPalette {
    pub primary: Color,
    pub secondary: Color,
    pub tertiary: Color,
}

impl Default for EffectPalette {
    fn default() -> Self {
        Self {
            primary: Color {
                r: 0.25,
                g: 0.55,
                b: 1.0,
                a: 1.0,
            },
            secondary: Color {
                r: 0.62,
                g: 0.28,
                b: 0.98,
                a: 1.0,
            },
            tertiary: Color {
                r: 0.14,
                g: 0.85,
                b: 0.78,
                a: 1.0,
            },
        }
    }
}

/// Rune [`Compositor`] executing `rune-ui::effects` wgpu shader slots.
///
/// Automatically reports `animating() == false` and `draws_own() == false` whenever
/// `EffectGate::allows_gpu_fx()` is false (reduced motion, GPU disabled, or a game is running)
/// or when no effect slots appeared in the frame.
pub struct UiEffectsCompositor<S: 'static> {
    gate_fn: Box<dyn Fn(&S) -> EffectGate>,
    palette: EffectPalette,
    gpu: Option<GpuState>,
    last_gate: EffectGate,
    had_slots: bool,
    time_secs: f32,
}

impl<S: 'static> UiEffectsCompositor<S> {
    pub fn new(gate_fn: impl Fn(&S) -> EffectGate + 'static) -> Self {
        Self {
            gate_fn: Box::new(gate_fn),
            palette: EffectPalette::default(),
            gpu: None,
            last_gate: EffectGate::default(),
            had_slots: false,
            time_secs: 0.0,
        }
    }

    pub fn with_palette(mut self, palette: EffectPalette) -> Self {
        self.palette = palette;
        self
    }

    /// True if a `mark` belongs to `rune-ui::effects`.
    pub fn owns_mark(mark: u32) -> bool {
        (mark & EFFECT_MARK_MASK) == EFFECT_MARK_BASE
    }
}

impl<S: 'static> Compositor<S> for UiEffectsCompositor<S> {
    fn render(&mut self, f: &mut Frame<'_, S>) {
        let gate = (self.gate_fn)(&f.rt.state);
        self.last_gate = gate;
        if gate.allows_gpu_fx() {
            self.time_secs += f.dt as f32;
        }

        let dev_key = f.device_key();
        let device = f.device;
        let queue = f.queue;
        let clear = f.rt.clear_color();
        let win_w = (f.size.0 as f32).max(1.0);
        let win_h = (f.size.1 as f32).max(1.0);
        let (ptr_u, ptr_v) = match f.rt.pointer() {
            Some((px, py)) => ((px / win_w).clamp(0.0, 1.0), (py / win_h).clamp(0.0, 1.0)),
            None => (0.5, 0.5),
        };

        if self.gpu.as_ref().is_none_or(|g| g.device_key != dev_key) {
            self.gpu = Some(GpuState::new(device, dev_key));
        }

        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        let mut saw_slot = false;
        let time_secs = self.time_secs;
        let pal = self.palette;

        if let Some(gpu) = self.gpu.as_mut() {
            f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
                if !Self::owns_mark(site.mark) || !gate.allows_gpu_fx() {
                    return;
                }
                saw_slot = true;
                let r = site.instance.rect;
                let uniforms = EffectUniforms {
                    rect: [r[0] - site.origin[0], r[1] - site.origin[1], r[2], r[3]],
                    target: [
                        site.size[0] as f32,
                        site.size[1] as f32,
                        site.scale,
                        if site.srgb_blend { 1.0 } else { 0.0 },
                    ],
                    params: [site.data[0], time_secs, site.data[1], site.data[2]],
                    extra: [
                        site.data[3] * site.scale,
                        ptr_u,
                        ptr_v,
                        (site.mark & 0xFFFF) as f32,
                    ],
                    color_a: [pal.primary.r, pal.primary.g, pal.primary.b, pal.primary.a],
                    color_b: [
                        pal.secondary.r,
                        pal.secondary.g,
                        pal.secondary.b,
                        pal.secondary.a,
                    ],
                    color_c: [
                        pal.tertiary.r,
                        pal.tertiary.g,
                        pal.tertiary.b,
                        pal.tertiary.a,
                    ],
                };
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("rune-ui-fx-ubo"),
                    contents: bytemuck::bytes_of(&uniforms),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("rune-ui-fx-bg"),
                    layout: &gpu.bgl,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buf.as_entire_binding(),
                    }],
                });
                let pipe = gpu.pipeline_for(device, site.format);
                pass.set_pipeline(pipe);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..6, 0..1);
            });
        }
        self.had_slots = saw_slot;
    }

    fn release_gpu(&mut self) {
        self.gpu = None;
    }

    fn animating(&self) -> bool {
        self.had_slots && self.last_gate.allows_gpu_fx()
    }

    fn draws_own(&self) -> bool {
        self.had_slots && self.last_gate.allows_gpu_fx()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_gate_pauses_gpu_fx_when_game_running_or_reduced_motion() {
        let normal = EffectGate::default();
        assert!(normal.allows_gpu_fx());
        let gaming = EffectGate {
            game_running: true,
            ..normal
        };
        assert!(!gaming.allows_gpu_fx());
        assert!(!gaming.allows_animation());
        let reduced = EffectGate {
            reduced_motion: true,
            ..normal
        };
        assert!(!reduced.allows_gpu_fx());
        let no_gpu = EffectGate {
            gpu_enabled: false,
            ..normal
        };
        assert!(!no_gpu.allows_gpu_fx());
        assert!(no_gpu.allows_animation());
    }
}
