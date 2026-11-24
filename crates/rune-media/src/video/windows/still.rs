//! Headless runs: a clip's facts and its picture at a virtual time, decoded synchronously on
//! the CPU, so `--snap` and `--frames` are the same every run. No player, no sound.

use super::super::hls::abr::{AbrConfig, AbrController, AbrVariant};
use super::super::hls::http::{
    CancelToken, MAX_PLAYLIST_BYTES, MAX_SEGMENT_BYTES, fetch_with_retry,
};
use super::super::hls::playlist::{MapSegment, MediaPlaylist, Playlist, parse_playlist};
use super::super::hls::prefetch::SegmentFormat;
use super::super::{Kind, MediaSource, Meta};
use super::mf::{self, Decode, Picture};
use crate::native_surface::ByteFrame;
use std::collections::HashMap;
use std::sync::Arc;
use windows::Win32::Media::MediaFoundation::{IMFSourceReader, MF_SOURCE_READERF_ENDOFSTREAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

pub(crate) enum Still {
    Direct(ReaderStill),
    Hls(Box<HlsStill>),
}

pub(crate) struct ReaderStill {
    reader: IMFSourceReader,
    video: u32,
    picture: Picture,
    /// The last picture read at or before the time asked, and the one read after it: a later
    /// time reads on from them rather than seeking.
    current: Option<(f64, ByteFrame)>,
    next: Option<(f64, ByteFrame)>,
}

pub(crate) struct HlsStill {
    playlist: MediaPlaylist,
    headers: Vec<(String, String)>,
    map_cache: HashMap<MapSegment, Arc<Vec<u8>>>,
    current_segment: Option<(usize, ReaderStill)>,
}

/// Opens `source` and reads its facts; for video, a reader for its pictures too.
#[allow(unsafe_code)]
pub(super) fn open(
    source: &MediaSource,
    headers: &[(String, String)],
    kind: Kind,
    is_hls: bool,
    display_size: Option<(u32, u32)>,
) -> Result<(Option<Still>, Meta), String> {
    // SAFETY: COM on this thread for the reader; left initialised, as the thread may open more.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .ok();
    if is_hls {
        return open_hls(source, headers, kind, display_size);
    }
    let target = mf::Target::from_source(source)?;
    let lib = mf::library()?;
    let opened = mf::open(lib, &lib.resolve(&target)?, kind, Decode::Cpu, false)?;
    let meta = Meta {
        duration: opened.duration,
        size: opened.picture.map(|p| p.natural),
        frame: opened.picture.map_or(1.0 / 30.0, |p| p.frame),
        sound: opened.has_sound,
    };
    let still = match (opened.video, opened.picture) {
        (Some(video), Some(picture)) => Some(Still::Direct(ReaderStill {
            reader: opened.reader,
            video,
            picture,
            current: None,
            next: None,
        })),
        _ => None,
    };
    Ok((still, meta))
}

fn open_hls(
    source: &MediaSource,
    headers: &[(String, String)],
    kind: Kind,
    display_size: Option<(u32, u32)>,
) -> Result<(Option<Still>, Meta), String> {
    let MediaSource::Url(url) = source else {
        return Err("HLS playback requires an http:// or https:// URL".into());
    };
    let cancel = CancelToken::new();
    let resp = fetch_with_retry(url, headers, None, MAX_PLAYLIST_BYTES, &cancel)
        .map_err(|e| e.to_string())?;
    let text =
        String::from_utf8(resp.body).map_err(|_| "HLS playlist is not valid UTF-8".to_string())?;
    let media = match parse_playlist(&text, &resp.final_url)? {
        Playlist::Media(m) => m,
        Playlist::Master(master) => {
            let abr = AbrController::new(
                AbrVariant::from_playlist(&master.variants),
                display_size,
                AbrConfig::default(),
            );
            let variant_url = &master.variants[abr.current_index()].uri;
            let v_resp = fetch_with_retry(variant_url, headers, None, MAX_PLAYLIST_BYTES, &cancel)
                .map_err(|e| e.to_string())?;
            let v_text = String::from_utf8(v_resp.body)
                .map_err(|_| "HLS variant playlist is not valid UTF-8".to_string())?;
            match parse_playlist(&v_text, &v_resp.final_url)? {
                Playlist::Media(m) => m,
                Playlist::Master(_) => {
                    return Err("nested HLS master playlists are not supported".into());
                }
            }
        }
    };

    let mut hls = HlsStill {
        playlist: media,
        headers: headers.to_vec(),
        map_cache: HashMap::new(),
        current_segment: None,
    };
    let (reader_still, has_sound, seg_dur, picture) = hls.open_segment(0, kind)?;
    let duration = hls.playlist.total_duration.max(seg_dur);
    let meta = Meta {
        duration,
        size: picture.map(|p| p.natural),
        frame: picture.map_or(1.0 / 30.0, |p| p.frame),
        sound: has_sound,
    };
    if let Some(rs) = reader_still {
        hls.current_segment = Some((0, rs));
        Ok((Some(Still::Hls(Box::new(hls))), meta))
    } else {
        Ok((None, meta))
    }
}

impl HlsStill {
    fn open_segment(
        &mut self,
        index: usize,
        kind: Kind,
    ) -> Result<(Option<ReaderStill>, bool, f64, Option<Picture>), String> {
        let seg = self
            .playlist
            .segments
            .get(index)
            .ok_or("HLS segment index out of range")?
            .clone();
        let cancel = CancelToken::new();
        let init = if let Some(map) = &seg.map {
            if let Some(cached) = self.map_cache.get(map) {
                Some(cached.clone())
            } else {
                let resp = fetch_with_retry(
                    &map.uri,
                    &self.headers,
                    map.byte_range,
                    MAX_SEGMENT_BYTES,
                    &cancel,
                )
                .map_err(|e| format!("HLS init segment failed: {e}"))?;
                let arc = Arc::new(resp.body);
                self.map_cache.insert(map.clone(), arc.clone());
                Some(arc)
            }
        } else {
            None
        };
        let resp = fetch_with_retry(
            &seg.uri,
            &self.headers,
            seg.byte_range,
            MAX_SEGMENT_BYTES,
            &cancel,
        )
        .map_err(|e| format!("HLS segment failed: {e}"))?;
        let format = SegmentFormat::detect(init.is_some(), &seg.uri, &resp.body);
        let combined = match &init {
            Some(init) => {
                let mut buf = Vec::with_capacity(init.len() + resp.body.len());
                buf.extend_from_slice(init);
                buf.extend_from_slice(&resp.body);
                buf
            }
            None => resp.body,
        };
        let lib = mf::library()?;
        let source = lib.resolve_bytes(&combined, format.ext())?;
        let opened = mf::open(lib, &source, kind, Decode::Cpu, false)?;
        let rs = match (opened.video, opened.picture) {
            (Some(video), Some(picture)) => Some(ReaderStill {
                reader: opened.reader,
                video,
                picture,
                current: None,
                next: None,
            }),
            _ => None,
        };
        Ok((rs, opened.has_sound, opened.duration, opened.picture))
    }

    fn frame(&mut self, at: f64) -> Result<ByteFrame, String> {
        let target = self
            .playlist
            .segment_at(at)
            .ok_or("HLS playlist has no segments")?;
        let needs_open = self
            .current_segment
            .as_ref()
            .is_none_or(|(idx, _)| *idx != target.segment_index);
        if needs_open {
            let (rs, _, _, _) = self.open_segment(target.segment_index, Kind::Video)?;
            let rs = rs.ok_or("no video track in HLS segment")?;
            self.current_segment = Some((target.segment_index, rs));
        }
        let (_, rs) = self
            .current_segment
            .as_mut()
            .ok_or("no video track in HLS segment")?;
        rs.frame_segment(target.offset_in_segment)
    }
}

impl Still {
    /// The picture showing at clip second `at`.
    pub(super) fn frame(&mut self, at: f64) -> Result<ByteFrame, String> {
        match self {
            Self::Direct(rs) => rs.frame(at),
            Self::Hls(hls) => hls.frame(at),
        }
    }
}

impl ReaderStill {
    /// Decodes a frame from an HLS segment without requiring a container seek index: reads
    /// forward from the start of the freshly opened segment until `offset` is reached, and
    /// normalizes sample timestamps relative to the segment's first sample.
    fn frame_segment(&mut self, offset: f64) -> Result<ByteFrame, String> {
        if let Some((t, f)) = &self.current
            && *t <= offset + self.picture.frame * 0.5
            && self
                .next
                .as_ref()
                .is_some_and(|(nt, _)| *nt > offset + self.picture.frame * 0.5)
        {
            return Ok(copy(f));
        }
        let mut base: Option<f64> = None;
        loop {
            let read = mf::read(&self.reader, self.video)?;
            if let Some(sample) = &read.sample {
                let b = *base.get_or_insert(read.time);
                let rel = (read.time - b).max(0.0);
                let frame = mf::nv12_bytes(sample, &self.picture)?;
                if rel > offset + self.picture.frame * 0.5 && self.current.is_some() {
                    self.next = Some((rel, frame));
                    break;
                }
                self.current = Some((rel, frame));
            }
            if read.flags & MF_SOURCE_READERF_ENDOFSTREAM.0 != 0 {
                break;
            }
        }
        self.current
            .as_ref()
            .map(|(_, f)| copy(f))
            .ok_or_else(|| "no picture at that time".into())
    }
    fn frame(&mut self, at: f64) -> Result<ByteFrame, String> {
        let shows = |t: f64| t <= at + self.picture.frame * 0.5;
        let reads_on = self
            .current
            .as_ref()
            .is_some_and(|(t, _)| shows(*t) && at - t < 1.0);
        if !reads_on {
            mf::seek(&self.reader, at)?;
            (self.current, self.next) = (None, None);
        }
        if let Some((t, _)) = &self.next {
            if !shows(*t) {
                return self
                    .current
                    .as_ref()
                    .map(|(_, f)| copy(f))
                    .ok_or_else(|| "no picture at that time".into());
            }
            self.current = self.next.take();
        }
        loop {
            let read = mf::read(&self.reader, self.video)?;
            if let Some(sample) = &read.sample {
                let frame = mf::nv12_bytes(sample, &self.picture)?;
                if !shows(read.time) && self.current.is_some() {
                    self.next = Some((read.time, frame));
                    break;
                }
                self.current = Some((read.time, frame));
            }
            if read.flags & MF_SOURCE_READERF_ENDOFSTREAM.0 != 0 {
                break;
            }
        }
        self.current
            .as_ref()
            .map(|(_, f)| copy(f))
            .ok_or_else(|| "no picture at that time".into())
    }
}

fn copy(f: &ByteFrame) -> ByteFrame {
    ByteFrame {
        width: f.width,
        height: f.height,
        format: f.format,
        color: f.color,
        planes: f.planes.clone(),
    }
}
