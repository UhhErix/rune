//! Liquid Glass composite ABI (`GlassShapeGpu`, `EpochUniform`) and composite pipeline builder.

use bytemuck::{Pod, Zeroable};

use super::plan::EpochPlan;
use super::pyramid::MIPS;
use crate::render::{Blend, CLIP_WGSL, blend_state, dxbc, metallib};

/// Mode flag bits packed into `GlassShapeGpu::light[1]`.
pub const MODE_REGULAR: u32 = 0;
pub const MODE_CLEAR: u32 = 1;
pub const MODE_CUSTOM: u32 = 2;
pub const FLAG_LARGE: u32 = 1 << 2;
pub const FLAG_CURSOR_LIGHT: u32 = 1 << 3;
pub const FLAG_REDUCED_TRANSPARENCY: u32 = 1 << 4;
pub const FLAG_INCREASED_CONTRAST: u32 = 1 << 5;
pub const FLAG_HAS_TINT: u32 = 1 << 6;

/// Maximum number of shapes evaluated in a single `KIND_LIQUID_GLASS` composite draw.
pub const MAX_SHAPES_PER_DRAW: usize = 32;

/// Per-shape GPU descriptor uploaded to `@group(1) @binding(3)` (`array<GlassShape>`, 192 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct GlassShapeGpu {
    /// Device-pixel rect `[x, y, w, h]`.
    pub rect: [f32; 4],
    /// Corner radii `[tl, tr, br, bl]` in device px (negative = continuous corners).
    pub radii: [f32; 4],
    /// `[brightness, contrast, saturation, sigma_px]`.
    pub tone: [f32; 4],
    /// Tint colour in sRGB-encoded space `[r, g, b, a]`.
    pub tint: [f32; 4],
    /// Rim edge alphas `[top, side, bottom, width_px]`.
    pub rim: [f32; 4],
    /// Rim colour in sRGB-encoded space `[r, g, b]` + specular intensity `[w]`.
    pub rim_c: [f32; 4],
    /// Inner shadow `[offset_y_px, blur_px, spread_px]` + sheen alpha `[w]`.
    pub inner: [f32; 4],
    /// Inner shadow colour in sRGB-encoded space `[r, g, b, a]`.
    pub inner_c: [f32; 4],
    /// Optics `[bevel_px, max_refract_px, dispersion, sheen_stop]`.
    pub bend: [f32; 4],
    /// `[light_angle_rad, mode_flags, union_group_id, shape_opacity]`.
    pub light: [f32; 4],
    /// Touch glow `[center_x_px, center_y_px, radius_px, strength]`.
    pub glow: [f32; 4],
    /// `[shadow_blur_px, shadow_y_px, hover_amount, materialize]`.
    pub extra: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<GlassShapeGpu>() == 192);
    assert!(std::mem::offset_of!(GlassShapeGpu, rect) == 0);
    assert!(std::mem::offset_of!(GlassShapeGpu, radii) == 16);
    assert!(std::mem::offset_of!(GlassShapeGpu, tone) == 32);
    assert!(std::mem::offset_of!(GlassShapeGpu, tint) == 48);
    assert!(std::mem::offset_of!(GlassShapeGpu, rim) == 64);
    assert!(std::mem::offset_of!(GlassShapeGpu, rim_c) == 80);
    assert!(std::mem::offset_of!(GlassShapeGpu, inner) == 96);
    assert!(std::mem::offset_of!(GlassShapeGpu, inner_c) == 112);
    assert!(std::mem::offset_of!(GlassShapeGpu, bend) == 128);
    assert!(std::mem::offset_of!(GlassShapeGpu, light) == 144);
    assert!(std::mem::offset_of!(GlassShapeGpu, glow) == 160);
    assert!(std::mem::offset_of!(GlassShapeGpu, extra) == 176);
};

/// Per-epoch uniform block bound at `@group(1) @binding(4)` (96 bytes, 256-byte dynamic stride).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct EpochUniform {
    pub pyramid_map: [f32; 4],
    pub backdrop_min: [f32; 4],
    pub clamp_a: [f32; 4],
    pub clamp_b: [f32; 4],
    pub pointer: [f32; 4],
    pub flags: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<EpochUniform>() == 96);
    assert!(std::mem::offset_of!(EpochUniform, pyramid_map) == 0);
    assert!(std::mem::offset_of!(EpochUniform, backdrop_min) == 16);
    assert!(std::mem::offset_of!(EpochUniform, clamp_a) == 32);
    assert!(std::mem::offset_of!(EpochUniform, clamp_b) == 48);
    assert!(std::mem::offset_of!(EpochUniform, pointer) == 64);
    assert!(std::mem::offset_of!(EpochUniform, flags) == 80);
};

/// Full composite WGSL source: `clip.wgsl` prepended to `liquid_glass.wgsl`.
pub(crate) fn composite_wgsl() -> String {
    format!(
        "{CLIP_WGSL}\n{}",
        include_str!("../../shaders/liquid_glass.wgsl")
    )
}

pub(super) fn composite_from_wgsl(device: &wgpu::Device, wgsl: &str) -> dxbc::Shaders {
    dxbc::Shaders::one(device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("liquid glass composite"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    }))
}

pub(super) fn make_composite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shaders: &dxbc::Shaders,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("liquid glass composite"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shaders.vertex,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(metallib::uber_vertex_layout())],
        },
        primitive: metallib::uber_primitive(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shaders.fragment,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend_state(Blend::SourceOver)),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Builds the `EpochUniform` for one capture epoch (or still-mode fallback).
pub(super) fn build_epoch_uniform(
    epoch: &EpochPlan,
    pyramid_size: [u32; 2],
    viewport: (u32, u32),
    pointer: [f32; 4],
    still_mode: bool,
    target_is_srgb: bool,
    saturation_scale: f32,
) -> EpochUniform {
    let vp_w = viewport.0.max(1) as f32;
    let vp_h = viewport.1.max(1) as f32;
    let ext_w = (pyramid_size[0] as f32 * 2.0).max(1.0);
    let ext_h = (pyramid_size[1] as f32 * 2.0).max(1.0);
    let p_min_u = ((epoch.region[0] - epoch.origin[0]) / ext_w).clamp(0.0, 1.0);
    let p_min_v = ((epoch.region[1] - epoch.origin[1]) / ext_h).clamp(0.0, 1.0);
    let p_max_u = ((epoch.region[2] - epoch.origin[0]) / ext_w).clamp(p_min_u, 1.0);
    let p_max_v = ((epoch.region[3] - epoch.origin[1]) / ext_h).clamp(p_min_v, 1.0);
    let b_min_u = ((epoch.region[0] + 0.5) / vp_w).clamp(0.0, 1.0);
    let b_min_v = ((epoch.region[1] + 0.5) / vp_h).clamp(0.0, 1.0);
    let b_max_u = ((epoch.region[2] - 0.5) / vp_w).clamp(b_min_u, 1.0);
    let b_max_v = ((epoch.region[3] - 0.5) / vp_h).clamp(b_min_v, 1.0);
    EpochUniform {
        pyramid_map: [epoch.origin[0], epoch.origin[1], 1.0 / ext_w, 1.0 / ext_h],
        backdrop_min: [1.0 / vp_w, 1.0 / vp_h, p_min_u, p_min_v],
        clamp_a: [p_max_u, p_max_v, b_min_u, b_min_v],
        clamp_b: [b_max_u, b_max_v, vp_w, vp_h],
        pointer,
        flags: [
            if still_mode { 1.0 } else { 0.0 },
            if target_is_srgb { 1.0 } else { 0.0 },
            saturation_scale,
            MIPS as f32,
        ],
    }
}
