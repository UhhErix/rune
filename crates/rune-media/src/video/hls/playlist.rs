//! RFC 8216 M3U8 master and media playlist parser and seek-to-segment mapper.

#![forbid(unsafe_code)]

use super::resolve_url;
use std::collections::HashMap;

/// A parsed HLS playlist: either a multivariant (master) playlist or a media playlist.
#[derive(Clone, Debug, PartialEq)]
pub enum Playlist {
    Master(MasterPlaylist),
    Media(MediaPlaylist),
}

/// Multivariant (master) playlist listing selectable video/audio renditions.
#[derive(Clone, Debug, PartialEq)]
pub struct MasterPlaylist {
    pub variants: Vec<Variant>,
    pub renditions: Vec<Rendition>,
}

/// One `#EXT-X-STREAM-INF` variant stream.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: u64,
    pub average_bandwidth: Option<u64>,
    pub resolution: Option<(u32, u32)>,
    pub codecs: Option<String>,
    pub frame_rate: Option<f64>,
    pub audio_group: Option<String>,
}

/// One `#EXT-X-MEDIA` alternate rendition entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendition {
    pub kind: String,
    pub group_id: String,
    pub name: String,
    pub uri: Option<String>,
    pub default: bool,
    pub autoselect: bool,
    pub language: Option<String>,
}

/// Byte subrange within a resource (`#EXT-X-BYTERANGE` or `#EXT-X-MAP` `BYTERANGE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub length: u64,
    pub offset: u64,
}

impl ByteRange {
    /// Formats as an RFC 7233 `Range` HTTP header value (`bytes=start-end`, inclusive).
    pub fn http_range_header(self) -> String {
        let end = self.offset.saturating_add(self.length.saturating_sub(1));
        format!("bytes={}-{}", self.offset, end)
    }
}

/// Media initialization section (`#EXT-X-MAP`, e.g. fMP4 `ftyp`+`moov`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MapSegment {
    pub uri: String,
    pub byte_range: Option<ByteRange>,
}

/// One media segment from `#EXTINF` and its URI.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSegment {
    pub sequence: u64,
    pub uri: String,
    pub duration: f64,
    /// Cumulative start time from the beginning of the playlist, in seconds.
    pub start_time: f64,
    pub byte_range: Option<ByteRange>,
    pub map: Option<MapSegment>,
    pub discontinuity: bool,
    pub discontinuity_sequence: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaylistType {
    #[default]
    Unknown,
    Event,
    Vod,
}

/// Media playlist containing an ordered timeline of downloadable segments.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaPlaylist {
    pub target_duration: f64,
    pub media_sequence: u64,
    pub discontinuity_sequence: u64,
    pub playlist_type: PlaylistType,
    pub end_list: bool,
    pub total_duration: f64,
    pub segments: Vec<MediaSegment>,
}

/// Result of mapping a seek timestamp to the nearest media segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeekTarget {
    pub segment_index: usize,
    pub segment_start: f64,
    pub offset_in_segment: f64,
}

impl MediaPlaylist {
    /// Maps `seconds` to the segment covering that point on the playlist timeline.
    /// Negative or non-finite values clamp to segment 0; values at or past `total_duration`
    /// map to the final segment.
    pub fn segment_at(&self, seconds: f64) -> Option<SeekTarget> {
        if self.segments.is_empty() {
            return None;
        }
        let t = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        if t <= 0.0 {
            return Some(SeekTarget {
                segment_index: 0,
                segment_start: 0.0,
                offset_in_segment: 0.0,
            });
        }
        // First segment whose end time is strictly greater than `t`.
        let idx = self
            .segments
            .partition_point(|seg| seg.start_time + seg.duration <= t);
        let segment_index = idx.min(self.segments.len() - 1);
        let seg = &self.segments[segment_index];
        let offset = (t - seg.start_time).clamp(0.0, seg.duration);
        Some(SeekTarget {
            segment_index,
            segment_start: seg.start_time,
            offset_in_segment: offset,
        })
    }
}

/// Parses an M3U8 playlist (master or media), resolving relative URIs against `base_url`.
/// Rejects encrypted / DRM-protected playlists (any `#EXT-X-KEY` or `#EXT-X-SESSION-KEY`
/// whose `METHOD` is not `NONE`).
pub fn parse_playlist(text: &str, base_url: &str) -> Result<Playlist, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    match lines.next() {
        Some("#EXTM3U") => {}
        Some(first) if first.starts_with("#EXTM3U") => {}
        _ => return Err("not an HLS playlist (missing #EXTM3U header)".into()),
    }

    let raw_lines: Vec<&str> = lines.collect();
    let is_master = raw_lines.iter().any(|l| {
        l.starts_with("#EXT-X-STREAM-INF:")
            || l.starts_with("#EXT-X-MEDIA:")
            || l.starts_with("#EXT-X-I-FRAME-STREAM-INF:")
    });

    if is_master {
        parse_master(&raw_lines, base_url).map(Playlist::Master)
    } else {
        parse_media(&raw_lines, base_url).map(Playlist::Media)
    }
}

fn parse_master(lines: &[&str], base_url: &str) -> Result<MasterPlaylist, String> {
    let mut variants = Vec::new();
    let mut renditions = Vec::new();
    let mut pending_stream: Option<PendingVariant> = None;

    for &line in lines {
        if let Some(attrs_str) = line.strip_prefix("#EXT-X-SESSION-KEY:") {
            check_encryption(attrs_str)?;
        } else if let Some(attrs_str) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            pending_stream = Some(parse_stream_inf(attrs_str)?);
        } else if let Some(attrs_str) = line.strip_prefix("#EXT-X-MEDIA:") {
            renditions.push(parse_media_rendition(attrs_str, base_url)?);
        } else if line.starts_with('#') {
            continue;
        } else if let Some(info) = pending_stream.take() {
            let uri = resolve_url(base_url, line)?;
            variants.push(Variant {
                uri,
                bandwidth: info.bandwidth,
                average_bandwidth: info.average_bandwidth,
                resolution: info.resolution,
                codecs: info.codecs,
                frame_rate: info.frame_rate,
                audio_group: info.audio_group,
            });
        }
    }

    if variants.is_empty() {
        return Err("HLS master playlist contains no stream variants".into());
    }
    Ok(MasterPlaylist {
        variants,
        renditions,
    })
}

struct PendingVariant {
    bandwidth: u64,
    average_bandwidth: Option<u64>,
    resolution: Option<(u32, u32)>,
    codecs: Option<String>,
    frame_rate: Option<f64>,
    audio_group: Option<String>,
}

fn parse_stream_inf(attrs_str: &str) -> Result<PendingVariant, String> {
    let mut bandwidth = None;
    let mut average_bandwidth = None;
    let mut resolution = None;
    let mut codecs = None;
    let mut frame_rate = None;
    let mut audio_group = None;

    for (key, val) in parse_attributes(attrs_str) {
        match key {
            "BANDWIDTH" => {
                bandwidth = val.parse::<u64>().ok();
            }
            "AVERAGE-BANDWIDTH" => {
                average_bandwidth = val.parse::<u64>().ok();
            }
            "RESOLUTION" => {
                resolution = parse_resolution(val);
            }
            "CODECS" => {
                let unquoted = unquote(val);
                if !unquoted.is_empty() {
                    codecs = Some(unquoted.to_string());
                }
            }
            "FRAME-RATE" => {
                frame_rate = val
                    .parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite() && *f > 0.0);
            }
            "AUDIO" => {
                let unquoted = unquote(val);
                if !unquoted.is_empty() {
                    audio_group = Some(unquoted.to_string());
                }
            }
            _ => {}
        }
    }

    let bandwidth =
        bandwidth.ok_or_else(|| "HLS #EXT-X-STREAM-INF missing BANDWIDTH".to_string())?;
    Ok(PendingVariant {
        bandwidth,
        average_bandwidth,
        resolution,
        codecs,
        frame_rate,
        audio_group,
    })
}

fn parse_media_rendition(attrs_str: &str, base_url: &str) -> Result<Rendition, String> {
    let mut kind = String::new();
    let mut group_id = String::new();
    let mut name = String::new();
    let mut uri = None;
    let mut default = false;
    let mut autoselect = false;
    let mut language = None;

    for (key, val) in parse_attributes(attrs_str) {
        match key {
            "TYPE" => kind = unquote(val).to_string(),
            "GROUP-ID" => group_id = unquote(val).to_string(),
            "NAME" => name = unquote(val).to_string(),
            "URI" => {
                let raw = unquote(val);
                if !raw.is_empty() {
                    uri = Some(resolve_url(base_url, raw)?);
                }
            }
            "DEFAULT" => default = val.eq_ignore_ascii_case("YES"),
            "AUTOSELECT" => autoselect = val.eq_ignore_ascii_case("YES"),
            "LANGUAGE" => language = Some(unquote(val).to_string()),
            _ => {}
        }
    }

    Ok(Rendition {
        kind,
        group_id,
        name,
        uri,
        default,
        autoselect,
        language,
    })
}

fn parse_media(lines: &[&str], base_url: &str) -> Result<MediaPlaylist, String> {
    let mut target_duration = 0.0;
    let mut media_sequence: u64 = 0;
    let mut discontinuity_sequence: u64 = 0;
    let mut cur_discontinuity_seq: u64 = 0;
    let mut playlist_type = PlaylistType::Unknown;
    let mut end_list = false;

    let mut segments = Vec::new();
    let mut next_seq: Option<u64> = None;
    let mut timeline = 0.0;
    let mut current_map: Option<MapSegment> = None;
    let mut pending_duration: Option<f64> = None;
    let mut pending_byte_range: Option<(u64, Option<u64>)> = None;
    let mut pending_discontinuity = false;
    let mut last_range_end: HashMap<String, u64> = HashMap::new();

    for &line in lines {
        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            target_duration = rest.trim().parse::<f64>().unwrap_or(0.0).max(0.0);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            if let Ok(seq) = rest.trim().parse::<u64>() {
                media_sequence = seq;
                if next_seq.is_none() {
                    next_seq = Some(seq);
                }
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-DISCONTINUITY-SEQUENCE:") {
            if let Ok(seq) = rest.trim().parse::<u64>() {
                discontinuity_sequence = seq;
                cur_discontinuity_seq = seq;
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-PLAYLIST-TYPE:") {
            playlist_type = match rest.trim() {
                "VOD" => PlaylistType::Vod,
                "EVENT" => PlaylistType::Event,
                _ => PlaylistType::Unknown,
            };
        } else if line == "#EXT-X-ENDLIST" {
            end_list = true;
        } else if let Some(attrs_str) = line.strip_prefix("#EXT-X-KEY:") {
            check_encryption(attrs_str)?;
        } else if let Some(attrs_str) = line.strip_prefix("#EXT-X-MAP:") {
            current_map = Some(parse_map(attrs_str, base_url)?);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            pending_byte_range = Some(parse_byte_range_spec(rest.trim())?);
        } else if line == "#EXT-X-DISCONTINUITY" {
            pending_discontinuity = true;
            cur_discontinuity_seq = cur_discontinuity_seq.saturating_add(1);
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let dur_part = rest.split(',').next().unwrap_or("").trim();
            let dur = dur_part
                .parse::<f64>()
                .ok()
                .filter(|d| d.is_finite() && *d >= 0.0)
                .ok_or_else(|| format!("invalid #EXTINF duration: {line}"))?;
            pending_duration = Some(dur);
        } else if line.starts_with('#') {
            continue;
        } else if let Some(duration) = pending_duration.take() {
            let uri = resolve_url(base_url, line)?;
            let byte_range = if let Some((length, explicit_offset)) = pending_byte_range.take() {
                let offset =
                    explicit_offset.unwrap_or_else(|| *last_range_end.get(&uri).unwrap_or(&0));
                last_range_end.insert(uri.clone(), offset.saturating_add(length));
                Some(ByteRange { length, offset })
            } else {
                None
            };
            let seq = *next_seq.get_or_insert(media_sequence);
            next_seq = Some(seq.saturating_add(1));
            let discontinuity = std::mem::take(&mut pending_discontinuity);
            segments.push(MediaSegment {
                sequence: seq,
                uri,
                duration,
                start_time: timeline,
                byte_range,
                map: current_map.clone(),
                discontinuity,
                discontinuity_sequence: cur_discontinuity_seq,
            });
            timeline += duration;
        }
    }

    if segments.is_empty() {
        return Err("HLS media playlist contains no segments".into());
    }
    if target_duration == 0.0 {
        target_duration = segments.iter().map(|s| s.duration).fold(0.0_f64, f64::max);
    }

    Ok(MediaPlaylist {
        target_duration,
        media_sequence,
        discontinuity_sequence,
        playlist_type,
        end_list,
        total_duration: timeline,
        segments,
    })
}

fn check_encryption(attrs_str: &str) -> Result<(), String> {
    let mut method = None;
    let mut keyformat = None;
    for (k, v) in parse_attributes(attrs_str) {
        match k {
            "METHOD" => method = Some(unquote(v)),
            "KEYFORMAT" => keyformat = Some(unquote(v)),
            _ => {}
        }
    }
    match method {
        None | Some("NONE") => Ok(()),
        Some(other) => {
            if let Some(fmt) = keyformat {
                Err(format!(
                    "encrypted/DRM HLS stream is not supported (METHOD={other}, KEYFORMAT={fmt})"
                ))
            } else {
                Err(format!(
                    "encrypted/DRM HLS stream is not supported (METHOD={other})"
                ))
            }
        }
    }
}

fn parse_map(attrs_str: &str, base_url: &str) -> Result<MapSegment, String> {
    let mut uri = None;
    let mut byte_range = None;
    for (key, val) in parse_attributes(attrs_str) {
        match key {
            "URI" => {
                uri = Some(resolve_url(base_url, unquote(val))?);
            }
            "BYTERANGE" => {
                let (length, offset) = parse_byte_range_spec(unquote(val))?;
                byte_range = Some(ByteRange {
                    length,
                    offset: offset.unwrap_or(0),
                });
            }
            _ => {}
        }
    }
    let uri = uri.ok_or_else(|| "HLS #EXT-X-MAP missing URI".to_string())?;
    Ok(MapSegment { uri, byte_range })
}

fn parse_byte_range_spec(spec: &str) -> Result<(u64, Option<u64>), String> {
    if let Some((len_s, off_s)) = spec.split_once('@') {
        let length = len_s
            .parse::<u64>()
            .map_err(|_| format!("invalid BYTERANGE length: {spec}"))?;
        let offset = off_s
            .parse::<u64>()
            .map_err(|_| format!("invalid BYTERANGE offset: {spec}"))?;
        if length == 0 {
            return Err(format!("zero-length BYTERANGE: {spec}"));
        }
        Ok((length, Some(offset)))
    } else {
        let length = spec
            .parse::<u64>()
            .map_err(|_| format!("invalid BYTERANGE length: {spec}"))?;
        if length == 0 {
            return Err(format!("zero-length BYTERANGE: {spec}"));
        }
        Ok((length, None))
    }
}

fn parse_resolution(val: &str) -> Option<(u32, u32)> {
    let (w, h) = val.split_once('x').or_else(|| val.split_once('X'))?;
    let w = w.parse::<u32>().ok().filter(|&n| n > 0)?;
    let h = h.parse::<u32>().ok().filter(|&n| n > 0)?;
    Some((w, h))
}

fn unquote(val: &str) -> &str {
    val.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(val)
}

/// Parses an RFC 8216 attribute list (`KEY=VALUE,KEY2="A,B"`), respecting quoted commas.
pub(crate) fn parse_attributes(input: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b',' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && bytes[i] != b',' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            break;
        }
        let key = input[key_start..i].trim();
        i += 1; // skip '='
        let val_start = i;
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let val = input[val_start..i].trim();
            out.push((key, val));
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
        } else {
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            let val = input[val_start..i].trim();
            out.push((key, val));
        }
    }
    out
}
