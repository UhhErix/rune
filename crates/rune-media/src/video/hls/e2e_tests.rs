//! End-to-end HLS tests using a tiny in-memory generated H.264 Baseline `I_PCM` fMP4 stream
//! served over a local HTTP/1.1 test server (zero committed binary fixtures).

use super::super::{Clock, Player, PlayerState};
use crate::native_surface::Source;
use rune_image::Pixels;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// --- Minimal H.264 Baseline (I_PCM) + fMP4 (ftyp/moov + styp/moof/mdat) generator ---

struct BitWriter {
    bytes: Vec<u8>,
    cur: u8,
    bits: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            cur: 0,
            bits: 0,
        }
    }
    fn write_bit(&mut self, b: u32) {
        self.cur = (self.cur << 1) | ((b & 1) as u8);
        self.bits += 1;
        if self.bits == 8 {
            self.bytes.push(self.cur);
            self.cur = 0;
            self.bits = 0;
        }
    }
    fn write_bits(&mut self, val: u32, count: u8) {
        for i in (0..count).rev() {
            self.write_bit((val >> i) & 1);
        }
    }
    fn write_ue(&mut self, val: u32) {
        let code = val + 1;
        let len = 32 - code.leading_zeros();
        for _ in 0..(len - 1) {
            self.write_bit(0);
        }
        self.write_bits(code, len as u8);
    }
    fn write_se(&mut self, val: i32) {
        let code = if val <= 0 {
            (-2 * val) as u32
        } else {
            (2 * val - 1) as u32
        };
        self.write_ue(code);
    }
    fn align_zero(&mut self) {
        if self.bits > 0 {
            self.cur <<= 8 - self.bits;
            self.bytes.push(self.cur);
            self.cur = 0;
            self.bits = 0;
        }
    }
    fn finish_rbsp(mut self) -> Vec<u8> {
        self.write_bit(1);
        self.align_zero();
        self.bytes
    }
}

fn ebsp_from_rbsp(nal_header: u8, rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len() + 4);
    out.push(nal_header);
    let mut zeros = 0usize;
    for &b in rbsp {
        if zeros >= 2 && b <= 3 {
            out.push(0x03);
            zeros = 0;
        }
        out.push(b);
        if b == 0 {
            zeros += 1;
        } else {
            zeros = 0;
        }
    }
    out
}

fn build_sps(width: u32, height: u32) -> Vec<u8> {
    let mb_w = (width / 16).max(1);
    let mb_h = (height / 16).max(1);
    let mut w = BitWriter::new();
    w.write_bits(66, 8); // Baseline profile_idc
    w.write_bits(0xC0, 8); // constraint_set0_flag | constraint_set1_flag
    w.write_bits(10, 8); // level_idc = 1.0
    w.write_ue(0); // seq_parameter_set_id
    w.write_ue(0); // log2_max_frame_num_minus4
    w.write_ue(2); // pic_order_cnt_type = 2
    w.write_ue(0); // max_num_ref_frames
    w.write_bit(0); // gaps_in_frame_num_value_allowed_flag
    w.write_ue(mb_w - 1); // pic_width_in_mbs_minus1
    w.write_ue(mb_h - 1); // pic_height_in_map_units_minus1
    w.write_bit(1); // frame_mbs_only_flag
    w.write_bit(0); // direct_8x8_inference_flag
    w.write_bit(0); // frame_cropping_flag
    w.write_bit(0); // vui_parameters_present_flag
    ebsp_from_rbsp(0x67, &w.finish_rbsp())
}

fn build_pps() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write_ue(0); // pic_parameter_set_id
    w.write_ue(0); // seq_parameter_set_id
    w.write_bit(0); // entropy_coding_mode_flag (CAVLC)
    w.write_bit(0); // bottom_field_pic_order_in_frame_present_flag
    w.write_ue(0); // num_slice_groups_minus1
    w.write_ue(0); // num_ref_idx_l0_default_active_minus1
    w.write_ue(0); // num_ref_idx_l1_default_active_minus1
    w.write_bit(0); // weighted_pred_flag
    w.write_bits(0, 2); // weighted_bipred_idc
    w.write_se(0); // pic_init_qp_minus26
    w.write_se(0); // pic_init_qs_minus26
    w.write_se(0); // chroma_qp_index_offset
    w.write_bit(0); // deblocking_filter_control_present_flag
    w.write_bit(0); // constrained_intra_pred_flag
    w.write_bit(0); // redundant_pic_cnt_present_flag
    ebsp_from_rbsp(0x68, &w.finish_rbsp())
}

/// Generates one H.264 IDR slice composed of solid-color `I_PCM` macroblocks.
fn build_idr_slice(
    width: u32,
    height: u32,
    y_val: u8,
    cb_val: u8,
    cr_val: u8,
    idr_pic_id: u32,
) -> Vec<u8> {
    let mb_count = (width / 16).max(1) * (height / 16).max(1);
    let mut w = BitWriter::new();
    w.write_ue(0); // first_mb_in_slice
    w.write_ue(7); // slice_type = 7 (I slice)
    w.write_ue(0); // pic_parameter_set_id
    w.write_bits(0, 4); // frame_num (4 bits since log2_max_frame_num_minus4 = 0)
    w.write_ue(idr_pic_id); // idr_pic_id
    w.write_bit(0); // no_output_of_prior_pics_flag
    w.write_bit(0); // long_term_reference_flag
    w.write_se(0); // slice_qp_delta
    for _ in 0..mb_count {
        w.write_ue(25); // mb_type = 25 (I_PCM in I-slice)
        w.align_zero();
        w.bytes.extend(std::iter::repeat_n(y_val, 256));
        w.bytes.extend(std::iter::repeat_n(cb_val, 64));
        w.bytes.extend(std::iter::repeat_n(cr_val, 64));
    }
    ebsp_from_rbsp(0x65, &w.finish_rbsp())
}

fn mp4_box(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&((8 + payload.len()) as u32).to_be_bytes());
    out.extend_from_slice(fourcc);
    out.extend_from_slice(payload);
    out
}

fn mp4_full_box(fourcc: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.push(version);
    buf.push(((flags >> 16) & 0xff) as u8);
    buf.push(((flags >> 8) & 0xff) as u8);
    buf.push((flags & 0xff) as u8);
    buf.extend_from_slice(payload);
    mp4_box(fourcc, &buf)
}

fn build_fmp4_init(width: u32, height: u32, timescale: u32) -> Vec<u8> {
    let sps = build_sps(width, height);
    let pps = build_pps();

    let mut ftyp_body = Vec::new();
    ftyp_body.extend_from_slice(b"isom");
    ftyp_body.extend_from_slice(&0x200u32.to_be_bytes());
    ftyp_body.extend_from_slice(b"isomiso5avc1mp41");
    let ftyp = mp4_box(b"ftyp", &ftyp_body);

    // mvhd (version 0)
    let mut mvhd_body = Vec::new();
    mvhd_body.extend_from_slice(&0u32.to_be_bytes()); // creation
    mvhd_body.extend_from_slice(&0u32.to_be_bytes()); // modification
    mvhd_body.extend_from_slice(&timescale.to_be_bytes());
    mvhd_body.extend_from_slice(&0u32.to_be_bytes()); // duration = 0 (fragmented)
    mvhd_body.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // rate 1.0
    mvhd_body.extend_from_slice(&0x0100u16.to_be_bytes()); // volume 1.0
    mvhd_body.extend_from_slice(&[0u8; 10]); // reserved
    for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        mvhd_body.extend_from_slice(&v.to_be_bytes());
    }
    mvhd_body.extend_from_slice(&[0u8; 24]); // pre_defined
    mvhd_body.extend_from_slice(&2u32.to_be_bytes()); // next_track_id
    let mvhd = mp4_full_box(b"mvhd", 0, 0, &mvhd_body);

    // tkhd (version 0, flags = 3 enabled|in_movie)
    let mut tkhd_body = Vec::new();
    tkhd_body.extend_from_slice(&0u32.to_be_bytes());
    tkhd_body.extend_from_slice(&0u32.to_be_bytes());
    tkhd_body.extend_from_slice(&1u32.to_be_bytes()); // track_id
    tkhd_body.extend_from_slice(&0u32.to_be_bytes());
    tkhd_body.extend_from_slice(&0u32.to_be_bytes()); // duration
    tkhd_body.extend_from_slice(&[0u8; 8]);
    tkhd_body.extend_from_slice(&0u16.to_be_bytes());
    tkhd_body.extend_from_slice(&0u16.to_be_bytes());
    tkhd_body.extend_from_slice(&0u16.to_be_bytes());
    tkhd_body.extend_from_slice(&0u16.to_be_bytes());
    for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        tkhd_body.extend_from_slice(&v.to_be_bytes());
    }
    tkhd_body.extend_from_slice(&(width << 16).to_be_bytes());
    tkhd_body.extend_from_slice(&(height << 16).to_be_bytes());
    let tkhd = mp4_full_box(b"tkhd", 0, 3, &tkhd_body);

    // mdhd
    let mut mdhd_body = Vec::new();
    mdhd_body.extend_from_slice(&0u32.to_be_bytes());
    mdhd_body.extend_from_slice(&0u32.to_be_bytes());
    mdhd_body.extend_from_slice(&timescale.to_be_bytes());
    mdhd_body.extend_from_slice(&0u32.to_be_bytes());
    mdhd_body.extend_from_slice(&0x55c4u16.to_be_bytes()); // und
    mdhd_body.extend_from_slice(&0u16.to_be_bytes());
    let mdhd = mp4_full_box(b"mdhd", 0, 0, &mdhd_body);

    // hdlr
    let mut hdlr_body = Vec::new();
    hdlr_body.extend_from_slice(&0u32.to_be_bytes());
    hdlr_body.extend_from_slice(b"vide");
    hdlr_body.extend_from_slice(&[0u8; 12]);
    hdlr_body.extend_from_slice(b"VideoHandler\0");
    let hdlr = mp4_full_box(b"hdlr", 0, 0, &hdlr_body);

    // vmhd, dinf/dref
    let vmhd = mp4_full_box(b"vmhd", 0, 1, &[0u8; 8]);
    let url_box = mp4_full_box(b"url ", 0, 1, &[]);
    let mut dref_body = 1u32.to_be_bytes().to_vec();
    dref_body.extend_from_slice(&url_box);
    let dinf = mp4_box(b"dinf", &mp4_full_box(b"dref", 0, 0, &dref_body));

    // avcC
    let mut avcc_body = vec![1, sps[1], sps[2], sps[3], 0xff, 0xe1];
    avcc_body.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    avcc_body.extend_from_slice(&sps);
    avcc_body.push(1);
    avcc_body.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    avcc_body.extend_from_slice(&pps);
    let avcc = mp4_box(b"avcC", &avcc_body);

    // avc1 visual sample entry
    let mut avc1_body = Vec::new();
    avc1_body.extend_from_slice(&[0u8; 6]);
    avc1_body.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
    avc1_body.extend_from_slice(&[0u8; 16]);
    avc1_body.extend_from_slice(&(width as u16).to_be_bytes());
    avc1_body.extend_from_slice(&(height as u16).to_be_bytes());
    avc1_body.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // horiz res 72 dpi
    avc1_body.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // vert res 72 dpi
    avc1_body.extend_from_slice(&0u32.to_be_bytes());
    avc1_body.extend_from_slice(&1u16.to_be_bytes()); // frame_count
    avc1_body.extend_from_slice(&[0u8; 32]); // compressorname
    avc1_body.extend_from_slice(&0x0018u16.to_be_bytes()); // depth 24
    avc1_body.extend_from_slice(&(-1i16).to_be_bytes());
    avc1_body.extend_from_slice(&avcc);
    let avc1 = mp4_box(b"avc1", &avc1_body);

    let mut stsd_body = 1u32.to_be_bytes().to_vec();
    stsd_body.extend_from_slice(&avc1);
    let stsd = mp4_full_box(b"stsd", 0, 0, &stsd_body);

    let empty_table = |tag: &[u8; 4]| mp4_full_box(tag, 0, 0, &0u32.to_be_bytes());
    let mut stsz_body = Vec::new();
    stsz_body.extend_from_slice(&0u32.to_be_bytes());
    stsz_body.extend_from_slice(&0u32.to_be_bytes());
    let stsz = mp4_full_box(b"stsz", 0, 0, &stsz_body);

    let mut stbl_body = Vec::new();
    stbl_body.extend_from_slice(&stsd);
    stbl_body.extend_from_slice(&empty_table(b"stts"));
    stbl_body.extend_from_slice(&empty_table(b"stsc"));
    stbl_body.extend_from_slice(&stsz);
    stbl_body.extend_from_slice(&empty_table(b"stco"));
    let stbl = mp4_box(b"stbl", &stbl_body);

    let mut minf_body = Vec::new();
    minf_body.extend_from_slice(&vmhd);
    minf_body.extend_from_slice(&dinf);
    minf_body.extend_from_slice(&stbl);
    let minf = mp4_box(b"minf", &minf_body);

    let mut mdia_body = Vec::new();
    mdia_body.extend_from_slice(&mdhd);
    mdia_body.extend_from_slice(&hdlr);
    mdia_body.extend_from_slice(&minf);
    let mdia = mp4_box(b"mdia", &mdia_body);

    let mut trak_body = Vec::new();
    trak_body.extend_from_slice(&tkhd);
    trak_body.extend_from_slice(&mdia);
    let trak = mp4_box(b"trak", &trak_body);

    // mvex/trex
    let mut trex_body = Vec::new();
    trex_body.extend_from_slice(&1u32.to_be_bytes()); // track_id
    trex_body.extend_from_slice(&1u32.to_be_bytes()); // default_sample_description_index
    trex_body.extend_from_slice(&(timescale / 4).to_be_bytes()); // default_sample_duration (0.25s)
    trex_body.extend_from_slice(&0u32.to_be_bytes());
    trex_body.extend_from_slice(&0u32.to_be_bytes());
    let mvex = mp4_box(b"mvex", &mp4_full_box(b"trex", 0, 0, &trex_body));

    let mut moov_body = Vec::new();
    moov_body.extend_from_slice(&mvhd);
    moov_body.extend_from_slice(&trak);
    moov_body.extend_from_slice(&mvex);
    let moov = mp4_box(b"moov", &moov_body);

    let mut out = ftyp;
    out.extend_from_slice(&moov);
    out
}

fn build_fmp4_segment(
    width: u32,
    height: u32,
    seq_num: u32,
    base_decode_time: u64,
    sample_duration: u32,
    frames: &[(u8, u8, u8)],
) -> Vec<u8> {
    let mut styp_body = Vec::new();
    styp_body.extend_from_slice(b"msdh");
    styp_body.extend_from_slice(&0u32.to_be_bytes());
    styp_body.extend_from_slice(b"msdhmsix");
    let styp = mp4_box(b"styp", &styp_body);

    let mut mdat_payload = Vec::new();
    let mut sample_sizes = Vec::with_capacity(frames.len());
    for (idx, &(y, cb, cr)) in frames.iter().enumerate() {
        let slice = build_idr_slice(width, height, y, cb, cr, idx as u32);
        let len = slice.len() as u32;
        mdat_payload.extend_from_slice(&len.to_be_bytes());
        mdat_payload.extend_from_slice(&slice);
        sample_sizes.push(len + 4);
    }
    let mdat = mp4_box(b"mdat", &mdat_payload);

    let mfhd = mp4_full_box(b"mfhd", 0, 0, &seq_num.to_be_bytes());

    // tfhd: flags = 0x020000 (default-base-is-moof) | 0x000008 (default-sample-duration-present) | 0x000020 (default-sample-flags-present)
    let mut tfhd_body = Vec::new();
    tfhd_body.extend_from_slice(&1u32.to_be_bytes()); // track_id
    tfhd_body.extend_from_slice(&sample_duration.to_be_bytes());
    tfhd_body.extend_from_slice(&0x0200_0000u32.to_be_bytes()); // sample_depends_on = 2 (I-frame)
    let tfhd = mp4_full_box(b"tfhd", 0, 0x02_0028, &tfhd_body);

    // tfdt (version 1: 64-bit baseMediaDecodeTime)
    let tfdt = mp4_full_box(b"tfdt", 1, 0, &base_decode_time.to_be_bytes());

    // Build trun with placeholder data_offset, then patch with exact moof size + 8.
    let build_moof = |data_offset: i32| {
        let mut trun_body = Vec::new();
        trun_body.extend_from_slice(&(sample_sizes.len() as u32).to_be_bytes());
        trun_body.extend_from_slice(&data_offset.to_be_bytes());
        for &sz in &sample_sizes {
            trun_body.extend_from_slice(&sz.to_be_bytes());
        }
        // flags = 0x000001 (data-offset-present) | 0x000200 (sample-size-present)
        let trun = mp4_full_box(b"trun", 0, 0x00_0201, &trun_body);

        let mut traf_body = Vec::new();
        traf_body.extend_from_slice(&tfhd);
        traf_body.extend_from_slice(&tfdt);
        traf_body.extend_from_slice(&trun);
        let traf = mp4_box(b"traf", &traf_body);

        let mut moof_body = Vec::new();
        moof_body.extend_from_slice(&mfhd);
        moof_body.extend_from_slice(&traf);
        mp4_box(b"moof", &moof_body)
    };

    let moof_len = build_moof(0).len();
    let moof = build_moof((moof_len + 8) as i32);

    let mut out = styp;
    out.extend_from_slice(&moof);
    out.extend_from_slice(&mdat);
    out
}

// --- Local HTTP/1.1 test server with Range (206) support ---

struct TestHlsServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
    _handle: std::thread::JoinHandle<()>,
}

impl Drop for TestHlsServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Poke listener so accept loop exits cleanly.
        let addr = self.base_url.trim_start_matches("http://");
        drop(std::net::TcpStream::connect(addr));
    }
}

impl TestHlsServer {
    fn start(routes: HashMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener.set_nonblocking(false));
        let stop = Arc::new(AtomicBool::new(false));
        let stop_bg = stop.clone();
        let requests = Arc::new(AtomicUsize::new(0));
        let req_bg = requests.clone();

        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if stop_bg.load(Ordering::Acquire) {
                    break;
                }
                req_bg.fetch_add(1, Ordering::Relaxed);
                serve_one(stream, &routes);
            }
        });

        Self {
            base_url: format!("http://{addr}"),
            stop,
            requests,
            _handle: handle,
        }
    }
}

fn serve_one(mut stream: std::net::TcpStream, routes: &HashMap<String, Vec<u8>>) {
    drop(stream.set_read_timeout(Some(Duration::from_secs(2))));
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    if n == 0 {
        return;
    }
    let req = String::from_utf8_lossy(&buf[..n]);
    let path = req
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/");
    let range = req.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        if k.trim().eq_ignore_ascii_case("range") {
            let spec = v.trim().strip_prefix("bytes=")?;
            let (s, e) = spec.split_once('-')?;
            Some((s.parse::<usize>().ok()?, e.parse::<usize>().ok()?))
        } else {
            None
        }
    });

    let Some(body) = routes.get(path) else {
        drop(stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ));
        return;
    };

    if let Some((start, end)) = range {
        let end = end.min(body.len().saturating_sub(1));
        if start <= end && start < body.len() {
            let slice = &body[start..=end];
            let head = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len(),
                slice.len()
            );
            drop(stream.write_all(head.as_bytes()));
            drop(stream.write_all(slice));
            return;
        }
    }

    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    drop(stream.write_all(head.as_bytes()));
    drop(stream.write_all(body));
}

fn build_test_routes() -> HashMap<String, Vec<u8>> {
    let mut routes = HashMap::new();
    let timescale = 1000u32;
    let sample_dur = 250u32; // 4 fps -> 2 frames = 0.5s per segment

    // Low variant: 64x48 (4x3 macroblocks — above Media Foundation's 48px H.264 floor),
    // 3 segments with distinct Y luminance values (40, 130, 220).
    let low_init = build_fmp4_init(64, 48, timescale);
    let low_seg0 = build_fmp4_segment(64, 48, 1, 0, sample_dur, &[(40, 128, 128), (40, 128, 128)]);
    let low_seg1 = build_fmp4_segment(
        64,
        48,
        2,
        500,
        sample_dur,
        &[(130, 128, 128), (130, 128, 128)],
    );
    let low_seg2 = build_fmp4_segment(
        64,
        48,
        3,
        1000,
        sample_dur,
        &[(220, 128, 128), (220, 128, 128)],
    );

    // High variant: 128x96, 3 segments.
    let high_init = build_fmp4_init(128, 96, timescale);
    let high_seg0 =
        build_fmp4_segment(128, 96, 1, 0, sample_dur, &[(40, 128, 128), (40, 128, 128)]);
    let high_seg1 = build_fmp4_segment(
        128,
        96,
        2,
        500,
        sample_dur,
        &[(130, 128, 128), (130, 128, 128)],
    );
    let high_seg2 = build_fmp4_segment(
        128,
        96,
        3,
        1000,
        sample_dur,
        &[(220, 128, 128), (220, 128, 128)],
    );

    let master = "\
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-STREAM-INF:BANDWIDTH=400000,RESOLUTION=64x48,CODECS=\"avc1.42c00a\"
/low/media.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2000000,RESOLUTION=128x96,CODECS=\"avc1.42c00a\"
/high/media.m3u8
";
    let low_media = "\
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:1
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:0.5,
seg0.m4s
#EXTINF:0.5,
seg1.m4s
#EXTINF:0.5,
seg2.m4s
#EXT-X-ENDLIST
";
    let high_media = "\
#EXTM3U
#EXT-X-VERSION:7
#EXT-X-TARGETDURATION:1
#EXT-X-PLAYLIST-TYPE:VOD
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:0.5,
seg0.m4s
#EXTINF:0.5,
seg1.m4s
#EXTINF:0.5,
seg2.m4s
#EXT-X-ENDLIST
";

    // Combined single-file byterange stream.
    let mut combined = Vec::new();
    let init_len = low_init.len();
    combined.extend_from_slice(&low_init);
    let s0_off = combined.len();
    let s0_len = low_seg0.len();
    combined.extend_from_slice(&low_seg0);
    let s1_len = low_seg1.len();
    combined.extend_from_slice(&low_seg1);

    let byterange_media = format!(
        "#EXTM3U\n\
#EXT-X-VERSION:7\n\
#EXT-X-TARGETDURATION:1\n\
#EXT-X-PLAYLIST-TYPE:VOD\n\
#EXT-X-MAP:URI=\"all.mp4\",BYTERANGE=\"{init_len}@0\"\n\
#EXTINF:0.5,\n\
#EXT-X-BYTERANGE:{s0_len}@{s0_off}\n\
all.mp4\n\
#EXTINF:0.5,\n\
#EXT-X-BYTERANGE:{s1_len}\n\
all.mp4\n\
#EXT-X-ENDLIST\n"
    );

    let encrypted = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:0.5,\nseg0.m4s\n#EXT-X-ENDLIST\n";

    routes.insert("/master.m3u8".into(), master.as_bytes().to_vec());
    routes.insert("/low/media.m3u8".into(), low_media.as_bytes().to_vec());
    routes.insert("/low/init.mp4".into(), low_init);
    routes.insert("/low/seg0.m4s".into(), low_seg0);
    routes.insert("/low/seg1.m4s".into(), low_seg1);
    routes.insert("/low/seg2.m4s".into(), low_seg2);
    routes.insert("/high/media.m3u8".into(), high_media.as_bytes().to_vec());
    routes.insert("/high/init.mp4".into(), high_init);
    routes.insert("/high/seg0.m4s".into(), high_seg0);
    routes.insert("/high/seg1.m4s".into(), high_seg1);
    routes.insert("/high/seg2.m4s".into(), high_seg2);
    routes.insert("/byterange/media.m3u8".into(), byterange_media.into_bytes());
    routes.insert("/byterange/all.mp4".into(), combined);
    routes.insert("/encrypted.m3u8".into(), encrypted.as_bytes().to_vec());
    routes
}

fn red_poster() -> Pixels {
    let p = Pixels::new();
    p.set(
        4,
        4,
        [255u8, 0, 0, 255]
            .into_iter()
            .cycle()
            .take(4 * 4 * 4)
            .collect(),
    );
    p
}

fn mean_luma(player: &Player) -> u8 {
    let frame = player
        .surface()
        .latest()
        .expect("frame should be on surface");
    match &frame.source {
        Source::Bytes(b) => {
            let sum: u64 = b.planes[0].iter().map(|&v| u64::from(v)).sum();
            (sum / (b.planes[0].len().max(1) as u64)) as u8
        }
        _ => panic!("expected CPU ByteFrame in headless test"),
    }
}

#[cfg(target_os = "windows")]
#[test]
fn e2e_hls_fmp4_playback_seek_pause_resume_loop_and_callbacks() {
    let server = TestHlsServer::start(build_test_routes());
    let wake = rune_core::live::Wake::new(Arc::new(|| {}));
    let ready_hits = Arc::new(AtomicUsize::new(0));
    let error_hits = Arc::new(AtomicUsize::new(0));
    let r_flag = ready_hits.clone();
    let e_flag = error_hits.clone();

    let player = Player::url(format!("{}/master.m3u8", server.base_url))
        .with_poster(red_poster())
        .with_on_ready(move || {
            r_flag.fetch_add(1, Ordering::SeqCst);
        })
        .with_on_error(move |_| {
            e_flag.fetch_add(1, Ordering::SeqCst);
        });

    assert!(player.is_hls());
    assert!(player.muted(), "trailers default to muted");
    assert!(!player.is_playing());

    // Initial pull at t=0 decodes segment 0 (low 16x16 variant, Y ~ 40) and fires on_ready once.
    let _ = player.surface().pull_video(Clock::Virtual(0.0), &wake);
    assert_eq!(player.state(), PlayerState::Ready);
    assert_eq!(ready_hits.load(Ordering::SeqCst), 1);
    assert_eq!(error_hits.load(Ordering::SeqCst), 0);
    assert_eq!(player.natural_size(), Some((64, 48)));
    assert!((player.duration().unwrap_or(0.0) - 1.5).abs() < 0.05);
    let y0 = mean_luma(&player);
    assert!(
        (i16::from(y0) - 40).abs() <= 8,
        "seg0 expected luma ~40, got {y0}"
    );

    // Seek to segment 1 (t = 0.6s, Y ~ 130).
    player.seek(0.6);
    let _ = player.surface().pull_video(Clock::Virtual(0.0), &wake);
    let y1 = mean_luma(&player);
    assert!(
        (i16::from(y1) - 130).abs() <= 8,
        "seg1 expected luma ~130, got {y1}"
    );

    // Seek to segment 2 (t = 1.1s, Y ~ 220).
    player.seek(1.1);
    let _ = player.surface().pull_video(Clock::Virtual(0.0), &wake);
    let y2 = mean_luma(&player);
    assert!(
        (i16::from(y2) - 220).abs() <= 8,
        "seg2 expected luma ~220, got {y2}"
    );

    // Play, pause, resume, and loop past 1.5s back to segment 0 (Y ~ 40).
    player.set_looping(true);
    player.play();
    let _ = player.surface().pull_video(Clock::Virtual(1.0), &wake);
    assert_eq!(player.state(), PlayerState::Playing);
    player.pause();
    let paused_t = player.time();
    let _ = player.surface().pull_video(Clock::Virtual(1.5), &wake);
    assert_eq!(player.state(), PlayerState::Ready);
    assert!((player.time() - paused_t).abs() < 1e-6, "paused time holds");

    player.play();
    let _ = player.surface().pull_video(Clock::Virtual(2.0), &wake);
    let _ = player.surface().pull_video(Clock::Virtual(2.5), &wake);
    let y_loop = mean_luma(&player);
    assert!(
        (i16::from(y_loop) - 40).abs() <= 8,
        "looped back to seg0 (~40), got {y_loop}"
    );
    assert_eq!(ready_hits.load(Ordering::SeqCst), 1, "on_ready fires once");
    assert!(server.requests.load(Ordering::Relaxed) >= 3);
}

#[cfg(target_os = "windows")]
#[test]
fn e2e_hls_byterange_fmp4_stream() {
    let server = TestHlsServer::start(build_test_routes());
    let wake = rune_core::live::Wake::new(Arc::new(|| {}));

    let player = Player::url(format!("{}/byterange/media.m3u8", server.base_url));
    let _ = player.surface().pull_video(Clock::Virtual(0.0), &wake);
    assert_eq!(player.state(), PlayerState::Ready);
    let y0 = mean_luma(&player);
    assert!(
        (i16::from(y0) - 40).abs() <= 8,
        "byterange seg0 expected ~40, got {y0}"
    );

    player.seek(0.6);
    let _ = player.surface().pull_video(Clock::Virtual(0.0), &wake);
    let y1 = mean_luma(&player);
    assert!(
        (i16::from(y1) - 130).abs() <= 8,
        "byterange seg1 expected ~130, got {y1}"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn e2e_hls_error_falls_back_to_poster_and_invokes_on_error() {
    let server = TestHlsServer::start(build_test_routes());
    let wake = rune_core::live::Wake::new(Arc::new(|| {}));

    for path in ["/missing.m3u8", "/encrypted.m3u8"] {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let cap_clone = captured.clone();
        let player = Player::url(format!("{}{path}", server.base_url))
            .with_poster(red_poster())
            .with_on_error(move |msg| {
                cap_clone.lock().unwrap().push(msg.to_string());
            });

        let _ = player.surface().pull_video(Clock::Virtual(0.0), &wake);
        assert!(
            matches!(player.state(), PlayerState::Failed(_)),
            "{path}: {:?}",
            player.state()
        );
        let msgs = captured.lock().unwrap();
        assert_eq!(msgs.len(), 1, "{path}: expected one on_error callback");
        if path == "/encrypted.m3u8" {
            assert!(msgs[0].contains("AES-128"), "{}", msgs[0]);
        }
        // Surface must hold the red BGRA poster ([0, 0, 255, 255]), not the grey placeholder ([0x46, 0x46, 0x46, 0xff]).
        let frame = player
            .surface()
            .latest()
            .expect("poster fallback frame must be present");
        match &frame.source {
            Source::Bytes(b) => {
                assert_eq!((b.width, b.height), (4, 4));
                assert_eq!(
                    &b.planes[0][..4],
                    &[0, 0, 255, 255],
                    "expected red poster in BGRA"
                );
            }
            _ => panic!("expected poster ByteFrame"),
        }
    }
}

#[test]
fn e2e_hls_prefetch_session_fetches_segments_and_supports_seek_and_abr() {
    use super::http::CancelToken;
    use super::prefetch::{HlsSession, NextSegment, pack_display_size};
    use std::sync::atomic::AtomicU64;

    let server = TestHlsServer::start(build_test_routes());
    let display_size = Arc::new(AtomicU64::new(pack_display_size(32, 32)));
    let active_variant = Arc::new(Mutex::new(None));
    let cancel = CancelToken::new();

    let session = HlsSession::open(
        &format!("{}/master.m3u8", server.base_url),
        Vec::new(),
        0.0,
        1,
        display_size,
        active_variant,
        &cancel,
    )
    .unwrap();

    assert!((session.total_duration() - 1.5).abs() < 1e-6);
    let seg0 = match session.next_segment(1, Duration::from_secs(5)) {
        NextSegment::Ready(s) => s,
        other => panic!("expected seg0, got {:?}", std::mem::discriminant(&other)),
    };
    assert_eq!(seg0.index, 0);
    assert!(seg0.init.is_some());
    assert!(!seg0.data.is_empty());

    // Seek to segment 2 (1.1s) with generation 2.
    let target = session.seek(1.1, 2).unwrap();
    assert_eq!(target.segment_index, 2);
    let seg2 = match session.next_segment(2, Duration::from_secs(5)) {
        NextSegment::Ready(s) => s,
        other => panic!(
            "expected seg2 after seek, got {:?}",
            std::mem::discriminant(&other)
        ),
    };
    assert_eq!(seg2.index, 2);
    assert_eq!(seg2.generation, 2);

    // Next after segment 2 is EndOfStream.
    assert!(matches!(
        session.next_segment(2, Duration::from_secs(2)),
        NextSegment::EndOfStream
    ));
    session.stop();
}

#[cfg(target_os = "windows")]
#[test]
fn e2e_hls_live_windows_decoder_thread_decodes_and_seeks_segments() {
    let server = TestHlsServer::start(build_test_routes());
    let (dur, size, got_frame, got_seek_frame) =
        super::super::windows::hls::test_live_decode_and_seek(
            &format!("{}/master.m3u8", server.base_url),
            1.1,
        );
    assert!((dur - 1.5).abs() < 0.05, "expected duration 1.5, got {dur}");
    assert_eq!(size, Some((64, 48)));
    assert!(got_frame, "HLS decoder should decode frames from seg0");
    assert!(
        got_seek_frame,
        "HLS decoder should decode frame at seek target generation 2"
    );
}
