//! Crisp QR code element builder (`qr_code`, `QrView`) for sign-in flows (Steam QR, Discord QR)
//! and shareable links, rendered as a pixel-sharp vector [`Shape`] with an ISO quiet zone.

use crate::color::Color;
use crate::el::{El, canvas};
use crate::image::{EcLevel, QrCode, QrError};
use crate::shape::{Path, Shape};

/// Encodes `data` into a crisp black-on-white QR code element sized `size_pt` square with a
/// 4-module quiet zone (`EcLevel::Medium`).
pub fn qr_code(data: impl AsRef<[u8]>, size_pt: f32) -> Result<El, QrError> {
    let code = QrCode::encode_with_ec(data, EcLevel::Medium)?;
    Ok(QrView::new(&code).size(size_pt).build())
}

/// Builder for rendering a [`QrCode`] into a crisp vector [`El`].
pub struct QrView<'a> {
    code: &'a QrCode,
    size: f32,
    quiet_zone: usize,
    dark: Color,
    light: Color,
    label: Option<String>,
}

impl<'a> QrView<'a> {
    /// Creates a QR view with default 160 pt side, 4-module quiet zone, black modules on white.
    pub fn new(code: &'a QrCode) -> Self {
        Self {
            code,
            size: 160.0,
            quiet_zone: 4,
            dark: Color::BLACK,
            light: Color::WHITE,
            label: None,
        }
    }

    /// Square display size in logical points.
    pub fn size(mut self, points: f32) -> Self {
        self.size = points.max(16.0);
        self
    }

    /// Quiet zone width in modules on every side (ISO recommends 4).
    pub fn quiet_zone(mut self, modules: usize) -> Self {
        self.quiet_zone = modules;
        self
    }

    /// Module foreground colour (default black).
    pub fn dark(mut self, color: Color) -> Self {
        self.dark = color;
        self
    }

    /// Quiet-zone and background colour (default white).
    pub fn light(mut self, color: Color) -> Self {
        self.light = color;
        self
    }

    /// Accessible image description (e.g. `"Steam sign-in code"`).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Builds the QR code [`El`] with a [`Shape`] view box sized to `modules + 2 * quiet_zone`.
    pub fn build(self) -> El {
        let n = self.code.size();
        let q = self.quiet_zone;
        let total = (n + 2 * q) as f32;
        let mut shape = Shape::new().view_box(total, total);
        for y in 0..n {
            // Merge horizontal runs of dark modules per row into single rectangles for compact geometry.
            let mut x = 0;
            while x < n {
                if !self.code.is_dark(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < n && self.code.is_dark(x, y) {
                    x += 1;
                }
                let (x0, x1) = ((start + q) as f32, (x + q) as f32);
                let (y0, y1) = ((y + q) as f32, (y + q + 1) as f32);
                let rect = Path::new()
                    .move_to([x0, y0])
                    .line_to([x1, y0])
                    .line_to([x1, y1])
                    .line_to([x0, y1])
                    .close();
                shape = shape.fill_path(&rect, self.dark);
            }
        }
        let mut out = canvas(shape).size(self.size, self.size).fill(self.light);
        if let Some(label) = self.label {
            out = out.label(label);
        }
        out
    }
}

#[cfg(test)]
#[path = "qr_tests.rs"]
mod tests;
