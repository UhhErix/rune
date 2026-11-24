// Clean-room original WGSL procedural UI shaders for `rune-ui::effects`.
// Zero code ported from any third-party shader or React Bits repository.
//
// Effect kinds (`u.params.x` as integer):
//   0 = Aurora background
//   1 = Particles background
//   2 = Waves background
//   3 = Grid background (animated dots/lines + radial spotlight pulse)
//   4 = BorderBeam (luminous sweeping beam along rounded-rect SDF perimeter)
//   5 = GlowBorder (rotating / pointer-reactive radial glow along rounded-rect SDF)
//   6 = Iridescence background (clean-room thin-film interference backdrop)
//   7 = SpotlightRim (pointer-tracking radial spotlight border & subtle surface lift)

struct Uniforms {
    rect: vec4<f32>,      // x0, y0, width, height in device pixels
    viewport: vec4<f32>,  // target_w, target_h, scale, srgb_encode (1.0 or 0.0)
    params: vec4<f32>,    // kind, time, speed, intensity
    extra: vec4<f32>,     // radius_px, pointer_u, pointer_v, seed
    color_a: vec4<f32>,   // linear RGB + alpha
    color_b: vec4<f32>,   // linear RGB + alpha
    color_c: vec4<f32>,   // linear RGB + alpha
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let uv = corners[vid];
    let px = u.rect.xy + uv * u.rect.zw;
    let ndc_x = (px.x / max(u.viewport.x, 1.0)) * 2.0 - 1.0;
    let ndc_y = 1.0 - (px.y / max(u.viewport.y, 1.0)) * 2.0;
    var out: VsOut;
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        hash21(p),
        hash21(p + vec2<f32>(19.37, 73.11))
    );
}

fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

fn linear_to_srgb_ch(c: f32) -> f32 {
    let x = clamp(c, 0.0, 1.0);
    if (x <= 0.0031308) {
        return x * 12.92;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

fn encode_out(rgb: vec3<f32>, alpha: f32) -> vec4<f32> {
    let a = clamp(alpha, 0.0, 1.0);
    var col = max(rgb, vec3<f32>(0.0));
    if (u.viewport.w > 0.5) {
        col = vec3<f32>(
            linear_to_srgb_ch(col.r),
            linear_to_srgb_ch(col.g),
            linear_to_srgb_ch(col.b)
        );
    }
    return vec4<f32>(col * a, a);
}

fn shade_aurora(uv: vec2<f32>, t: f32, intensity: f32) -> vec4<f32> {
    let p = vec2<f32>(uv.x * 2.2, uv.y * 1.4);
    let w1 = sin(p.x * 2.1 + t * 0.7) * 0.18 + cos(p.x * 3.7 - t * 0.45) * 0.09;
    let w2 = cos(p.x * 1.6 - t * 0.55 + 1.4) * 0.22 + sin(p.x * 4.3 + t * 0.35) * 0.07;
    let band1 = exp(-pow((uv.y - 0.38 - w1) * 4.8, 2.0));
    let band2 = exp(-pow((uv.y - 0.56 - w2) * 4.0, 2.0));
    let shimmer = 0.75 + 0.25 * sin(uv.x * 9.0 + t * 1.3 + uv.y * 5.0);
    let rgb = (u.color_a.rgb * band1 + u.color_b.rgb * band2 + u.color_c.rgb * band1 * band2) * shimmer;
    let vignette = smoothstep(0.0, 0.18, uv.x) * smoothstep(1.0, 0.82, uv.x);
    let alpha = clamp((band1 * 0.7 + band2 * 0.65) * vignette * intensity, 0.0, 1.0);
    return vec4<f32>(rgb, alpha);
}

fn shade_particles(uv: vec2<f32>, aspect: f32, t: f32, intensity: f32) -> vec4<f32> {
    var accum = vec3<f32>(0.0);
    var alpha = 0.0;
    let ptr = vec2<f32>(u.extra.y, u.extra.z);
    for (var layer: i32 = 0; layer < 3; layer = layer + 1) {
        let lf = f32(layer);
        let density = 7.0 + lf * 5.0;
        let drift = vec2<f32>(t * (0.08 + lf * 0.04), -t * (0.12 + lf * 0.05));
        let parallax = (ptr - vec2<f32>(0.5, 0.5)) * (0.06 * (lf + 1.0));
        let g = (vec2<f32>(uv.x * aspect, uv.y) + drift + parallax) * density;
        let cell = floor(g);
        let f = fract(g);
        let rnd = hash22(cell + vec2<f32>(lf * 31.7, u.extra.w));
        let center = 0.2 + 0.6 * rnd + 0.08 * vec2<f32>(sin(t + rnd.x * 6.28), cos(t * 0.9 + rnd.y * 6.28));
        let d = length(f - center);
        let size = 0.05 + 0.07 * hash21(cell + vec2<f32>(4.1, lf));
        let twinkle = 0.55 + 0.45 * sin(t * 2.2 + rnd.x * 12.56);
        let glow = smoothstep(size, 0.0, d) * twinkle;
        let tint = mix(u.color_a.rgb, u.color_b.rgb, rnd.y);
        accum = accum + tint * glow * (0.9 - lf * 0.2);
        alpha = alpha + glow * (0.75 - lf * 0.15);
    }
    return vec4<f32>(accum, clamp(alpha * intensity, 0.0, 1.0));
}

fn shade_waves(uv: vec2<f32>, t: f32, intensity: f32) -> vec4<f32> {
    var rgb = vec3<f32>(0.0);
    var alpha = 0.0;
    for (var i: i32 = 0; i < 4; i = i + 1) {
        let fi = f32(i);
        let freq = 2.6 + fi * 1.15;
        let phase = t * (0.8 + fi * 0.22) + fi * 1.7;
        let y_wave = 0.30 + fi * 0.15 + 0.07 * sin(uv.x * freq * 3.14159 + phase)
            + 0.03 * cos(uv.x * freq * 5.2 - phase * 0.7);
        let dist = abs(uv.y - y_wave);
        let ribbon = smoothstep(0.055, 0.0, dist);
        let fill_below = smoothstep(y_wave - 0.01, y_wave + 0.18, uv.y) * 0.14;
        let col = mix(u.color_a.rgb, u.color_b.rgb, fi / 3.0);
        rgb = rgb + col * (ribbon * 0.65 + fill_below);
        alpha = max(alpha, (ribbon * 0.7 + fill_below) * intensity);
    }
    return vec4<f32>(rgb, clamp(alpha, 0.0, 1.0));
}

fn shade_grid(uv: vec2<f32>, rect_size: vec2<f32>, t: f32, intensity: f32) -> vec4<f32> {
    let cell_px = 28.0 * max(u.viewport.z, 1.0);
    let pos = uv * rect_size;
    let frac = fract(pos / cell_px);
    let line_x = smoothstep(0.05, 0.0, min(frac.x, 1.0 - frac.x));
    let line_y = smoothstep(0.05, 0.0, min(frac.y, 1.0 - frac.y));
    let grid_line = max(line_x, line_y);
    let dot_d = length(frac - vec2<f32>(0.5, 0.5));
    let dot_m = smoothstep(0.11, 0.04, dot_d);

    let center = vec2<f32>(u.extra.y, u.extra.z);
    let dist_c = length(uv - center);
    let pulse = exp(-pow((dist_c - fract(t * 0.22) * 1.2) * 8.0, 2.0));
    let radial = smoothstep(0.95, 0.12, dist_c);
    let combined = (grid_line * 0.45 + dot_m * 0.55) * radial + grid_line * pulse * 0.7;
    let col = mix(u.color_a.rgb, u.color_b.rgb, clamp(pulse, 0.0, 1.0));
    return vec4<f32>(col, clamp(combined * intensity, 0.0, 1.0));
}

fn shade_border_beam(uv: vec2<f32>, rect_size: vec2<f32>, t: f32, intensity: f32) -> vec4<f32> {
    let half_sz = rect_size * 0.5;
    let p = (uv - vec2<f32>(0.5, 0.5)) * rect_size;
    let r = min(u.extra.x, min(half_sz.x, half_sz.y));
    let d = sd_rounded_box(p, half_sz - vec2<f32>(1.5, 1.5), max(r - 1.5, 0.0));
    let stroke_px = 2.0 * max(u.viewport.z, 1.0);
    let edge_mask = smoothstep(stroke_px, 0.0, abs(d));

    let ang = atan2(p.y, p.x); // -pi..pi
    let beam_ang = (t * 1.4) % 6.2831853 - 3.1415926;
    var diff = abs(ang - beam_ang);
    diff = min(diff, 6.2831853 - diff);
    let beam = exp(-pow(diff * 2.2, 2.0));
    let col = mix(u.color_a.rgb, u.color_b.rgb, smoothstep(0.0, 1.2, diff));
    let alpha = edge_mask * (0.14 + 0.86 * beam) * intensity;
    return vec4<f32>(col, clamp(alpha, 0.0, 1.0));
}

fn shade_glow_border(uv: vec2<f32>, rect_size: vec2<f32>, t: f32, intensity: f32) -> vec4<f32> {
    let half_sz = rect_size * 0.5;
    let p = (uv - vec2<f32>(0.5, 0.5)) * rect_size;
    let r = min(u.extra.x, min(half_sz.x, half_sz.y));
    let d = sd_rounded_box(p, half_sz - vec2<f32>(2.0, 2.0), max(r - 2.0, 0.0));
    let glow_w = 10.0 * max(u.viewport.z, 1.0);
    let ring = exp(-pow(abs(d) / max(glow_w * 0.32, 1.0), 2.0));
    let ptr = (vec2<f32>(u.extra.y, u.extra.z) - vec2<f32>(0.5, 0.5)) * rect_size;
    let orbit = vec2<f32>(cos(t * 0.9), sin(t * 0.9)) * half_sz * 0.65;
    let focus = mix(orbit, ptr, 0.5);
    let hot = exp(-length(p - focus) / max(min(rect_size.x, rect_size.y) * 0.65, 1.0));
    let col = mix(u.color_a.rgb, u.color_b.rgb, hot);
    return vec4<f32>(col, clamp(ring * (0.3 + 0.7 * hot) * intensity, 0.0, 1.0));
}

fn shade_iridescence(uv: vec2<f32>, t: f32, intensity: f32) -> vec4<f32> {
    let p = (uv - vec2<f32>(0.5, 0.5)) * 2.4;
    let ptr = (vec2<f32>(u.extra.y, u.extra.z) - vec2<f32>(0.5, 0.5)) * 0.6;
    let q = p + ptr;
    let d = length(q);
    let wave = sin(d * 4.5 - t * 0.8 + atan2(q.y, q.x) * 2.0);
    let phase = d * 2.2 + wave * 0.45 + t * 0.35;
    let thin_film = vec3<f32>(
        0.5 + 0.5 * cos(phase + 0.0),
        0.5 + 0.5 * cos(phase + 2.094),
        0.5 + 0.5 * cos(phase + 4.188)
    );
    let tinted = mix(u.color_a.rgb, thin_film * 0.85 + u.color_b.rgb * 0.35, 0.72);
    let alpha = clamp((0.35 + 0.35 * (0.5 + 0.5 * wave)) * intensity, 0.0, 1.0);
    return vec4<f32>(tinted, alpha);
}

fn shade_spotlight_rim(uv: vec2<f32>, rect_size: vec2<f32>, intensity: f32) -> vec4<f32> {
    let half_sz = rect_size * 0.5;
    let p = (uv - vec2<f32>(0.5, 0.5)) * rect_size;
    let r = min(u.extra.x, min(half_sz.x, half_sz.y));
    let d = sd_rounded_box(p, half_sz - vec2<f32>(1.0, 1.0), max(r - 1.0, 0.0));
    let rim = smoothstep(2.0 * max(u.viewport.z, 1.0), 0.0, abs(d));
    let inside = smoothstep(1.0, -1.0, d);
    let ptr = (vec2<f32>(u.extra.y, u.extra.z) - vec2<f32>(0.5, 0.5)) * rect_size;
    let radius_spot = max(min(rect_size.x, rect_size.y) * 0.75, 48.0);
    let falloff = exp(-length(p - ptr) / radius_spot);
    let col = mix(u.color_a.rgb, u.color_b.rgb, falloff);
    let alpha = (rim * (0.18 + 0.82 * falloff) + inside * falloff * 0.14) * intensity;
    return vec4<f32>(col, clamp(alpha, 0.0, 1.0));
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kind = i32(round(u.params.x));
    let t = u.params.y * u.params.z;
    let intensity = clamp(u.params.w, 0.0, 1.5);
    let rect_size = max(u.rect.zw, vec2<f32>(1.0, 1.0));
    let aspect = rect_size.x / rect_size.y;

    var res = vec4<f32>(0.0);
    if (kind == 0) {
        res = shade_aurora(in.uv, t, intensity);
    } else if (kind == 1) {
        res = shade_particles(in.uv, aspect, t, intensity);
    } else if (kind == 2) {
        res = shade_waves(in.uv, t, intensity);
    } else if (kind == 3) {
        res = shade_grid(in.uv, rect_size, t, intensity);
    } else if (kind == 4) {
        res = shade_border_beam(in.uv, rect_size, t, intensity);
    } else if (kind == 5) {
        res = shade_glow_border(in.uv, rect_size, t, intensity);
    } else if (kind == 6) {
        res = shade_iridescence(in.uv, t, intensity);
    } else {
        res = shade_spotlight_rim(in.uv, rect_size, intensity);
    }

    // Clip cleanly to rounded rect bounds when radius > 0.
    let half_sz = rect_size * 0.5;
    let r = min(u.extra.x, min(half_sz.x, half_sz.y));
    if (r > 0.5 && (kind < 4 || kind == 6)) {
        let p = (in.uv - vec2<f32>(0.5, 0.5)) * rect_size;
        let d = sd_rounded_box(p, half_sz, r);
        let mask = smoothstep(1.0, -0.5, d);
        res.a = res.a * mask;
    }

    return encode_out(res.rgb, res.a);
}
