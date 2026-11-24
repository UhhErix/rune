use super::abr::{
    AbrConfig, AbrController, AbrInput, AbrVariant, decide_variant, select_initial_variant,
};
use super::http::{CancelToken, HttpError, MAX_PLAYLIST_BYTES, fetch_with_schedule};
use super::playlist::{ByteRange, Playlist, PlaylistType, parse_playlist};
use super::{is_hls_url, resolve_url};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn hls_url_detection_checks_path_extension_only() {
    assert!(is_hls_url("https://cdn.example.com/trailer/master.m3u8"));
    assert!(is_hls_url(
        "http://127.0.0.1:8080/live/stream.M3U8?token=abc#t=10"
    ));
    assert!(is_hls_url("https://cdn.example.com/playlist.m3u"));
    assert!(!is_hls_url("https://cdn.example.com/trailer.mp4"));
    assert!(!is_hls_url(
        "https://m3u8.example.com/video.mp4?next=a.m3u8"
    ));
    assert!(!is_hls_url("file:///C:/video/master.m3u8"));
    assert!(!is_hls_url("master.m3u8"));
}

#[test]
fn url_resolution_handles_relative_root_protocol_and_dot_segments() {
    let base = "https://cdn.example.com/games/123/hls/master.m3u8?auth=1";
    assert_eq!(
        resolve_url(base, "720p/index.m3u8").unwrap(),
        "https://cdn.example.com/games/123/hls/720p/index.m3u8"
    );
    assert_eq!(
        resolve_url(base, "../audio/en.m3u8").unwrap(),
        "https://cdn.example.com/games/123/audio/en.m3u8"
    );
    assert_eq!(
        resolve_url(base, "/shared/init.mp4").unwrap(),
        "https://cdn.example.com/shared/init.mp4"
    );
    assert_eq!(
        resolve_url(base, "//other.cdn.net/v/seg0.m4s").unwrap(),
        "https://other.cdn.net/v/seg0.m4s"
    );
    assert_eq!(
        resolve_url(base, "https://abs.example.com/a.ts").unwrap(),
        "https://abs.example.com/a.ts"
    );
}

#[test]
fn parse_master_playlist_fixture() {
    let m3u8 = "\u{feff}#EXTM3U\r\n\
#EXT-X-VERSION:7\r\n\
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aac\",NAME=\"English\",LANGUAGE=\"en\",DEFAULT=YES,AUTOSELECT=YES,URI=\"audio/en.m3u8\"\r\n\
#EXT-X-STREAM-INF:BANDWIDTH=800000,AVERAGE-BANDWIDTH=720000,RESOLUTION=640x360,CODECS=\"avc1.4d401e,mp4a.40.2\",FRAME-RATE=30.000,AUDIO=\"aac\"\r\n\
360p/index.m3u8\r\n\
#EXT-X-STREAM-INF:BANDWIDTH=2800000,RESOLUTION=1280x720,CODECS=\"avc1.64001f,mp4a.40.2\",FRAME-RATE=60.000\r\n\
720p/index.m3u8?sig=xyz\r\n\
#EXT-X-STREAM-INF:BANDWIDTH=5600000,RESOLUTION=1920x1080,CODECS=\"hvc1.1.6.L120.90,mp4a.40.2\"\r\n\
/1080p/index.m3u8\r\n";

    let Playlist::Master(master) =
        parse_playlist(m3u8, "https://video.example.com/trailers/game1/master.m3u8").unwrap()
    else {
        panic!("expected master playlist");
    };
    assert_eq!(master.renditions.len(), 1);
    assert_eq!(master.renditions[0].group_id, "aac");
    assert_eq!(
        master.renditions[0].uri.as_deref(),
        Some("https://video.example.com/trailers/game1/audio/en.m3u8")
    );
    assert!(master.renditions[0].default);

    assert_eq!(master.variants.len(), 3);
    assert_eq!(master.variants[0].bandwidth, 800_000);
    assert_eq!(master.variants[0].average_bandwidth, Some(720_000));
    assert_eq!(master.variants[0].resolution, Some((640, 360)));
    assert_eq!(
        master.variants[0].codecs.as_deref(),
        Some("avc1.4d401e,mp4a.40.2")
    );
    assert_eq!(master.variants[0].frame_rate, Some(30.0));
    assert_eq!(
        master.variants[0].uri,
        "https://video.example.com/trailers/game1/360p/index.m3u8"
    );

    assert_eq!(master.variants[1].resolution, Some((1280, 720)));
    assert_eq!(
        master.variants[1].uri,
        "https://video.example.com/trailers/game1/720p/index.m3u8?sig=xyz"
    );

    assert_eq!(master.variants[2].resolution, Some((1920, 1080)));
    assert_eq!(
        master.variants[2].uri,
        "https://video.example.com/1080p/index.m3u8"
    );
}

#[test]
fn parse_media_playlist_with_map_and_byteranges() {
    let m3u8 = "\
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:4
#EXT-X-MEDIA-SEQUENCE:10
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI=\"stream.mp4\",BYTERANGE=\"720@0\"
#EXTINF:4.000,
#EXT-X-BYTERANGE:10000@720
stream.mp4
#EXTINF:3.500,
#EXT-X-BYTERANGE:12000
stream.mp4
#EXTINF:2.500,
#EXT-X-BYTERANGE:8000
stream.mp4
#EXT-X-ENDLIST
";

    let Playlist::Media(media) =
        parse_playlist(m3u8, "https://cdn.example.com/v/media.m3u8").unwrap()
    else {
        panic!("expected media playlist");
    };
    assert_eq!(media.playlist_type, PlaylistType::Vod);
    assert!(media.end_list);
    assert_eq!(media.media_sequence, 10);
    assert!((media.total_duration - 10.0).abs() < 1e-6);
    assert_eq!(media.segments.len(), 3);

    let map = media.segments[0].map.as_ref().unwrap();
    assert_eq!(map.uri, "https://cdn.example.com/v/stream.mp4");
    assert_eq!(
        map.byte_range,
        Some(ByteRange {
            length: 720,
            offset: 0
        })
    );
    assert_eq!(map.byte_range.unwrap().http_range_header(), "bytes=0-719");

    assert_eq!(media.segments[0].sequence, 10);
    assert_eq!(
        media.segments[0].byte_range,
        Some(ByteRange {
            length: 10_000,
            offset: 720
        })
    );
    assert_eq!(
        media.segments[0].byte_range.unwrap().http_range_header(),
        "bytes=720-10719"
    );

    // Omitted @offset continues immediately after previous subrange of the same URI.
    assert_eq!(media.segments[1].sequence, 11);
    assert_eq!(
        media.segments[1].byte_range,
        Some(ByteRange {
            length: 12_000,
            offset: 10_720
        })
    );
    assert!((media.segments[1].start_time - 4.0).abs() < 1e-6);

    assert_eq!(media.segments[2].sequence, 12);
    assert_eq!(
        media.segments[2].byte_range,
        Some(ByteRange {
            length: 8_000,
            offset: 22_720
        })
    );
    assert!((media.segments[2].start_time - 7.5).abs() < 1e-6);
}

#[test]
fn parse_media_playlist_with_discontinuity_and_new_map() {
    let m3u8 = "\
#EXTM3U
#EXT-X-TARGETDURATION:6
#EXT-X-DISCONTINUITY-SEQUENCE:2
#EXT-X-MAP:URI=\"init_a.mp4\"
#EXTINF:6.0,
seg0.m4s
#EXT-X-DISCONTINUITY
#EXT-X-MAP:URI=\"init_b.mp4\"
#EXTINF:4.0,
seg1.m4s
#EXTINF:5.0,
seg2.m4s
#EXT-X-ENDLIST
";

    let Playlist::Media(media) =
        parse_playlist(m3u8, "https://cdn.example.com/clip/index.m3u8").unwrap()
    else {
        panic!("expected media playlist");
    };
    assert_eq!(media.discontinuity_sequence, 2);
    assert!(!media.segments[0].discontinuity);
    assert_eq!(media.segments[0].discontinuity_sequence, 2);
    assert_eq!(
        media.segments[0].map.as_ref().unwrap().uri,
        "https://cdn.example.com/clip/init_a.mp4"
    );

    assert!(media.segments[1].discontinuity);
    assert_eq!(media.segments[1].discontinuity_sequence, 3);
    assert_eq!(
        media.segments[1].map.as_ref().unwrap().uri,
        "https://cdn.example.com/clip/init_b.mp4"
    );

    assert!(!media.segments[2].discontinuity);
    assert_eq!(media.segments[2].discontinuity_sequence, 3);
}

#[test]
fn encrypted_and_drm_playlists_are_rejected_while_method_none_passes() {
    let aes = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:4.0,\nseg0.ts\n";
    let err = parse_playlist(aes, "https://example.com/a.m3u8").unwrap_err();
    assert!(err.contains("AES-128"), "{err}");

    let drm = "#EXTM3U\n#EXT-X-KEY:METHOD=SAMPLE-AES,URI=\"skd://1\",KEYFORMAT=\"com.apple.streamingkeydelivery\"\n#EXTINF:4.0,\nseg0.m4s\n";
    let err = parse_playlist(drm, "https://example.com/a.m3u8").unwrap_err();
    assert!(
        err.contains("SAMPLE-AES") && err.contains("streamingkeydelivery"),
        "{err}"
    );

    let none = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:4.0,\nseg0.ts\n#EXT-X-ENDLIST\n";
    assert!(parse_playlist(none, "https://example.com/a.m3u8").is_ok());
}

#[test]
fn seek_to_segment_mapping_covers_boundaries_and_clamping() {
    let m3u8 = "\
#EXTM3U
#EXT-X-TARGETDURATION:5
#EXTINF:4.0,
seg0.m4s
#EXTINF:3.0,
seg1.m4s
#EXTINF:5.0,
seg2.m4s
#EXT-X-ENDLIST
";
    let Playlist::Media(media) = parse_playlist(m3u8, "https://example.com/m.m3u8").unwrap() else {
        panic!("expected media");
    };
    let s0 = media.segment_at(0.0).unwrap();
    assert_eq!(s0.segment_index, 0);
    assert_eq!(s0.offset_in_segment, 0.0);

    let neg = media.segment_at(-5.0).unwrap();
    assert_eq!(neg.segment_index, 0);
    assert_eq!(neg.offset_in_segment, 0.0);

    let nan = media.segment_at(f64::NAN).unwrap();
    assert_eq!(nan.segment_index, 0);

    let mid0 = media.segment_at(2.5).unwrap();
    assert_eq!(mid0.segment_index, 0);
    assert!((mid0.offset_in_segment - 2.5).abs() < 1e-6);

    // Exact boundary 4.0 lands on start of segment 1.
    let b1 = media.segment_at(4.0).unwrap();
    assert_eq!(b1.segment_index, 1);
    assert!((b1.segment_start - 4.0).abs() < 1e-6);
    assert!((b1.offset_in_segment - 0.0).abs() < 1e-6);

    let mid2 = media.segment_at(9.25).unwrap();
    assert_eq!(mid2.segment_index, 2);
    assert!((mid2.segment_start - 7.0).abs() < 1e-6);
    assert!((mid2.offset_in_segment - 2.25).abs() < 1e-6);

    // Past duration clamps to last segment.
    let end = media.segment_at(99.0).unwrap();
    assert_eq!(end.segment_index, 2);
    assert!((end.offset_in_segment - 5.0).abs() < 1e-6);
}

fn sample_ladder() -> Vec<AbrVariant> {
    vec![
        AbrVariant {
            index: 0,
            bandwidth: 5_000_000,
            resolution: Some((1920, 1080)),
            codecs: Some("avc1.640028,mp4a.40.2".into()),
        },
        AbrVariant {
            index: 1,
            bandwidth: 800_000,
            resolution: Some((640, 360)),
            codecs: Some("avc1.4d401e,mp4a.40.2".into()),
        },
        AbrVariant {
            index: 2,
            bandwidth: 2_500_000,
            resolution: Some((1280, 720)),
            codecs: Some("avc1.64001f,mp4a.40.2".into()),
        },
        AbrVariant {
            index: 3,
            bandwidth: 128_000,
            resolution: None,
            codecs: Some("mp4a.40.2".into()),
        },
    ]
}

#[test]
fn abr_starts_low_steps_up_and_down_and_caps_by_element_size() {
    let variants = sample_ladder();
    let cfg = AbrConfig::default();

    // Start low: picks 360p (index 1, 800 kbps), ignoring audio-only index 3.
    assert_eq!(
        select_initial_variant(&variants, Some((1920, 1080))),
        Some(1)
    );

    // Step up requires healthy buffer AND confirmation hysteresis (2 samples by default).
    let d1 = decide_variant(
        &AbrInput {
            variants: &variants,
            current_index: 1,
            throughput_bps: 10_000_000.0,
            buffer_secs: 6.0,
            display_size: Some((1920, 1080)),
            consecutive_high: 0,
        },
        &cfg,
    );
    assert_eq!(d1.variant_index, 1);
    assert_eq!(d1.next_consecutive_high, 1);

    let d2 = decide_variant(
        &AbrInput {
            variants: &variants,
            current_index: 1,
            throughput_bps: 10_000_000.0,
            buffer_secs: 6.0,
            display_size: Some((1920, 1080)),
            consecutive_high: d1.next_consecutive_high,
        },
        &cfg,
    );
    assert_eq!(d2.variant_index, 2, "steps up one rung from 360p to 720p");
    assert_eq!(d2.next_consecutive_high, 0);

    // Display size cap: element is 1280x720, so 1080p must NOT be selected even at 50 Mbps.
    let capped = decide_variant(
        &AbrInput {
            variants: &variants,
            current_index: 2,
            throughput_bps: 50_000_000.0,
            buffer_secs: 8.0,
            display_size: Some((1280, 720)),
            consecutive_high: 5,
        },
        &cfg,
    );
    assert_eq!(capped.variant_index, 2);

    // Shrinking the element to 640x360 drops immediately from 720p to 360p.
    let shrunk = decide_variant(
        &AbrInput {
            variants: &variants,
            current_index: 2,
            throughput_bps: 50_000_000.0,
            buffer_secs: 8.0,
            display_size: Some((640, 360)),
            consecutive_high: 0,
        },
        &cfg,
    );
    assert_eq!(shrunk.variant_index, 1);

    // Tiny thumbnail smaller than all variants keeps the smallest variant rather than failing.
    assert_eq!(select_initial_variant(&variants, Some((320, 180))), Some(1));

    // Low buffer forces immediate step-down even with high throughput.
    let low_buf = decide_variant(
        &AbrInput {
            variants: &variants,
            current_index: 0,
            throughput_bps: 20_000_000.0,
            buffer_secs: 1.0,
            display_size: Some((1920, 1080)),
            consecutive_high: 0,
        },
        &cfg,
    );
    assert_eq!(low_buf.variant_index, 2);

    // Low throughput forces immediate step-down to sustainable rung.
    let low_bw = decide_variant(
        &AbrInput {
            variants: &variants,
            current_index: 0,
            throughput_bps: 1_200_000.0,
            buffer_secs: 6.0,
            display_size: Some((1920, 1080)),
            consecutive_high: 0,
        },
        &cfg,
    );
    assert_eq!(low_bw.variant_index, 1);

    // Stateful AbrController ramps up after sustained fast samples and drops on slow sample.
    let mut ctl = AbrController::new(variants, Some((1920, 1080)), cfg);
    assert_eq!(ctl.current_index(), 1);
    ctl.record_sample(1_000_000, Duration::from_millis(500)); // 16 Mbps
    assert_eq!(ctl.select(6.0, Some((1920, 1080))), 1);
    ctl.record_sample(1_000_000, Duration::from_millis(500));
    assert_eq!(ctl.select(6.0, Some((1920, 1080))), 2);
}

#[test]
fn http_retries_transient_503_and_does_not_retry_404() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let server_hits = hits.clone();

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten().take(4) {
            let mut s = stream;
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/");
            if path == "/flaky" {
                let c = server_hits.fetch_add(1, Ordering::SeqCst);
                if c < 2 {
                    drop(s.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
                } else {
                    drop(s.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    ));
                }
            } else {
                drop(s.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                ));
            }
        }
    });

    let cancel = CancelToken::new();
    let resp = fetch_with_schedule(
        &format!("http://{addr}/flaky"),
        &[],
        None,
        MAX_PLAYLIST_BYTES,
        &cancel,
        &[10, 10, 10],
    )
    .unwrap();
    assert_eq!(resp.body, b"ok");
    assert_eq!(hits.load(Ordering::SeqCst), 3);

    let err = fetch_with_schedule(
        &format!("http://{addr}/missing"),
        &[],
        None,
        MAX_PLAYLIST_BYTES,
        &cancel,
        &[10, 10],
    )
    .unwrap_err();
    assert!(
        matches!(err, HttpError::HttpStatus { status: 404, .. }),
        "{err:?}"
    );

    cancel.cancel();
    let cancelled = fetch_with_schedule(
        &format!("http://{addr}/flaky"),
        &[],
        None,
        MAX_PLAYLIST_BYTES,
        &cancel,
        &[10],
    )
    .unwrap_err();
    assert_eq!(cancelled, HttpError::Cancelled);
}
