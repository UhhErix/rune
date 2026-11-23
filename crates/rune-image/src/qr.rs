//! Pure-Rust ISO/IEC 18004 QR code encoder (versions 1..=10, error correction levels L/M/Q/H,
//! byte mode, exact Reed-Solomon GF(256) and all 8 ISO mask patterns).

use crate::{ImageError, Pixels, encode_png};

/// QR error correction level (ISO/IEC 18004).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EcLevel {
    /// ~7% recovery capacity.
    Low,
    /// ~15% recovery capacity (recommended default for screen display).
    #[default]
    Medium,
    /// ~25% recovery capacity.
    Quartile,
    /// ~30% recovery capacity.
    High,
}

impl EcLevel {
    const fn format_bits(self) -> u32 {
        match self {
            Self::Low => 0b01,
            Self::Medium => 0b00,
            Self::Quartile => 0b11,
            Self::High => 0b10,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::Quartile => 2,
            Self::High => 3,
        }
    }
}

/// Errors produced by [`QrCode::encode`] and [`QrCode::encode_with_ec`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QrError {
    /// Payload exceeds the maximum capacity supported by versions 1..=10 at the chosen EC level.
    DataTooLong { bytes: usize, max_bytes: usize },
}

impl std::fmt::Display for QrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DataTooLong { bytes, max_bytes } => {
                write!(f, "QR payload is {bytes} bytes (max {max_bytes} bytes)")
            }
        }
    }
}

impl std::error::Error for QrError {}

/// `(ec_codewords_per_block, num_blocks)` for versions 1..=10 and `[Low, Medium, Quartile, High]`.
const EC_TABLE: [[(usize, usize); 4]; 10] = [
    [(7, 1), (10, 1), (13, 1), (17, 1)],  // V1  (26 total)
    [(10, 1), (16, 1), (22, 1), (28, 1)], // V2  (44 total)
    [(15, 1), (26, 1), (18, 2), (22, 2)], // V3  (70 total)
    [(20, 1), (18, 2), (26, 2), (16, 4)], // V4  (100 total)
    [(26, 1), (24, 2), (18, 4), (22, 4)], // V5  (134 total)
    [(18, 2), (16, 4), (24, 4), (28, 4)], // V6  (172 total)
    [(20, 2), (18, 4), (18, 6), (26, 5)], // V7  (196 total)
    [(24, 2), (22, 4), (22, 6), (26, 6)], // V8  (242 total)
    [(30, 2), (22, 5), (20, 8), (24, 8)], // V9  (292 total)
    [(18, 4), (26, 5), (24, 8), (28, 8)], // V10 (346 total)
];

/// Total codewords (data + EC) per version 1..=10.
const TOTAL_CODEWORDS: [usize; 10] = [26, 44, 70, 100, 134, 172, 196, 242, 292, 346];

/// Encoded QR code matrix (dark/light modules, without quiet zone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrCode {
    version: u8,
    size: usize,
    ec: EcLevel,
    mask: u8,
    modules: Vec<bool>,
}

impl QrCode {
    /// Encodes `data` in byte mode using [`EcLevel::Medium`].
    pub fn encode(data: impl AsRef<[u8]>) -> Result<Self, QrError> {
        Self::encode_with_ec(data, EcLevel::Medium)
    }

    /// Encodes `data` in byte mode using the specified [`EcLevel`].
    pub fn encode_with_ec(data: impl AsRef<[u8]>, ec: EcLevel) -> Result<Self, QrError> {
        let bytes = data.as_ref();
        let (version, data_cap) = pick_version(bytes.len(), ec)?;
        let codewords = build_codewords(bytes, version, ec, data_cap);

        let size = 17 + 4 * usize::from(version);
        let mut base = Grid::new(size);
        draw_function_patterns(&mut base, version);
        place_codewords(&mut base, &codewords);

        let mut best_mask = 0u8;
        let mut best_score = u32::MAX;
        let mut best_modules = Vec::new();
        for mask in 0..8u8 {
            let mut candidate = base.clone();
            apply_mask(&mut candidate, mask);
            draw_format_bits(&mut candidate, ec, mask);
            if version >= 7 {
                draw_version_bits(&mut candidate, version);
            }
            let score = penalty_score(&candidate);
            if score < best_score {
                best_score = score;
                best_mask = mask;
                best_modules = candidate.modules;
            }
        }

        Ok(Self {
            version,
            size,
            ec,
            mask: best_mask,
            modules: best_modules,
        })
    }

    /// QR version (`1..=10`).
    pub const fn version(&self) -> u8 {
        self.version
    }

    /// Error correction level used to encode this symbol.
    pub const fn ec_level(&self) -> EcLevel {
        self.ec
    }

    /// Mask pattern selected (`0..=7`).
    pub const fn mask(&self) -> u8 {
        self.mask
    }

    /// Width and height of the symbol in modules (excluding quiet zone).
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Whether module `(x, y)` is dark (`false` outside bounds).
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        if x < self.size && y < self.size {
            self.modules[y * self.size + x]
        } else {
            false
        }
    }

    /// Rasterizes the QR code to straight sRGB RGBA8 with `module_px` pixels per module and a
    /// `quiet_modules` border on every side (ISO recommends 4 modules).
    pub fn to_rgba(
        &self,
        module_px: u32,
        quiet_modules: u32,
        dark: [u8; 4],
        light: [u8; 4],
    ) -> (u32, u32, Vec<u8>) {
        let scale = module_px.max(1);
        let total_mods = self.size as u32 + quiet_modules.saturating_mul(2);
        let side = total_mods.saturating_mul(scale);
        let mut rgba = Vec::with_capacity((side as usize) * (side as usize) * 4);
        for py in 0..side {
            let my = py / scale;
            let in_y = my >= quiet_modules && (my - quiet_modules) < self.size as u32;
            let row_y = my.wrapping_sub(quiet_modules) as usize;
            for px in 0..side {
                let mx = px / scale;
                let on = in_y
                    && mx >= quiet_modules
                    && (mx - quiet_modules) < self.size as u32
                    && self.is_dark((mx - quiet_modules) as usize, row_y);
                rgba.extend_from_slice(if on { &dark } else { &light });
            }
        }
        (side, side, rgba)
    }

    /// Rasterizes the QR code into a [`Pixels`] buffer ready for `el().pixels(...)`.
    pub fn to_pixels(
        &self,
        module_px: u32,
        quiet_modules: u32,
        dark: [u8; 4],
        light: [u8; 4],
    ) -> Pixels {
        let (w, h, rgba) = self.to_rgba(module_px, quiet_modules, dark, light);
        let pixels = Pixels::new();
        pixels.set(w, h, rgba);
        pixels
    }

    /// Encodes the QR code as crisp black-on-white PNG bytes.
    pub fn to_png(&self, module_px: u32, quiet_modules: u32) -> Result<Vec<u8>, ImageError> {
        let (w, h, rgba) = self.to_rgba(
            module_px,
            quiet_modules,
            [0, 0, 0, 255],
            [255, 255, 255, 255],
        );
        encode_png(w, h, &rgba)
    }
}

fn data_capacity_bytes(version: u8, ec: EcLevel) -> usize {
    let idx = usize::from(version - 1);
    let total = TOTAL_CODEWORDS[idx];
    let (ec_per_block, blocks) = EC_TABLE[idx][ec.index()];
    total - ec_per_block * blocks
}

fn max_byte_payload(version: u8, ec: EcLevel) -> usize {
    let cap_bits = data_capacity_bytes(version, ec) * 8;
    // 4-bit mode indicator (0100 = Byte) + character count indicator (8 bits V1..9, 16 bits V10+).
    let header_bits = 4 + if version < 10 { 8 } else { 16 };
    cap_bits.saturating_sub(header_bits) / 8
}

fn pick_version(len: usize, ec: EcLevel) -> Result<(u8, usize), QrError> {
    for v in 1..=10u8 {
        if len <= max_byte_payload(v, ec) {
            return Ok((v, data_capacity_bytes(v, ec)));
        }
    }
    Err(QrError::DataTooLong {
        bytes: len,
        max_bytes: max_byte_payload(10, ec),
    })
}

fn build_codewords(bytes: &[u8], version: u8, ec: EcLevel, data_cap: usize) -> Vec<u8> {
    let mut bits = BitBuf::new();
    // Byte mode indicator: 0100.
    bits.push(0b0100, 4);
    let count_bits = if version < 10 { 8 } else { 16 };
    bits.push(bytes.len() as u32, count_bits);
    for &b in bytes {
        bits.push(u32::from(b), 8);
    }
    let cap_bits = data_cap * 8;
    let terminator = (cap_bits - bits.len()).min(4);
    bits.push(0, terminator);
    let pad_bits = (8 - (bits.len() % 8)) % 8;
    bits.push(0, pad_bits);
    let mut data = bits.into_bytes();
    let pad_bytes = [0xECu8, 0x11u8];
    let mut pad_i = 0;
    while data.len() < data_cap {
        data.push(pad_bytes[pad_i & 1]);
        pad_i += 1;
    }

    let idx = usize::from(version - 1);
    let total = TOTAL_CODEWORDS[idx];
    let (ec_len, num_blocks) = EC_TABLE[idx][ec.index()];
    let short_blocks = num_blocks - (total % num_blocks);
    let short_data_len = total / num_blocks - ec_len;
    let generator = rs_generator(ec_len);

    let mut data_blocks = Vec::with_capacity(num_blocks);
    let mut ec_blocks = Vec::with_capacity(num_blocks);
    let mut offset = 0;
    for b in 0..num_blocks {
        let len = short_data_len + usize::from(b >= short_blocks);
        let block = &data[offset..offset + len];
        offset += len;
        ec_blocks.push(rs_remainder(block, &generator));
        data_blocks.push(block);
    }

    let mut out = Vec::with_capacity(total);
    let max_data = short_data_len + usize::from(short_blocks < num_blocks);
    for i in 0..max_data {
        for block in &data_blocks {
            if i < block.len() {
                out.push(block[i]);
            }
        }
    }
    for i in 0..ec_len {
        for block in &ec_blocks {
            out.push(block[i]);
        }
    }
    out
}

struct BitBuf {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitBuf {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_len: 0,
        }
    }

    fn len(&self) -> usize {
        self.bit_len
    }

    fn push(&mut self, val: u32, count: usize) {
        for i in (0..count).rev() {
            if self.bit_len.is_multiple_of(8) {
                self.bytes.push(0);
            }
            if ((val >> i) & 1) != 0
                && let Some(last) = self.bytes.last_mut()
            {
                *last |= 1 << (7 - (self.bit_len % 8));
            }
            self.bit_len += 1;
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// GF(256) multiplication modulo primitive polynomial `x^8 + x^4 + x^3 + x^2 + 1` (`0x11D`).
pub(crate) fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut out = 0u8;
    for _ in 0..8 {
        if (b & 1) != 0 {
            out ^= a;
        }
        let hi = (a & 0x80) != 0;
        a <<= 1;
        if hi {
            a ^= 0x1D;
        }
        b >>= 1;
    }
    out
}

/// Reed-Solomon generator polynomial coefficients (excluding leading 1) for `degree` EC bytes.
fn rs_generator(degree: usize) -> Vec<u8> {
    let mut poly = vec![0u8; degree];
    poly[degree - 1] = 1;
    let mut root = 1u8;
    for _ in 0..degree {
        for j in 0..degree {
            poly[j] = gf_mul(poly[j], root);
            if j + 1 < degree {
                poly[j] ^= poly[j + 1];
            }
        }
        root = gf_mul(root, 0x02);
    }
    poly
}

fn rs_remainder(data: &[u8], generator: &[u8]) -> Vec<u8> {
    let mut rem = vec![0u8; generator.len()];
    for &b in data {
        let factor = b ^ rem[0];
        rem.rotate_left(1);
        if let Some(last) = rem.last_mut() {
            *last = 0;
        }
        for (r, &g) in rem.iter_mut().zip(generator.iter()) {
            *r ^= gf_mul(g, factor);
        }
    }
    rem
}

#[derive(Clone)]
struct Grid {
    size: usize,
    modules: Vec<bool>,
    is_function: Vec<bool>,
}

impl Grid {
    fn new(size: usize) -> Self {
        Self {
            size,
            modules: vec![false; size * size],
            is_function: vec![false; size * size],
        }
    }

    fn set_func(&mut self, x: usize, y: usize, dark: bool) {
        let idx = y * self.size + x;
        self.modules[idx] = dark;
        self.is_function[idx] = true;
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

fn alignment_centers(version: u8) -> Vec<usize> {
    if version == 1 {
        return Vec::new();
    }
    let size = 17 + 4 * usize::from(version);
    let num = usize::from(version) / 7 + 2;
    let step = ((usize::from(version) * 4 + num * 2 + 1) / (num * 2 - 2)) * 2;
    let mut coords = vec![6];
    let mut pos = size - 7;
    let mut tail = Vec::new();
    while tail.len() + 1 < num {
        tail.push(pos);
        pos = pos.saturating_sub(step);
    }
    tail.reverse();
    coords.extend(tail);
    coords
}

fn draw_function_patterns(g: &mut Grid, version: u8) {
    let s = g.size;
    // Timing patterns.
    for i in 0..s {
        g.set_func(6, i, i % 2 == 0);
        g.set_func(i, 6, i % 2 == 0);
    }
    // Three 7x7 finder patterns + 1-module light separators.
    for &(fx, fy) in &[(3isize, 3isize), (s as isize - 4, 3), (3, s as isize - 4)] {
        for dy in -4..=4isize {
            for dx in -4..=4isize {
                let (x, y) = (fx + dx, fy + dy);
                if x >= 0 && y >= 0 && (x as usize) < s && (y as usize) < s {
                    let dist = dx.abs().max(dy.abs());
                    let dark = dist != 2 && dist != 4;
                    g.set_func(x as usize, y as usize, dark);
                }
            }
        }
    }
    // Alignment patterns.
    let centers = alignment_centers(version);
    let n = centers.len();
    for (iy, &cy) in centers.iter().enumerate() {
        for (ix, &cx) in centers.iter().enumerate() {
            if (ix == 0 && iy == 0) || (ix == 0 && iy + 1 == n) || (ix + 1 == n && iy == 0) {
                continue;
            }
            for dy in -2..=2isize {
                for dx in -2..=2isize {
                    let dist = dx.abs().max(dy.abs());
                    g.set_func(
                        (cx as isize + dx) as usize,
                        (cy as isize + dy) as usize,
                        dist != 1,
                    );
                }
            }
        }
    }
    // Reserve format info (drawn with real mask bits later) + fixed dark module.
    draw_format_bits(g, EcLevel::Low, 0);
    if version >= 7 {
        draw_version_bits(g, version);
    }
}

fn draw_format_bits(g: &mut Grid, ec: EcLevel, mask: u8) {
    let s = g.size;
    let data = (ec.format_bits() << 3) | u32::from(mask & 7);
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    let bits = ((data << 10) | rem) ^ 0x5412;

    // First copy around top-left finder.
    for i in 0..=5 {
        g.set_func(8, i, ((bits >> i) & 1) != 0);
    }
    g.set_func(8, 7, ((bits >> 6) & 1) != 0);
    g.set_func(8, 8, ((bits >> 7) & 1) != 0);
    g.set_func(7, 8, ((bits >> 8) & 1) != 0);
    for i in 9..15 {
        g.set_func(14 - i, 8, ((bits >> i) & 1) != 0);
    }

    // Second copy beside bottom-left and top-right finders.
    for i in 0..8 {
        g.set_func(s - 1 - i, 8, ((bits >> i) & 1) != 0);
    }
    for i in 8..15 {
        g.set_func(8, s - 15 + i, ((bits >> i) & 1) != 0);
    }
    g.set_func(8, s - 8, true); // Dark module.
}

fn draw_version_bits(g: &mut Grid, version: u8) {
    let s = g.size;
    let mut rem = u32::from(version);
    for _ in 0..12 {
        rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
    }
    let bits = (u32::from(version) << 12) | rem;
    for i in 0..18 {
        let dark = ((bits >> i) & 1) != 0;
        let a = s - 11 + (i % 3);
        let b = i / 3;
        g.set_func(a, b, dark);
        g.set_func(b, a, dark);
    }
}

fn place_codewords(g: &mut Grid, codewords: &[u8]) {
    let s = g.size;
    let mut bit_idx = 0usize;
    let total_bits = codewords.len() * 8;
    let mut right = s as isize - 1;
    let mut upward = true;
    while right >= 1 {
        if right == 6 {
            right -= 1;
        }
        for step in 0..s {
            let y = if upward { s - 1 - step } else { step };
            for dx in 0..2isize {
                let x = (right - dx) as usize;
                let idx = y * s + x;
                if !g.is_function[idx] {
                    let dark = if bit_idx < total_bits {
                        ((codewords[bit_idx / 8] >> (7 - (bit_idx % 8))) & 1) != 0
                    } else {
                        false
                    };
                    g.modules[idx] = dark;
                    bit_idx += 1;
                }
            }
        }
        right -= 2;
        upward = !upward;
    }
}

fn apply_mask(g: &mut Grid, mask: u8) {
    let s = g.size;
    for y in 0..s {
        for x in 0..s {
            let idx = y * s + x;
            if g.is_function[idx] {
                continue;
            }
            let invert = match mask {
                0 => (x + y) % 2 == 0,
                1 => y % 2 == 0,
                2 => x % 3 == 0,
                3 => (x + y) % 3 == 0,
                4 => (x / 3 + y / 2) % 2 == 0,
                5 => (x * y) % 2 + (x * y) % 3 == 0,
                6 => ((x * y) % 2 + (x * y) % 3) % 2 == 0,
                7 => ((x + y) % 2 + (x * y) % 3) % 2 == 0,
                _ => false,
            };
            if invert {
                g.modules[idx] = !g.modules[idx];
            }
        }
    }
}

fn penalty_score(g: &Grid) -> u32 {
    let s = g.size;
    let mut score = 0u32;

    // Rule 1: runs of 5+ same-coloured modules in rows and columns.
    for y in 0..s {
        let (mut run_color, mut run_len) = (g.get(0, y), 1u32);
        for x in 1..s {
            let c = g.get(x, y);
            if c == run_color {
                run_len += 1;
                if run_len == 5 {
                    score += 3;
                } else if run_len > 5 {
                    score += 1;
                }
            } else {
                run_color = c;
                run_len = 1;
            }
        }
    }
    for x in 0..s {
        let (mut run_color, mut run_len) = (g.get(x, 0), 1u32);
        for y in 1..s {
            let c = g.get(x, y);
            if c == run_color {
                run_len += 1;
                if run_len == 5 {
                    score += 3;
                } else if run_len > 5 {
                    score += 1;
                }
            } else {
                run_color = c;
                run_len = 1;
            }
        }
    }

    // Rule 2: 2x2 blocks of identical colour.
    for y in 0..s - 1 {
        for x in 0..s - 1 {
            let c = g.get(x, y);
            if c == g.get(x + 1, y) && c == g.get(x, y + 1) && c == g.get(x + 1, y + 1) {
                score += 3;
            }
        }
    }

    // Rule 3: finder-like 1:1:3:1:1 patterns flanked by 4 light modules.
    let pat_a = [
        true, false, true, true, true, false, true, false, false, false, false,
    ];
    let pat_b = [
        false, false, false, false, true, false, true, true, true, false, true,
    ];
    for y in 0..s {
        for x in 0..=s - 11 {
            if (0..11).all(|k| g.get(x + k, y) == pat_a[k])
                || (0..11).all(|k| g.get(x + k, y) == pat_b[k])
            {
                score += 40;
            }
        }
    }
    for x in 0..s {
        for y in 0..=s - 11 {
            if (0..11).all(|k| g.get(x, y + k) == pat_a[k])
                || (0..11).all(|k| g.get(x, y + k) == pat_b[k])
            {
                score += 40;
            }
        }
    }

    // Rule 4: dark/light balance deviation from 50%.
    let dark = g.modules.iter().filter(|&&m| m).count() as i32;
    let total = (s * s) as i32;
    let k = ((dark * 20 - total * 10).abs() + total - 1) / total;
    score += (k.saturating_sub(1) as u32) * 10;
    score
}

#[cfg(test)]
#[path = "qr_tests.rs"]
mod tests;
