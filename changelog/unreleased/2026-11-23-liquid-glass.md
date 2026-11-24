- **`El::liquid_glass`, `LiquidGlass`, `El::glass_container`, `El::glass_group`, `El::glass_id`,
  `El::glass_union`, and `GlassPolicy` add opt-in GPU Liquid Glass rendering.** A shared
  per-epoch backdrop capture and 7-mip 13-tap Jimenez down-only pyramid reconstructs smooth
  cubic B-spline frost in Chromium sRGB blur space (`LiquidGlass::control` / `panel` / `pane` /
  `badge` / `immersive` and adaptive `regular` / `clear` / `identity`), paired with analytic
  bevel height-field normals, subtle `ratio^8` inward rim refraction, ±3% chromatic dispersion,
  backdrop saturation, `pow 6` Fresnel + cursor-leaning specular rims, `smin` container merging,
  interactive hover/press spring physics, layer hoisting out of fade/cache/blur/blend ancestors,
  non-blocking luminance probes (`Cx::glass_is_dark`), precompiled DX12 DXBC shaders, and
  system Reduce Transparency / Increase Contrast handling (`App::glass`, `Tx::set_glass_policy`).
  Apps that do not call `El::liquid_glass` pay zero extra GPU work and `El::backdrop_blur`
  keeps its default 4-blur cap and configurable `max_backdrop_blurs`.
