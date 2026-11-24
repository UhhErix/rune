//! Background HLS segment prefetcher: keeps 2–3 segments ahead, drives ABR variant switches,
//! caches `#EXT-X-MAP` initialization segments with a strict byte/entry cap, and cancels in-flight
//! transfers immediately on seek or teardown.

#![forbid(unsafe_code)]

use super::HlsVariantInfo;
use super::abr::{AbrConfig, AbrController, AbrVariant};
use super::http::{
    CancelToken, HttpError, MAX_PLAYLIST_BYTES, MAX_SEGMENT_BYTES, fetch_with_retry,
};
use super::playlist::{
    MapSegment, MasterPlaylist, MediaPlaylist, Playlist, SeekTarget, parse_playlist,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Number of segments prefetched ahead of the decoder.
pub const PREFETCH_DEPTH: usize = 3;
/// Maximum cached `#EXT-X-MAP` initialization segments.
const MAP_CACHE_MAX_ENTRIES: usize = 8;
/// Maximum total bytes across cached `#EXT-X-MAP` initialization segments (2 MiB).
const MAP_CACHE_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Container format of a downloaded HLS media segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentFormat {
    Fmp4,
    MpegTs,
}

impl SegmentFormat {
    /// File extension hint passed to Media Foundation's byte-stream resolver.
    pub fn ext(self) -> &'static str {
        match self {
            Self::Fmp4 => "mp4",
            Self::MpegTs => "ts",
        }
    }

    /// Infers container format from `EXT-X-MAP` presence, container magic bytes, and URI extension.
    pub fn detect(has_map: bool, uri: &str, bytes: &[u8]) -> Self {
        if has_map {
            return Self::Fmp4;
        }
        if bytes.len() >= 8 {
            let box_type = &bytes[4..8];
            if matches!(box_type, b"ftyp" | b"styp" | b"moof" | b"moov" | b"sidx") {
                return Self::Fmp4;
            }
        }
        if bytes.first() == Some(&0x47) && (bytes.len() < 189 || bytes.get(188) == Some(&0x47)) {
            return Self::MpegTs;
        }
        let path = uri.split(['?', '#']).next().unwrap_or(uri);
        if let Some((_, ext)) = path.rsplit_once('.') {
            if ext.eq_ignore_ascii_case("ts")
                || ext.eq_ignore_ascii_case("m2ts")
                || ext.eq_ignore_ascii_case("mts")
            {
                return Self::MpegTs;
            }
            if ext.eq_ignore_ascii_case("m4s")
                || ext.eq_ignore_ascii_case("mp4")
                || ext.eq_ignore_ascii_case("m4v")
            {
                return Self::Fmp4;
            }
        }
        Self::Fmp4
    }
}

/// One prefetched HLS segment ready for the platform decoder.
#[derive(Clone, Debug)]
pub struct FetchedSegment {
    pub generation: u32,
    pub index: usize,
    pub variant: usize,
    pub variant_info: Option<HlsVariantInfo>,
    pub start_time: f64,
    pub duration: f64,
    pub discontinuity: bool,
    pub format: SegmentFormat,
    /// Initialization segment (`#EXT-X-MAP`) prepended for fMP4 segments.
    pub init: Option<Arc<Vec<u8>>>,
    /// Media segment payload.
    pub data: Vec<u8>,
}

impl FetchedSegment {
    /// Concatenates `init` (if any) and `data` into a self-contained stream buffer for decoding.
    pub fn combined_bytes(&self) -> Vec<u8> {
        match &self.init {
            Some(init) => {
                let mut out = Vec::with_capacity(init.len() + self.data.len());
                out.extend_from_slice(init);
                out.extend_from_slice(&self.data);
                out
            }
            None => self.data.clone(),
        }
    }
}

/// Result of popping the next prefetched segment for the decoder.
pub enum NextSegment {
    Ready(FetchedSegment),
    EndOfStream,
    Pending,
    Failed(String),
}

struct SharedState {
    generation: u32,
    next_index: usize,
    /// When switching variants, align the new variant's segment index to this timeline timestamp.
    pending_seek_time: Option<f64>,
    queue: VecDeque<FetchedSegment>,
    eos_generation: Option<u32>,
    error: Option<String>,
    stop: bool,
    active_cancel: CancelToken,
    /// Current media playlist metadata (for seek-to-segment mapping).
    current_playlist: MediaPlaylist,
    active_variant_info: Option<HlsVariantInfo>,
}

struct MapCache {
    order: VecDeque<MapSegment>,
    entries: HashMap<MapSegment, Arc<Vec<u8>>>,
    total_bytes: usize,
}

impl MapCache {
    fn new() -> Self {
        Self {
            order: VecDeque::new(),
            entries: HashMap::new(),
            total_bytes: 0,
        }
    }

    fn get(&self, key: &MapSegment) -> Option<Arc<Vec<u8>>> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: MapSegment, bytes: Arc<Vec<u8>>) {
        if self.entries.contains_key(&key) {
            return;
        }
        let len = bytes.len();
        if len > MAP_CACHE_MAX_BYTES {
            return;
        }
        while self.entries.len() >= MAP_CACHE_MAX_ENTRIES
            || self.total_bytes.saturating_add(len) > MAP_CACHE_MAX_BYTES
        {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            }
        }
        self.total_bytes = self.total_bytes.saturating_add(len);
        self.order.push_back(key.clone());
        self.entries.insert(key, bytes);
    }
}

/// Active HLS playback session owning a background prefetch worker thread.
pub struct HlsSession {
    state: Arc<(Mutex<SharedState>, Condvar)>,
    total_duration: f64,
    active_variant: Arc<Mutex<Option<HlsVariantInfo>>>,
}

impl Drop for HlsSession {
    fn drop(&mut self) {
        self.stop();
    }
}

impl HlsSession {
    /// Fetches and parses the initial master/media playlist, selects the initial low variant,
    /// and spawns the background segment prefetch thread.
    pub fn open(
        url: &str,
        headers: Vec<(String, String)>,
        start_seconds: f64,
        generation: u32,
        display_size: Arc<AtomicU64>,
        active_variant_slot: Arc<Mutex<Option<HlsVariantInfo>>>,
        open_cancel: &CancelToken,
    ) -> Result<Self, String> {
        let resp = fetch_with_retry(url, &headers, None, MAX_PLAYLIST_BYTES, open_cancel)
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8(resp.body)
            .map_err(|_| "HLS playlist is not valid UTF-8".to_string())?;
        let parsed = parse_playlist(&text, &resp.final_url)?;

        let (master, mut variant_playlists, mut active_idx, initial_media, abr) = match parsed {
            Playlist::Master(master) => {
                let abr_variants = AbrVariant::from_playlist(&master.variants);
                let disp = unpack_display_size(display_size.load(Ordering::Relaxed));
                let abr = AbrController::new(abr_variants, disp, AbrConfig::default());
                let idx = abr.current_index();
                let variant_url = master.variants[idx].uri.clone();
                let v_resp = fetch_with_retry(
                    &variant_url,
                    &headers,
                    None,
                    MAX_PLAYLIST_BYTES,
                    open_cancel,
                )
                .map_err(|e| e.to_string())?;
                let v_text = String::from_utf8(v_resp.body)
                    .map_err(|_| "HLS variant playlist is not valid UTF-8".to_string())?;
                let media = match parse_playlist(&v_text, &v_resp.final_url)? {
                    Playlist::Media(m) => m,
                    Playlist::Master(_) => {
                        return Err("nested HLS master playlists are not supported".into());
                    }
                };
                let mut cache = HashMap::new();
                cache.insert(idx, media.clone());
                (Some(master), cache, idx, media, Some(abr))
            }
            Playlist::Media(media) => (None, HashMap::new(), 0, media, None),
        };

        let active_info = master
            .as_ref()
            .and_then(|m| m.variants.get(active_idx))
            .map(HlsVariantInfo::from);
        *active_variant_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = active_info.clone();

        let total_duration = initial_media.total_duration;
        let initial_index = initial_media
            .segment_at(start_seconds)
            .map_or(0, |t| t.segment_index);
        let state = Arc::new((
            Mutex::new(SharedState {
                generation,
                next_index: initial_index,
                pending_seek_time: None,
                queue: VecDeque::new(),
                eos_generation: None,
                error: None,
                stop: false,
                active_cancel: CancelToken::new(),
                current_playlist: initial_media.clone(),
                active_variant_info: active_info,
            }),
            Condvar::new(),
        ));

        let worker_state = state.clone();
        let worker_variant_slot = active_variant_slot.clone();
        let spawn = std::thread::Builder::new()
            .name("rune-hls-fetch".into())
            .spawn(move || {
                fetch_loop(
                    worker_state,
                    headers,
                    display_size,
                    worker_variant_slot,
                    master,
                    &mut variant_playlists,
                    &mut active_idx,
                    initial_media,
                    abr,
                );
            });
        if spawn.is_err() {
            return Err("couldn't start HLS prefetch thread".into());
        }

        Ok(Self {
            state,
            total_duration,
            active_variant: active_variant_slot,
        })
    }

    pub fn total_duration(&self) -> f64 {
        self.total_duration
    }

    pub fn active_variant(&self) -> Option<HlsVariantInfo> {
        self.active_variant
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Cancels in-flight downloads, clears queued segments, and seeks to the nearest segment for
    /// `seconds`. Returns the mapped [`SeekTarget`].
    pub fn seek(&self, seconds: f64, generation: u32) -> Option<SeekTarget> {
        let (lock, cv) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        let target = s.current_playlist.segment_at(seconds);
        let old_cancel = std::mem::replace(&mut s.active_cancel, CancelToken::new());
        s.generation = generation;
        s.queue.clear();
        s.eos_generation = None;
        s.error = None;
        s.next_index = target.map_or(0, |t| t.segment_index);
        s.pending_seek_time = Some(seconds.max(0.0));
        drop(s);
        old_cancel.cancel();
        cv.notify_all();
        target
    }

    /// Restarts prefetching from segment 0 for seamless looping.
    pub fn loop_restart(&self, generation: u32) {
        let (lock, cv) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        s.generation = generation;
        s.next_index = 0;
        s.eos_generation = None;
        s.pending_seek_time = None;
        cv.notify_all();
    }

    /// Pops the next prefetched segment for `generation`, waiting up to `wait`.
    pub fn next_segment(&self, generation: u32, wait: Duration) -> NextSegment {
        let (lock, cv) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = rune_core::time::now() + wait;
        loop {
            while s
                .queue
                .front()
                .is_some_and(|seg| seg.generation != generation)
            {
                s.queue.pop_front();
            }
            if let Some(seg) = s.queue.pop_front() {
                cv.notify_all();
                return NextSegment::Ready(seg);
            }
            if let Some(err) = s.error.clone() {
                return NextSegment::Failed(err);
            }
            if s.eos_generation == Some(generation) || s.stop {
                return NextSegment::EndOfStream;
            }
            let now = rune_core::time::now();
            if now >= deadline {
                return NextSegment::Pending;
            }
            let (guard, _) = cv
                .wait_timeout(s, deadline - now)
                .unwrap_or_else(|e| e.into_inner());
            s = guard;
        }
    }

    /// Stops the background prefetcher and cancels any in-flight HTTP request.
    pub fn stop(&self) {
        let (lock, cv) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        s.stop = true;
        let cancel = s.active_cancel.clone();
        drop(s);
        cancel.cancel();
        cv.notify_all();
    }
}

#[allow(clippy::too_many_arguments)]
fn fetch_loop(
    state: Arc<(Mutex<SharedState>, Condvar)>,
    headers: Vec<(String, String)>,
    display_size: Arc<AtomicU64>,
    active_variant_slot: Arc<Mutex<Option<HlsVariantInfo>>>,
    master: Option<MasterPlaylist>,
    variant_playlists: &mut HashMap<usize, MediaPlaylist>,
    active_idx: &mut usize,
    mut media: MediaPlaylist,
    mut abr: Option<AbrController>,
) {
    let mut map_cache = MapCache::new();
    let (lock, cv) = &*state;

    loop {
        let (generation, seg_idx, cancel, buffered_secs) = {
            let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if s.stop {
                    return;
                }
                if s.next_index >= media.segments.len() {
                    s.eos_generation = Some(s.generation);
                    cv.notify_all();
                    s = cv.wait(s).unwrap_or_else(|e| e.into_inner());
                    continue;
                }
                if s.queue.len() >= PREFETCH_DEPTH {
                    s = cv.wait(s).unwrap_or_else(|e| e.into_inner());
                    continue;
                }
                let buffered: f64 = s.queue.iter().map(|seg| seg.duration).sum();
                break (
                    s.generation,
                    s.next_index,
                    s.active_cancel.clone(),
                    buffered,
                );
            }
        };

        let seg = media.segments[seg_idx].clone();

        // Fetch `#EXT-X-MAP` init segment if present and not cached.
        let init = if let Some(map) = &seg.map {
            if let Some(cached) = map_cache.get(map) {
                Some(cached)
            } else {
                match fetch_with_retry(
                    &map.uri,
                    &headers,
                    map.byte_range,
                    MAX_SEGMENT_BYTES,
                    &cancel,
                ) {
                    Ok(resp) => {
                        let arc = Arc::new(resp.body);
                        map_cache.insert(map.clone(), arc.clone());
                        Some(arc)
                    }
                    Err(HttpError::Cancelled) => continue,
                    Err(err) => {
                        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
                        if s.generation == generation && !s.stop {
                            s.error = Some(format!("HLS init segment failed: {err}"));
                            cv.notify_all();
                        }
                        return;
                    }
                }
            }
        } else {
            None
        };

        // Fetch media segment payload.
        let seg_resp = match fetch_with_retry(
            &seg.uri,
            &headers,
            seg.byte_range,
            MAX_SEGMENT_BYTES,
            &cancel,
        ) {
            Ok(resp) => resp,
            Err(HttpError::Cancelled) => continue,
            Err(err) => {
                let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
                if s.generation == generation && !s.stop {
                    s.error = Some(format!("HLS segment failed: {err}"));
                    cv.notify_all();
                }
                return;
            }
        };

        let bytes_len = seg_resp.body.len();
        let elapsed = seg_resp.elapsed;
        let format = SegmentFormat::detect(init.is_some(), &seg.uri, &seg_resp.body);
        let variant_info = master
            .as_ref()
            .and_then(|m| m.variants.get(*active_idx))
            .map(HlsVariantInfo::from);

        let fetched = FetchedSegment {
            generation,
            index: seg_idx,
            variant: *active_idx,
            variant_info: variant_info.clone(),
            start_time: seg.start_time,
            duration: seg.duration,
            discontinuity: seg.discontinuity,
            format,
            init,
            data: seg_resp.body,
        };

        let next_start_time = seg.start_time + seg.duration;
        let committed = {
            let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
            if s.stop {
                return;
            }
            if s.generation != generation {
                false
            } else {
                s.queue.push_back(fetched);
                s.next_index = seg_idx + 1;
                cv.notify_all();
                true
            }
        };

        if !committed {
            continue;
        }

        // Update ABR with measured transfer throughput and current display size.
        if let (Some(abr_ctl), Some(master_pl)) = (abr.as_mut(), master.as_ref()) {
            abr_ctl.record_sample(bytes_len, elapsed);
            let disp = unpack_display_size(display_size.load(Ordering::Relaxed));
            let desired = abr_ctl.select(buffered_secs + seg.duration, disp);
            if desired != *active_idx && desired < master_pl.variants.len() {
                let loaded = if let Some(cached) = variant_playlists.get(&desired) {
                    Some(cached.clone())
                } else {
                    let uri = &master_pl.variants[desired].uri;
                    match fetch_with_retry(uri, &headers, None, MAX_PLAYLIST_BYTES, &cancel) {
                        Ok(r) => String::from_utf8(r.body)
                            .ok()
                            .and_then(|t| parse_playlist(&t, &r.final_url).ok())
                            .and_then(|p| match p {
                                Playlist::Media(m) => {
                                    variant_playlists.insert(desired, m.clone());
                                    Some(m)
                                }
                                Playlist::Master(_) => None,
                            }),
                        Err(_) => None,
                    }
                };
                if let Some(new_media) = loaded {
                    *active_idx = desired;
                    media = new_media.clone();
                    let new_info = Some(HlsVariantInfo::from(&master_pl.variants[desired]));
                    *active_variant_slot
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = new_info.clone();
                    let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if s.generation == generation && !s.stop {
                        s.current_playlist = new_media;
                        s.active_variant_info = new_info;
                        // Align next segment index on the new variant's timeline.
                        if next_start_time >= media.total_duration {
                            s.next_index = media.segments.len();
                        } else if let Some(mapped) = media.segment_at(next_start_time) {
                            s.next_index = mapped.segment_index;
                        }
                    }
                }
            }
        }
    }
}

/// Packs `(width, height)` into a single `u64` for lock-free reads from the paint path.
pub fn pack_display_size(width: u32, height: u32) -> u64 {
    (u64::from(width) << 32) | u64::from(height)
}

/// Unpacks a `u64` produced by [`pack_display_size`]; returns `None` when either axis is zero.
pub fn unpack_display_size(packed: u64) -> Option<(u32, u32)> {
    let w = (packed >> 32) as u32;
    let h = packed as u32;
    (w > 0 && h > 0).then_some((w, h))
}
