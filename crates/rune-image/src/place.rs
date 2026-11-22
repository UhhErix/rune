//! Alignment and aspect-preserving placement for surfaces and pixel buffers: which edge a spare
//! or cropped side is shared from, and whether the frame is scaled at all.

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceFit {
    #[default]
    Fit,
    Fill,
}

const fn align_share(a: Align) -> f32 {
    match a {
        Align::Start => 0.0,
        Align::Center => 0.5,
        Align::End => 1.0,
    }
}

fn clamp_share(v: f32) -> f32 {
    if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.5 }
}

/// `El::surface_anchor`, `El::surface_anchor_fraction` and `El::surface_unscaled`. The default
/// centres and scales by `SurfaceFit`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePlace {
    /// How much of the spare (or cropped) space along each axis lies before the image, in `0.0..=1.0`.
    pub anchor: [f32; 2],
    pub unscaled: bool,
}

impl SurfacePlace {
    pub const CENTRED: SurfacePlace = SurfacePlace { anchor: [0.5, 0.5], unscaled: false };

    /// Anchor along each axis by [`Align`].
    pub const fn align(horizontal: Align, vertical: Align) -> Self {
        Self { anchor: [align_share(horizontal), align_share(vertical)], unscaled: false }
    }

    /// Anchor along each axis by fraction in `0.0..=1.0` (CSS `object-position`); non-finite
    /// values fall back to `0.5`.
    pub fn fraction(horizontal: f32, vertical: f32) -> Self {
        Self { anchor: [clamp_share(horizontal), clamp_share(vertical)], unscaled: false }
    }

    /// Draw at one texel per device pixel (`El::surface_unscaled`).
    pub const fn unscaled(mut self, unscaled: bool) -> Self {
        self.unscaled = unscaled;
        self
    }

    /// How much of the spare (or cropped) space along each axis lies before the image, in `0.0..=1.0`.
    pub fn share(self) -> [f32; 2] {
        [clamp_share(self.anchor[0]), clamp_share(self.anchor[1])]
    }
}

impl Default for SurfacePlace {
    fn default() -> Self { Self::CENTRED }
}

/// The image's rect in device pixels inside `node` ([x, y, w, h]); it may overflow the node, which
/// then crops it. `unscaled` draws one texel per device pixel and ignores `fit`.
pub fn image_rect(node: [f32; 4], source: [u32; 2], fit: SurfaceFit, place: SurfacePlace) -> [f32; 4] {
    let (sw, sh) = (source[0] as f32, source[1] as f32);
    let scale = if place.unscaled {
        1.0
    } else {
        let (a, b) = (node[2] / sw, node[3] / sh);
        match fit {
            SurfaceFit::Fit => a.min(b),
            SurfaceFit::Fill => a.max(b),
        }
    };
    let [kx, ky] = place.share();
    let (w, h) = (sw * scale, sh * scale);
    let x = node[0] + (node[2] - w) * kx;
    let y = node[1] + (node[3] - h) * ky;
    // At unit scale favour sharp shared text over a subpixel translation of its image.
    if (scale - 1.0).abs() < 1e-6 { [x.round(), y.round(), w, h] } else { [x, y, w, h] }
}
