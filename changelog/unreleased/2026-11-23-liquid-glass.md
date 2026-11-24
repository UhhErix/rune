- **`El::liquid_glass`, `LiquidGlass`, `El::glass_container`, `El::glass_group`, `El::glass_id`,
  `El::glass_union`, `GlassPolicy` (`FollowSystem`, `Glass`, `Auto`, `BlurOnly`, `Solid`),
  `GlassTier`, `El::modal_scrim`, `El::modal_scrim_blur`, and `rune_kit::Dialog::{scrim,
  scrim_blur, liquid_glass}` / `modal_scrim` add opt-in GPU Liquid Glass rendering and translucent
  modal scrims.** A shared per-epoch backdrop capture and 7-mip 13-tap Jimenez down-only pyramid
  reconstructs smooth cubic B-spline frost in Chromium sRGB blur space (`LiquidGlass::control` /
  `panel` / `pane` / `badge` / `immersive` and adaptive `regular` / `clear` / `identity`), paired
  with analytic bevel height-field normals, subtle `ratio^8` inward rim refraction, ±3% chromatic
  dispersion, backdrop saturation, `pow 6` Fresnel + cursor-leaning specular rims, `smin`
  container/union merging, interactive hover/press spring physics + touch-point glow, layer
  hoisting out of fade/cache/blur/blend ancestors, non-blocking luminance probes
  (`Cx::glass_is_dark`), precompiled DX12 DXBC shaders, live Windows `EnableTransparency` /
  Increase Contrast / Reduced Motion handling, and low-end GPU/battery auto-downgrade
  (`Full -> BlurOnly -> Solid`). Apps that do not call `El::liquid_glass` pay zero extra GPU work
  and `El::backdrop_blur` keeps its dynamic growth and configurable `max_backdrop_blurs`.
