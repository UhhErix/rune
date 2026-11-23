// Liquid Glass down-only frost pyramid (13-tap Jimenez downsample, F0..F6) and coarse luminance
// probe pass. Pass 0 optionally encodes linear `*Srgb` backdrop texels to sRGB (`u.w.x > 0.5`) so
// the frost pyramid always filters in sRGB-encoded space, matching Chromium's `backdrop-filter`.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;

struct DownUniform {
    // x: encode linear->sRGB (1.0 on pass 0 from an *Srgb target, 0.0 otherwise).
    // y: probe count (used by fs_probe), z: mip level for coarse probe, w: unused.
    w: vec4<f32>,
    // Clamped source UV bounds: [min_u, min_v, max_u, max_v].
    region: vec4<f32>,
    // Source UV from target UV: uv * xy + zw.
    map: vec4<f32>,
    // Reserved / probe UV for single-slot fallback.
    extra: vec4<f32>,
};
@group(0) @binding(2) var<uniform> u: DownUniform;

struct V {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32) -> V {
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    var o: V;
    o.pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}

fn srgb_encode_ch(x: f32) -> f32 {
    let c = max(x, 0.0);
    return select(1.055 * pow(c, 1.0 / 2.4) - 0.055, c * 12.92, c <= 0.0031308);
}

fn srgb_encode3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_encode_ch(c.r), srgb_encode_ch(c.g), srgb_encode_ch(c.b));
}

fn tap(uv: vec2<f32>) -> vec4<f32> {
    var c = textureSampleLevel(src, smp, clamp(uv, u.region.xy, u.region.zw), 0.0);
    if (u.w.x > 0.5) {
        c = vec4<f32>(srgb_encode3(c.rgb), c.a);
    }
    return c;
}

@fragment
fn fs_down(in: V) -> @location(0) vec4<f32> {
    let t = 1.0 / vec2<f32>(textureDimensions(src, 0));
    let uv = in.uv * u.map.xy + u.map.zw;
    let a = tap(uv + t * vec2<f32>(-2.0, -2.0));
    let b = tap(uv + t * vec2<f32>(0.0, -2.0));
    let c = tap(uv + t * vec2<f32>(2.0, -2.0));
    let d = tap(uv + t * vec2<f32>(-1.0, -1.0));
    let e = tap(uv + t * vec2<f32>(1.0, -1.0));
    let f = tap(uv + t * vec2<f32>(-2.0, 0.0));
    let g = tap(uv);
    let h = tap(uv + t * vec2<f32>(2.0, 0.0));
    let i = tap(uv + t * vec2<f32>(-1.0, 1.0));
    let j = tap(uv + t * vec2<f32>(1.0, 1.0));
    let k = tap(uv + t * vec2<f32>(-2.0, 2.0));
    let l = tap(uv + t * vec2<f32>(0.0, 2.0));
    let m = tap(uv + t * vec2<f32>(2.0, 2.0));
    return (d + e + i + j) * 0.125
        + (a + b + g + f) * 0.03125
        + (b + c + h + g) * 0.03125
        + (f + g + l + k) * 0.03125
        + (g + h + m + l) * 0.03125;
}

struct ProbeCoord {
    // [uv_x, uv_y, coarse_lod, fine_lod]
    uv: vec4<f32>,
};
@group(1) @binding(0) var<storage, read> probes: array<ProbeCoord>;

@fragment
fn fs_probe(in: V) -> @location(0) vec4<f32> {
    let idx = u32(floor(in.pos.x));
    let count = u32(max(u.w.y, 1.0));
    let slot = min(idx, count - 1u);
    let p = probes[slot];
    let uv = clamp(p.uv.xy, u.region.xy, u.region.zw);
    let coarse = textureSampleLevel(src, smp, uv, p.uv.z).rgb;
    let fine = textureSampleLevel(src, smp, uv, p.uv.w).rgb;
    let luma = dot(coarse, vec3<f32>(0.2126, 0.7152, 0.0722));
    let detail = length(fine - coarse);
    return vec4<f32>(luma, detail, 0.0, 1.0);
}
