//! Lifecycle callbacks (`on_ready`, `on_error`) and poster-frame fallback helpers for [`Player`].

#![forbid(unsafe_code)]

use super::{MediaRequest, MediaSource, Player, PlayerState, Shared, hls};
use crate::native_surface::{ByteFrame, ColorInfo, Format, NativeSurface};
use rune_image::Pixels;
use std::sync::Arc;
use std::sync::atomic::Ordering;

type ReadyCallback = Arc<dyn Fn() + Send + Sync>;
type ErrorCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
pub(super) struct Callbacks {
    on_ready: Option<ReadyCallback>,
    on_error: Option<ErrorCallback>,
    ready_fired: bool,
    error_fired: bool,
}

impl Shared {
    /// Returns true if this clip uses HLS (`.m3u8`/`.m3u` URL or constructed via [`Player::hls`]).
    pub(crate) fn is_hls(&self) -> bool {
        self.force_hls || matches!(&self.source, MediaSource::Url(u) if hls::is_hls_url(u))
    }

    /// Updates the element's physical pixel size on screen for ABR resolution capping.
    #[doc(hidden)]
    pub fn set_display_size(&self, width: u32, height: u32) {
        self.display_size.store(
            hls::prefetch::pack_display_size(width, height),
            Ordering::Relaxed,
        );
    }

    /// Attaches a poster image displayed while loading and as the error fallback.
    #[doc(hidden)]
    pub fn set_poster(&self, surface: &NativeSurface, poster: Option<Pixels>) {
        *self.poster.lock().unwrap_or_else(|e| e.into_inner()) = poster.clone();
        if let Some(pixels) = poster {
            let show_now = {
                let c = self.ctl.lock().unwrap_or_else(|e| e.into_inner());
                !c.opened() || c.failed()
            };
            if show_now {
                let _ = push_poster(surface, &pixels);
            }
        }
    }

    /// Registers a one-shot callback invoked when the clip becomes ready (loaded).
    #[doc(hidden)]
    pub fn set_on_ready(&self, f: Arc<dyn Fn() + Send + Sync>) {
        self.callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_ready = Some(f);
        self.deliver_callbacks();
    }

    /// Registers a one-shot callback invoked when playback or loading fails.
    #[doc(hidden)]
    pub fn set_on_error(&self, f: Arc<dyn Fn(&str) + Send + Sync>) {
        self.callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_error = Some(f);
        self.deliver_callbacks();
    }

    /// Delivers `on_ready` / `on_error` callbacks outside any mutex lock.
    pub(super) fn deliver_callbacks(&self) {
        let (opened, failed_msg) = {
            let c = self.ctl.lock().unwrap_or_else(|e| e.into_inner());
            let err = match c.state() {
                PlayerState::Failed(msg) => Some(msg),
                _ => None,
            };
            (c.opened() && err.is_none(), err)
        };

        let ready_cb = if opened {
            let mut cbs = self.callbacks.lock().unwrap_or_else(|e| e.into_inner());
            if !cbs.ready_fired {
                if let Some(cb) = cbs.on_ready.clone() {
                    cbs.ready_fired = true;
                    Some(cb)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(cb) = ready_cb {
            cb();
        }

        let error_cb = if let Some(msg) = failed_msg {
            let mut cbs = self.callbacks.lock().unwrap_or_else(|e| e.into_inner());
            if !cbs.error_fired {
                if let Some(cb) = cbs.on_error.clone() {
                    cbs.error_fired = true;
                    Some((cb, msg))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some((cb, msg)) = error_cb {
            cb(&msg);
        }
    }

    /// Pushes the poster onto `surface` if one is set and non-empty; returns true when pushed.
    pub(super) fn push_poster_if_any(&self, surface: &NativeSurface) -> bool {
        let poster = self
            .poster
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        poster.as_ref().is_some_and(|p| push_poster(surface, p))
    }
}

fn push_poster(surface: &NativeSurface, poster: &Pixels) -> bool {
    let (w, h, mut rgba) = poster.to_rgba8();
    if w == 0 || h == 0 || rgba.len() != (w as usize) * (h as usize) * 4 {
        return false;
    }
    // `NativeSurface` expects BGRA byte order for `Format::Bgra`.
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let frame = ByteFrame {
        width: w,
        height: h,
        format: Format::Bgra,
        color: ColorInfo::default(),
        planes: [rgba, Vec::new()],
    };
    surface.push_bytes(frame, u64::MAX - 1).is_ok()
}

impl MediaRequest {
    /// An HLS (`.m3u8`) video stream, muted, carrying this request's HTTP headers.
    pub fn hls(self) -> Player {
        Player::create_inner(
            MediaSource::Url(self.url),
            super::Kind::Video,
            self.headers,
            true,
        )
    }
}

impl Player {
    /// An HLS video stream (`http(s)://` `.m3u8` URL or manifest endpoint), muted by default.
    pub fn hls(url: impl Into<String>) -> Self {
        Self::create_inner(
            MediaSource::Url(url.into()),
            super::Kind::Video,
            Vec::new(),
            true,
        )
    }

    /// Returns true if this player streams HLS (`.m3u8` URL or constructed with [`Player::hls`]).
    pub fn is_hls(&self) -> bool {
        self.shared.is_hls()
    }

    /// Sets a poster image shown before the first video frame arrives and as the fallback when
    /// loading or playback fails.
    pub fn set_poster(&self, poster: Option<Pixels>) {
        self.shared.set_poster(&self.surface, poster);
    }

    /// Builder form of [`set_poster`](Self::set_poster).
    pub fn with_poster(self, poster: Pixels) -> Self {
        self.set_poster(Some(poster));
        self
    }

    /// Returns the currently configured poster image, if any.
    pub fn poster(&self) -> Option<Pixels> {
        self.shared
            .poster
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Registers a callback invoked once when the clip becomes ready to play.
    pub fn on_ready(&self, f: impl Fn() + Send + Sync + 'static) {
        self.shared.set_on_ready(Arc::new(f));
    }

    /// Builder form of [`on_ready`](Self::on_ready).
    pub fn with_on_ready(self, f: impl Fn() + Send + Sync + 'static) -> Self {
        self.on_ready(f);
        self
    }

    /// Registers a callback invoked once if loading or playback fails (while the surface falls
    /// back to the poster image, if set).
    pub fn on_error(&self, f: impl Fn(&str) + Send + Sync + 'static) {
        self.shared.set_on_error(Arc::new(f));
    }

    /// Builder form of [`on_error`](Self::on_error).
    pub fn with_on_error(self, f: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_error(f);
        self
    }

    /// Updates the element's physical pixel size on screen to cap adaptive bitrate resolution.
    pub fn set_display_size(&self, width: u32, height: u32) {
        self.shared.set_display_size(width, height);
    }

    /// Returns the active HLS variant metadata once a master playlist has selected a rendition.
    pub fn active_variant(&self) -> Option<hls::HlsVariantInfo> {
        self.shared
            .active_variant
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl From<&Player> for Player {
    fn from(player: &Player) -> Self {
        player.clone()
    }
}

impl From<MediaSource> for Player {
    fn from(source: MediaSource) -> Self {
        Player::new(source)
    }
}

impl From<&str> for Player {
    fn from(src: &str) -> Self {
        Player::new(MediaSource::from(src))
    }
}

impl From<String> for Player {
    fn from(src: String) -> Self {
        Player::new(MediaSource::from(src))
    }
}
