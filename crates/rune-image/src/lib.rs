//! Content-addressed image decoding, linear-light mip generation, dynamic pixel buffers
//! (`pixels`), SVG and AVIF codecs, OS app icon rasterization and surface placement math. Decode
//! and mip generation run off the UI thread on shared decoder workers, never in a view or a render
//! frame. Apps normally reach this crate through `rune` (`rune::image`, `rune::Pixels`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
#[cfg(all(test, not(target_arch = "wasm32")))]
use std::sync::mpsc;

pub(crate) mod animated;
mod app_icon;
mod avif;
mod from_url;
#[cfg(feature = "svg")]
mod gzip;
#[cfg_attr(not(any(test, target_arch = "wasm32", feature = "svg")), allow(dead_code))]
mod header;
#[cfg(test)]
mod mip_tests;
mod mips;
#[doc(hidden)]
pub mod pixels;
#[doc(hidden)]
pub mod place;
pub mod qr;
mod svg;
#[cfg(test)]
mod tests;

pub use app_icon::app_icon_rgba;
#[cfg(not(target_arch = "wasm32"))]
#[doc(hidden)]
pub use from_url::font_file;
pub use pixels::Pixels;
pub use place::Align;
pub use place::SurfaceFit;
pub use qr::EcLevel;
pub use qr::QrCode;
pub use qr::QrError;
#[doc(hidden)]
pub use svg::svg_rgba;
pub use svg::svg_rgba_cover;

#[cfg(test)]
use mips::byte;
use mips::mip_chain;

/// Largest image side Rune decodes or accepts on native and web: 8192 pixels, so 8-bit RGBA
/// stays within [`MAX_ALLOC`] before mips.
pub(crate) const MAX_SIDE: u32 = 8192;
/// Largest decoded raster buffer Rune allocates: 256 MiB.
pub(crate) const MAX_ALLOC: u64 = 256 * 1024 * 1024;

pub type ImageId = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageError(pub String);

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
}
impl std::error::Error for ImageError {}

/// Pixels are sRGB-encoded *linear-premultiplied* RGB plus linear alpha. Sampling with
/// Rgba8UnormSrgb yields linear-premultiplied RGBA, including between mip levels.
pub(crate) struct Mip {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
pub(crate) struct Decoded {
    /// The image, or an animation's first frame.
    pub mips: Vec<Mip>,
    /// An animation's other frames and timing.
    pub motion: Option<Box<animated::Motion>>,
}
impl Decoded {
    pub(crate) fn decoded_bytes(&self) -> usize {
        let base: usize = self.mips.iter().map(|m| m.pixels.len()).sum();
        let motion: usize = self.motion.as_ref().map_or(0, |m| {
            m.frames.iter().flat_map(|f| f.iter()).map(|mip| mip.pixels.len()).sum()
        });
        base.saturating_add(motion)
    }
}
struct State {
    id: ImageId,
    pixels: Pixels,
    decoded: OnceLock<Result<Arc<Decoded>, ImageError>>,
}

/// A cheap shared handle. Pending images paint nothing until their worker wakes the app.
#[derive(Clone)]
pub struct Image(Arc<State>, Pixels);
impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image").field("id", &self.id()).field("size", &self.size()).finish()
    }
}
impl Image {
    /// A handle on `state`, whose pixels keep it in the cache while drawn.
    fn new(state: Arc<State>) -> Image {
        let pixels = state.pixels.clone().keeping(state.clone());
        Image(state, pixels)
    }

    /// The shared static pixel handle, ready to pass to `El::pixels` after decoding. While
    /// any clone of it lives (an element drawing it), the cache hands back this image.
    pub fn pixels(&self) -> &Pixels { &self.1 }
    pub fn id(&self) -> ImageId { self.0.id }
    pub fn size(&self) -> Option<[u32; 2]> {
        let mip = self.decoded()?.ok()?.mips.first()?;
        Some([mip.width, mip.height])
    }
    /// Exact decoded pixel bytes held by this image across all mip levels and animation frames,
    /// or `0` while pending or on decode error.
    pub fn decoded_bytes(&self) -> usize {
        self.decoded().and_then(Result::ok).map_or(0, Decoded::decoded_bytes)
    }
    pub fn is_ready(&self) -> bool { self.0.decoded.get().is_some() }
    pub fn error(&self) -> Option<&ImageError> { self.0.decoded.get()?.as_ref().err() }
    /// Wait for fixture assets before advancing a deterministic headless scene. Interactive
    /// apps use the cache's waker instead; decode still happens on the cache worker.
    pub fn wait(&self) -> Result<[u32; 2], &ImageError> {
        #[cfg(target_arch = "wasm32")]
        let decoded = self.0.decoded.get().expect("Image::wait cannot block on web; use is_ready and the cache waker");
        #[cfg(not(target_arch = "wasm32"))]
        let decoded = self.0.decoded.wait();
        let mip = &decoded.as_ref()?.mips[0];
        Ok([mip.width, mip.height])
    }
    pub(crate) fn decoded(&self) -> Option<Result<&Decoded, &ImageError>> {
        self.0.decoded.get().map(|r| r.as_ref().map(Arc::as_ref))
    }
    // Pixels are stored before `decoded` is published (so `Image::wait` and `is_ready` never see
    // empty pixels), and windows wake after `decoded` is set (so a woken frame sees `size`).
    fn finish(&self, decoded: Result<Decoded, ImageError>) {
        let decoded = decoded.map(Arc::new);
        let has_mips = if let Ok(mips) = &decoded {
            self.0.pixels.store_mips(mips.clone());
            true
        } else {
            false
        };
        if self.0.decoded.set(decoded).is_ok() && has_mips {
            self.0.pixels.source.notify();
        }
    }
}

enum Input {
    Encoded(Arc<[u8]>),
    EncodedCapped(Arc<[u8]>, u32),
    Rgba(u32, u32, Arc<[u8]>),
    Icon(#[cfg_attr(target_arch = "wasm32", allow(dead_code))] std::path::PathBuf, #[cfg_attr(target_arch = "wasm32", allow(dead_code))] u32),
    Svg(Arc<[u8]>, u32),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf, u32),
    #[cfg(not(target_arch = "wasm32"))]
    FileCapped(std::path::PathBuf, u32),
    /// `ImageCache::url`, and `ImageCache::file` on web.
    Url(String, u32),
    /// A decoder that panics, standing in for a codec bug.
    #[cfg(test)]
    Panic,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    Gated(Box<Input>, mpsc::Receiver<()>),
}

impl Input {
    fn decode(self) -> Result<Decoded, ImageError> {
        match self {
            Input::Encoded(bytes) => decode(&bytes),
            Input::EncodedCapped(bytes, px) => decode_capped(&bytes, px),
            Input::Rgba(w, h, bytes) => mip_chain(w, h, &bytes),
            Input::Svg(bytes, px) => svg_rgba(&bytes, px).and_then(|(w, h, bytes)| mip_chain(w, h, &bytes)),
            #[cfg(not(target_arch = "wasm32"))]
            Input::Icon(path, px) => app_icon_rgba(&path, px).and_then(|(w, h, bytes)| mip_chain(w, h, &bytes)),
            #[cfg(not(target_arch = "wasm32"))]
            Input::File(path, px) => {
                let bytes = read_regular_file(&path)?;
                if has_svg_extension(&path) || is_svg(&bytes) {
                    svg_rgba(&bytes, px).and_then(|(w, h, bytes)| mip_chain(w, h, &bytes))
                } else {
                    decode(&bytes)
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Input::FileCapped(path, px) => {
                let bytes = std::fs::read(&path).map_err(|e| ImageError(format!("{}: {e}", path.display())))?;
                if has_svg_extension(&path) || is_svg(&bytes) {
                    svg_rgba(&bytes, px).and_then(|(w, h, bytes)| mip_chain(w, h, &bytes))
                } else {
                    decode_capped(&bytes, px)
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Input::Url(url, px) => Input::File(from_url::file_path(&url)?, px).decode(),
            #[cfg(target_arch = "wasm32")]
            Input::Icon(..) => Err(ImageError("filesystem images are unsupported on web".into())),
            // Fetched asynchronously by `ImageCache::request`.
            #[cfg(target_arch = "wasm32")]
            Input::Url(url, _) => Err(ImageError(format!("{url}: fetched, not decoded"))),
            #[cfg(test)]
            Input::Panic => panic!("decoder bug"),
            #[cfg(all(test, not(target_arch = "wasm32")))]
            Input::Gated(input, release) => {
                release.recv().map_err(|_| ImageError("test gate dropped".into()))?;
                input.decode()
            }
        }
    }

    /// `decode`, with a panic in a codec (a malformed file can still trip one) failing this
    /// image instead of unwinding the worker, which would leave it pending and fail every later one.
    #[cfg(not(target_arch = "wasm32"))]
    fn decode_caught(self) -> Result<Decoded, ImageError> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.decode())).unwrap_or_else(|panic| {
            let why = panic.downcast_ref::<&str>().copied().or_else(|| panic.downcast_ref::<String>().map(String::as_str));
            Err(ImageError(format!("image decoder panicked: {}", why.unwrap_or("no message"))))
        })
    }
}
#[cfg(not(target_arch = "wasm32"))]
struct Job { image: Image, input: Input, waker: rune_core::Waker, cache: Weak<Mutex<CacheInner>> }
/// Decoder threads for the process, at most one per core.
#[cfg(not(target_arch = "wasm32"))]
const DECODERS: usize = 4;
/// Straight RGBA images up to this many pixels (64 x 64) finish on the calling thread.
#[cfg(not(target_arch = "wasm32"))]
const INLINE_RGBA_PIXELS: u64 = 64 * 64;

/// The images waiting to decode, shared by every cache, and how many decoders are draining them.
/// A decoder is a task on Rune's shared threads (`task::spawn`) that decodes until none is left:
/// a session opening on several screenshots decodes them side by side, and with nothing to
/// decode no thread stays for it (each kept decoder thread held its stack and its allocator
/// pages for the process's life). Task threads reserve the stack `MAX_SVG_DEPTH` needs
/// (`rune_core::task`'s `STACK`).
#[cfg(not(target_arch = "wasm32"))]
static DECODING: Mutex<(std::collections::VecDeque<Job>, usize)> = Mutex::new((std::collections::VecDeque::new(), 0));

/// Queues `job`, and starts a decoder for it while fewer than one per core (at most
/// [`DECODERS`]) run. False when no decoder runs and none could start.
#[cfg(not(target_arch = "wasm32"))]
fn decode_later(job: Job) -> bool {
    let mut d = DECODING.lock().unwrap_or_else(|e| e.into_inner());
    d.0.push_back(job);
    let most = DECODERS.min(std::thread::available_parallelism().map_or(1, |n| n.get()));
    if d.1 >= most {
        return true;
    }
    d.1 += 1;
    drop(d);
    if rune_core::task::spawn("rune-images", decode_jobs).is_ok() {
        return true;
    }
    let mut d = DECODING.lock().unwrap_or_else(|e| e.into_inner());
    d.1 -= 1;
    if d.1 > 0 {
        return true;
    }
    d.0.pop_back();
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_jobs() {
    loop {
        let job = {
            let mut d = DECODING.lock().unwrap_or_else(|e| e.into_inner());
            let Some(job) = d.0.pop_front() else {
                d.1 -= 1;
                return;
            };
            job
        };
        job.image.finish(job.input.decode_caught());
        if let Some(inner) = job.cache.upgrade() {
            let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
            guard.on_decoded(job.image.id(), &job.image);
        }
        (job.waker)();
    }
}

struct RetainedEntry {
    _state: Arc<State>,
    bytes: usize,
    last_used: u64,
}

struct CacheInner {
    entries: HashMap<ImageId, Weak<State>>,
    retained: HashMap<ImageId, RetainedEntry>,
    retained_bytes: usize,
    max_bytes: usize,
    tick: u64,
}

impl CacheInner {
    fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            retained: HashMap::new(),
            retained_bytes: 0,
            max_bytes,
            tick: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    fn retain_and_touch(&mut self, state: &Arc<State>) {
        if self.max_bytes == 0 {
            return;
        }
        if state.decoded.get().is_some_and(Result::is_err) {
            return;
        }
        let bytes = state
            .decoded
            .get()
            .and_then(|r| r.as_ref().ok())
            .map_or(0, |d| d.decoded_bytes());
        if bytes > self.max_bytes {
            self.remove_retained(&state.id);
            return;
        }
        let tick = self.next_tick();
        if let Some(entry) = self.retained.get_mut(&state.id) {
            self.retained_bytes = self.retained_bytes.saturating_sub(entry.bytes).saturating_add(bytes);
            entry.bytes = bytes;
            entry.last_used = tick;
        } else {
            self.retained_bytes = self.retained_bytes.saturating_add(bytes);
            self.retained.insert(state.id, RetainedEntry { _state: Arc::clone(state), bytes, last_used: tick });
        }
        self.evict_to_budget(Some(state.id));
    }

    fn on_decoded(&mut self, id: ImageId, image: &Image) {
        if self.max_bytes == 0 {
            return;
        }
        if image.error().is_some() {
            self.remove_retained(&id);
            return;
        }
        let bytes = image.decoded_bytes();
        if bytes > self.max_bytes {
            self.remove_retained(&id);
            return;
        }
        if let Some(entry) = self.retained.get_mut(&id) {
            self.retained_bytes = self.retained_bytes.saturating_sub(entry.bytes).saturating_add(bytes);
            entry.bytes = bytes;
        }
        self.evict_to_budget(None);
    }

    fn remove_retained(&mut self, id: &ImageId) {
        if let Some(old) = self.retained.remove(id) {
            self.retained_bytes = self.retained_bytes.saturating_sub(old.bytes);
        }
    }

    fn evict_to_budget(&mut self, protect: Option<ImageId>) {
        if self.max_bytes == 0 {
            self.retained.clear();
            self.retained_bytes = 0;
            return;
        }
        while self.retained_bytes > self.max_bytes && !self.retained.is_empty() {
            let victim = self
                .retained
                .iter()
                .filter(|(id, _)| protect.is_none_or(|p| **id != p) || self.retained.len() == 1)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(id, _)| *id);
            let Some(id) = victim else { break };
            self.remove_retained(&id);
        }
        self.entries.retain(|_, value| value.strong_count() > 0);
    }

    fn evict_all(&mut self) {
        self.retained.clear();
        self.retained_bytes = 0;
        self.entries.retain(|_, value| value.strong_count() > 0);
    }
}

/// Construct once during app setup. The cache deduplicates by content, not a path or pointer;
/// dropping all handles releases decoded pixels unless a byte budget retains recently used
/// entries (see [`ImageCache::with_budget`]). Its images decode on the process's decoder
/// threads, which every cache shares. The waker should be the one supplied by `AppSpec::setup`.
pub struct ImageCache {
    inner: Arc<Mutex<CacheInner>>,
    #[cfg(not(target_arch = "wasm32"))]
    waker: rune_core::Waker,
    #[cfg(target_arch = "wasm32")]
    wake: rune_core::Waker,
}
impl ImageCache {
    #[cfg(target_arch = "wasm32")]
    pub fn new(wake: rune_core::Waker) -> Self {
        Self::with_budget(wake, 0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(waker: rune_core::Waker) -> Self {
        Self::with_budget(waker, 0)
    }

    /// Construct a cache that retains recently used decoded images up to `max_bytes` (evicting
    /// least-recently-used entries once their decoded pixel bytes exceed `max_bytes`). Pass `0`
    /// to disable strong retention (matching [`ImageCache::new`]).
    #[cfg(target_arch = "wasm32")]
    pub fn with_budget(wake: rune_core::Waker, max_bytes: usize) -> Self {
        Self { inner: Arc::new(Mutex::new(CacheInner::new(max_bytes))), wake }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_budget(waker: rune_core::Waker, max_bytes: usize) -> Self {
        Self { inner: Arc::new(Mutex::new(CacheInner::new(max_bytes))), waker }
    }

    /// Update the decoded-byte LRU budget and immediately evict oldest entries past `max_bytes`.
    pub fn set_budget(&self, max_bytes: usize) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.max_bytes = max_bytes;
        inner.evict_to_budget(None);
    }

    /// Current LRU byte budget (`0` means no strong retention).
    pub fn budget(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).max_bytes
    }

    /// Total decoded pixel bytes currently retained by the cache's LRU table.
    pub fn retained_bytes(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).retained_bytes
    }

    /// Number of entries currently retained by the cache's LRU table.
    pub fn retained_count(&self) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).retained.len()
    }

    /// Mark `image` as most-recently-used in the cache's LRU table.
    pub fn touch(&self, image: &Image) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.retain_and_touch(&image.0);
    }

    /// Release all strong handles held by the cache's LRU table. Images still drawn on screen
    /// stay alive through their `Pixels`; off-screen images are freed immediately.
    pub fn evict_all(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.evict_all();
    }

    /// PNG (APNG animates), JPEG, GIF and WebP (both animate), BMP, ICO, AVIF (with the `avif`
    /// feature; its first frame), modern ICNS containing a PNG representation, or SVG (plain or
    /// gzipped SVGZ, recognised by its content). An SVG rasterizes at twice its own size; use
    /// [`ImageCache::svg`] to rasterize it at the size it draws at. Legacy ICNS codecs return an
    /// explicit error; callers can provide platform-decoded RGBA instead. An animation decodes
    /// every frame on the worker, and plays only while an element draws it in a shown window.
    /// On web the browser decodes everything but PNG, SVG and ICNS, matching the raster formats
    /// Chrome decodes (including animated AVIF and JPEG XL), so GIF, WebP, animated AVIF and
    /// animated JPEG XL show their first frame there.
    pub fn load(&self, bytes: impl Into<Arc<[u8]>>) -> Image {
        let bytes = bytes.into();
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-encoded\0");
        hash.update(&bytes);
        let id = *hash.finalize().as_bytes();
        self.request(id, Input::Encoded(bytes))
    }

    /// Like [`ImageCache::load`], but downscales raster images (including animation frames) on
    /// the decoder worker so neither side exceeds `size_pt * scale` device pixels (up to 4096)
    /// before building mip chains.
    pub fn load_capped(&self, bytes: impl Into<Arc<[u8]>>, size_pt: f32, scale: f32) -> Image {
        let bytes = bytes.into();
        let px = cap_pixels(size_pt, scale);
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-encoded-capped\0");
        hash.update(&px.to_le_bytes());
        hash.update(&bytes);
        self.request(*hash.finalize().as_bytes(), Input::EncodedCapped(bytes, px))
    }

    /// SVG or SVGZ bytes rasterized to fit `size_pt` points square at `scale` device pixels per
    /// point (pass the window's `Env::scale`), keeping the drawing's aspect ratio.
    /// Rasterize at the largest size the image draws at: smaller draws use its mips, so 16 pt at
    /// 2× stays crisp. Handles are shared by content, size and scale.
    pub fn svg(&self, bytes: impl Into<Arc<[u8]>>, size_pt: f32, scale: f32) -> Image {
        let bytes = bytes.into();
        let px = icon_pixels(size_pt, scale);
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-svg\0");
        hash.update(&px.to_le_bytes());
        hash.update(&bytes);
        self.request(*hash.finalize().as_bytes(), Input::Svg(bytes, px))
    }

    /// Bytes with a MIME type, as from a data URL or a `Content-Type` header: `image/svg+xml`
    /// rasterizes like [`ImageCache::svg`], anything else decodes like [`ImageCache::load`].
    pub fn load_mime(&self, bytes: impl Into<Arc<[u8]>>, mime: &str, size_pt: f32, scale: f32) -> Image {
        let essence = mime.split(';').next().unwrap_or("").trim();
        if essence.eq_ignore_ascii_case("image/svg+xml") { self.svg(bytes, size_pt, scale) } else { self.load(bytes) }
    }

    /// An image file, read and decoded on the worker. `.svg` and `.svgz` files (or SVG content)
    /// rasterize like [`ImageCache::svg`]; other formats decode like [`ImageCache::load`], ignoring
    /// the size. Handles are shared by path, size and scale, so an edited file needs a new handle
    /// once the old ones drop. On web the path is a URL relative to the page, fetched as
    /// [`ImageCache::url`] does, so `cache.file("assets/logo.webp", ..)` works in both builds.
    pub fn file(&self, path: impl AsRef<std::path::Path>, size_pt: f32, scale: f32) -> Image {
        let path = path.as_ref().to_path_buf();
        let px = icon_pixels(size_pt, scale);
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-file\0");
        hash.update(path.as_os_str().as_encoded_bytes());
        hash.update(&[0]);
        hash.update(&px.to_le_bytes());
        #[cfg(target_arch = "wasm32")]
        let input = Input::Url(path.to_string_lossy().into_owned(), px);
        #[cfg(not(target_arch = "wasm32"))]
        let input = Input::File(path, px);
        self.request(*hash.finalize().as_bytes(), input)
    }

    /// Like [`ImageCache::file`], but downscales raster files (including animation frames) on
    /// the decoder worker so neither side exceeds `size_pt * scale` device pixels (up to 4096)
    /// before building mip chains.
    pub fn file_capped(&self, path: impl AsRef<std::path::Path>, size_pt: f32, scale: f32) -> Image {
        let path = path.as_ref().to_path_buf();
        let px = cap_pixels(size_pt, scale);
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-file-capped\0");
        hash.update(path.as_os_str().as_encoded_bytes());
        hash.update(&[0]);
        hash.update(&px.to_le_bytes());
        #[cfg(target_arch = "wasm32")]
        let input = Input::Url(path.to_string_lossy().into_owned(), px);
        #[cfg(not(target_arch = "wasm32"))]
        let input = Input::FileCapped(path, px);
        self.request(*hash.finalize().as_bytes(), input)
    }

    /// Straight-alpha sRGB RGBA8. Validation, premultiplication and mips happen off-thread.
    pub fn rgba(&self, width: u32, height: u32, bytes: impl Into<Arc<[u8]>>) -> Image {
        let bytes = bytes.into();
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-rgba8\0");
        hash.update(&width.to_le_bytes());
        hash.update(&height.to_le_bytes());
        hash.update(&bytes);
        self.request(*hash.finalize().as_bytes(), Input::Rgba(width, height, bytes))
    }

    /// The Finder icon for an app bundle or any file path, rasterized at 2× `size_pt`.
    /// See [`ImageCache::app_icon_scaled`].
    pub fn app_icon(&self, path: impl AsRef<std::path::Path>, size_pt: f32) -> Image {
        self.app_icon_scaled(path, size_pt, 2.0)
    }

    /// The Finder icon for `path`, rasterized at `size_pt * scale` pixels square (pass the
    /// window's display scale). Handles are shared by (path, size, scale) rather than by
    /// content, so repeated calls never re-rasterize while a handle is alive. Lookup and
    /// rasterization run on the cache worker like any decode, so this is safe to call from a view.
    /// Missing paths still get the system's generic document icon on macOS. Windows reads an
    /// executable's or icon file's first icon; Linux reads a `.desktop` entry's `Icon=` from the
    /// hicolor theme or pixmaps (PNG or SVG), or an image file itself. Paths without an icon
    /// resolve to an error, so callers can draw their own fallback.
    pub fn app_icon_scaled(&self, path: impl AsRef<std::path::Path>, size_pt: f32, scale: f32) -> Image {
        let path = path.as_ref().to_path_buf();
        let px = icon_pixels(size_pt, scale);
        let mut hash = blake3::Hasher::new();
        hash.update(b"rune-app-icon\0");
        hash.update(path.as_os_str().as_encoded_bytes());
        hash.update(&[0]);
        hash.update(&size_pt.to_bits().to_le_bytes());
        hash.update(&scale.to_bits().to_le_bytes());
        self.request(*hash.finalize().as_bytes(), Input::Icon(path, px))
    }

    fn request(&self, id: ImageId, input: Input) -> Image {
        self.request_set(id, input, None)
    }

    fn request_set(&self, id: ImageId, input: Input, srcset: Option<Arc<str>>) -> Image {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = inner.entries.get(&id).and_then(Weak::upgrade) {
            inner.retain_and_touch(&state);
            return Image::new(state);
        }
        inner.entries.retain(|_, value| value.strong_count() > 0);
        let url: Option<Arc<str>> = match &input {
            Input::Url(url, _) => Some(url.as_str().into()),
            #[cfg(not(target_arch = "wasm32"))]
            Input::File(path, _) | Input::FileCapped(path, _) => Some(path.to_string_lossy().into()),
            _ => None,
        };
        let image = Image::new(Arc::new(State { id, pixels: Pixels::from_url(url, srcset), decoded: OnceLock::new() }));
        inner.entries.insert(id, Arc::downgrade(&image.0));
        inner.retain_and_touch(&image.0);
        // A small straight RGBA image (a 16 pt tile at 2x) is ready at once: its mips take
        // microseconds, less than handing it to a decoder, and the view that asked for it draws it
        // in the same frame rather than a placeholder first. It still wakes the app once, as a
        // decoded one does: a caller outside a view's build (an overlay's state, a poll) is
        // redrawn by that wake alone.
        #[cfg(not(target_arch = "wasm32"))]
        if let Input::Rgba(w, h, _) = &input && u64::from(*w) * u64::from(*h) <= INLINE_RGBA_PIXELS {
            image.finish(input.decode_caught());
            inner.on_decoded(id, &image);
            drop(inner);
            (self.waker)();
            return image;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !decode_later(Job {
            image: image.clone(),
            input,
            waker: self.waker.clone(),
            cache: Arc::downgrade(&self.inner),
        }) {
            image.0.decoded.set(Err(ImageError("image worker stopped".into()))).ok();
            inner.remove_retained(&id);
        }
        #[cfg(target_arch = "wasm32")]
        {
            // The task holds the image weakly, so a request dropped before it runs (a view's
            // guess, replaced in the same frame once layout knows the size) fetches nothing,
            // and one dropped while its bytes download is never decoded.
            let weak = Arc::downgrade(&image.0);
            let wake = self.wake.clone();
            let cache_weak = Arc::downgrade(&self.inner);
            wasm_bindgen_futures::spawn_local(async move {
                let decoded = match input {
                    Input::Url(url, px) => {
                        if !wanted(&weak) { return; }
                        let fetched = from_url::fetch(&url).await;
                        if !wanted(&weak) { return; }
                        match fetched { Ok(fetched) => from_url::land_in_slices(&url, fetched, px).await, Err(e) => Err(e) }
                    }
                    Input::Encoded(bytes) if browser_decodes(&bytes) => {
                        let decoded = from_url::decode_in_browser(&bytes).await;
                        if !wanted(&weak) { return; }
                        match decoded { Ok(fetched) => from_url::land_in_slices("image", fetched, 0).await, Err(e) => Err(e) }
                    }
                    input => input.decode(),
                };
                let Some(state) = weak.upgrade() else { return };
                let img = Image::new(state);
                img.finish(decoded);
                if let Some(inner) = cache_weak.upgrade() {
                    let mut guard = inner.lock().unwrap_or_else(|e| e.into_inner());
                    guard.on_decoded(id, &img);
                }
                wake();
            });
        }
        image
    }
}

/// Encoded bytes a web build hands to the browser's codecs: everything but PNG (decoded exactly,
/// alpha and all, by Rune), SVG and ICNS.
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn browser_decodes(bytes: &[u8]) -> bool {
    !bytes.starts_with(b"\x89PNG\r\n\x1a\n") && !bytes.starts_with(b"icns") && !is_svg(bytes)
}

/// Whether anything still holds the image a fetch is for.
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
fn wanted(state: &std::sync::Weak<State>) -> bool { state.strong_count() > 0 }

fn icon_pixels(size_pt: f32, scale: f32) -> u32 {
    let px = (size_pt * scale).ceil();
    if px.is_finite() { px.clamp(1.0, 1024.0) as u32 } else { 1 }
}

fn cap_pixels(size_pt: f32, scale: f32) -> u32 {
    let px = (size_pt * scale).ceil();
    if px.is_finite() { px.clamp(1.0, 4096.0) as u32 } else { 1 }
}

pub(crate) fn fit_within(w: u32, h: u32, max_side: u32) -> (u32, u32) {
    if max_side == 0 || (w <= max_side && h <= max_side) {
        return (w, h);
    }
    let scale = max_side as f64 / w.max(h) as f64;
    let nw = ((w as f64 * scale).round() as u32).clamp(1, max_side);
    let nh = ((h as f64 * scale).round() as u32).clamp(1, max_side);
    (nw, nh)
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn has_svg_extension(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"))
}

/// Gzip (SVGZ), or markup after an optional BOM and whitespace. No raster format starts with `<`.
fn is_svg(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0x1f, 0x8b]) { return true; }
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'<')
}

/// Reads a regular file up to [`MAX_ALLOC`] bytes, refusing directories, FIFOs, sockets and
/// device files before and after opening so a special file cannot block or exhaust memory.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_regular_file(path: &std::path::Path) -> Result<Vec<u8>, ImageError> {
    use std::io::Read;
    let err = |e: std::io::Error| ImageError(format!("{}: {e}", path.display()));
    let meta = std::fs::metadata(path).map_err(err)?;
    if !meta.is_file() {
        return Err(ImageError(format!("{}: not a regular file", path.display())));
    }
    if meta.len() > MAX_ALLOC {
        return Err(ImageError(format!("{}: file is too large", path.display())));
    }
    let file = std::fs::File::open(path).map_err(err)?;
    let handle_meta = file.metadata().map_err(err)?;
    if !handle_meta.is_file() {
        return Err(ImageError(format!("{}: not a regular file", path.display())));
    }
    let cap = handle_meta.len().min(MAX_ALLOC) as usize;
    let mut out = Vec::with_capacity(cap.min(64 * 1024));
    file.take(MAX_ALLOC + 1).read_to_end(&mut out).map_err(err)?;
    if out.len() as u64 > MAX_ALLOC {
        return Err(ImageError(format!("{}: file is too large", path.display())));
    }
    Ok(out)
}

fn decode(bytes: &[u8]) -> Result<Decoded, ImageError> {
    decode_capped(bytes, 0)
}

fn decode_capped(bytes: &[u8], max_side: u32) -> Result<Decoded, ImageError> {
    if is_svg(bytes) {
        let (w, h, rgba) = svg_rgba(bytes, max_side)?;
        return mip_chain(w, h, &rgba);
    }
    if avif::is_avif(bytes) {
        let (w, h, rgba) = avif::rgba(bytes)?;
        if max_side > 0 && (w > max_side || h > max_side) {
            if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
                let (nw, nh) = fit_within(w, h, max_side);
                let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
                return mip_chain(resized.width(), resized.height(), resized.as_raw());
            }
            return Err(ImageError("invalid AVIF buffer size".into()));
        }
        return mip_chain(w, h, &rgba);
    }
    let bytes = if bytes.starts_with(b"icns") { icns_png(bytes)? } else { bytes };
    if let Some(animation) = animated::decode_capped(bytes, max_side) { return animation; }
    use image::ImageDecoder;
    let error = |e: image::ImageError| ImageError(e.to_string());
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format().map_err(|e| ImageError(e.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits.clone());
    // `ImageReader::decode`, keeping the decoder to read its EXIF orientation (a phone's JPEG
    // stores its pixels as the sensor saw them and says how to turn them upright).
    let mut decoder = reader.into_decoder().map_err(error)?;
    limits.reserve(decoder.total_bytes()).map_err(error)?;
    decoder.set_limits(limits).map_err(error)?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder).map_err(error)?;
    image.apply_orientation(orientation);
    let image = image.into_rgba8();
    let image = if max_side > 0 && (image.width() > max_side || image.height() > max_side) {
        let (nw, nh) = fit_within(image.width(), image.height(), max_side);
        image::imageops::resize(&image, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    mip_chain(image.width(), image.height(), image.as_raw())
}

/// A PNG decoded to 8-bit RGBA by the PNG decoder alone. `image::load_from_memory_with_format`
/// dispatches on the format at run time, which links every codec the crate is built with (JPEG,
/// WebP, GIF, BMP, ICO) into an app that only ever reads PNGs from the clipboard or a cursor.
/// The same decoder and limits as that call's PNG arm.
#[doc(hidden)]
pub fn png_rgba8(bytes: &[u8]) -> image::ImageResult<image::RgbaImage> {
    let decoder = image::codecs::png::PngDecoder::with_limits(std::io::Cursor::new(bytes), image::Limits::default())?;
    Ok(image::DynamicImage::from_decoder(decoder)?.into_rgba8())
}

/// Encodes straight-alpha sRGB RGBA8 pixels into PNG bytes.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ImageError> {
    use image::ImageEncoder;
    if width == 0 || height == 0 {
        return Err(ImageError("cannot encode empty image".into()));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| ImageError("image dimensions overflow".into()))?;
    if rgba.len() != expected {
        return Err(ImageError(format!(
            "RGBA length {} does not match {}x{} ({expected} bytes)",
            rgba.len(),
            width,
            height
        )));
    }
    let mut out = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    encoder
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| ImageError(format!("PNG encode failed: {e}")))?;
    Ok(out)
}

fn icns_png(bytes: &[u8]) -> Result<&[u8], ImageError> {
    let invalid = || ImageError("invalid ICNS container or no supported PNG representation".into());
    let len = u32::from_be_bytes(bytes.get(4..8).ok_or_else(invalid)?.try_into().unwrap()) as usize;
    if len != bytes.len() { return Err(invalid()); }
    let mut pos = 8;
    let mut best = None;
    let mut area = 0u64;
    while pos < len {
        let header = bytes.get(pos..pos + 8).ok_or_else(invalid)?;
        let size = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
        if size < 8 { return Err(invalid()); }
        let end = pos.checked_add(size).filter(|end| *end <= len).ok_or_else(invalid)?;
        let chunk = &bytes[pos + 8..end];
        if chunk.starts_with(b"\x89PNG\r\n\x1a\n") && chunk.len() >= 24 {
            let w = u32::from_be_bytes(chunk[16..20].try_into().unwrap());
            let h = u32::from_be_bytes(chunk[20..24].try_into().unwrap());
            let candidate = u64::from(w) * u64::from(h);
            if w <= MAX_SIDE && h <= MAX_SIDE && candidate > area { best = Some(chunk); area = candidate; }
        }
        pos = end;
    }
    best.ok_or_else(invalid)
}
