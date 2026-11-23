# Liquid Glass for Rune: design

Status: phase 1, design only. Phase 2 builds this after review.

The design adds `El::liquid_glass(LiquidGlass)`. It is a material that frosts, tones, tints, bends and lights
what the window has already drawn behind an element, in a single shading draw per element. It reads one
backdrop capture that the elements in a frame share. `El::backdrop_blur` stays as it is.

Paths are repo-relative. Rune paths are under `crates/`. Exo paths are prefixed `exo:` and are relative to
the Exo launcher repo. Line numbers are from the `exo` branch at `fc71db1a`.

**Retarget (phase 2).** The material now matches Apple's Liquid Glass (iOS 26, macOS 26), not the web launcher's
CSS glass. §0 says what that means and where each value comes from. It replaces the web-token presets of §4 and the
answers that followed the web (σ 10/16, Chromium's blend space). The capture architecture of §1–§3 stays: one
backdrop capture per epoch, a shared frost pyramid, one composite draw per glass shape.

## 0. Apple parity

### What Apple documents

| Concept | Apple's API | Rune |
| --- | --- | --- |
| Material and variants | `glassEffect(_:in:)` with `Glass.regular`, `.clear`, `.identity`; `UIGlassEffect(style:)` | `El::liquid_glass(LiquidGlass::regular() / clear() / identity())` |
| Tint | `Glass.tint(_:)`, `UIGlassEffect.tintColor` | `LiquidGlass::tint(Color)` |
| Interaction | `Glass.interactive(_:)`, `UIGlassEffect.isInteractive` | `LiquidGlass::interactive()` |
| Default shape | "the regular variant … along with a Capsule shape" (`glassEffect(_:in:)`) | A capsule unless the element sets `radius` |
| Merging | `GlassEffectContainer(spacing:)`, `UIGlassContainerEffect.spacing`: "the distance between elements at which they begin to merge" | `El::glass_container(spacing)` |
| Morphing | `glassEffectID(_:in:)` with a `@Namespace`; `GlassEffectTransition.matchedGeometry` / `.materialize` / `.identity` | `El::glass_id(id)`: a shared-element flight (`El::matched`) inside the container; glass that appears unmatched materializes |
| Union | `glassEffectUnion(id:namespace:)`: "All Liquid Glass effects with the same shape and Liquid Glass variant will be combined into a single shape" | `El::glass_union(id)` |
| Container rendering | "the glass container will render all glass elements in one combined view, behind the visual effect view's contentView" (`UIGlassContainerEffect`); "glass can not sample other glass" (WWDC25 323) | One composite draw per container, before any member's content; members share one capture |

Sources: [HIG › Materials](https://developer.apple.com/design/human-interface-guidelines/materials);
[SwiftUI `Glass`](https://developer.apple.com/documentation/swiftui/glass),
[`glassEffect(_:in:)`](https://developer.apple.com/documentation/swiftui/view/glasseffect(_:in:)),
[`GlassEffectContainer`](https://developer.apple.com/documentation/swiftui/glasseffectcontainer),
[`glassEffectID(_:in:)`](https://developer.apple.com/documentation/swiftui/view/glasseffectid(_:in:)),
[`glassEffectUnion(id:namespace:)`](https://developer.apple.com/documentation/swiftui/view/glasseffectunion(id:namespace:)),
[`GlassEffectTransition`](https://developer.apple.com/documentation/swiftui/glasseffecttransition),
[Applying Liquid Glass to custom views](https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views);
[`UIGlassEffect`](https://developer.apple.com/documentation/uikit/uiglasseffect),
[`UIGlassContainerEffect`](https://developer.apple.com/documentation/uikit/uiglasscontainereffect);
WWDC25 [219 "Meet Liquid Glass"](https://developer.apple.com/videos/play/wwdc2025/219/) and
[323 "Build a SwiftUI app with the new design"](https://developer.apple.com/videos/play/wwdc2025/323/).

### What Apple describes but doesn't number

- **Lensing** is "the primary way Liquid Glass visually defines itself"; the material "bends, shapes, and
  concentrates light" (219, Dynamics).
- **Highlights** "respond to geometry"; lights "move in space, causing light to travel around the material,
  defining its silhouette"; "in some cases, the lighting responds to device motion" (219, Principles).
- **Adaptivity.** Regular "blurs and adjusts the luminosity of background content" (HIG). Its shadow gets more opaque
  over text and less opaque over a solid light background. Small elements flip between light and dark with what is
  under them, and their symbols flip too. Large elements adapt but don't flip (219, Adaptivity).
- **Size.** Larger glass "simulates a thicker, more substantial material. It casts deeper, richer shadows, has more
  pronounced lensing and refraction effects, and a softer scattering of light" (219).
- **Clear** "is permanently more transparent", has no adaptive behaviours and "needs a dimming layer". The HIG's
  number: "If the underlying content is bright, consider adding a dark dimming layer of 35% opacity."
- **Tint** "generates a range of tones that are mapped to content brightness underneath" (219), unlike a flat fill.
- **Interaction.** Glass "reacts to user interaction by scaling, bouncing, and shimmering" (323). It "illuminates
  from within", with a glow that starts "right under your fingertips" and spreads through the element (219).
- **Appearing.** Glass "materialize[s] in and out by gradually modulating the light bending and lensing" (219), not by
  a plain fade.
- **Accessibility** (219). Reduce Transparency "makes Liquid Glass frostier and obscures more of the content behind
  it". Increase Contrast "makes elements predominantly black or white and highlights them with a contrasting border".
  Reduce Motion "decreases the intensity of some effects and disables any elastic properties".

### Values, and where each comes from

Apple publishes no shader parameters. These values were measured from Apple's own published images and video:
the June 2025 newsroom stills (the iOS 26 and macOS Tahoe 26 announcements, about 1 px per point) and the WWDC25 219
session video (1920×1080). Measurement scripts and crops are kept out of the repo, as is the side-by-side board; the
report that accompanies phase 2 holds them. Where nothing could be measured, the value is marked *tuned*.

| Property | Rune value | Source |
| --- | --- | --- |
| Rim width | 1 pt (in device px: `max(1, k_px)`) | Newsroom iOS 26 hero, lock-screen buttons: rim FWHM 1.0–1.5 px. Music mini-player and Dock (macOS Tahoe): a 1 px peak. |
| Rim contrast | Peak +0.16 linear white on the lit side, 0.45× of that opposite, 0.1× at 90° | Same stills: +20…+45 (8-bit) over the interior at the top-left and bottom-right, about +5 at top-right and bottom-left. |
| Specular shape | Two lobes along the light axis: the primary faces the light, the secondary is opposite at 0.5×, `pow(|cos|, 3)` falloff (half width ≈ 45°) | WWDC 219, 00:46–00:56, a glass sphere: lobes +85 and +40…+50 opposite each other, 2–4 px on a 340 px radius. |
| Light | Fixed above the window at the top-left. Each shape's lobe angle follows the shape's position, its velocity tilts the lobes, and interactive glass adds the pointer as a second light | 219: highlights "respond to geometry" and "move in space"; *tuned*. |
| Lensing profile | Sample offset toward the centre, `D(d) = Dmax·(1 − d/b)²` for `d < b`, 0 beyond | WWDC 219 at 00:50, glass over a grid: the displacement is 0 past 0.17 r, 34 px at 30 px from the rim, 13 px at 40 px, about 136 px at the rim. A `(1−t)²` fit has residual under 15%. |
| Bevel `b` | `0.17 × min(w, h)/2`, clamped to 2…12 pt | Same fit: b ≈ 58 px on r = 341 px. The 12 pt cap keeps large panels subtle (coordinator answer 1). |
| Refraction `Dmax` | `2.3 × b` | Same fit: 136 / 58. |
| Dispersion | R × (1 − 0.03), B × (1 + 0.03) of `D`, in the bevel band only | WWDC 219 at 00:50: a rainbow fringe of 3–4 px on a 136 px displacement. |
| Frost, regular | σ = `clamp(2 + 0.04·min(w,h), 3, 10)` pt (44 pt control → 3.8, 300 pt menu → 10) | HIG: regular "blurs". 219: larger means "softer scattering". Under the Music mini-player and the lock-screen platter, detail at about 8 pt is gone. |
| Frost, clear | σ = 0.6 pt | HIG: clear "is highly translucent". |
| Clear dimming | 35% black, scaled by `smoothstep(0.35, 0.65, L)` of the backdrop under the shape | HIG Materials, 35%. |
| Regular tone | Mix toward white by 0.20 over a bright backdrop, toward black by 0.24 over a dark one, `smoothstep(0.3, 0.7, L)` between them, saturation 1.35 | HIG figures: darker over dark, lighter over light. The macOS Dock measures +26 over 95 (8-bit). Saturation is *tuned* ("picking colors from nearby content", 323). |
| Large elements don't flip | The tone's mix is held at the light side for shapes over 200 pt on both axes (menus, sidebars) | 219. |
| Tint | `tint(c)` maps `c` to backdrop brightness: `mix(c·0.75, c, L)` at 0.55 alpha over the frost | 219, "a range of tones … mapped to content brightness"; alpha is *tuned* against the solid-fill comparison in 219. |
| Shadow | y 2 pt, blur `0.25·min(w,h)` clamped to 6…18 pt; opacity 0.06 over a flat light backdrop up to 0.20 over detail (text) | 219, adaptive shadow rule; numbers *tuned*. |
| Press scale | 1.10, and up to 1.18 along the drag direction (gel stretch), on a press of interactive glass | WWDC 219, 03:52–03:55, share button: rest Ø 432–456 px, pressed 478 × 540 px. |
| Press spring | Response 0.21 s, damping ratio 0.6 (about 10% overshoot). Release uses the same spring | Same shot: rest to peak in 3–4 frames at 30 fps, then a 5 px overshoot on 45 px of travel and back. |
| Drag follow | The glass translates toward the pointer by 0.25 of its offset, capped at 0.12 × its size | Same shot: 117 px of drift on a 444 px button. |
| Touch glow | Radial white from the touch point. The radius springs from 0 to the shape's diagonal in about 0.2 s, at 0.22 strength at the point and 0.10 at the rim | 219, "illuminates from within … right under your fingertips"; peak brightening +45 (8-bit) in the concept shot. The shipped strength is *tuned* lower. |
| Hover | +0.05 white over the glass (pointer platforms only) | *tuned*. macOS 26 controls brighten under the pointer; iOS has no hover. |
| Merge | `smin(d₁, d₂, k)` with `k = spacing`: shapes join when their nearest edges are within `spacing` | `UIGlassContainerEffect.spacing`, and "morphs … when the eraser's nearest edge is less than or equal to the container's spacing" (Applying Liquid Glass). |
| Materialize | Opacity, lensing and frost ramp together from 0 | 219; `GlassEffectTransition.materialize`. |
| Reduce Transparency | σ × 2.5 (at least 10 pt), and tone mix 0.75 | 219. |
| Increase Contrast | Tone mix 0.9 toward black or white, plus a 1 pt border in the opposite tone at 0.6 | 219. |
| Reduce Motion | No stretch, follow or overshoot (the press is critically damped). The light is fixed (no velocity tilt). Glass ids don't fly (`El::matched` is already off under Reduce Motion) | 219. |

**Recreations, used for the math only.** kube.io's "Liquid Glass in the browser" (no licence stated, so no code
is copied) gives the convex-squircle height profile and IOR 1.5; the fitted `(1 − t)²` displacement follows the same
curve to within the fit error. `liquid-glass-react` (MIT, © 2025 Max Rovensky) shows per-channel displacement for
dispersion and pointer-driven elasticity. Neither is a dependency, and no code was copied.

### What this changes in the build

- **One draw per container, not per element.** A container's members register shapes (rect, radii, variant, tint,
  interaction state) into a side-table range. One `KIND_LIQUID_GLASS` record covers the union bounds inflated by the
  shadow reach. Its shader evaluates `smin` over the shapes (at most 32 per draw, and a container with more splits),
  so the merged blob, its lensing normal (the SDF gradient by central differences) and its rim are all exact where
  shapes meet. A lone glass element is a container of one.
- **Same-frame adaptivity.** The composite reads the coarsest pyramid levels under the shape for `L` (the mean
  luminance) and `detail` (`|F3 − F6|`), and derives the tone, the dimming and the shadow opacity from them in the
  same frame. Nothing waits on a readback.
- **Foreground flip on the CPU.** After each epoch's down chain, a probe pass writes one texel per glass shape with an
  id (`L`, `detail`) into a small `Rgba16Float` strip. That strip is copied into a ring of three `MAP_READ` buffers and
  mapped asynchronously; a later frame reads whichever buffer is ready, and the frame never stalls. The runtime
  smooths `L` (a 150 ms time constant) and flips `Cx::glass_is_dark(id)` with hysteresis (0.45 / 0.55), so symbols
  and text can follow the glass.
- **Interaction reuses the runtime's press tracks.** Press, hover and `press_scale` already exist (`P_PRESS`,
  `P_HOVER`, `P_PRESS_SCALE` in `crates/rune/src/runtime/input/hover.rs`). Interactive glass adds the glass press
  spring, the stretch and follow, and the touch glow, which reads the pointer position at paint time.
- **Morph reuses `El::matched`.** `glass_id` flies the shape with the runtime's shared-element spring. During the
  flight the shape stays in its container's draw, so it merges and splits with its neighbours.

## 1. Where it plugs in

### What exists today

`El::backdrop_blur` already does a correct but expensive form of this:

| Step | Where | What it does |
| --- | --- | --- |
| API | `crates/rune/src/el/backdrop.rs:35` | Stores `sigma` in the cold visual field `crates/rune/src/el/visual.rs:66`. |
| Paint | `crates/rune/src/runtime/paint/build/layers.rs:332-338` | `blur_hole` pushes a `KIND_BACKDROP_BLUR` record **before** the element's own layer and shadows, then increments a per-frame counter checked against `max_backdrop_blurs()` (:334). |
| Record | `crates/rune/src/runtime/paint/slots.rs:38-60` | `push_backdrop_blur`: `params = [sigma*k_px, KIND_BACKDROP_BLUR, 0, k_px]`, `fill.a = opacity`, signed radii for continuous corners, a rounded ancestor clip folded in. It returns the hole radius that outer shadows leave clear. |
| Kind | `crates/rune/src/render/instance.rs:49,54-56` | `Kind::BackdropBlur = 13`, above `UBER_MAX_KIND_THRESHOLD`, so the uber shader never draws it. `Kind::COUNT = 14` (:63). |
| Batch | `crates/rune/src/render/upload.rs:308-318`, `:415` | Effect and blur records go to `Renderer::effects`. `backdrop_used` makes sure one pooled full-window backdrop copy exists per target format (`crates/rune/src/render/pass.rs:267`), then calls `prepare_backdrop_blur`. |
| Prepare | `crates/rune/src/render/backdrop_blur.rs:451-503` | Builds the pipelines synchronously on first use and plans one region per record (`plan_region` :54-85). Only the first `blur_cap` records are planned (`take(blur_cap)` :470). |
| Draw | `crates/rune/src/render.rs:479-509` → `crates/rune/src/render/backdrop_blur.rs:508-539` | For **each** record: `ActivePass::split` (`crates/rune/src/render/pass.rs:150-242`) ends the pass, copies the target, runs a full 7-level down + up dual-filter pyramid scissored to the record (`encode` :407-442), re-opens the pass with `LoadOp::Load`, then draws `backdrop_composite.wgsl`. |
| Layers | `crates/rune/src/render/layers.rs:779-802`, `crates/rune/src/render.rs:615-644` | Isolated layers render into their own targets **before** the frame pass (`draw_load` calls `encode_layers_timed` at :617). A blur inside a layer therefore sees only that layer. |
| Rest | `crates/rune/src/render/layers.rs:280-301` | At rest the backdrop copies and the pyramid are given back when the last frame read none. |
| Partial frames | `crates/rune/src/render/damaged.rs:184` | `redraws_in_place` refuses (`"backdrop blur"`), so a frame with a blur redraws the whole target. |
| CPU boot | `crates/rune/src/render/cpu_boot.rs:195-210`, `crates/rune/src/render/cpu_boot/backdrop.rs` | The CPU first frame mirrors the pyramid at layer depth 0 and refuses a blur inside a layer (`NeedsGpu::BackdropBlur`). |

There are two costs. Each blurred element pays for a pass split, a copy and a full pyramid. The cap of 4
(`MAX_BACKDROP_BLURS`, `crates/rune/src/render/backdrop_blur.rs:13`) is enforced twice, once in paint and once
in prepare. Exo works around the cap with `BlurBudget`, which turns the Bar's blur off while a modal or popover is
open (`exo: apps/launcher/native-ui/crates/exo-ui/src/shell.rs:34-90`).

### What Liquid Glass adds

| # | File (new or touched) | Change |
| --- | --- | --- |
| 1 | `crates/rune/src/el/liquid_glass.rs` (new) | `LiquidGlass`, its presets and builders, `El::liquid_glass`, `El::glass_group`. |
| 2 | `crates/rune/src/el/visual.rs:66` | A cold field `liquid_glass: Option<Box<LiquidGlass>>` beside `backdrop_blur`. The hot path stays the same size. |
| 3 | `crates/rune/src/runtime/paint/build/layers.rs:332-338` | `glass_hole` beside `blur_hole`, at the same place in paint order and with **no cap check**. It also hoists the record over alpha-only ancestor layers (§3.5). |
| 4 | `crates/rune/src/runtime/paint/glass.rs` (new; `slots.rs` stays small) | `push_liquid_glass`: the record and its `GlassParams` row in a side table owned by the paint subsystem, plus the solid fallback when the glass policy resolves to solid (§5). |
| 5 | `crates/rune/src/render/instance.rs:49-63` | `Kind::LiquidGlass = 14`, `KIND_LIQUID_GLASS`, `COUNT = 15`, and an assert that it is above `UBER_MAX_KIND_THRESHOLD` (also in `render/abi_tests.rs:828-830`). |
| 6 | `crates/rune/src/frame/host_step.rs:624-625` | `renderer.set_liquid_glass(rt.liquid_glass_params())`, beside `set_fade_stops` and `set_max_backdrop_blurs`. This follows the existing side-table hand-off; `Renderer::prepare`'s signature is unchanged. |
| 7 | `crates/rune/src/render/upload.rs:311-318` | `backdrop_used` also counts glass records. `prepare_liquid_glass` runs after `prepare_backdrop_blur`. |
| 8 | `crates/rune/src/render/liquid_glass/` (new: `mod.rs`, `plan.rs` for epochs, `pyramid.rs`, `composite.rs`) | Pipelines, epoch planning, the shared frost pyramid and the composite. |
| 9 | `crates/rune/src/render.rs:485-487` | A `Kind::LiquidGlass` branch in `draw_with_effects`: at an epoch start, `split` and build the pyramid; then one draw per record. |
| 10 | `crates/rune/src/render/pass.rs:150` | No change to `split`. The epoch's union rect goes in its existing `copy: Option<[f32; 4]>` argument, so only that region is copied. |
| 11 | `crates/rune/src/shaders/liquid_glass_down.wgsl`, `liquid_glass.wgsl` (new) | The down chain (the same 13-tap filter as `fs_down` in `backdrop_blur.wgsl`) and the composite (§2). |
| 12 | `crates/rune/src/render/dxbc.rs:63-65,254` | Precompiled DXBC for both, regenerated with `regen_dxbc`, so DX12 never runs FXC on the first glass frame. |
| 13 | `crates/rune/src/render/first_use.rs:11,27` | `make_liquid_glass` and `first_use_ms`, so the first build is measured. |
| 14 | `crates/rune/src/render/damaged.rs:184` | `"liquid glass"`: a frame with glass is a full frame. |
| 15 | `crates/rune/src/render/cpu_boot.rs:195-210` | `NeedsGpu::LiquidGlass`: the CPU boot refuses a frame that has glass (§3). |
| 16 | `crates/rune/src/render/layers.rs:280-301` | The rest gives back the glass pyramid exactly when it gives back the blur pyramid. |
| 17 | `crates/rune/src/platform/preferences.rs` | `reduced_transparency()` per platform (§5). |
| 18 | `crates/rune/src/runtime/api.rs:220-222`, `runtime/cx.rs:63,144,314`, `runtime/commit.rs:358` | `set_system_reduced_transparency`, `Cx::system_reduced_transparency()`, and the commit state, mirroring `system_increased_contrast`. |
| 19 | `crates/rune/src/app.rs:164-167` | `App::glass(GlassPolicy)`, a builder (`AppSpec` is built as a literal, so no pub field). |
| 20 | `changelog/unreleased/<date>-liquid-glass.md` | One bullet. |

### Pass diagram

Everything below is recorded in **one** command encoder and submitted once per frame.

```
encode_layers_timed (render/layers.rs:779)        isolated layers, drawn before the frame pass
  └─ glass whose only ancestors are alpha-only layers was hoisted out by paint (§3.5);
     glass inside a cache/blur/blend layer samples that layer, as backdrop_blur does

frame pass  (render.rs:615 draw_load → draw_with_effects render.rs:479)
  │ instances 0..a                      app content behind the Bar
  │
  ├─▶ EPOCH 1 at glass record a ───────────────────────────────────────────────────────
  │     end pass
  │     copy  target ⟶ backdrop      only the union of epoch 1's sample regions (pass.rs:204)
  │     down  backdrop ⟶ F0 (½) ⟶ F1 … ⟶ F6 (1/128)  13-tap, scissored to the union, Rgba16Float
  │     re-open pass, LoadOp::Load
  │ glass a       1 draw   Bar · search capsule ┐
  │ instances     its icon + text               │ one capture for the whole
  │ glass b       1 draw   Bar · nav capsule    │ Bar (glass_group, or an
  │ instances     icons                         │ inferred non-overlap)
  │ glass c..h    1 draw each, tools, mini-player ┘
  │ instances …   rest of the page
  │
  ├─▶ EPOCH 2 at glass record p ───────────────────────────────────────────────────────
  │     p's sample region overlaps pixels drawn since epoch 1 (the Bar's glass) → capture again
  │ glass p       1 draw   popover (sees the Bar's glass, its icons and the page)
  │ instances     menu rows
  │ glass q       1 draw   a badge inside the popover: its region overlaps p → EPOCH 3,
  │                        unless it is LiquidGlass::clear() (no frost, no capture)
  └─ end pass ⟶ present
```

Each epoch costs one pass split, one region copy and one down chain. Each element costs one draw. Nothing is
capped.

## 2. The composite pass

### Epochs: one capture shared by many elements

`render/liquid_glass/plan.rs` walks the frame's glass records in stream order during `prepare`:

- **Sample region** of a record, in device pixels: its rect inflated by `blur_reach = 3·σ_px + 2·2^(lod+1)` (the
  B-spline footprint at the sampled level) plus `refraction_reach = depth_px` when refraction is on, then clipped
  to the record's clip and the viewport.
- **Dirty since capture**: the union of the device-pixel bounds of every instance drawn after the current epoch's
  capture point. This includes glass records, shadows inflated by their blur, and clipped rects. It is computed
  once per frame over the instance range between consecutive glass records, and only on frames that have glass.
- A record **joins** the current epoch when its sample region misses the dirty union. Every pixel it reads is then
  unchanged since the capture, so the shared capture is exact for it.
- Otherwise it **starts a new epoch**: it captures again at its own stream position.
- `El::glass_group()` declares that every glass in a subtree shares one capture taken where the group's first
  glass draws. Glass in a group does not see other glass in the same group, which is Apple's
  `GlassEffectContainer` behaviour. Glass after the group sees all of it. Without the group, two capsules 8 pt apart
  with σ = 14 pt reach into each other's icons and would take two epochs. With it they take one. Exo's Bar is one
  group.
- The pyramid is a single texture set reused by every epoch. GPU order makes this safe: epoch 1's draws read it
  before epoch 2's down chain writes it.

The result is that cost scales with **depth** (page → Bar → popover → nested glass, usually 1 to 3 epochs), not
with the number of elements. Sixty-four badges over one page are one epoch and 64 draws.

**Glass over glass.** A later record whose region overlaps earlier glass gets a new epoch, so it frosts the earlier
glass, its rim and its text. A popover over the Bar sees the Bar's glass, as the web launcher's top-layer popovers
do. Nested glass (a badge in a popover) sees its parent's glass. CSS draws the latter clear because of backdrop
roots (`exo: apps/launcher/ui/src/styles/glass.css:227-233`). `LiquidGlass::clear()` keeps the web's look on
purpose and skips the capture.

### Sigma → mip

The frost pyramid is a **down-only** chain: F0 is half resolution and F6 is 1/128, made with the 13-tap filter that
`fs_down` uses (`crates/rune/src/shaders/backdrop_blur.wgsl`). The composite reconstructs any σ by sampling two
adjacent levels with a 4-tap bicubic B-spline (as `bspline`/`ui_at` do in
`crates/rune-light/src/shaders/glass.wgsl:144,165`) and mixing them trilinearly. `GLASS_SIGMAS[k]` is the measured
line spread of "down to level k, then B-spline back up", measured the same way `SIGMAS` was
(`crates/rune/src/render/backdrop_blur.rs:28`). `lod` is the position of σ in that table, interpolated in log₂.
That gives one pyramid for every σ in the frame, where `backdrop_blur` builds an up chain for each σ. If
phase 2's quality gate fails (step 3), the fallback is one up chain per **distinct** σ per epoch. That is still
independent of element count, because Exo uses three σ values.

### WGSL outline (`crates/rune/src/shaders/liquid_glass.wgsl`)

```wgsl
// group(0): the uber globals (viewport, clip stops, fade stops), the same as backdrop_composite.
// group(1): this epoch's frost and the per-element table.
@group(1) @binding(0) var frost: texture_2d<f32>;        // F0..F6 as mips, Rgba16Float, linear
@group(1) @binding(1) var samp: sampler;                 // linear, clamp-to-edge, linear mip filter
@group(1) @binding(2) var<storage, read> glass: array<Glass>;
@group(1) @binding(3) var<uniform> epoch: Epoch;         // union origin/size in device px, F0 texel size, still_mode

struct Glass {           // 144 B, indexed by record.params.z
  tone:   vec4f,         // brightness, contrast, saturation, sigma_px
  tint:   vec4f,         // premultiplied, linear: drawn over the toned frost
  rim:    vec4f,         // top, side, bottom alpha; width_px (default: one device pixel)
  rim_c:  vec4f,         // rim colour (white)
  inner:  vec4f,         // inset: offset_y_px, blur_px, spread_px, _
  inner_c:vec4f,         // inset colour, premultiplied (white .1 = the web's top glow)
  bend:   vec4f,         // bevel_px, ior, depth_px, dispersion   (bevel 0 = off)
  light:  vec4f,         // angle (rad), specular, sharpness, _
  sheen:  vec4f,         // top alpha, stop (0..1 of height), _, _
};

fn frost_at(p: vec2f, lod: f32) -> vec3f {
  let uv = (p - epoch.origin) * epoch.inv_size;                 // device px → union uv
  let lo = floor(lod);
  return mix(bspline(frost, samp, uv, lo), bspline(frost, samp, uv, lo + 1.0), lod - lo);
}

fn tone(c_lin: vec3f, g: Glass) -> vec3f {
  // CSS filter math runs on sRGB-encoded values. Brightness, then contrast, then saturate. Saturate commutes
  // with both (its rows sum to 1), so this matches every chain in which brightness comes before contrast,
  // which is every Exo token.
  var c = srgb_encode(c_lin) * g.tone.x;
  c = (c - 0.5) * g.tone.y + 0.5;
  let l = dot(c, vec3f(0.2126, 0.7152, 0.0722));               // CSS saturate()'s luma row
  c = mix(vec3f(l), c, g.tone.z);                              // == CSS's saturate matrix
  return srgb_decode(clamp(c, vec3f(0.0), vec3f(1.0)));
}

@fragment fn fs(in: Vary) -> @location(0) vec4f {
  let r = rec(in.index);                         // the KIND_LIQUID_GLASS instance
  let g = glass[u32(r.params.z)];
  let p = in.screen;                             // device px
  let d = sd_shape(p, r.rect, r.radii);          // rounded or continuous (signed radii), < 0 inside
  var cov = aa(d) * clip_alpha_stops(p, r.clip, r.fade) * r.fill.a;  // clip, rounded clip, fade, opacity
  if (cov <= 0.0) { discard; }
  let n = sd_normal(p, r.rect, r.radii);         // analytic gradient, outward

  // Rim refraction: only inside the bevel band, so a flat interior pays nothing.
  var off = vec2f(0.0);
  let inside = -d;
  if (g.bend.x > 0.0 && inside < g.bend.x) {
    let t = 1.0 - inside / g.bend.x;             // 0 where the flat starts, 1 at the edge
    off = -n * bend_slope(t) * g.bend.z * (1.0 - 1.0 / g.bend.y);   // rune-fx bend_at profile, Snell scaled by IOR
  }
  let lod = sigma_lod(g.tone.w);
  var f: vec3f;
  if (epoch.still_mode != 0u) {
    f = vec3f(0.0);                              // capture unavailable: tint becomes the still fill
  } else if (g.bend.w > 0.0 && any(off != vec2f(0.0))) {
    f = vec3f(frost_at(p + off * (1.0 - g.bend.w), lod).r,     // dispersion (off by default):
              frost_at(p + off, lod).g,                       // three taps in the bevel band only
              frost_at(p + off * (1.0 + g.bend.w), lod).b);
  } else {
    f = frost_at(p + off, lod);
  }

  var c = tone(f, g);
  c = c * (1.0 - g.tint.a) + g.tint.rgb;                      // tint (the element's glass fill)

  let y = (p.y - r.rect.y) / r.rect.w;                         // sheen: top wash to `stop`
  c += (1.0 - smoothstep(0.0, g.sheen.y, y)) * g.sheen.x * (1.0 - c);

  let di = sd_shape(p - vec2f(0.0, g.inner.x), inset(r.rect, g.inner.z), r.radii);   // inner shadow/glow
  let ia = smoothstep(-0.5 * g.inner.y, 0.5 * g.inner.y, di) * aa(d);   // lit outside the moved hole: CSS inset box-shadow by SDF
  c = c * (1.0 - g.inner_c.a * ia) + g.inner_c.rgb * ia;

  // Edge light: a hairline ring whose alpha follows the normal (top / sides / bottom, as --glassy-ring), plus an
  // optional light-angle specular across the bevel band (rune-fx distort.wgsl:146-160).
  let ring = aa(d) - aa(d + g.rim.w);                         // the outermost rim.w px of the shape
  let ra = select(mix(g.rim.y, g.rim.z, max(n.y, 0.0)), mix(g.rim.y, g.rim.x, max(-n.y, 0.0)), n.y < 0.0);
  let band = select(ring, 1.0 - smoothstep(0.0, g.bend.x, inside), g.bend.x > 0.0);
  let spec = g.light.y * pow(max(dot(n, -vec2f(cos(g.light.x), sin(g.light.x))), 0.0), g.light.z) * band;
  let lit = clamp(ra * ring + spec, 0.0, 1.0);
  c = c * (1.0 - lit * g.rim_c.a) + g.rim_c.rgb * lit;

  return vec4f(c, 1.0) * cov;                                  // premultiplied, SourceOver
}
```

The record keeps the existing `Instance` ABI. It uses `rect`, `radii` (signed for continuous corners), `fill.a`
(opacity), `clip`, `fade`, and `params = [sigma_px, KIND_LIQUID_GLASS, table_index, k_px]`. `border` carries the
still fill for the uber fallback (§5). Outer shadows are not part of the composite. The element's own
`shadow`/`box_shadow` is the hook, drawn after the record with its interior left clear, as `push_backdrop_blur`
does (`crates/rune/src/runtime/paint/slots.rs:58-59`).

Contiguous records in one epoch with nothing between them become one instanced draw. Otherwise each record is a
single draw at its stream position. Either way there is **one shading pass per element and no per-element
pass, copy or pyramid**.

## 3. Glass from the first frame

What has to hold is this: the first frame that shows an element shows it as glass, and frame 2 shows the same
pixels. Each item below closes one way that could fail.

1. **No cap.** `glass_hole` has no counter. Every glass element emits a record. There is no "first N are glass and
   the rest are fills" (contrast `crates/rune/src/runtime/paint/build/layers.rs:334` and
   `crates/rune/src/render/backdrop_blur.rs:470`).
2. **Same encoder, same submission, stream order.** The capture, the down chain and the composite are recorded
   inline in `draw_with_effects` (`crates/rune/src/render.rs:479`). The composite always reads this frame's pixels,
   never last frame's and never an empty texture.
3. **Synchronous pipelines on native.** `prepare_liquid_glass` builds the down and composite pipelines for the
   target format in the `prepare` of the first frame that has a glass record. This is what `prepare_backdrop_blur`
   does (`crates/rune/src/render/backdrop_blur.rs:451`), not the `breaker`'s deferred path. DX12 uses precompiled
   DXBC (`crates/rune/src/render/dxbc.rs`), so the cost is a PSO creation, measured by `first_use_ms`. On the web the
   pipeline is used in the frame that builds it, and the error scope's late answer only matters if the device
   refused it: the breaker then trips to solid. A pop can therefore only happen on a broken device, and then it goes
   glass → solid, never fallback → glass on every launch. CI validates the WGSL through naga (the existing DXBC/ABI
   tests), so a refusal is a driver fault.
4. **Resources are ensured every glass frame.** The backdrop copy and pyramid are ensured in `prepare`
   (`crates/rune/src/render/upload.rs:311-318` pattern). The rest (`crates/rune/src/render/layers.rs:280-301`)
   gives them back only when the last frame drew no glass. When it does, the next glass frame re-creates them
   synchronously, before encoding.
5. **Fades and scales.** Opacity multiplies coverage (`fill.a`), so the element's own fade never isolates the glass.
   The record goes ahead of any layer the element opens, as `blur_hole` already does (`layers.rs:332-333`). Paint
   also **hoists** a glass record over an ancestor's layer when that layer is alpha-only (presented opacity,
   `layer_opacity`, `group_opacity`, a dissolve; source-over composite; no live transform) and the glass is the
   first thing painted inside it. The record moves to just before that layer's begin marker, and the layer's opacity
   is folded into `fill.a`. Without this a popover whose wrapper fades would frost an empty layer for the whole
   fade, because layers render before the frame pass (`crates/rune/src/render/layers.rs:779-802`). The trade-off:
   mid-fade, text over the glass composites over the glass rather than with it. The difference is `o·(1-o)·αtext·(G-B)`,
   where G is a blurred, tinted B, so it is small and gone at both ends of the fade. Scale transitions scale the
   record's rect and radii at paint time. Refraction and rim widths are in points and scale with them.
6. **Partial frames.** `damaged.rs:184` refuses partial frames with glass, so the whole target is redrawn and an
   epoch never reads a half-updated target.
7. **CPU boot.** `NeedsGpu::LiquidGlass` (`crates/rune/src/render/cpu_boot.rs:195-210`) makes the CPU first frame
   refuse, so the first frame that has glass is a GPU frame. There is no CPU-to-GPU hand-over where the glass changes.
   A CPU mirror is possible later (tone and frost are linear filters; `cpu_boot/backdrop.rs` already mirrors the
   pyramid), but it is not needed for correctness.
8. **Regression test.** A fresh `Renderer` draws a scene offscreen twice. Frame 1 must equal frame 2 byte for byte,
   and frame 1's glass region must differ from the same scene drawn in still mode (§6, step 5).

The one exception is a frame where the policy says solid (§5). That frame is solid from the start, by design.

## 4. API

**Phase 2 API (Apple parity, supersedes the block below):**

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidGlass { /* variant, tint, interactive, tuning */ }
impl LiquidGlass {
    pub fn regular() -> Self;                  // Glass.regular
    pub fn clear() -> Self;                    // Glass.clear: 35% dimming over bright content
    pub fn identity() -> Self;                 // Glass.identity: no effect
    pub fn tint(self, c: Color) -> Self;       // Glass.tint(_:)
    pub fn interactive(self) -> Self;          // Glass.interactive(): press spring, stretch, glow, hover
    pub fn frost(self, sigma_pt: f32) -> Self; // tuning: override the size-derived frost
    pub fn refraction(self, k: f32) -> Self;   // tuning: scale lensing (1 = Apple, 0 = flat)
    pub fn still(self, c: Color) -> Self;      // the solid fill when glass is off
}
impl El {
    pub fn liquid_glass(self, g: LiquidGlass) -> Self;   // .glassEffect(g, in: capsule or this El's radius)
    pub fn glass_container(self, spacing: f32) -> Self;  // GlassEffectContainer(spacing:)
    pub fn glass_id(self, id: impl Into<String>) -> Self;    // glassEffectID (namespace = the window)
    pub fn glass_union(self, id: impl Into<String>) -> Self; // glassEffectUnion
}
impl Cx { pub fn glass_is_dark(&self, id: &str) -> bool; }  // smoothed backdrop luminance under glass `id`
```

`GlassPolicy`, `App::glass` and `Tx::set_glass_policy` are unchanged from below. The web-token presets that follow
are no longer built.

**Phase 1 API (web tokens, superseded):**

```rust
/// A Liquid Glass material: frosts, tones, tints, bends and lights what this window has drawn behind the element.
/// Fields are private; builders keep it extensible without breaking apps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidGlass { /* blur, brightness, contrast, saturation, tint, rim, inner, refraction, light, sheen, still */ }

impl LiquidGlass {
    // Presets: Exo's web tokens, so an app reads the same as the web launcher.
    pub fn control() -> Self;    // small glass: blur 10, brightness .62, saturation 1.8, ring rim, top glow
    pub fn panel() -> Self;      // large glass: blur 16, saturation 1.8, brightness .66, tint black .36
    pub fn pane() -> Self;       // panes: blur 10, brightness .62, saturation 1.8
    pub fn badge() -> Self;      // badges: blur 10, brightness .34, contrast .9, saturation 1.6
    pub fn immersive() -> Self;  // over art: blur 18, brightness .78, saturation 1.9, tint white .06
    pub fn clear() -> Self;      // glass inside glass: no frost, no tone, rim + sheen only (no capture)

    pub fn blur(self, sigma: f32) -> Self;                     // points, as CSS blur(); 0 = no frost
    pub fn brightness(self, k: f32) -> Self;                   // 1 = unchanged
    pub fn contrast(self, k: f32) -> Self;                     // 1 = unchanged
    pub fn saturation(self, k: f32) -> Self;                   // default 1.8
    pub fn tint(self, color: Color) -> Self;                   // over the frost (the web's background-color)
    pub fn rim(self, top: f32, side: f32, bottom: f32) -> Self;// edge light alpha by edge; default .17/.06/.03
    pub fn rim_color(self, color: Color) -> Self;              // default white
    pub fn rim_width(self, points: f32) -> Self;               // default: one device pixel (El::hairline's rule)
    pub fn inner_shadow(self, y: f32, blur: f32, spread: f32, color: Color) -> Self; // inset glow or shade
    pub fn sheen(self, alpha: f32, stop: f32) -> Self;         // white wash from the top to `stop` of the height
    pub fn refraction(self, bevel: f32, ior: f32) -> Self;     // rim lens; bevel 0 = off (default)
    pub fn depth(self, points: f32) -> Self;                   // how far the rim bends (default = bevel)
    pub fn dispersion(self, k: f32) -> Self;                   // 0 = off (default); ~0.1 for a subtle fringe
    pub fn specular(self, intensity: f32, light_angle: f32) -> Self; // light-angle highlight; 0 = off
    pub fn still(self, color: Color) -> Self;                  // solid fill when glass is off (§5)
}

impl El {
    /// Draws this element's background as `glass`, clipped to its rounded or continuous corners, its clip and its
    /// fades; opacity and scale apply to the glass as to a fill. Children, border and shadows draw on top as usual.
    pub fn liquid_glass(self, glass: LiquidGlass) -> Self;
    /// Liquid glass in this subtree shares one backdrop capture and doesn't see itself (a toolbar).
    pub fn glass_group(self) -> Self;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlassPolicy { #[default] FollowSystem, Glass, Solid }
impl App { pub fn glass(self, policy: GlassPolicy) -> Self; }   // builder, not a field
impl Tx  { pub fn set_glass_policy(&mut self, policy: GlassPolicy); } // in-app setting (Exo's Material)
impl Cx  { pub fn system_reduced_transparency(&self) -> bool; pub fn glass_on(&self) -> bool; }
```

- The radius, continuous corners and clip come from the element (`radius`, `continuous_corners`,
  `clip_rounded`), as `backdrop_blur` does. There is no radius inside `LiquidGlass`.
- An element's `fill` still draws over the glass, as with `backdrop_blur`. Presets put the glass fill into `tint`
  so that `fill` stays free for state layers (hover, press).
- `backdrop_blur`, `max_backdrop_blurs` and `system_material` are unchanged. An element with both
  `backdrop_blur` and `liquid_glass` uses `liquid_glass`, and debug builds warn once.

### Exo example

```rust
use rune::{col, el, row, Color, LiquidGlass};
let ring = |g: LiquidGlass| g.rim(0.17, 0.06, 0.03).inner_shadow(16.0, 28.0, -22.0, Color::WHITE.alpha(0.1));
let bar = row().gap(8.0).glass_group()                                       // one capture for the whole Bar
    .child(search_field().liquid_glass(ring(LiquidGlass::control()).still(t.glassy_still_control)).radius(RADIUS_PILL))
    .child(nav_tabs().liquid_glass(ring(LiquidGlass::control())).radius(RADIUS_PILL));
let menu = col().liquid_glass(ring(LiquidGlass::panel()).still(t.glassy_still_panel))
    .radius(RADIUS_PANEL).continuous_corners().box_shadow(t.shadow_raised)  // outer shadow: the El's own
    .transition(popover_transition());                                       // glass from its first faded frame
let popover = el().liquid_glass(ring(LiquidGlass::panel()).refraction(6.0, 1.5)) // sees the Bar's glass
    .radius(RADIUS_PANEL).continuous_corners().child(rows);
```

### Web tokens → fields

Web tokens are from `exo: apps/launcher/ui/src/tokens.css` and `exo: apps/launcher/ui/src/styles/glass.css`.
Native values are from `exo: apps/launcher/native-ui/crates/exo-kit/src/tokens.rs`.

| Web token | Value | `LiquidGlass` | Preset |
| --- | --- | --- | --- |
| `--exo-control-glass` (tokens.css:198-206) | `blur(10px) brightness(.62) saturate(1.8)` | `blur(10) brightness(.62) saturation(1.8)` | `control` |
| `--exo-fill` (tokens.css:200) | `transparent` (solid: `--exo-solid-fill`) | `tint(transparent)` | `control` |
| large glass `backdrop-filter` (glass.css:54-71) | `blur(16px) saturate(1.8) var(--glassy-overlay-tone)` | `blur(16) saturation(1.8)` | `panel` |
| `--glassy-overlay-tone` | `brightness(.66)` | `brightness(.66)` (order is irrelevant: saturate commutes) | `panel` |
| `--glassy-overlay` | `rgb(0 0 0 / .36)` | `tint(black .36)` | `panel` |
| pane (glass.css:256-271), `--glassy-pane-tone` (tokens.css:235-242) | `blur(10px) brightness(.62) saturate(1.8)` | `blur(10) brightness(.62) saturation(1.8)` | `pane` |
| badge (glass.css:139-147) | `blur(10px) brightness(.34) contrast(.9) saturate(1.6)` | `blur(10) brightness(.34) contrast(.9) saturation(1.6)` | `badge` |
| `--exo-immersive-glass` | `blur(18px) brightness(.78) saturate(1.9)` | `blur(18) brightness(.78) saturation(1.9)` | `immersive` |
| `--exo-immersive-fill` | `rgb(255 255 255 / .06)` | `tint(white .06)` | `immersive` |
| `--glassy-ring` edges (tokens.css:177-179) | inset `--exo-hair` white .17 top, .06 sides, .03 bottom | `rim(.17, .06, .03)`, `rim_width` = one device pixel (`--exo-hair`) | all |
| `--glassy-ring` glow (tokens.css:179) | `inset 0 16px 28px -22px rgb(255 255 255 / .1)` | `inner_shadow(16, 28, -22, white .1)` | all |
| `--exo-glass-rim` (tokens.css:222) | `--glassy-ring, --glassy-rim` (`--glassy-rim` is transparent) | the two rows above | all |
| `--glassy-shadow` | none | none (the El's shadow) | `control` |
| `--glassy-shadow-raised` | `0 0 40px rgb(0 0 0 / .5)` | the El's `box_shadow` (outer shadow hook) | `panel` |
| `--glassy-sheen` | `none` on Void (native: `[(0, white .09), (.32, transparent)]`, tokens.rs:89) | `sheen(0, 0)`; native look: `sheen(.09, .32)` | — |
| `--glassy-still*` (tokens.css:238-241) | black .8 / .92 (panel) / .75 (pane) / .65 (control) | `still(..)` | each |
| `--exo-solid-fill` | `#000` | `still` under `GlassPolicy::Solid` | — |
| glass inside glass (glass.css:227-233) | clear | `LiquidGlass::clear()` | `clear` |
| `--glassy-words` | `0 1px 2px rgb(0 0 0 / .45)` | not glass: the children's text shadow | — |
| press light (`exo: apps/launcher/ui/src/lib/glass.ts`, glass.css:383-417) | radial 80px white .16 at the pointer | not a field: Exo keeps it as a radial overlay child | — |
| `prefers-contrast` / `forced-colors` (tokens.css:269-274, glass.css:446-474) | glass goes solid | forced solid when increased contrast or a contrast theme is on | — |
| no fading glass (tokens.css:400-401) | glass isn't faded on the web | not needed: glass fades (§3.5) | — |
| bent edge (glass.css:37-38) | none: "at 1080p a bent edge read as concave and stepped" | `refraction` off in every preset | — |

Native today uses σ 14 / 24 (`GLASS_CONTROL_SIGMA`, `GLASS_PANEL_SIGMA`, tokens.rs:153-154) where the web uses 10 / 16.
The presets follow the web. See Q4.

## 5. Accessibility and fallbacks

**Reading the setting.** `platform/preferences.rs` gains `reduced_transparency()`. It is read where
`reduced_motion`, `contrast` and `increased_contrast` are read already: at window creation and focus
(`crates/rune/src/window/host.rs:348-351`), on host events (`crates/rune/src/input/host_events.rs:544-547`), on
`WM_SETTINGCHANGE`/`WM_THEMECHANGED` (`crates/rune/src/a11y/windows.rs:59` → `crates/rune/src/a11y/mod.rs:176-178`), and on
overlays (`crates/rune/src/overlay.rs:472`). It is never read in the frame loop.

| Platform | Source |
| --- | --- |
| Windows | `RegGetValueW(HKCU, "Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", RRF_RT_REG_DWORD)`; `0` means reduced. One registry read per settings change, no file I/O. This is the value behind both Personalization › Colors › Transparency effects and Accessibility › Visual effects. |
| macOS | `NSWorkspace.accessibilityDisplayShouldReduceTransparency`, re-read on the display-options notification already observed (`crates/rune/src/a11y/macos.rs:44`). |
| iOS | `UIAccessibility.isReduceTransparencyEnabled` and its status notification. |
| Web | `matchMedia("(prefers-reduced-transparency: reduce)")` with a change listener, posted like the `motion` event (`crates/rune/src/platform/web/input.rs:281`). |
| Linux | No standard switch, so `false`. |
| All | `RUNE_REDUCE_TRANSPARENCY=1` turns it on for captures and checks, like `RUNE_INCREASE_CONTRAST` (`crates/rune/src/platform/preferences.rs:79-82`). |

**Policy.** Glass is on when:

```
policy == Glass, or (policy == FollowSystem and !reduced_transparency)
and !increased_contrast and contrast().is_none()
```

Paint decides this (`runtime/paint/glass.rs`). When glass is off, the element paints its `still` colour as an
ordinary uber `Rect` plus the rim ring and glow as ordinary instances. That means no record, no capture, no GPU
work, and a CPU boot can paint it. Under a contrast theme, `still` is replaced by the theme's `window` colour and
the rim by its `text` colour (`SystemContrast`, `crates/rune/src/platform/preferences.rs:110-126`).

**Fallback when capture is impossible** (the policy says glass, but the frame can't capture):

| Cause | Detected at | Draws |
| --- | --- | --- |
| A caller's own pass (`Renderer::draw_range`) | `ActivePass::splittable` (`crates/rune/src/render/pass.rs:96`) | The composite in still mode: tint = `still`, with rim, glow and sheen. |
| Target without `COPY_SRC` | the same | The same. |
| No backdrop pooled for this format | `split` returns `None` (`crates/rune/src/render/pass.rs:161`) | The same. |
| Composite pipeline refused (breaker) | `prepare_liquid_glass` | `prepare` rewrites the record in the upload copy as a `Kind::Rect` with `fill = border = still`, so the uber shader draws a plain still fill. |
| CPU boot frame | `NeedsGpu::LiquidGlass` | No CPU frame. The first frame is GPU (§3.7). |
| Device lost | the renderer is rebuilt | The next frame is glass again (§3.3, §3.4). |

## 6. Cost and memory

### GPU at 1920×1080, RTX 3070, DX12 (estimate)

The estimate assumes 448 GB/s and treats the work as bandwidth-bound, with B-spline taps hitting the texture cache.

| Work | Per | Worst case (full-screen union) | Typical Exo (Bar + popover regions) |
| --- | --- | --- | --- |
| Pass split (end, then begin with Load) | epoch | ~0.01-0.02 ms | ~0.01-0.02 ms |
| Region copy, Bgra8 | epoch | 8.3 MB read + write ≈ 0.04 ms | ≈ 0.005-0.01 ms |
| Down chain F0-F6, Rgba16Float | epoch | ≈ 0.04-0.05 ms | ≈ 0.01 ms |
| Composite: 8 bilinear taps (+ 16 in the bevel band with dispersion) | element | 16 elements over 25% of the screen ≈ 0.03-0.06 ms | ≈ 0.02 ms |

- One epoch with 16 elements: **≈ 0.15 ms**. Two epochs (Bar + popover): **≈ 0.25-0.3 ms**. Three: **≈ 0.4 ms**.
  The 0.5 ms budget holds up to about three full-screen epochs, or about ten region-sized epochs.
- The worst case is 16 ungrouped, mutually overlapping elements, which makes 16 epochs. With region-scissored copies
  that is ≈ 16 × 0.03 ≈ 0.5 ms, right at the budget. `glass_group` is the documented fix, and the bench (below)
  reports epochs per frame so a regression is visible.
- Today, 4 `backdrop_blur`s cost 4 splits, 4 copies and 4 down+up pyramids. That is about the same as 4 epochs,
  so Exo should get cheaper as well as uncapped.
- CPU: epoch planning is one pass over the instance range between glass records, on frames with glass only. The side
  table is 144 B per element.

### Memory at 1920×1080

| Resource | Size | Note |
| --- | --- | --- |
| Backdrop copy (per target format) | 1920×1080×4 = 8.3 MB | Already pooled for `backdrop_blur` (`crates/rune/src/render/pass.rs:267`) and shared. New only if no blur is in use. |
| Frost pyramid F0-F6, Rgba16Float | 960×540×8 × 4/3 ≈ 5.5 MB | One for the frame, whatever the number of elements or epochs. Sizes are rounded to `STEP` like the blur's. |
| Layer-format backdrop | +16.6 MB | Only when a glass record samples an isolated layer (not hoisted). |
| Side table and uniforms | 144 B × N + 256 B × epochs | 64 elements ≈ 9 KB. |
| **Total new** | **≈ 5.5 MB** (≈ 14 MB with the copy) | About 4× at 4K (≈ 22 / 55 MB). Given back at rest when no glass is drawn (`crates/rune/src/render/layers.rs:280-301`). |

### How phase 2 measures it

- **GPU time.** wgpu timestamp queries under the existing `frame-profile` feature (`crates/rune/Cargo.toml:34`,
  `Renderer::draw_timed`, `crates/rune/src/render.rs:610`). Each epoch's down chain is its own pass, so it gets
  begin/end timestamps. Composite time is the frame pass with glass minus the same scene drawn in still mode. The
  bench reports `capture_ms`, `pyramid_ms`, `composite_ms`, `epochs` and `elements` per frame.
- **Bench.** `crates/rune/examples/liquid_glass_bench.rs` (requires `frame-profile`) draws a 1920×1080 offscreen
  scene with photographic art behind:
  - `exo`: a Bar group of 6 capsules, a mini-player, a popover with 4 rows, 4 badges and 2 toasts. That is 16
    elements in 3 epochs.
  - `stress64`: 64 badges.
  - `ungrouped16`: the worst case.

  Run it with `--profile perf`, in paired, alternating glass/still runs, 10 × 300 frames, compared by median, with
  outliers kept. A headless number leaves out swap-chain present and compositor cost; the doc for the bench says so.
- **Gate.** The median glass − still delta is ≤ 0.5 ms on an RTX 3070 (DX12, 1080p) for `exo`. It is reported, but
  not gated, for `stress64` and `ungrouped16`.
- **Memory.** The pyramid is added to `report_pools` (`alloc-sites`, `crates/rune/src/render/upload.rs:319`), and a
  unit test asserts the pyramid's byte size at 1080p.

## 7. Risks and open questions

### Risks

1. **The B-spline-from-down-chain frost may shimmer.** Large σ under scrolling content could crawl, because the
   level grid is screen-aligned. The mitigation is to align each epoch's union to the F6 grid, as `plan_region`
   aligns (`crates/rune/src/render/backdrop_blur.rs:80-82`), and to gate it (§8 step 3). The fallback is one up
   chain per distinct σ per epoch.
2. **Epoch inference can split more than expected.** Blur reach is large (3σ is 84 px at σ = 14 pt at 2×). Siblings
   within reach of each other's shadows or icons start new epochs unless grouped. The bench reports epochs, and
   debug builds can log "epoch split by node N".
3. **Hoisting over alpha-only layers** makes text over the glass composite slightly differently mid-fade (§3.5). If
   glass isn't the first thing painted inside the layer, it falls back to CSS behaviour: it samples the layer, and
   the fade shows little frost.
4. **Tiled GPUs** (Apple, mobile) pay a full tile store and load for each split. The cost is per epoch, not per
   element, but iOS budgets need their own measurement.
5. **DXBC drift.** The new shaders must be added to `regen_dxbc` and `dxbc_tests`, or DX12 falls back to FXC on the
   first glass frame and the first frame is late (though still glass).

### Open questions

1. **Refraction in Exo.** The web launcher removed bent edges because they read as concave and stepped at 1080p
   (`exo: apps/launcher/ui/src/styles/glass.css:37-38`, `exo: apps/launcher/ui/src/lib/glass.ts`). The GPU version
   uses float offsets, bilinear frost and an analytic normal, so it should not step, but the look is a design call.
   The proposal: refraction off in every preset; Exo turns it on per surface (popover only?) after a side-by-side
   at 1080p.
2. **The Windows Transparency switch.** Exo deliberately does not follow it and offers Settings › Material
   (`exo: apps/launcher/docs/architecture/liquid-glass.md:26`). The proposal: Rune defaults to `FollowSystem`, which
   is right for an upstream framework and for accessibility. Exo keeps its current behaviour with
   `App::glass(GlassPolicy::Glass)` and maps Material Solid to `Tx::set_glass_policy(Solid)`. Should Exo follow the
   system instead?
3. **Glass inside `El::cache`/`El::blur`/blend layers.** These sample the layer (CSS semantics). Should cached
   layers hoist their first glass too? That would keep the cache valid when the content behind changes.
4. **σ for Exo native.** The presets use the web's 10 / 16. Native currently blurs at 14 / 24. Should Exo match
   the web, or keep 14 / 24 through `.blur(..)`?
5. **Overlay windows.** A popover in a separate OS window cannot sample the main window, so it gets the still fill.
   The menus checked are in-window `El::anchored` elements
   (`exo: apps/launcher/native-ui/crates/exo-ui/src/captures.rs:169-172`, `exo: .../exo-ui/src/library.rs:308`), and
   `exo-ui` uses no `OverlaySpec`. So this matters only if Exo later moves a menu into its own window. Should such
   menus use `system_material` there instead?
6. **The blend space of the frost.** The tone runs in sRGB-encoded values, as CSS does. The blur runs in the
   target's sampling space, which for an `*Srgb` target is linear, like `backdrop_blur`. Chromium blurs encoded
   values, so dark-on-light edges may look a little brighter than on the web. Should the down chain encode first
   when `Renderer::srgb_blend` is on?
7. **`BlurBudget`.** With no cap, Exo can delete `BlurBudget`
   (`exo: apps/launcher/native-ui/crates/exo-ui/src/shell.rs:34-90`, tests at `main.rs:1558,1797`) and keep the Bar
   glass on under modals. Is that the desired look, or should the Bar dim under a modal anyway?
8. **CPU boot.** Refusing CPU boot frames that contain glass delays the first frame until the GPU is ready on
   windows whose first view has glass (Exo's Bar does). Is that acceptable, or should phase 2 include a CPU mirror
   for refraction-free glass (exact for every preset)?

## 8. Phase 2 build plan

The coordinator's answers to §7 for phase 2: refraction on, kept subtle, with no stepped edges at 1080p (Q1); Exo
sets `GlassPolicy::Glass`, Rune defaults to `FollowSystem` (Q2); glass inside cache, blur and blend layers hoists
and samples the real backdrop (Q3); `BlurBudget` stays for `backdrop_blur` (Q7); no CPU boot frame for a first view
with glass, given the frame-1 == frame-2 test (Q8). Q4 and Q6 are replaced by §0: the values are Apple's, not the
web's.

Every step ends with `cargo fmt --check`, `cargo lint` (clippy, `-D warnings`), the fork suite and
`bash scripts/check-local.sh`. GPU pixel tests use the headless harness of
`crates/rune/src/render/backdrop_blur_tests.rs` (`Gpu::headless()`, `Renderer::new(.., OFFSCREEN_FORMAT)`,
`Offscreen`, `draw`, `read`).

1. **Types and API.** `el/liquid_glass.rs`: `LiquidGlass` (`regular`, `clear`, `identity`, `tint`, `interactive`,
   tuning builders), `El::liquid_glass`, `El::glass_container`, `El::glass_id`, `El::glass_union`; `GlassPolicy`,
   `App::glass`, `Tx::set_glass_policy`. The §0 value table lives in one `render/liquid_glass/apple.rs` so each
   number has one home and a source comment.
   *Tests:* builders clamp non-finite values; `identity()` paints nothing; the size-derived bevel, refraction and
   frost follow §0 for a 44 pt control and a 300 pt menu.
2. **Record, kind and shape table.** `Kind::LiquidGlass = 14`, `COUNT = 15`. Paint pushes one record per container,
   placed where the container's first member paints, with the members' shapes in a side table (rect, radii,
   variant, tint, press, glow, opacity). Glass draws behind every member's content. `NeedsGpu::LiquidGlass`,
   `damaged.rs` refusal.
   *Tests:* no cap (70 elements, 70 shapes); a container of 3 is one record with 3 shapes; a lone glass is a
   container of one; members' content paints after the record; ABI asserts.
3. **Capture and frost.** One capture per epoch (§2) and a down-only pyramid; σ from §0.
   *Tests:* frost line spread within 10% of a Gaussian at σ 3, 6, 10 pt; the shimmer gate (risk 1).
4. **Composite.** Lensing `D = Dmax(1 − d/b)²` toward the centre in the bevel band, dispersion ±3%, the two-lobe
   specular, the adaptive tone from coarse levels, clear's dimming, tint by brightness, the shadow in the same
   draw.
   *Tests:* a 1920×1080, scale 1 render of a capsule over a fine grid has no stair-stepping along the rim (the
   row-to-row change of the rim's sub-pixel position is monotone and under 0.5 px per row); a grid line near the rim
   is displaced inward; the centre equals the frost; regular over white is lighter than over black; clear over white
   is dimmed; frame 1 == frame 2 on a fresh renderer.
5. **Containers.** `smin` with `k = spacing` over the shape table, the SDF gradient as the lensing normal.
   *Tests:* two capsules 8 pt apart in a container with spacing 10 fill the gap's centre; with spacing 4 they don't;
   the merged rim has no seam.
6. **Hoisting.** Glass inside an alpha-only, cache, blur or blend layer moves its record to the frame pass at the
   layer's position and samples the real backdrop (Q3).
   *Tests:* a fading popover frosts the window; glass in `cache()` frosts what is behind the cache.
7. **Luminance probe and `Cx::glass_is_dark`.** A probe pass per epoch, a ring of three `MAP_READ` buffers mapped
   asynchronously, 150 ms smoothing with hysteresis.
   *Tests:* glass over white reads light and over black reads dark within a few frames; the frame never waits on a
   map (a counter of blocking polls stays 0).
8. **Interactive.** Press with the §0 spring and scale, stretch and follow, the touch glow from the pointer, hover.
   *Tests:* a press of interactive glass grows its shape to 1.10; release springs back with overshoot; the glow
   brightens the pointer side; non-interactive glass doesn't scale.
9. **Morph and materialize.** `glass_id` rides `El::matched` in its container's draw; unmatched glass materializes.
   *Tests:* a glass id moving between two containers flies; mid-flight it merges with neighbours.
10. **Accessibility.** Reduce Transparency, Increase Contrast and Reduce Motion as §0.
    *Tests:* each setting changes the record's parameters as listed; Reduce Motion stops the press overshoot.
11. **DX12 and first use.** DXBC through `regen_dxbc`, `first_use` entries.
12. **Bench.** GPU timestamps at 1080p for the Exo-like scene; `scripts/motion-audit` scenarios for press and morph;
    the changelog entry.
13. **Exo check** (in a scratch clone, not committed): regular for controls, clear over media and covers, the Bar
    in a container so its tabs merge; parity snaps before and after.

Upstream: the feature lives in `crates/rune/src/el/liquid_glass.rs`, `crates/rune/src/render/liquid_glass/`, two
WGSL files, the preferences addition and one changelog entry. It touches existing files only at the integration
points in §1, and each file stays under 800 lines. Exo-specific values stay in the presets' doc comments as "the
web launcher's tokens". This doc stays in the fork.
