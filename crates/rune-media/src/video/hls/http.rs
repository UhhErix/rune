//! HTTP(S) client for HLS playlists and segments with cancellation, Range requests, custom
//! headers, and exponential-backoff retries.

#![forbid(unsafe_code)]

use super::playlist::ByteRange;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Maximum playlist response size (4 MiB).
pub const MAX_PLAYLIST_BYTES: usize = 4 * 1024 * 1024;
/// Maximum single media segment size (64 MiB).
pub const MAX_SEGMENT_BYTES: usize = 64 * 1024 * 1024;

/// Retry delays for transient failures (network drops, 408/429/5xx).
pub const RETRY_DELAYS_MS: &[u64] = &[150, 400, 1_000];

/// Cooperative cancellation token that also invokes an abort callback (e.g. closing an active
/// WinHTTP handle) to unblock in-flight network I/O immediately.
#[derive(Clone, Default)]
pub struct CancelToken {
    inner: Arc<CancelInner>,
}

#[derive(Default)]
struct CancelInner {
    cancelled: AtomicBool,
    abort: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

pub(crate) struct AbortRegistration {
    inner: Arc<CancelInner>,
}

impl Drop for AbortRegistration {
    fn drop(&mut self) {
        let _ = self
            .inner
            .abort
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
    }
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
        let hook = self
            .inner
            .abort
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(f) = hook {
            f();
        }
    }

    pub(crate) fn register_abort(&self, hook: Box<dyn FnOnce() + Send>) -> AbortRegistration {
        if self.is_cancelled() {
            hook();
        } else {
            let mut slot = self.inner.abort.lock().unwrap_or_else(|e| e.into_inner());
            if self.is_cancelled() {
                drop(slot);
                hook();
            } else {
                *slot = Some(hook);
            }
        }
        AbortRegistration {
            inner: self.inner.clone(),
        }
    }
}

/// Completed HTTP response payload and timing.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub final_url: String,
    pub elapsed: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpError {
    Cancelled,
    HttpStatus { status: u16, url: String },
    Network(String),
}

impl HttpError {
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Cancelled => false,
            Self::HttpStatus { status, .. } => matches!(status, 408 | 429 | 500 | 502 | 503 | 504),
            Self::Network(_) => true,
        }
    }
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("HLS request cancelled"),
            Self::HttpStatus { status, url } => write!(f, "HTTP {status} for {url}"),
            Self::Network(msg) => f.write_str(msg),
        }
    }
}

pub(crate) struct ParsedUrl {
    pub https: bool,
    pub host: String,
    pub port: u16,
    pub path_and_query: String,
}

pub(crate) fn parse_http_url(url: &str) -> Result<ParsedUrl, String> {
    let (https, rest) = if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        (true, &url[8..])
    } else if url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://") {
        (false, &url[7..])
    } else {
        return Err(format!("unsupported URL scheme: {url}"));
    };
    let without_frag = rest.split('#').next().unwrap_or(rest);
    let (authority, path_and_query) = match without_frag.find('/') {
        Some(i) => (&without_frag[..i], &without_frag[i..]),
        None => (without_frag, "/"),
    };
    if authority.is_empty() {
        return Err(format!("missing host in URL: {url}"));
    }
    let default_port = if https { 443 } else { 80 };
    let (host, port) = if let Some(after_bracket) = authority.strip_prefix('[') {
        let Some((ipv6, tail)) = after_bracket.split_once(']') else {
            return Err(format!("invalid IPv6 host in URL: {url}"));
        };
        let port = if let Some(p) = tail.strip_prefix(':') {
            p.parse::<u16>()
                .map_err(|_| format!("invalid port in URL: {url}"))?
        } else if tail.is_empty() {
            default_port
        } else {
            return Err(format!("invalid authority in URL: {url}"));
        };
        (ipv6.to_string(), port)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        let port = p
            .parse::<u16>()
            .map_err(|_| format!("invalid port in URL: {url}"))?;
        (h.to_string(), port)
    } else {
        (authority.to_string(), default_port)
    };
    if host.is_empty() {
        return Err(format!("missing host in URL: {url}"));
    }
    Ok(ParsedUrl {
        https,
        host,
        port,
        path_and_query: path_and_query.to_string(),
    })
}

/// Fetches `url` with exponential-backoff retries on transient errors and cooperative cancellation.
pub fn fetch_with_retry(
    url: &str,
    headers: &[(String, String)],
    range: Option<ByteRange>,
    max_bytes: usize,
    cancel: &CancelToken,
) -> Result<HttpResponse, HttpError> {
    fetch_with_schedule(url, headers, range, max_bytes, cancel, RETRY_DELAYS_MS)
}

/// Like [`fetch_with_retry`] with a custom backoff delay schedule (for deterministic tests).
pub fn fetch_with_schedule(
    url: &str,
    headers: &[(String, String)],
    range: Option<ByteRange>,
    max_bytes: usize,
    cancel: &CancelToken,
    delays_ms: &[u64],
) -> Result<HttpResponse, HttpError> {
    let mut attempt = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        match fetch_once(url, headers, range, max_bytes, cancel) {
            Ok(resp) => return Ok(resp),
            Err(err) => {
                if !err.is_retryable() || attempt >= delays_ms.len() {
                    return Err(err);
                }
                if !sleep_interruptible(Duration::from_millis(delays_ms[attempt]), cancel) {
                    return Err(HttpError::Cancelled);
                }
                attempt += 1;
            }
        }
    }
}

fn sleep_interruptible(duration: Duration, cancel: &CancelToken) -> bool {
    let mut remaining = duration;
    let slice = Duration::from_millis(20);
    while !remaining.is_zero() {
        if cancel.is_cancelled() {
            return false;
        }
        let step = remaining.min(slice);
        std::thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
    !cancel.is_cancelled()
}

fn fetch_once(
    url: &str,
    headers: &[(String, String)],
    range: Option<ByteRange>,
    max_bytes: usize,
    cancel: &CancelToken,
) -> Result<HttpResponse, HttpError> {
    #[cfg(target_os = "windows")]
    {
        super::winhttp::get_once(url, headers, range, max_bytes, cancel)
    }
    #[cfg(not(target_os = "windows"))]
    {
        fetch_plain_http(url, headers, range, max_bytes, cancel)
    }
}

/// Minimal pure-std HTTP/1.1 GET client for `http://` URLs on non-Windows platforms (used by
/// cross-platform tests against local HTTP servers).
#[cfg(not(target_os = "windows"))]
fn fetch_plain_http(
    url: &str,
    headers: &[(String, String)],
    range: Option<ByteRange>,
    max_bytes: usize,
    cancel: &CancelToken,
) -> Result<HttpResponse, HttpError> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let mut current_url = url.to_string();
    for _ in 0..5 {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        let parsed = parse_http_url(&current_url).map_err(HttpError::Network)?;
        if parsed.https {
            return Err(HttpError::Network(
                "HTTPS HLS fetching on this platform requires the native player".into(),
            ));
        }
        let started = rune_core::time::now();
        let addr = format!("{}:{}", parsed.host, parsed.port);
        let mut stream = TcpStream::connect(&addr)
            .map_err(|e| HttpError::Network(format!("connect {addr}: {e}")))?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));

        let mut req = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n",
            parsed.path_and_query, parsed.host, parsed.port
        );
        for (k, v) in headers {
            if !k.contains(['\r', '\n']) && !v.contains(['\r', '\n']) {
                req.push_str(k);
                req.push_str(": ");
                req.push_str(v);
                req.push_str("\r\n");
            }
        }
        if let Some(r) = range {
            req.push_str("Range: ");
            req.push_str(&r.http_range_header());
            req.push_str("\r\n");
        }
        req.push_str("\r\n");
        stream
            .write_all(req.as_bytes())
            .map_err(|e| HttpError::Network(e.to_string()))?;

        let mut raw = Vec::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            if cancel.is_cancelled() {
                return Err(HttpError::Cancelled);
            }
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if raw.len().saturating_add(n) > max_bytes.saturating_add(64 * 1024) {
                        return Err(HttpError::Network(format!(
                            "HTTP response exceeded {max_bytes} byte limit"
                        )));
                    }
                    raw.extend_from_slice(&buf[..n]);
                }
                Err(e) => return Err(HttpError::Network(e.to_string())),
            }
        }
        let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
            return Err(HttpError::Network("malformed HTTP response".into()));
        };
        let head = String::from_utf8_lossy(&raw[..split]);
        let mut lines = head.lines();
        let status_line = lines.next().unwrap_or("");
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u16>().ok())
            .ok_or_else(|| HttpError::Network("invalid HTTP status line".into()))?;

        let mut content_type = None;
        let mut location = None;
        let mut chunked = false;
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                let k = k.trim();
                let v = v.trim();
                if k.eq_ignore_ascii_case("content-type") {
                    content_type = Some(v.to_string());
                } else if k.eq_ignore_ascii_case("location") {
                    location = Some(v.to_string());
                } else if k.eq_ignore_ascii_case("transfer-encoding")
                    && v.to_ascii_lowercase().contains("chunked")
                {
                    chunked = true;
                }
            }
        }
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if let Some(loc) = location {
                current_url = super::resolve_url(&current_url, &loc).map_err(HttpError::Network)?;
                continue;
            }
        }
        if !(200..300).contains(&status) {
            return Err(HttpError::HttpStatus {
                status,
                url: current_url,
            });
        }
        let body_bytes = &raw[split + 4..];
        let body = if chunked {
            decode_chunked(body_bytes, max_bytes)?
        } else {
            body_bytes.to_vec()
        };
        if body.len() > max_bytes {
            return Err(HttpError::Network(format!(
                "HTTP response exceeded {max_bytes} byte limit"
            )));
        }
        return Ok(HttpResponse {
            status,
            body,
            content_type,
            final_url: current_url,
            elapsed: started.elapsed(),
        });
    }
    Err(HttpError::Network("too many HTTP redirects".into()))
}

#[cfg(not(target_os = "windows"))]
fn decode_chunked(mut data: &[u8], max_bytes: usize) -> Result<Vec<u8>, HttpError> {
    let mut out = Vec::new();
    loop {
        let Some(crlf) = data.windows(2).position(|w| w == b"\r\n") else {
            return Err(HttpError::Network("truncated chunked response".into()));
        };
        let size_str = std::str::from_utf8(&data[..crlf])
            .map_err(|_| HttpError::Network("invalid chunk size".into()))?
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        let len = usize::from_str_radix(size_str, 16)
            .map_err(|_| HttpError::Network("invalid hex chunk size".into()))?;
        data = &data[crlf + 2..];
        if len == 0 {
            break;
        }
        if data.len() < len + 2 {
            return Err(HttpError::Network("truncated chunk body".into()));
        }
        if out.len().saturating_add(len) > max_bytes {
            return Err(HttpError::Network(format!(
                "HTTP response exceeded {max_bytes} byte limit"
            )));
        }
        out.extend_from_slice(&data[..len]);
        data = &data[len + 2..];
    }
    Ok(out)
}
