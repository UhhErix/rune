//! Windows WinHTTP client: dynamically loads `winhttp.dll` from System32 on first HLS request
//! (zero Cargo dependencies, zero import-table entries, native SChannel HTTPS & proxy support).

#![allow(unsafe_code)]

use super::http::{CancelToken, HttpError, HttpResponse, parse_http_url};
use super::playlist::ByteRange;
use std::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};
use windows::Win32::Foundation::{GetLastError, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::core::{HSTRING, PCSTR, PCWSTR};

type HInternet = *mut c_void;

const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
const WINHTTP_ADDREQ_FLAG_ADD: u32 = 0x2000_0000;
const WINHTTP_ADDREQ_FLAG_REPLACE: u32 = 0x8000_0000;
const WINHTTP_QUERY_CONTENT_TYPE: u32 = 1;
const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;
const WINHTTP_OPTION_URL: u32 = 34;

type WinHttpOpenFn = unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, u32) -> HInternet;
type WinHttpSetTimeoutsFn = unsafe extern "system" fn(HInternet, i32, i32, i32, i32) -> i32;
type WinHttpConnectFn = unsafe extern "system" fn(HInternet, PCWSTR, u16, u32) -> HInternet;
type WinHttpOpenRequestFn = unsafe extern "system" fn(
    HInternet,
    PCWSTR,
    PCWSTR,
    PCWSTR,
    PCWSTR,
    *const PCWSTR,
    u32,
) -> HInternet;
type WinHttpAddRequestHeadersFn = unsafe extern "system" fn(HInternet, PCWSTR, u32, u32) -> i32;
type WinHttpSendRequestFn =
    unsafe extern "system" fn(HInternet, PCWSTR, u32, *const c_void, u32, u32, usize) -> i32;
type WinHttpReceiveResponseFn = unsafe extern "system" fn(HInternet, *mut c_void) -> i32;
type WinHttpQueryHeadersFn =
    unsafe extern "system" fn(HInternet, u32, PCWSTR, *mut c_void, *mut u32, *mut u32) -> i32;
type WinHttpQueryOptionFn = unsafe extern "system" fn(HInternet, u32, *mut c_void, *mut u32) -> i32;
type WinHttpQueryDataAvailableFn = unsafe extern "system" fn(HInternet, *mut u32) -> i32;
type WinHttpReadDataFn = unsafe extern "system" fn(HInternet, *mut c_void, u32, *mut u32) -> i32;
type WinHttpCloseHandleFn = unsafe extern "system" fn(HInternet) -> i32;

struct WinHttp {
    connect: WinHttpConnectFn,
    open_request: WinHttpOpenRequestFn,
    add_request_headers: WinHttpAddRequestHeadersFn,
    send_request: WinHttpSendRequestFn,
    receive_response: WinHttpReceiveResponseFn,
    query_headers: WinHttpQueryHeadersFn,
    query_option: WinHttpQueryOptionFn,
    query_data_available: WinHttpQueryDataAvailableFn,
    read_data: WinHttpReadDataFn,
    close_handle: WinHttpCloseHandleFn,
    session: usize,
}

// SAFETY: WinHTTP session handles created with WinHttpOpen (sync mode) are safe to share across
// threads for opening independent connections and requests.
unsafe impl Send for WinHttp {}
// SAFETY: WinHTTP session handles are thread-safe per Win32 WinHTTP documentation.
unsafe impl Sync for WinHttp {}

static WINHTTP: OnceLock<Result<WinHttp, String>> = OnceLock::new();

fn api() -> Result<&'static WinHttp, String> {
    WINHTTP.get_or_init(load).as_ref().map_err(Clone::clone)
}

unsafe fn load_sym<F: Copy>(module: HMODULE, name: &std::ffi::CStr) -> Option<F> {
    // SAFETY: `GetProcAddress` looks up an exported symbol by NUL-terminated ASCII name.
    let f = unsafe { GetProcAddress(module, PCSTR(name.as_ptr().cast())) }?;
    debug_assert_eq!(size_of::<F>(), size_of_val(&f));
    // SAFETY: caller matches `F` to WinHTTP's documented `extern "system"` signature.
    Some(unsafe { std::mem::transmute_copy(&f) })
}

fn load() -> Result<WinHttp, String> {
    let missing = || "winhttp.dll is not available on this system".to_string();
    // SAFETY: loads Windows' System32 winhttp.dll only.
    let module = unsafe {
        LoadLibraryExW(
            &HSTRING::from("winhttp.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map_err(|_| missing())?;
    // SAFETY: every symbol is looked up by its documented WinHTTP export name and signature.
    unsafe {
        let open: WinHttpOpenFn = load_sym(module, c"WinHttpOpen").ok_or_else(missing)?;
        let set_timeouts: WinHttpSetTimeoutsFn =
            load_sym(module, c"WinHttpSetTimeouts").ok_or_else(missing)?;
        let connect: WinHttpConnectFn = load_sym(module, c"WinHttpConnect").ok_or_else(missing)?;
        let open_request: WinHttpOpenRequestFn =
            load_sym(module, c"WinHttpOpenRequest").ok_or_else(missing)?;
        let add_request_headers: WinHttpAddRequestHeadersFn =
            load_sym(module, c"WinHttpAddRequestHeaders").ok_or_else(missing)?;
        let send_request: WinHttpSendRequestFn =
            load_sym(module, c"WinHttpSendRequest").ok_or_else(missing)?;
        let receive_response: WinHttpReceiveResponseFn =
            load_sym(module, c"WinHttpReceiveResponse").ok_or_else(missing)?;
        let query_headers: WinHttpQueryHeadersFn =
            load_sym(module, c"WinHttpQueryHeaders").ok_or_else(missing)?;
        let query_option: WinHttpQueryOptionFn =
            load_sym(module, c"WinHttpQueryOption").ok_or_else(missing)?;
        let query_data_available: WinHttpQueryDataAvailableFn =
            load_sym(module, c"WinHttpQueryDataAvailable").ok_or_else(missing)?;
        let read_data: WinHttpReadDataFn =
            load_sym(module, c"WinHttpReadData").ok_or_else(missing)?;
        let close_handle: WinHttpCloseHandleFn =
            load_sym(module, c"WinHttpCloseHandle").ok_or_else(missing)?;

        let ua = HSTRING::from("Rune-Media-HLS/1.0");
        let mut session = open(
            PCWSTR(ua.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            session = open(
                PCWSTR(ua.as_ptr()),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            );
        }
        if session.is_null() {
            return Err(format!("WinHttpOpen failed ({})", last_error()));
        }
        let _ = set_timeouts(session, 10_000, 10_000, 15_000, 15_000);
        Ok(WinHttp {
            connect,
            open_request,
            add_request_headers,
            send_request,
            receive_response,
            query_headers,
            query_option,
            query_data_available,
            read_data,
            close_handle,
            session: session as usize,
        })
    }
}

fn last_error() -> u32 {
    // SAFETY: `GetLastError` reads the calling thread's Win32 error code.
    unsafe { GetLastError().0 }
}

/// Shared single-close wrapper so both the request thread and `CancelToken::cancel` can safely
/// attempt to close an in-flight WinHTTP request handle without ever double-closing it.
struct SharedHandle {
    close: WinHttpCloseHandleFn,
    raw: Mutex<Option<usize>>,
}

impl SharedHandle {
    fn new(close: WinHttpCloseHandleFn, handle: HInternet) -> Arc<Self> {
        Arc::new(Self {
            close,
            raw: Mutex::new(Some(handle as usize)),
        })
    }

    fn close_once(&self) {
        let taken = self.raw.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(addr) = taken {
            // SAFETY: `addr` was taken exclusively from `raw`, so `WinHttpCloseHandle` runs once.
            unsafe { (self.close)(addr as HInternet) };
        }
    }
}

struct OwnedHandle {
    close: WinHttpCloseHandleFn,
    handle: HInternet,
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: `self.handle` is an owned non-null WinHTTP handle.
            unsafe { (self.close)(self.handle) };
        }
    }
}

struct RequestGuard {
    handle: Arc<SharedHandle>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.handle.close_once();
    }
}

pub(super) fn get_once(
    url: &str,
    headers: &[(String, String)],
    range: Option<ByteRange>,
    max_bytes: usize,
    cancel: &CancelToken,
) -> Result<HttpResponse, HttpError> {
    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }
    let api = api().map_err(HttpError::Network)?;
    let parsed = parse_http_url(url).map_err(HttpError::Network)?;
    let started = rune_core::time::now();

    let host_w = HSTRING::from(parsed.host.as_str());
    let path_w = HSTRING::from(parsed.path_and_query.as_str());
    let method_w = HSTRING::from("GET");

    // SAFETY: `api.session` is a live process-wide WinHTTP session handle; `host_w` is NUL-terminated UTF-16.
    let conn_raw = unsafe {
        (api.connect)(
            api.session as HInternet,
            PCWSTR(host_w.as_ptr()),
            parsed.port,
            0,
        )
    };
    if conn_raw.is_null() {
        return Err(HttpError::Network(format!(
            "WinHttpConnect failed ({})",
            last_error()
        )));
    }
    let _conn = OwnedHandle {
        close: api.close_handle,
        handle: conn_raw,
    };

    let flags = if parsed.https { WINHTTP_FLAG_SECURE } else { 0 };
    // SAFETY: `conn_raw` is valid for `_conn`'s lifetime; UTF-16 strings are NUL-terminated.
    let req_raw = unsafe {
        (api.open_request)(
            conn_raw,
            PCWSTR(method_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        )
    };
    if req_raw.is_null() {
        return Err(HttpError::Network(format!(
            "WinHttpOpenRequest failed ({})",
            last_error()
        )));
    }

    let shared_req = SharedHandle::new(api.close_handle, req_raw);
    let _req_guard = RequestGuard {
        handle: shared_req.clone(),
    };
    let abort_handle = shared_req.clone();
    let _cancel_reg = cancel.register_abort(Box::new(move || abort_handle.close_once()));

    if cancel.is_cancelled() {
        return Err(HttpError::Cancelled);
    }

    // Add request headers and optional Range header.
    let mut header_block = String::new();
    for (name, value) in headers {
        if !name.contains(['\r', '\n']) && !value.contains(['\r', '\n']) {
            header_block.push_str(name);
            header_block.push_str(": ");
            header_block.push_str(value);
            header_block.push_str("\r\n");
        }
    }
    if let Some(r) = range {
        header_block.push_str("Range: ");
        header_block.push_str(&r.http_range_header());
        header_block.push_str("\r\n");
    }
    if !header_block.is_empty() {
        let hdr_w = HSTRING::from(header_block.as_str());
        // SAFETY: `req_raw` is alive unless cancelled; WinHTTP rejects invalid handles cleanly.
        let ok = unsafe {
            (api.add_request_headers)(
                req_raw,
                PCWSTR(hdr_w.as_ptr()),
                u32::MAX,
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            )
        };
        if ok == 0 {
            if cancel.is_cancelled() {
                return Err(HttpError::Cancelled);
            }
            return Err(HttpError::Network(format!(
                "WinHttpAddRequestHeaders failed ({})",
                last_error()
            )));
        }
    }

    // SAFETY: `req_raw` is valid for `_req_guard` unless cancelled, which causes WinHTTP to fail with ERROR_WINHTTP_OPERATION_CANCELLED.
    let sent = unsafe { (api.send_request)(req_raw, PCWSTR::null(), 0, std::ptr::null(), 0, 0, 0) };
    if sent == 0 {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        return Err(HttpError::Network(format!(
            "HTTP request failed ({})",
            last_error()
        )));
    }

    // SAFETY: paired with `WinHttpSendRequest` on `req_raw`.
    let recv = unsafe { (api.receive_response)(req_raw, std::ptr::null_mut()) };
    if recv == 0 {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        return Err(HttpError::Network(format!(
            "HTTP response failed ({})",
            last_error()
        )));
    }

    let mut status_code: u32 = 0;
    let mut status_size = size_of::<u32>() as u32;
    // SAFETY: queries a numeric 32-bit status code into `status_code`.
    let got_status = unsafe {
        (api.query_headers)(
            req_raw,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            (&raw mut status_code).cast(),
            &mut status_size,
            std::ptr::null_mut(),
        )
    };
    if got_status == 0 {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        return Err(HttpError::Network(format!(
            "failed to read HTTP status ({})",
            last_error()
        )));
    }

    let status = status_code as u16;
    if !(200..300).contains(&status) {
        return Err(HttpError::HttpStatus {
            status,
            url: url.to_string(),
        });
    }

    let content_type = query_string_header(api, req_raw, WINHTTP_QUERY_CONTENT_TYPE);
    let final_url =
        query_string_option(api, req_raw, WINHTTP_OPTION_URL).unwrap_or_else(|| url.to_string());

    let mut body = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        if cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        let mut avail: u32 = 0;
        // SAFETY: queries available unread bytes on `req_raw`.
        let ok = unsafe { (api.query_data_available)(req_raw, &mut avail) };
        if ok == 0 {
            if cancel.is_cancelled() {
                return Err(HttpError::Cancelled);
            }
            return Err(HttpError::Network(format!(
                "HTTP read availability failed ({})",
                last_error()
            )));
        }
        if avail == 0 {
            break;
        }
        let to_read = (avail as usize).min(chunk.len());
        let mut read_len: u32 = 0;
        // SAFETY: `chunk` has at least `to_read` writable bytes.
        let ok = unsafe {
            (api.read_data)(
                req_raw,
                chunk.as_mut_ptr().cast(),
                to_read as u32,
                &mut read_len,
            )
        };
        if ok == 0 {
            if cancel.is_cancelled() {
                return Err(HttpError::Cancelled);
            }
            return Err(HttpError::Network(format!(
                "HTTP body read failed ({})",
                last_error()
            )));
        }
        if read_len == 0 {
            break;
        }
        let next_len = body.len().saturating_add(read_len as usize);
        if next_len > max_bytes {
            return Err(HttpError::Network(format!(
                "HTTP response exceeded {max_bytes} byte limit"
            )));
        }
        body.extend_from_slice(&chunk[..read_len as usize]);
    }

    Ok(HttpResponse {
        status,
        body,
        content_type,
        final_url,
        elapsed: started.elapsed(),
    })
}

fn query_string_header(api: &WinHttp, req: HInternet, info_level: u32) -> Option<String> {
    let mut byte_len: u32 = 0;
    // SAFETY: probes required UTF-16 buffer byte length.
    unsafe {
        (api.query_headers)(
            req,
            info_level,
            PCWSTR::null(),
            std::ptr::null_mut(),
            &mut byte_len,
            std::ptr::null_mut(),
        )
    };
    if byte_len == 0 || byte_len > 8192 {
        return None;
    }
    let mut buf = vec![0u16; (byte_len as usize).div_ceil(2)];
    // SAFETY: `buf` has at least `byte_len` bytes.
    let ok = unsafe {
        (api.query_headers)(
            req,
            info_level,
            PCWSTR::null(),
            buf.as_mut_ptr().cast(),
            &mut byte_len,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    let units = (byte_len as usize / 2).min(buf.len());
    Some(
        String::from_utf16_lossy(&buf[..units])
            .trim_end_matches('\0')
            .to_string(),
    )
}

fn query_string_option(api: &WinHttp, req: HInternet, option: u32) -> Option<String> {
    let mut byte_len: u32 = 0;
    // SAFETY: probes required UTF-16 buffer byte length.
    unsafe { (api.query_option)(req, option, std::ptr::null_mut(), &mut byte_len) };
    if byte_len == 0 || byte_len > 16_384 {
        return None;
    }
    let mut buf = vec![0u16; (byte_len as usize).div_ceil(2)];
    // SAFETY: `buf` has at least `byte_len` bytes.
    let ok = unsafe { (api.query_option)(req, option, buf.as_mut_ptr().cast(), &mut byte_len) };
    if ok == 0 {
        return None;
    }
    let units = (byte_len as usize / 2).min(buf.len());
    Some(
        String::from_utf16_lossy(&buf[..units])
            .trim_end_matches('\0')
            .to_string(),
    )
}
