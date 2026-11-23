//! Shared down-only frost pyramid (`F0..F6`, `Rgba16Float`, single 7-mip texture) for Liquid Glass.
//!
//! One pyramid is pooled per renderer and reused across every capture epoch in a frame (GPU command
//! order guarantees epoch `k`'s composite draws finish reading the pyramid before epoch `k + 1`'s
//! down chain overwrites it). Freed at rest by `Renderer::give_back` when no glass drew.

use bytemuck::{Pod, Zeroable};

use super::plan::EpochPlan;
use crate::render::layers;

/// Number of mip levels in the down-only frost pyramid (F0 = 1/2 res .. F6 = 1/128 res).
pub const MIPS: u32 = 7;

/// Pyramid level-0 width/height alignment in half-res texels so every mip halves cleanly without
/// reallocating on small region changes.
pub const STEP: u32 = 1 << (MIPS - 1);

/// Uniform dynamic offset stride (WebGPU worst-case `min_uniform_buffer_offset_alignment`).
pub(super) const SLOT: u64 = 256;

/// Uniform slots written per epoch (one per mip `0..MIPS`).
pub(super) const SLOTS_PER_EPOCH: usize = MIPS as usize;

/// Measured line-spread sigmas (device px) for 13-tap Jimenez down to level `m` + cubic B-spline up.
pub const GLASS_SIGMAS: [f32; MIPS as usize] = [1.76, 3.75, 7.62, 15.30, 30.63, 61.27, 122.55];

/// Highest mip level needed to reconstruct `sigma_px` via trilinear B-spline.
#[inline]
pub fn sigma_max_mip(sigma_px: f32) -> usize {
    if sigma_px <= GLASS_SIGMAS[0] {
        return 0;
    }
    for (m, &s) in GLASS_SIGMAS.iter().enumerate().skip(1) {
        if sigma_px <= s {
            return m;
        }
    }
    MIPS as usize - 1
}

/// GPU uniform for `liquid_glass_down.wgsl` (`DownUniform`, 64 bytes).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct DownUniform {
    pub w: [f32; 4],
    pub region: [f32; 4],
    pub map: [f32; 4],
    pub extra: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<DownUniform>() == 64);
    assert!(std::mem::offset_of!(DownUniform, w) == 0);
    assert!(std::mem::offset_of!(DownUniform, region) == 16);
    assert!(std::mem::offset_of!(DownUniform, map) == 32);
    assert!(std::mem::offset_of!(DownUniform, extra) == 48);
};

/// Pooled 7-mip `Rgba16Float` frost pyramid texture and per-mip views/bindings.
pub(super) struct FrostPyramid {
    pub size: [u32; 2],
    /// Full mip-chain view (`0..MIPS`) bound by the composite and probe passes.
    pub full_view: wgpu::TextureView,
    /// Per-mip single-level views (`0..MIPS`) used as render targets and down sources.
    pub mip_views: Vec<wgpu::TextureView>,
    /// Bind groups for down pass `m` (`1..MIPS`), reading mip `m - 1`.
    pub down_binds: Vec<wgpu::BindGroup>,
}

impl FrostPyramid {
    /// Total GPU byte size of this 7-mip `Rgba16Float` pyramid (8 bytes per texel).
    pub fn byte_size(&self) -> usize {
        pyramid_bytes(self.size)
    }
}

/// Computes exact GPU memory usage in bytes for a `MIPS`-level `Rgba16Float` pyramid of level-0
/// dimensions `size` (in half-resolution texels).
pub fn pyramid_bytes(size: [u32; 2]) -> usize {
    let mut total = 0usize;
    for m in 0..MIPS {
        let w = (size[0] >> m).max(1) as usize;
        let h = (size[1] >> m).max(1) as usize;
        total += w * h * 8; // Rgba16Float = 8 B/texel
    }
    total
}

/// Inclusive-start, exclusive-end scissor rect `(x0, y0, x1, y1)` at mip `m` covering `region`
/// plus a 2-texel filter apron.
pub(super) fn mip_scissor(
    region: [f32; 4],
    origin: [f32; 2],
    pyramid_size: [u32; 2],
    m: usize,
) -> (u32, u32, u32, u32) {
    let k = (2u32 << m) as f32;
    let (w, h) = ((pyramid_size[0] >> m).max(1), (pyramid_size[1] >> m).max(1));
    let x = (((region[0] - origin[0]) / k).floor() as u32)
        .saturating_sub(2)
        .min(w - 1);
    let y = (((region[1] - origin[1]) / k).floor() as u32)
        .saturating_sub(2)
        .min(h - 1);
    let r = (((region[2] - origin[0]) / k).ceil() as u32 + 2).min(w);
    let b = (((region[3] - origin[1]) / k).ceil() as u32 + 2).min(h);
    (x, y, r.max(x + 1), b.max(y + 1))
}

/// Writes per-mip `DownUniform` entries for one epoch into `dst` (slice of `SLOTS_PER_EPOCH * SLOT` bytes).
pub(super) fn write_epoch_down_uniforms(
    dst: &mut [u8],
    epoch: &EpochPlan,
    pyramid_size: [u32; 2],
    viewport: (u32, u32),
    encode_srgb_on_pass0: bool,
) {
    let vp = [viewport.0.max(1) as f32, viewport.1.max(1) as f32];
    let extent = [pyramid_size[0] as f32 * 2.0, pyramid_size[1] as f32 * 2.0];
    let inside = [
        (epoch.region[0] - epoch.origin[0]) / extent[0],
        (epoch.region[1] - epoch.origin[1]) / extent[1],
        (epoch.region[2] - epoch.origin[0]) / extent[0],
        (epoch.region[3] - epoch.origin[1]) / extent[1],
    ];
    for m in 0..MIPS as usize {
        let (region, map, encode_flag) = if m == 0 {
            (
                [
                    epoch.region[0] / vp[0],
                    epoch.region[1] / vp[1],
                    epoch.region[2] / vp[0],
                    epoch.region[3] / vp[1],
                ],
                [
                    extent[0] / vp[0],
                    extent[1] / vp[1],
                    epoch.origin[0] / vp[0],
                    epoch.origin[1] / vp[1],
                ],
                if encode_srgb_on_pass0 { 1.0 } else { 0.0 },
            )
        } else {
            (inside, [1.0, 1.0, 0.0, 0.0], 0.0)
        };
        let u = DownUniform {
            w: [encode_flag, 0.0, 0.0, 0.0],
            region,
            map,
            extra: [0.0; 4],
        };
        let offset = m * SLOT as usize;
        dst[offset..offset + std::mem::size_of::<DownUniform>()]
            .copy_from_slice(bytemuck::bytes_of(&u));
    }
}

/// Allocates a `FrostPyramid` of level-0 dimensions `size`.
pub(super) fn create_pyramid(
    device: &wgpu::Device,
    size: [u32; 2],
    down_layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    uniforms: &wgpu::Buffer,
) -> FrostPyramid {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("liquid glass frost pyramid"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: layers::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let full_view = tex.create_view(&wgpu::TextureViewDescriptor {
        base_mip_level: 0,
        mip_level_count: Some(MIPS),
        ..Default::default()
    });
    let mip_views: Vec<wgpu::TextureView> = (0..MIPS)
        .map(|m| {
            tex.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: m,
                mip_level_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    let uniform_res = wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer: uniforms,
        offset: 0,
        size: wgpu::BufferSize::new(std::mem::size_of::<DownUniform>() as u64),
    });
    let down_binds = (1..MIPS as usize)
        .map(|m| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("liquid glass down level"),
                layout: down_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&mip_views[m - 1]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform_res.clone(),
                    },
                ],
            })
        })
        .collect();
    FrostPyramid {
        size,
        full_view,
        mip_views,
        down_binds,
    }
}
