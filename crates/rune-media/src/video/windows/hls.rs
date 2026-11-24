//! Windows HLS decoder driver: feeds prefetched fMP4 and MPEG-TS segments directly into Media
//! Foundation's existing `IMFSourceReader` via an in-memory `IMFByteStream`, reusing Rune's
//! D3D11 hardware decode ring, DirectComposition video layer, and WASAPI audio output.

#![allow(unsafe_code)]

use super::super::Kind;
use super::super::hls::http::CancelToken;
use super::super::hls::prefetch::{FetchedSegment, HlsSession, NextSegment};
use super::audio::Output;
use super::decoder::{
    DEVICE_CHANGED, Facts, Job, Landing, Stream, log, logging, next_read, rune_device, superseded,
    us,
};
use super::gpu::{self, Ring};
use super::inbox::{Decoded, Inbox, News, Pic, Pipe};
use super::layer::Layer;
use super::machine::Anchor;
use super::mf::{self, Decode};
use rune_core::time::Instant;
use std::sync::{Arc, MutexGuard};
use std::time::Duration;
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_DECODER, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::core::Interface;

const AHEAD: usize = 3;
const PARK: Duration = Duration::from_secs(3);
const RING_POLL: Duration = Duration::from_millis(2);
const LAYER_LEAD: Duration = Duration::from_millis(2);
const SEGMENT_WAIT_SLICE: Duration = Duration::from_millis(50);

pub(super) fn run(pipe: &Pipe, job: Job) {
    rune_core::marks::mark("video.hls.open");
    let generation = crate::native_surface::windows::device_generation();
    let mut d = match HlsDecoder::open(pipe, &job) {
        Ok(Some(d)) => d,
        Ok(None) => return, // stopped before open completed
        Err(_) if gpu::lost_since(generation) => {
            pipe.lock().news.push(News::Parked);
            (job.notify)();
            return;
        }
        Err(e) => {
            pipe.lock().news.push(News::Failed(e));
            (job.notify)();
            return;
        }
    };
    rune_core::marks::mark("video.hls.opened");
    pipe.lock().news.push(News::Opened(d.facts));
    (job.notify)();
    match d.decode(pipe, &job) {
        Err(e) if e == DEVICE_CHANGED || gpu::lost_since(generation) => {
            pipe.lock().news.push(News::Parked);
            (job.notify)();
        }
        Err(e) => {
            pipe.lock().news.push(News::Failed(e));
            (job.notify)();
        }
        Ok(()) => {}
    }
}

struct HlsDecoder {
    session: HlsSession,
    lib: &'static mf::Library,
    device: Option<Arc<gpu::Device>>,
    manager: Option<IMFDXGIDeviceManager>,
    reader: IMFSourceReader,
    facts: Facts,
    ring: Option<Ring>,
    output: Option<Output>,
    video: Option<u32>,
    audio: Option<u32>,
    video_open: bool,
    audio_open: bool,
    seg_start: f64,
    seg_discontinuity: bool,
    seg_base_pts: Option<f64>,
    layer: Option<Layer>,
    presented_at: Option<Instant>,
    awaiting_gpu: bool,
    opened_us: u128,
}

impl HlsDecoder {
    fn open(pipe: &Pipe, job: &Job) -> Result<Option<Self>, String> {
        let t0 = job.started;
        let super::super::MediaSource::Url(url) = &job.source else {
            return Err("HLS playback requires an http:// or https:// URL".into());
        };
        let lib = mf::library()?;
        let software = std::env::var_os("RUNE_VIDEO_DECODE").is_some_and(|v| v == "software");
        let open_cancel = CancelToken::new();

        let (device, session) = std::thread::scope(|s| {
            let device = (job.kind == Kind::Video).then(|| s.spawn(rune_device));
            let session = HlsSession::open(
                url,
                job.headers.clone(),
                job.start,
                job.generation,
                job.display_size.clone(),
                job.active_variant.clone(),
                &open_cancel,
            );
            let device = device.and_then(|d| d.join().ok()).flatten();
            (device, session)
        });
        let session = session?;

        let first_seg = match wait_for_segment(pipe, &session, job.generation)? {
            Some(seg) => seg,
            None => return Ok(None),
        };

        let manager = device
            .as_ref()
            .filter(|_| !software)
            .map(|d| d.manager(lib))
            .transpose()?;
        let decode = manager.as_ref().map_or(Decode::Cpu, Decode::Gpu);
        let bytes = first_seg.combined_bytes();
        let source = lib.resolve_bytes(&bytes, first_seg.format.ext())?;
        let opened = mf::open(lib, &source, job.kind, decode, true)?;
        let picture = opened.picture;
        let awaiting_gpu = !software
            && device.is_none()
            && picture.is_some()
            && crate::native_surface::windows::presenting_device().is_none();
        let ring = match (&device, &picture) {
            (Some(device), Some(picture)) => Some(Ring::new(device.clone(), picture)?),
            _ => None,
        };

        let total_duration = session.total_duration().max(opened.duration);
        let meta = super::super::Meta {
            duration: total_duration,
            size: picture.map(|p| p.natural),
            frame: picture.map_or(1.0 / 30.0, |p| p.frame),
            sound: opened.has_sound,
        };
        let facts = Facts {
            meta,
            picture,
            sound: opened.sound,
            hardware: false,
            gpu: ring.is_some(),
        };
        let opened_us = us(t0);

        let mut d = Self {
            session,
            lib,
            device,
            manager,
            reader: opened.reader,
            facts,
            ring,
            output: None,
            video: opened.video,
            audio: opened.audio,
            video_open: picture.is_some(),
            audio_open: false,
            seg_start: first_seg.start_time,
            seg_discontinuity: first_seg.discontinuity,
            seg_base_pts: None,
            layer: None,
            presented_at: None,
            awaiting_gpu,
            opened_us,
        };
        d.select_sound(job.sound.is_some() || job.kind == Kind::Audio, &job.clock);
        Ok(Some(d))
    }

    fn open_segment(&mut self, seg: &FetchedSegment, job: &Job, pipe: &Pipe) -> Result<(), String> {
        let decode = self.manager.as_ref().map_or(Decode::Cpu, Decode::Gpu);
        let bytes = seg.combined_bytes();
        let source = self.lib.resolve_bytes(&bytes, seg.format.ext())?;
        let opened = mf::open(self.lib, &source, job.kind, decode, true)?;

        let old_pic = self.facts.picture;
        let new_pic = opened.picture;
        let pic_changed = match (old_pic, new_pic) {
            (Some(a), Some(b)) => {
                a.coded != b.coded
                    || a.crop != b.crop
                    || a.natural != b.natural
                    || a.format != b.format
            }
            (None, Some(_)) | (Some(_), None) => true,
            (None, None) => false,
        };

        if pic_changed {
            self.ring = match (&self.device, &new_pic) {
                (Some(device), Some(picture)) => Some(Ring::new(device.clone(), picture)?),
                _ => None,
            };
            self.layer = None;
            let mut s = pipe.lock();
            s.layer_surface = None;
            s.layer_shown = None;
        }

        let sound_changed = self.facts.sound != opened.sound;
        self.reader = opened.reader;
        self.video = opened.video;
        self.audio = opened.audio;
        self.video_open = new_pic.is_some();
        self.seg_start = seg.start_time;
        self.seg_discontinuity = seg.discontinuity;
        self.seg_base_pts = None;

        self.facts.picture = new_pic;
        self.facts.sound = opened.sound;
        self.facts.meta.size = new_pic.map(|p| p.natural);
        self.facts.meta.frame = new_pic.map_or(self.facts.meta.frame, |p| p.frame);
        self.facts.meta.sound = opened.has_sound;
        self.facts.gpu = self.ring.is_some();

        if pic_changed {
            pipe.lock().news.push(News::Updated(self.facts));
            (job.notify)();
        }

        let want_sound = self.output.is_some() || job.kind == Kind::Audio;
        if sound_changed {
            self.output = None;
        }
        self.select_sound(want_sound, &job.clock);
        Ok(())
    }

    fn select_sound(&mut self, on: bool, clock: &Arc<super::audio::Clock>) {
        let Some(sound) = self.facts.sound else {
            self.audio_open = false;
            return;
        };
        if on && self.output.is_none() {
            match Output::open(sound, clock.clone()) {
                Ok(output) => self.output = Some(output),
                Err(e) => eprintln!("rune: no sound for HLS clip: {e}"),
            }
        }
        if !on {
            self.output = None;
        }
        if let Some(index) = self.audio {
            // SAFETY: stream selection on this thread's Media Foundation reader.
            unsafe { self.reader.SetStreamSelection(index, self.output.is_some()) }.ok();
        }
        self.audio_open = self.output.is_some();
    }

    fn normalize_pts(&mut self, raw_time: f64) -> f64 {
        let base = *self.seg_base_pts.get_or_insert_with(|| {
            if !self.seg_discontinuity && (raw_time - self.seg_start).abs() < 0.25 {
                self.seg_start
            } else {
                raw_time
            }
        });
        self.seg_start + (raw_time - base).max(0.0)
    }

    fn decode(&mut self, pipe: &Pipe, job: &Job) -> Result<(), String> {
        let mut generation = job.generation;
        let mut landing = Landing::at(job.start);
        let mut epoch = 0.0;
        let mut first = true;
        let mut waiting_since = rune_core::time::now();
        let mut burst = 0;

        loop {
            let stream;
            {
                let mut s = pipe.lock();
                loop {
                    if s.stop {
                        self.session.stop();
                        return Ok(());
                    }
                    if s.seek.is_some() {
                        let mut seek_aborted = false;
                        s = pipe.land_seeks(s, |to, g| {
                            generation = g;
                            landing = Landing::at(to);
                            epoch = 0.0;
                            let _ = self.session.seek(to, g);
                            if let Some(o) = &self.output {
                                o.flush();
                            }
                            match wait_for_segment(pipe, &self.session, g)? {
                                Some(seg) => self.open_segment(&seg, job, pipe)?,
                                None => seek_aborted = true,
                            }
                            Ok::<(), String>(())
                        })?;
                        if seek_aborted {
                            return Ok(());
                        }
                        continue;
                    }
                    if self.output.is_some() != s.sound.is_some() {
                        let on = s.sound.is_some();
                        drop(s);
                        self.select_sound(on, &job.clock);
                        s = pipe.lock();
                    }
                    if let (Some(o), Some(gain)) = (&self.output, s.sound) {
                        o.set_gain(gain);
                    }
                    self.follow_layer(&mut s);
                    self.reclaim(&mut s);
                    if self.awaiting_gpu
                        && crate::native_surface::windows::presenting_device().is_some()
                    {
                        return Err(DEVICE_CHANGED.into());
                    }
                    let ended = s.ended == Some(generation);
                    let room = s.frames.len() < AHEAD;
                    let texture = self.ring.as_ref().is_none_or(Ring::free);
                    let sound_full = self.output.as_ref().is_some_and(Output::full);
                    let sound = !ended && self.audio_open && !sound_full;
                    let picture = !ended && self.video_open && room && texture;
                    let idle = !ended && !self.video_open && !self.audio_open;
                    if let Some(next) = next_read(sound, picture, landing.landed(), burst) {
                        stream = Some(next);
                        break;
                    }
                    if idle {
                        stream = None;
                        break;
                    }
                    let touched = s.touched.unwrap_or(waiting_since);
                    let playing_sound = self.output.as_ref().is_some_and(|o| !o.idle());
                    if job.kind == Kind::Video && !playing_sound && touched.elapsed() > PARK {
                        self.session.stop();
                        s.news.push(News::Parked);
                        drop(s);
                        (job.notify)();
                        return Ok(());
                    }
                    let patience = if (self.video_open && room && !texture) || sound_full {
                        RING_POLL
                    } else {
                        PARK
                    };
                    s = pipe.wait(s, patience);
                    waiting_since = waiting_since.max(touched);
                }
            }

            if stream.is_none() {
                // Current segment exhausted: pull next prefetched segment, loop, or end.
                match self.advance_segment(pipe, job, &mut generation, &mut landing, &mut epoch)? {
                    Advance::Continue => continue,
                    Advance::Stop => return Ok(()),
                }
            }

            let index = match stream {
                Some(Stream::Sound) => {
                    burst += 1;
                    self.audio
                }
                Some(Stream::Picture) => {
                    burst = 0;
                    self.video
                }
                None => None,
            };
            let mut read = mf::read(
                &self.reader,
                index.unwrap_or(MF_SOURCE_READER_ANY_STREAM.0 as u32),
            )?;
            if read.flags & MF_SOURCE_READERF_ERROR.0 != 0 {
                return Err("HLS decoder stopped with an error".into());
            }
            let is_video = self.video == Some(read.stream);
            if read.flags & MF_SOURCE_READERF_ENDOFSTREAM.0 != 0 {
                if is_video {
                    self.video_open = false;
                } else {
                    self.audio_open = false;
                }
            }
            if let Some(sample) = read.sample.take() {
                let clip_time = self.normalize_pts(read.time);
                let pts = epoch + clip_time;
                if is_video {
                    if landing.keeps(clip_time, self.facts.meta.frame)
                        && !self.late_on_layer(pipe, job, pts)
                    {
                        let (picture, texture) = self.picture(&sample)?;
                        let blitted = self.blit_on_layer(pipe, job, texture);
                        drop(sample);
                        let decoded = Decoded {
                            generation,
                            pts,
                            picture,
                        };
                        match blitted.and_then(|clock| self.wait_until_due(pipe, job, clock, pts)) {
                            Some(mut s) => {
                                s.push_presented(decoded, rune_core::time::now());
                                drop(s);
                                if !self.present(pipe, generation) {
                                    (job.notify)();
                                }
                            }
                            None => {
                                if pipe.lock().push(decoded) {
                                    (job.notify)();
                                }
                            }
                        }
                        if first {
                            rune_core::marks::mark("video.hls.first_frame");
                            if logging() {
                                log!(
                                    "HLS opened in {} us, first picture at {} us",
                                    self.opened_us,
                                    us(job.started)
                                );
                            }
                            first = false;
                        }
                    }
                } else if let Some(o) = &self.output {
                    o.push(&sample, pts, landing.skip_before, generation);
                }
            }
        }
    }

    fn advance_segment(
        &mut self,
        pipe: &Pipe,
        job: &Job,
        generation: &mut u32,
        landing: &mut Landing,
        epoch: &mut f64,
    ) -> Result<Advance, String> {
        loop {
            {
                let s = pipe.lock();
                if s.stop {
                    self.session.stop();
                    return Ok(Advance::Stop);
                }
                if s.seek.is_some() {
                    return Ok(Advance::Continue);
                }
            }
            match self.session.next_segment(*generation, SEGMENT_WAIT_SLICE) {
                NextSegment::Ready(seg) => {
                    self.open_segment(&seg, job, pipe)?;
                    return Ok(Advance::Continue);
                }
                NextSegment::Pending => continue,
                NextSegment::Failed(err) => return Err(err),
                NextSegment::EndOfStream => {
                    let looping = pipe.lock().looping;
                    if looping && self.facts.meta.duration > 0.0 {
                        *epoch += self.facts.meta.duration;
                        *landing = Landing::at(0.0);
                        self.session.loop_restart(*generation);
                        continue;
                    }
                    pipe.lock().ended = Some(*generation);
                    if let Some(o) = &self.output {
                        o.end(*generation);
                    }
                    (job.notify)();
                    return Ok(Advance::Continue);
                }
            }
        }
    }

    fn reclaim(&mut self, s: &mut Inbox) {
        for dropped in s.unread.drain(..) {
            if let (Some(ring), Pic::Gpu(planes, value)) = (&mut self.ring, &dropped.picture) {
                ring.unread(planes, *value);
            }
        }
    }

    fn follow_layer(&mut self, s: &mut Inbox) {
        let Some(size) = s.layer.filter(|_| !s.layer_refused) else {
            if s.layer.is_none() && self.layer.take().is_some() {
                s.layer_surface = None;
                s.layer_shown = None;
            }
            return;
        };
        let made = match &mut self.layer {
            Some(layer) => layer.resize(size),
            None => self.make_layer(size).map(|layer| {
                s.layer_surface = Some(layer.surface());
                self.layer = Some(layer);
                self.presented_at = None;
            }),
        };
        if let Err(e) = made {
            if logging() {
                log!("no layer for HLS clip: {e}; window draws pictures");
            }
            s.layer_refused = true;
            s.layer_surface = None;
            s.layer_shown = None;
            self.layer = None;
        }
    }

    fn make_layer(&self, size: (u32, u32)) -> Result<Layer, String> {
        let picture = self.facts.picture.ok_or("no video stream")?;
        let device = self
            .ring
            .as_ref()
            .filter(|_| self.facts.hardware)
            .map(Ring::device)
            .ok_or("software pictures draw in the UI")?;
        Layer::new(device, picture, size)
    }

    fn late_on_layer(&self, pipe: &Pipe, job: &Job, pts: f64) -> bool {
        let (Some(_), Some(clock)) = (&self.layer, job.clock.get()) else {
            return false;
        };
        {
            let s = pipe.lock();
            if s.layer.is_none() || s.layer_refused {
                return false;
            }
        }
        let now = rune_core::time::now();
        superseded(
            clock.position(now),
            pts,
            self.facts.meta.frame,
            self.presented_at
                .map(|at| now.saturating_duration_since(at)),
        )
    }

    fn blit_on_layer(
        &mut self,
        pipe: &Pipe,
        job: &Job,
        texture: Option<(ID3D11Texture2D, u32)>,
    ) -> Option<Anchor> {
        let (Some(layer), Some((texture, index)), Some(clock)) =
            (&mut self.layer, texture, job.clock.get())
        else {
            return None;
        };
        if pipe.lock().layer.is_none() {
            return None;
        }
        if layer.blit(&texture, index).is_err() {
            pipe.lock().layer_refused = true;
            return None;
        }
        Some(clock)
    }

    fn wait_until_due<'a>(
        &self,
        pipe: &'a Pipe,
        job: &Job,
        clock: Anchor,
        pts: f64,
    ) -> Option<MutexGuard<'a, Inbox>> {
        self.layer.as_ref()?;
        let at = clock
            .when(pts)
            .checked_sub(LAYER_LEAD)
            .unwrap_or_else(|| clock.when(pts));
        let mut s = pipe.lock();
        loop {
            if s.stop || s.seek.is_some() || s.layer.is_none() {
                return None;
            }
            let now = rune_core::time::now();
            if now >= at {
                break;
            }
            s = pipe.wait(s, at - now);
        }
        (job.clock.get() == Some(clock)).then_some(s)
    }

    fn present(&mut self, pipe: &Pipe, generation: u32) -> bool {
        let Some(layer) = &self.layer else {
            return false;
        };
        if layer.present().is_err() {
            pipe.lock().layer_refused = true;
            return false;
        }
        pipe.lock().layer_shown = Some((layer.size, generation));
        self.presented_at = Some(rune_core::time::now());
        true
    }

    fn picture(
        &mut self,
        sample: &IMFSample,
    ) -> Result<(Pic, Option<(ID3D11Texture2D, u32)>), String> {
        let picture = self.facts.picture.ok_or("no video stream")?;
        // SAFETY: COM reads on a sample owned by this decoder thread.
        let texture = unsafe {
            sample
                .GetBufferByIndex(0)
                .ok()
                .and_then(|b| b.cast::<IMFDXGIBuffer>().ok())
                .and_then(|b| {
                    let mut raw = std::ptr::null_mut();
                    b.GetResource(&ID3D11Texture2D::IID, &mut raw).ok()?;
                    Some((
                        ID3D11Texture2D::from_raw(raw),
                        b.GetSubresourceIndex().ok()?,
                    ))
                })
        };
        if self.ring.as_ref().is_some_and(Ring::stale) {
            let device = gpu::device()?;
            if !self.ring.as_mut().is_some_and(|r| r.adopt(&device)) {
                return Err(DEVICE_CHANGED.into());
            }
        }
        let Some(ring) = &mut self.ring else {
            return mf::nv12_bytes(sample, &picture).map(|bytes| (Pic::Cpu(bytes), None));
        };
        let slot = ring.take()?.ok_or("no free video texture")?;
        let (planes, value) = match &texture {
            Some((texture, index)) => {
                let mut desc = Default::default();
                // SAFETY: plain query on a live D3D11 texture.
                unsafe { texture.GetDesc(&mut desc) };
                self.facts.hardware = desc.BindFlags & D3D11_BIND_DECODER.0 as u32 != 0;
                ring.copy(slot, texture, *index)
            }
            None => ring.upload(slot, &mf::nv12_bytes(sample, &picture)?),
        };
        Ok((Pic::Gpu(planes, value), texture))
    }
}

enum Advance {
    Continue,
    Stop,
}

fn wait_for_segment(
    pipe: &Pipe,
    session: &HlsSession,
    generation: u32,
) -> Result<Option<FetchedSegment>, String> {
    loop {
        if pipe.lock().stop {
            session.stop();
            return Ok(None);
        }
        match session.next_segment(generation, SEGMENT_WAIT_SLICE) {
            NextSegment::Ready(seg) => return Ok(Some(seg)),
            NextSegment::Pending => continue,
            NextSegment::EndOfStream => {
                return Err("HLS playlist ended before first segment".into());
            }
            NextSegment::Failed(err) => return Err(err),
        }
    }
}

#[cfg(test)]
pub(crate) fn test_live_decode_and_seek(
    url: &str,
    seek_to: f64,
) -> (f64, Option<(u32, u32)>, bool, bool) {
    use super::super::MediaSource;
    use super::super::hls::prefetch::pack_display_size;
    use super::audio::Clock as AudioClock;
    use super::decoder::start;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicU64;

    let pipe = start(Job {
        source: MediaSource::Url(url.to_string()),
        headers: Vec::new(),
        hls: true,
        kind: Kind::Video,
        start: 0.0,
        generation: 1,
        looping: false,
        sound: None,
        notify: Box::new(|| {}),
        clock: Arc::new(AudioClock::default()),
        display_size: Arc::new(AtomicU64::new(pack_display_size(64, 48))),
        active_variant: Arc::new(Mutex::new(None)),
        started: rune_core::time::now(),
    });

    let deadline = rune_core::time::now() + Duration::from_secs(10);
    let mut opened = None;
    let mut got_frame = false;
    while rune_core::time::now() < deadline {
        let mut s = pipe.lock();
        for n in s.news.drain(..) {
            match n {
                News::Opened(f) => opened = Some(f),
                News::Failed(e) => panic!("HLS live decoder failed: {e}"),
                News::Updated(_) | News::Parked => {}
            }
        }
        if !s.frames.is_empty() {
            got_frame = true;
            s.frames.clear();
            drop(s);
            pipe.poke();
            break;
        }
        drop(s);
        std::thread::sleep(Duration::from_millis(10));
    }
    let facts = opened.expect("HLS decoder should send News::Opened");

    {
        let mut s = pipe.lock();
        s.seek = Some((seek_to, 2));
    }
    pipe.poke();

    let mut got_seek_frame = false;
    let seek_deadline = rune_core::time::now() + Duration::from_secs(10);
    while rune_core::time::now() < seek_deadline {
        let s = pipe.lock();
        if s.frames
            .iter()
            .any(|f| f.generation == 2 && f.pts >= seek_to - 0.25)
        {
            got_seek_frame = true;
            break;
        }
        drop(s);
        std::thread::sleep(Duration::from_millis(10));
    }

    pipe.lock().stop = true;
    pipe.poke();
    (
        facts.meta.duration,
        facts.meta.size,
        got_frame,
        got_seek_frame,
    )
}
