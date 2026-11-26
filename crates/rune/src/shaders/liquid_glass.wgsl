// Liquid Glass composite shader (`El::liquid_glass`, `KIND_LIQUID_GLASS`).
// Prepended with `clip.wgsl` (`sd_rounded`, `sd_continuous`, `clip_alpha_stops`).
//
// Evaluates up to 32 shapes in a container via smooth-minimum (`smin`), computes the bevel
// height-field normal, `ratio^8` smoothstep inward refraction, per-channel chromatic dispersion,
// 4-tap bicubic B-spline frost reconstruction from the shared down-only pyramid, backdrop
// saturation and adaptive/Chromium tone, `pow 6` Fresnel + cursor-leaning two-lobe specular rim,
// interactive touch glow, hover brightening, accessibility overrides and adaptive drop shadow.
//
// Portions of the bevel height-field normal, `ratio^8` refraction profile, `pow 6` Fresnel,
// cursor-leaning rim and saturation math are adapted from bilalgursen/liquid-glass (MIT).

struct Globals {
    viewport: vec2<f32>,
    pad: vec2<f32>,
    flags: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(3) var<uniform> projection: vec4<f32>;

struct GlassShape {
    // Device-pixel rect [x, y, w, h] (with press scale/stretch/follow/hover applied).
    rect: vec4<f32>,
    // Corner radii [tl, tr, br, bl] in device px (negative = continuous corners).
    radii: vec4<f32>,
    // [brightness, contrast, saturation, sigma_px].
    tone: vec4<f32>,
    // Tint colour in sRGB-encoded space [r, g, b, a].
    tint: vec4<f32>,
    // Rim alpha by edge [top, side, bottom, width_px].
    rim: vec4<f32>,
    // Rim colour in sRGB-encoded space [r, g, b] + specular intensity [w].
    rim_c: vec4<f32>,
    // Inner shadow [offset_y_px, blur_px, spread_px] + sheen alpha [w].
    inner: vec4<f32>,
    // Inner shadow colour in sRGB-encoded space [r, g, b, a].
    inner_c: vec4<f32>,
    // Optics [bevel_px, max_refract_px, dispersion, sheen_stop].
    bend: vec4<f32>,
    // [light_angle_rad, mode_flags, union_group_id, shape_opacity].
    light: vec4<f32>,
    // Touch glow [center_x_px, center_y_px, radius_px, strength].
    glow: vec4<f32>,
    // [shadow_blur_px, shadow_y_px, hover_amount, materialize].
    extra: vec4<f32>,
};

struct Epoch {
    // [origin_x, origin_y, inv_extent_x, inv_extent_y] -> device px to pyramid UV.
    pyramid_map: vec4<f32>,
    // [inv_vp_w, inv_vp_h, pyramid_min_u, pyramid_min_v].
    backdrop_min: vec4<f32>,
    // [pyramid_max_u, pyramid_max_v, backdrop_min_u, backdrop_min_v].
    clamp_a: vec4<f32>,
    // [backdrop_max_u, backdrop_max_v, vp_w, vp_h].
    clamp_b: vec4<f32>,
    // [pointer_x_px, pointer_y_px, pointer_mix, _].
    pointer: vec4<f32>,
    // [still_mode, target_is_srgb, saturation_scale, mip_count].
    flags: vec4<f32>,
};

@group(1) @binding(0) var frost: texture_2d<f32>;
@group(1) @binding(1) var backdrop: texture_2d<f32>;
@group(1) @binding(2) var smp: sampler;
@group(1) @binding(3) var<storage, read> shapes: array<GlassShape>;
@group(1) @binding(4) var<uniform> epoch: Epoch;

struct In {
    @builtin(vertex_index) vi: u32,
    @location(0) rect: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) border: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) uv: vec4<f32>,
    @location(6) clip: vec4<f32>,
    @location(7) fade: vec4<u32>,
};

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) rect: vec4<f32>,
    @location(1) @interpolate(flat) still: vec4<f32>,
    @location(2) @interpolate(flat) alpha: f32,
    @location(3) @interpolate(flat) params: vec4<f32>,
    @location(4) @interpolate(flat) uv: vec4<f32>,
    @location(5) @interpolate(flat) clip: vec4<f32>,
    @location(6) @interpolate(flat) fade: vec4<u32>,
};

@vertex
fn vs(i: In) -> Out {
    let corner = vec2<f32>(f32(i.vi & 1u), f32(i.vi >> 1u));
    let p = i.rect.xy - vec2<f32>(1.0) + corner * (i.rect.zw + vec2<f32>(2.0));
    let size_px = select(g.viewport, projection.zw, projection.z > 0.0);
    let local = p - projection.xy;
    var o: Out;
    o.pos = vec4<f32>(local.x / size_px.x * 2.0 - 1.0, 1.0 - local.y / size_px.y * 2.0, 0.0, 1.0);
    o.rect = i.rect;
    o.still = i.border;
    o.alpha = i.fill.a;
    o.params = i.params;
    o.uv = i.uv;
    o.clip = i.clip;
    o.fade = i.fade;
    return o;
}

// Measured line-spread sigmas (device px) of 13-tap Jimenez down to level k + cubic B-spline up.
const SIGMA_0: f32 = 1.76;
const SIGMA_1: f32 = 3.75;
const SIGMA_2: f32 = 7.62;
const SIGMA_3: f32 = 15.30;
const SIGMA_4: f32 = 30.63;
const SIGMA_5: f32 = 61.27;
const SIGMA_6: f32 = 122.55;

fn srgb_encode_ch(x: f32) -> f32 {
    let c = max(x, 0.0);
    return select(1.055 * pow(c, 1.0 / 2.4) - 0.055, c * 12.92, c <= 0.0031308);
}

fn srgb_encode3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_encode_ch(c.r), srgb_encode_ch(c.g), srgb_encode_ch(c.b));
}

fn srgb_decode_ch(x: f32) -> f32 {
    let c = clamp(x, 0.0, 1.0);
    return select(pow((c + 0.055) / 1.055, 2.4), c / 12.92, c <= 0.04045);
}

fn srgb_decode3(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(srgb_decode_ch(c.r), srgb_decode_ch(c.g), srgb_decode_ch(c.b));
}

fn clamp_pyramid_uv(uv: vec2<f32>, lod: f32) -> vec2<f32> {
    let dim = vec2<f32>(textureDimensions(frost, i32(lod)));
    let pad = 0.5 / max(dim, vec2<f32>(1.0));
    let lo = epoch.backdrop_min.zw + pad;
    let hi = max(epoch.clamp_a.xy - pad, lo);
    return clamp(uv, lo, hi);
}

// 4-tap cubic B-spline reconstruction of one pyramid level so large frost stays smooth and
// diamond-free at every sigma.
fn bspline(uv: vec2<f32>, lod: f32) -> vec3<f32> {
    let dim = vec2<f32>(textureDimensions(frost, i32(lod)));
    let st = uv * dim - vec2<f32>(0.5);
    let i = floor(st);
    let f = st - i;
    let omf = vec2<f32>(1.0) - f;
    let w0 = omf * omf * omf * (1.0 / 6.0);
    let w1 = (vec2<f32>(4.0) - 6.0 * f * f + 3.0 * f * f * f) * (1.0 / 6.0);
    let w2 = (vec2<f32>(1.0) + 3.0 * f + 3.0 * f * f - 3.0 * f * f * f) * (1.0 / 6.0);
    let w3 = f * f * f * (1.0 / 6.0);
    let g0 = w0 + w1;
    let g1 = w2 + w3;
    let h0 = clamp_pyramid_uv((i - vec2<f32>(0.5) + w1 / g0) / dim, lod);
    let h1 = clamp_pyramid_uv((i + vec2<f32>(1.5) + w3 / g1) / dim, lod);
    let a = textureSampleLevel(frost, smp, vec2<f32>(h0.x, h0.y), lod).rgb;
    let b = textureSampleLevel(frost, smp, vec2<f32>(h1.x, h0.y), lod).rgb;
    let c = textureSampleLevel(frost, smp, vec2<f32>(h0.x, h1.y), lod).rgb;
    let d = textureSampleLevel(frost, smp, vec2<f32>(h1.x, h1.y), lod).rgb;
    return g0.y * (g0.x * a + g1.x * b) + g1.y * (g0.x * c + g1.x * d);
}

fn sigma_to_lod(sigma: f32) -> vec2<f32> {
    let s2 = sigma * sigma;
    if (sigma <= SIGMA_1) {
        let t = clamp((s2 - SIGMA_0 * SIGMA_0) / (SIGMA_1 * SIGMA_1 - SIGMA_0 * SIGMA_0), 0.0, 1.0);
        return vec2<f32>(0.0, t);
    } else if (sigma <= SIGMA_2) {
        let t = clamp((s2 - SIGMA_1 * SIGMA_1) / (SIGMA_2 * SIGMA_2 - SIGMA_1 * SIGMA_1), 0.0, 1.0);
        return vec2<f32>(1.0, t);
    } else if (sigma <= SIGMA_3) {
        let t = clamp((s2 - SIGMA_2 * SIGMA_2) / (SIGMA_3 * SIGMA_3 - SIGMA_2 * SIGMA_2), 0.0, 1.0);
        return vec2<f32>(2.0, t);
    } else if (sigma <= SIGMA_4) {
        let t = clamp((s2 - SIGMA_3 * SIGMA_3) / (SIGMA_4 * SIGMA_4 - SIGMA_3 * SIGMA_3), 0.0, 1.0);
        return vec2<f32>(3.0, t);
    } else if (sigma <= SIGMA_5) {
        let t = clamp((s2 - SIGMA_4 * SIGMA_4) / (SIGMA_5 * SIGMA_5 - SIGMA_4 * SIGMA_4), 0.0, 1.0);
        return vec2<f32>(4.0, t);
    } else {
        let t = clamp((s2 - SIGMA_5 * SIGMA_5) / (SIGMA_6 * SIGMA_6 - SIGMA_5 * SIGMA_5), 0.0, 1.0);
        return vec2<f32>(5.0, t);
    }
}

fn sharp_at(p: vec2<f32>) -> vec3<f32> {
    let uv = clamp(p * epoch.backdrop_min.xy, epoch.clamp_a.zw, epoch.clamp_b.xy);
    var c = textureSampleLevel(backdrop, smp, uv, 0.0).rgb;
    if (epoch.flags.y > 0.5) {
        c = srgb_encode3(c);
    }
    return c;
}

fn frost_at(p: vec2<f32>, sigma: f32) -> vec3<f32> {
    let puv = (p - epoch.pyramid_map.xy) * epoch.pyramid_map.zw;
    if (sigma <= 0.05) {
        return sharp_at(p);
    }
    if (sigma < SIGMA_0) {
        let f0 = bspline(puv, 0.0);
        let t = clamp((sigma * sigma) / (SIGMA_0 * SIGMA_0), 0.0, 1.0);
        return mix(sharp_at(p), f0, t);
    }
    let lf = sigma_to_lod(sigma);
    let lo = lf.x;
    let hi = min(lo + 1.0, max(epoch.flags.w - 1.0, 0.0));
    let a = bspline(puv, lo);
    if (lf.y <= 0.001 || hi == lo) {
        return a;
    }
    return mix(a, bspline(puv, hi), lf.y);
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    if (k <= 0.001) {
        return min(a, b);
    }
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn sd_one(p: vec2<f32>, s: GlassShape) -> f32 {
    let half = s.rect.zw * 0.5;
    return sd_rounded(p - (s.rect.xy + half), half, s.radii);
}

// Evaluates the container SDF over [first .. first + count]. `k_eff = 2 * spacing_px` so two
// shapes begin merging at their midpoint exactly when their nearest edges are `spacing_px` apart.
struct SdfHit {
    d: f32,
    nearest: u32,
};

fn eval_sdf(p: vec2<f32>, first: u32, count: u32, k_eff: f32) -> SdfHit {
    var d_first = sd_one(p, shapes[first]);
    var nearest = first;
    var best_d = d_first;
    var cur_group_id = shapes[first].light.z;
    var group_d = d_first;
    var total_d = 1e9;
    var has_total = false;
    for (var idx = 1u; idx < count; idx++) {
        let si = first + idx;
        let di = sd_one(p, shapes[si]);
        if (di < best_d) {
            best_d = di;
            nearest = si;
        }
        let uid = shapes[si].light.z;
        // Shapes sharing a non-zero `glass_union` id are sorted contiguously by the painter and
        // merge seamlessly even at `spacing == 0`; distinct union groups only blend by `k_eff`.
        if (uid > 0.5 && uid == cur_group_id) {
            group_d = smin(group_d, di, max(k_eff, 20.0));
        } else {
            total_d = select(group_d, smin(total_d, group_d, k_eff), has_total);
            has_total = true;
            cur_group_id = uid;
            group_d = di;
        }
    }
    let final_d = select(group_d, smin(total_d, group_d, k_eff), has_total);
    return SdfHit(final_d, nearest);
}

fn eval_sdf_only(p: vec2<f32>, first: u32, count: u32, k_eff: f32) -> f32 {
    return eval_sdf(p, first, count, k_eff).d;
}

// Bevel dome height profile adapted from bilalgursen/liquid-glass (MIT).
fn bevel_height(d: f32, bevel: f32) -> f32 {
    let t = pow(clamp(-d / max(bevel, 1.0), 0.0, 1.0), 1.0 / 1.4);
    return sqrt(max(1.0 - (1.0 - t) * (1.0 - t), 0.0));
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let p = in.pos.xy + projection.xy;
    let clip_cov = clip_alpha_stops(p, in.clip, in.fade, g.flags.zw) * in.alpha;
    if (clip_cov <= 0.0001) {
        discard;
    }

    let first = u32(max(in.params.z, 0.0) + 0.5);
    let count = clamp(u32(max(in.uv.x, 1.0) + 0.5), 1u, 32u);
    let k_eff = max(in.uv.y, 0.0) * 2.0;

    let hit = eval_sdf(p, first, count, k_eff);
    let d = hit.d;
    let s = shapes[hit.nearest];
    let materialize = clamp(s.extra.w, 0.0, 1.0);
    let shape_alpha = clamp(s.light.w, 0.0, 1.0) * materialize;

    // Smooth sub-pixel coverage (1 px AA transition).
    let mask = clamp(0.5 - d, 0.0, 1.0) * shape_alpha;

    // Coarse backdrop luminance & detail under the shape (same-frame adaptivity, zero readback).
    let puv = (p - epoch.pyramid_map.xy) * epoch.pyramid_map.zw;
    var bg_l = 0.5;
    var bg_detail = 0.0;
    if (epoch.flags.x < 0.5) {
        let c5 = textureSampleLevel(frost, smp, clamp_pyramid_uv(puv, 5.0), 5.0).rgb;
        let c3 = textureSampleLevel(frost, smp, clamp_pyramid_uv(puv, 3.0), 3.0).rgb;
        bg_l = dot(c5, vec3<f32>(0.2126, 0.7152, 0.0722));
        bg_detail = clamp(length(c3 - c5) * 3.0, 0.0, 1.0);
    }

    // Adaptive drop shadow outside the glass (Apple variants; web presets set shadow_blur = 0).
    var shadow_a = 0.0;
    let sh_blur = s.extra.x;
    if (sh_blur > 0.1 && d > -1.0) {
        let d_sh = eval_sdf_only(p - vec2<f32>(0.0, s.extra.y), first, count, k_eff);
        let soft = 1.0 - smoothstep(-sh_blur * 0.45, sh_blur * 1.5, d_sh);
        let outside = smoothstep(-0.5, 1.5, d);
        let sh_op = mix(0.20, 0.44, bg_detail);
        shadow_a = clamp(soft * outside * sh_op * shape_alpha, 0.0, 0.85);
    }

    if (mask <= 0.0005) {
        if (shadow_a <= 0.0005) {
            discard;
        }
        return vec4<f32>(0.0, 0.0, 0.0, shadow_a * clip_cov);
    }

    let flags = u32(max(s.light.y, 0.0) + 0.5);
    let variant = flags & 3u; // 0 = Regular, 1 = Clear, 2 = Custom/Web
    let is_large = (flags & 4u) != 0u;
    let cursor_light = (flags & 8u) != 0u;
    let reduced_trans = (flags & 16u) != 0u;
    let inc_contrast = (flags & 32u) != 0u;
    let has_tint = (flags & 64u) != 0u;
    let blur_only = (flags & 128u) != 0u;

    let center = s.rect.xy + s.rect.zw * 0.5;
    let half = s.rect.zw * 0.5;
    let bevel = max(s.bend.x, 1.0);
    let inside = max(-d, 0.0);

    // Bevel height-field normal & SDF outward direction (computed only near bevel/rim and skipped
    // entirely in low-end `BlurOnly` mode so iGPUs avoid the 4 finite-difference SDF taps).
    var dir = vec2<f32>(0.0, -1.0);
    var normal = vec3<f32>(0.0, 0.0, 1.0);
    let min_half = min(half.x, half.y);
    let lens_depth = clamp(bevel * 1.45, 4.0, max(min_half * 0.85, 4.0));
    let near_edge = !blur_only && inside < max(lens_depth, bevel * 1.5) + 4.0;
    if (near_edge) {
        let e = 0.75;
        let dx_p = eval_sdf_only(p + vec2<f32>(e, 0.0), first, count, k_eff);
        let dx_m = eval_sdf_only(p - vec2<f32>(e, 0.0), first, count, k_eff);
        let dy_p = eval_sdf_only(p + vec2<f32>(0.0, e), first, count, k_eff);
        let dy_m = eval_sdf_only(p - vec2<f32>(0.0, e), first, count, k_eff);
        let grad_d = vec2<f32>(dx_p - dx_m, dy_p - dy_m) / (2.0 * e);
        let gl = length(grad_d);
        if (gl > 1e-5) {
            dir = grad_d / gl;
        }
        let hx = bevel_height(dx_p, bevel) - bevel_height(dx_m, bevel);
        let hy = bevel_height(dy_p, bevel) - bevel_height(dy_m, bevel);
        let grad_h = vec2<f32>(hx, hy) / (2.0 * e);
        normal = normalize(vec3<f32>(-grad_h * bevel * 0.72, 1.0));
    }

    // Smooth convex-bevel inward refraction: C2-smooth quadratic/quartic lensing across the
    // bevel shoulder, bounded cleanly to the shape's half-extent so thin pills never jump across
    // their opposite edge or tear neon chromatic artifacts at the outer rim.
    var bend = vec2<f32>(0.0);
    if (!blur_only && !reduced_trans && s.bend.y > 0.001 && near_edge) {
        let u = clamp(1.0 - inside / max(lens_depth, 1.0), 0.0, 1.0);
        let u2 = u * u;
        let curve = u2 * (0.40 + 0.60 * u2);
        let max_disp = min(s.bend.y, max(min_half * 0.62, 2.0));
        let edge_fade = smoothstep(1.2, -0.5, d);
        bend = -dir * max_disp * curve * edge_fade * materialize;
    }

    // Frost reconstruction + optional per-channel chromatic dispersion in the bevel band.
    let sigma = s.tone.w * materialize;
    var c: vec3<f32>;
    if (epoch.flags.x > 0.5) {
        c = in.still.rgb;
    } else if (!blur_only && !reduced_trans && s.bend.z > 0.0005 && dot(bend, bend) > 0.0025) {
        let disp = s.bend.z;
        let cr = frost_at(p + bend * (1.0 - disp), sigma).r;
        let cg = frost_at(p + bend, sigma).g;
        let cb = frost_at(p + bend * (1.0 + disp), sigma).b;
        c = vec3<f32>(cr, cg, cb);
    } else {
        c = frost_at(p + bend, sigma);
    }

    // Backdrop saturation (per-shape * pass uniform) + variant tone & tint.
    let sat = max(s.tone.z * epoch.flags.z, 0.0);
    if (epoch.flags.x < 0.5) {
        if (variant == 2u) {
            // Custom / Chromium CSS backdrop-filter chain in sRGB-encoded space:
            // brightness -> contrast -> saturate, then premultiplied tint overlay.
            c = c * s.tone.x;
            c = (c - vec3<f32>(0.5)) * s.tone.y + vec3<f32>(0.5);
            let l_web = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
            c = clamp(mix(vec3<f32>(l_web), c, sat), vec3<f32>(0.0), vec3<f32>(1.0));
            c = c * (1.0 - s.tint.a) + s.tint.rgb;
            if (reduced_trans) {
                c = mix(c, in.still.rgb, 0.88);
            }
            if (inc_contrast) {
                c = mix(c, vec3<f32>(0.0), 0.35);
            }
        } else {
            let l_sat = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
            c = clamp(mix(vec3<f32>(l_sat), c, sat), vec3<f32>(0.0), vec3<f32>(1.0));
            if (variant == 0u) {
                // Apple Regular: adaptive tone (lighter over light, darker over dark; large
                // elements > 200 pt stay on the light side).
                let bright = select(smoothstep(0.30, 0.70, bg_l), 1.0, is_large);
                var light_mix = 0.20;
                var dark_mix = 0.24;
                if (inc_contrast) {
                    // Darker tint + high contrast separation under Increase Contrast.
                    light_mix = 0.78;
                    dark_mix = 0.92;
                }
                let dark_col = mix(c, vec3<f32>(0.0), dark_mix);
                let light_col = mix(c, vec3<f32>(1.0), light_mix);
                c = mix(dark_col, light_col, bright);
                if (reduced_trans) {
                    // Solid frosted fill (>= 88% opaque) anchored to the shape's overall backdrop
                    // luminance so sharp background splits flatten cleanly under Reduce Transparency.
                    let cpuv = (center - epoch.pyramid_map.xy) * epoch.pyramid_map.zw;
                    let c5_center = textureSampleLevel(frost, smp, clamp_pyramid_uv(cpuv, 5.0), 5.0).rgb;
                    let center_l = select(dot(c5_center, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.5, epoch.flags.x >= 0.5);
                    let solid_frost = mix(vec3<f32>(0.16), vec3<f32>(0.90), select(smoothstep(0.35, 0.65, center_l), 1.0, is_large));
                    c = mix(c, solid_frost, 0.88);
                }
                if (inc_contrast) {
                    c = mix(c, vec3<f32>(0.0), select(0.28, 0.14, bright > 0.5));
                }
            } else {
                // Apple Clear: clean legibility scrim over bright backdrops without muddying chroma.
                let dim_a = 0.28 * smoothstep(0.38, 0.72, bg_l);
                c = mix(c, vec3<f32>(0.0), dim_a);
                if (reduced_trans) {
                    let cpuv = (center - epoch.pyramid_map.xy) * epoch.pyramid_map.zw;
                    let c5_center = textureSampleLevel(frost, smp, clamp_pyramid_uv(cpuv, 5.0), 5.0).rgb;
                    let center_l = select(dot(c5_center, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.5, epoch.flags.x >= 0.5);
                    let solid_frost = mix(vec3<f32>(0.14), vec3<f32>(0.88), smoothstep(0.35, 0.65, center_l));
                    c = mix(c, solid_frost, 0.88);
                }
                if (inc_contrast) {
                    c = mix(c, vec3<f32>(0.0), 0.55);
                }
            }
            if (has_tint) {
                let mapped_tint = mix(s.tint.rgb * 0.75, s.tint.rgb, bg_l);
                let tint_a = select(s.tint.a, min(s.tint.a + 0.20, 0.92), inc_contrast);
                c = mix(c, mapped_tint, tint_a);
            }
        }
    }

    // Optional top sheen wash.
    if (s.inner.w > 0.0005 && s.bend.w > 0.0005) {
        let yn = clamp((p.y - s.rect.y) / max(s.rect.w, 1.0), 0.0, 1.0);
        c = c + (1.0 - smoothstep(0.0, s.bend.w, yn)) * s.inner.w * (vec3<f32>(1.0) - c);
    }

    // Optional inset glow / inner shadow by SDF.
    if (s.inner_c.a > 0.0005) {
        let inset_half = max(half - vec2<f32>(s.inner.z), vec2<f32>(0.0));
        let di = sd_rounded(p - vec2<f32>(0.0, s.inner.x) - center, inset_half, s.radii);
        let blur_half = 0.5 * max(s.inner.y, 1.0);
        let ia = smoothstep(-blur_half, blur_half, di) * clamp(0.5 - d, 0.0, 1.0);
        c = c * (1.0 - s.inner_c.a * ia) + s.inner_c.rgb * ia;
    }

    // Rim light + smooth curved bevel meniscus/Fresnel + dual-lobe specular highlight.
    // In low-end `BlurOnly` mode, emit a cheap constant hairline rim without Fresnel/specular ALU.
    let rim_w = max(1.0, s.rim.w);
    let ring = clamp(0.5 - d, 0.0, 1.0) * (1.0 - smoothstep(0.45, 0.45 + rim_w, -d));
    if (blur_only) {
        let simple_lit = clamp(s.rim.x * ring * materialize, 0.0, 1.0);
        c = clamp(c * (1.0 - simple_lit * 0.20) + s.rim_c.rgb * simple_lit, vec3<f32>(0.0), vec3<f32>(1.0));
    } else {
        // Silky inner curved bevel meniscus + physical Fresnel wrap (fades smoothly to 0 in flat interior).
        let edge_t = clamp(1.0 - inside / max(bevel * 1.15, 3.0), 0.0, 1.0);
        let meniscus = edge_t * edge_t * edge_t;
        let fresnel_phys = pow(clamp(1.0 - normal.z, 0.0, 1.0), 2.4);
        let bevel_glow = max(fresnel_phys * 0.85, meniscus * 0.55);
        let studio_dir = vec2<f32>(cos(s.light.x), sin(s.light.x));
        let studio_cos = dot(dir, studio_dir);
        let s_facing = clamp(studio_cos, 0.0, 1.0);
        let s_away = clamp(-studio_cos, 0.0, 1.0);
        // Continuous glass edge wrap: subtle ambient perimeter + bright studio arc + opposing internal caustic.
        let studio_lobes = 0.24 + 0.76 * (s_facing * s_facing) + 0.42 * (s_away * s_away * s_away);
        let studio_fresnel = 0.16 + 0.68 * (s_facing * s_facing) + 0.34 * (s_away * s_away);
        var lobes = studio_lobes;
        var fresnel_w = studio_fresnel;
        if (cursor_light && epoch.pointer.z > 0.001) {
            let to_cursor = epoch.pointer.xy - center;
            let cl = length(to_cursor);
            if (cl > 1.0) {
                let cursor_dir = to_cursor / cl;
                let cursor_cos = dot(dir, cursor_dir);
                let c_facing = clamp(cursor_cos, 0.0, 1.0);
                let c_away = clamp(-cursor_cos, 0.0, 1.0);
                let aimed_lobes = 0.14 + 0.86 * (c_facing * c_facing) + 0.22 * (c_away * c_away * c_away);
                let aimed_fresnel = 0.12 + 0.76 * (c_facing * c_facing) + 0.14 * (c_away * c_away);
                let lean = clamp(epoch.pointer.z * 0.65, 0.0, 0.65);
                lobes = mix(studio_lobes, aimed_lobes, lean);
                fresnel_w = mix(studio_fresnel, aimed_fresnel, lean);
            }
        }
        let edge_a = select(mix(s.rim.y, s.rim.z, max(dir.y, 0.0)), mix(s.rim.y, s.rim.x, max(-dir.y, 0.0)), dir.y < 0.0);
        let spec_strength = s.rim_c.a;
        let rim_hit = edge_a * lobes * ring * (0.50 + 0.50 * spec_strength);
        let fresnel_hit = bevel_glow * fresnel_w * spec_strength * 0.18 * (0.40 + 0.60 * edge_a);
        let lit = clamp((rim_hit + fresnel_hit) * materialize, 0.0, 1.0);
        c = clamp(c + s.rim_c.rgb * lit * (vec3<f32>(1.0) - c * 0.38), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    // Interactive hover brighten (+0.05) and radial touch glow from pointer contact.
    if (s.extra.z > 0.001) {
        c = clamp(c + vec3<f32>(s.extra.z * 0.05), vec3<f32>(0.0), vec3<f32>(1.0));
    }
    if (s.glow.w > 0.001) {
        let gd = length(p - s.glow.xy) / max(s.glow.z, 1.0);
        let fall = exp(-gd * gd * 2.0);
        let boost = s.glow.w * mix(0.10, 0.22, fall);
        c = clamp(c + vec3<f32>(boost), vec3<f32>(0.0), vec3<f32>(1.0));
    }

    // Increase Contrast: solid high-contrast rim in the opposite tone at 0.95 alpha.
    if (inc_contrast) {
        let border_w = max(1.5, 1.5 * in.params.w);
        let b_ring = clamp(0.5 - d, 0.0, 1.0) - clamp(0.5 - (d + border_w), 0.0, 1.0);
        let b_col = select(vec3<f32>(1.0), vec3<f32>(0.0), bg_l > 0.55);
        c = mix(c, b_col, b_ring * 0.95);
    }

    // Convert back to linear if rendering into an *Srgb target (`target_is_srgb > 0.5`).
    var out_rgb = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    if (epoch.flags.y > 0.5) {
        out_rgb = srgb_decode3(out_rgb);
    }

    // Premultiplied glass over premultiplied adaptive drop shadow.
    let glass_a = mask;
    let final_a = glass_a + shadow_a * (1.0 - glass_a);
    let final_rgb = out_rgb * glass_a;
    return vec4<f32>(final_rgb, final_a) * clip_cov;
}
