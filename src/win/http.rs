//! A minimal HTTPS GET over WinHTTP, the system's own HTTP stack (plan §13):
//! no TLS crate, the system proxy settings, and system certificate checks.
//! Used only by the updater.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

use crate::win::wide;

#[derive(Debug)]
pub enum HttpError {
    /// The server answered with a status other than 200.
    Status(u32),
    /// The body was larger than the caller allows.
    TooLarge,
    Cancelled,
    /// Anything else, with a message for a notice.
    Failed(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpError::Status(code) => write!(f, "the server answered {code}"),
            HttpError::TooLarge => write!(f, "the download was larger than expected"),
            HttpError::Cancelled => write!(f, "cancelled"),
            HttpError::Failed(message) => write!(f, "{message}"),
        }
    }
}

impl From<windows::core::Error> for HttpError {
    fn from(e: windows::core::Error) -> Self {
        HttpError::Failed(e.message())
    }
}

/// A WinHTTP handle, closed on drop.
struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void) -> Result<Self, HttpError> {
        if raw.is_null() {
            Err(windows::core::Error::from_thread().into())
        } else {
            Ok(Self(raw))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// GET an `https://` URL, following redirects, and return the body. Gives
/// up past `max_bytes` or once `cancel` is set (checked between reads).
pub fn get(
    url: &str,
    accept: &str,
    max_bytes: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, HttpError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| HttpError::Failed("only https:// URLs are fetched".into()))?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let agent = wide(&format!("smolcast/{}", env!("CARGO_PKG_VERSION")));
    let session = Handle::new(unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    })?;
    // Resolve, connect, send, receive (ms).
    unsafe { WinHttpSetTimeouts(session.0, 10_000, 10_000, 30_000, 30_000) }?;
    let host = wide(host);
    let connection = Handle::new(unsafe {
        WinHttpConnect(
            session.0,
            PCWSTR(host.as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        )
    })?;
    let path = wide(path);
    let request = Handle::new(unsafe {
        WinHttpOpenRequest(
            connection.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_OPEN_REQUEST_FLAGS(WINHTTP_FLAG_SECURE.0),
        )
    })?;
    let headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
    unsafe { WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0) }?;
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }?;

    let mut status = 0u32;
    let mut size = size_of::<u32>() as u32;
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
    }?;
    if status != 200 {
        return Err(HttpError::Status(status));
    }

    let mut body = Vec::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(HttpError::Cancelled);
        }
        let mut read = 0u32;
        unsafe {
            WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        }?;
        if read == 0 {
            return Ok(body);
        }
        if body.len() as u64 + u64::from(read) > max_bytes {
            return Err(HttpError::TooLarge);
        }
        body.extend_from_slice(&buffer[..read as usize]);
    }
}
