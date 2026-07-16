use bytes::Bytes;
use http::Request as HttpRequest;
use http::header;
use http_body_util::{BodyExt, Full};
use std::io::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

use crate::client::{
    HttpBody, RequestTimeouts, send_request_with_shared_abort, shared_abort_signal,
};
use tokio::time::{Duration, Instant};

const DEFAULT_DOWNLOAD_SMALL_THRESHOLD: usize = 64 * 1024;
const MAX_DOWNLOAD_REDIRECTS: usize = 10;

pub struct DownloadOptions {
    url: String,
    dest: std::path::PathBuf,
    sink: Option<Box<dyn BodySink>>,
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    resume: bool,
}

impl DownloadOptions {
    /// Build download options for a target URL and destination file path.
    pub fn new(url: impl Into<String>, dest: impl Into<std::path::PathBuf>) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sink: None,
            request_timeout: None,
            connect_timeout: None,
            resume: false,
        }
    }

    /// Mirror each downloaded chunk into an additional sink.
    pub fn with_sink(mut self, sink: Box<dyn BodySink>) -> Self {
        self.sink = Some(sink);
        self
    }

    /// Override the request timeout for this download.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Override the socket-connect timeout for this download.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Resume a previously-interrupted download by appending to the existing
    /// `.part` file if one is present and sending `Range: bytes=N-`.
    ///
    /// If the remote server returns 200 OK (does not honor the range), the
    /// existing `.part` is truncated and the full body is rewritten.
    /// If it returns 416 (range not satisfiable), a `.part` file that already
    /// matches the server's complete length is finalized; otherwise the
    /// `.part` is discarded and the call fails.
    pub fn with_resume(mut self) -> Self {
        self.resume = true;
        self
    }

    fn timeouts(&self) -> RequestTimeouts {
        RequestTimeouts {
            request_timeout: self.request_timeout,
            connect_timeout: self.connect_timeout,
        }
    }
}

pub trait BodySink: Send {
    fn write(&mut self, chunk: &[u8]) -> Result<(), String>;
    fn close(&mut self, result: &Result<(), String>);
}

/// Download a resource directly on the current task.
pub async fn download(
    options: DownloadOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<(), String> {
    let timeouts = options.timeouts();
    download_resource(
        &options.url,
        &options.dest,
        abort_rx,
        options.sink,
        timeouts,
        options.resume,
    )
    .await
}

/// Spawn a background download and receive completion through a oneshot channel.
pub fn spawn_download(
    options: DownloadOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<oneshot::Receiver<Result<(), String>>, String> {
    let timeouts = options.timeouts();
    request_download_inner(
        options.url,
        options.dest,
        abort_rx,
        options.sink,
        timeouts,
        options.resume,
    )
}

pub fn request_download(
    url: impl Into<String>,
    dest: impl Into<std::path::PathBuf>,
    abort_rx: Option<oneshot::Receiver<()>>,
    sink: Option<Box<dyn BodySink>>,
) -> Result<oneshot::Receiver<Result<(), String>>, String> {
    request_download_inner(url, dest, abort_rx, sink, RequestTimeouts::default(), false)
}

fn request_download_inner(
    url: impl Into<String>,
    dest: impl Into<std::path::PathBuf>,
    abort_rx: Option<oneshot::Receiver<()>>,
    sink: Option<Box<dyn BodySink>>,
    timeouts: RequestTimeouts,
    resume: bool,
) -> Result<oneshot::Receiver<Result<(), String>>, String> {
    let (completion_tx, completion_rx) = oneshot::channel();

    let url = url.into();
    let dest = dest.into();
    let network_access_guard = crate::http::current_network_access_guard();
    crate::RongExecutor::global().spawn(async move {
        let res = crate::http::scope_network_access_guard_opt(
            network_access_guard,
            download_resource(&url, &dest, abort_rx, sink, timeouts, resume),
        )
        .await;
        let _ = completion_tx.send(res);
    });

    Ok(completion_rx)
}

async fn download_resource(
    url: &str,
    dest: &std::path::PathBuf,
    abort_rx: Option<oneshot::Receiver<()>>,
    sink: Option<Box<dyn BodySink>>,
    timeouts: RequestTimeouts,
    resume: bool,
) -> Result<(), String> {
    let mut sink_opt = sink;

    if let Some(parent) = dest.parent()
        && let Err(e) = tokio::fs::create_dir_all(parent).await
    {
        return finalize_sink(sink_opt, Err(format!("create dir: {}", e)));
    }

    let temp_path = dest.with_extension("part");

    // Determine resume offset from an existing `.part` file.
    let resume_offset: u64 = if resume {
        match tokio::fs::metadata(&temp_path).await {
            Ok(meta) if meta.is_file() => meta.len(),
            _ => 0,
        }
    } else {
        0
    };

    // When resuming we open in append mode to preserve existing bytes;
    // otherwise truncate as before.
    let mut file = if resume_offset > 0 {
        match tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&temp_path)
            .await
        {
            Ok(f) => f,
            Err(e) => return finalize_sink(sink_opt, Err(format!("open: {}", e))),
        }
    } else {
        match tokio::fs::File::create(&temp_path).await {
            Ok(f) => f,
            Err(e) => return finalize_sink(sink_opt, Err(format!("open: {}", e))),
        }
    };

    let small_threshold = DEFAULT_DOWNLOAD_SMALL_THRESHOLD;
    let mut current_url = url.to_string();
    let mut redirect_count = 0usize;
    let abort_signal = shared_abort_signal(abort_rx);
    let request_deadline = timeouts
        .request_timeout
        .map(|timeout| Instant::now() + timeout);
    let resp = loop {
        let request = match build_download_request(&current_url, resume_offset) {
            Ok(request) => request,
            Err(e) => return finalize_sink(sink_opt, Err(e)),
        };
        if let Err(err) = crate::http::check_current_network_access(&request) {
            return finalize_sink(sink_opt, Err(err.to_string()));
        }
        let remaining_timeouts = match remaining_request_timeouts(timeouts, request_deadline) {
            Ok(timeouts) => timeouts,
            Err(e) => return finalize_sink(sink_opt, Err(e)),
        };
        let resp = match send_request_with_shared_abort(
            request,
            small_threshold,
            abort_signal.clone(),
            crate::client::DEFAULT_STREAM_COALESCE_TARGET,
            remaining_timeouts,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => return finalize_sink(sink_opt, Err(e)),
        };

        if !is_download_redirect(resp.status) {
            break resp;
        }
        if redirect_count >= MAX_DOWNLOAD_REDIRECTS {
            return finalize_sink(sink_opt, Err("too many redirects".to_string()));
        }
        current_url = match redirect_target(&current_url, &resp.headers) {
            Ok(url) => url,
            Err(e) => return finalize_sink(sink_opt, Err(e)),
        };
        redirect_count += 1;
    };

    let mut expected_partial_body_len = None;
    if resume_offset == 0 && resp.status == http::StatusCode::PARTIAL_CONTENT {
        return finalize_sink(
            sink_opt,
            Err("unexpected partial response without a resume range".to_string()),
        );
    }
    if resume_offset > 0 {
        if resp.status == http::StatusCode::OK {
            drop(file);
            file = match tokio::fs::File::create(&temp_path).await {
                Ok(f) => f,
                Err(e) => return finalize_sink(sink_opt, Err(format!("reopen: {}", e))),
            };
        } else if resp.status == http::StatusCode::PARTIAL_CONTENT {
            match content_range(&resp.headers) {
                Ok(ContentRange::Bytes {
                    start,
                    end,
                    complete_length,
                }) if start == resume_offset => {
                    let range_end = end.checked_add(1).ok_or_else(|| {
                        "resume Content-Range end exceeds supported length".to_string()
                    });
                    let range_end = match range_end {
                        Ok(range_end) => range_end,
                        Err(e) => return finalize_sink(sink_opt, Err(e)),
                    };
                    match complete_length {
                        Some(length) if range_end == length => {
                            expected_partial_body_len = Some(end - start + 1);
                        }
                        Some(length) => {
                            return finalize_sink(
                                sink_opt,
                                Err(format!(
                                    "resume Content-Range ends before complete length: end {}, length {}",
                                    end, length
                                )),
                            );
                        }
                        None => {
                            return finalize_sink(
                                sink_opt,
                                Err("resume Content-Range must include a complete length"
                                    .to_string()),
                            );
                        }
                    }
                }
                Ok(ContentRange::Bytes { start, .. }) => {
                    return finalize_sink(
                        sink_opt,
                        Err(format!(
                            "resume content range starts at {}, expected {}",
                            start, resume_offset
                        )),
                    );
                }
                Ok(ContentRange::Unsatisfied { .. }) => {
                    return finalize_sink(
                        sink_opt,
                        Err("invalid content range for partial response".to_string()),
                    );
                }
                Err(e) => return finalize_sink(sink_opt, Err(e)),
            }
        } else if resp.status == http::StatusCode::RANGE_NOT_SATISFIABLE {
            let already_complete = matches!(
                content_range(&resp.headers),
                Ok(ContentRange::Unsatisfied {
                    complete_length: Some(length),
                }) if length == resume_offset
            );
            drop(file);
            if already_complete {
                if let Err(e) = tokio::fs::rename(&temp_path, dest).await {
                    return finalize_sink(sink_opt, Err(format!("rename: {}", e)));
                }
                return finalize_sink(sink_opt, Ok(()));
            }
            let _ = tokio::fs::remove_file(&temp_path).await;
            return finalize_sink(
                sink_opt,
                Err("range not satisfiable; stale `.part` discarded".to_string()),
            );
        } else if resp.status.is_success() {
            return finalize_sink(
                sink_opt,
                Err(format!(
                    "unexpected resume response status: {}",
                    resp.status
                )),
            );
        }
    }
    if !resp.status.is_success() {
        return finalize_sink(sink_opt, Err(format!("http status: {}", resp.status)));
    }

    let mut sink_active = true;
    let forward = |data: &[u8], sink_opt: &mut Option<Box<dyn BodySink>>, active: &mut bool| {
        if *active
            && let Some(sink) = sink_opt.as_mut()
            && let Err(err) = sink.write(data)
        {
            let sink_err = Err(err.clone());
            sink.close(&sink_err);
            *sink_opt = None;
            *active = false;
        }
    };

    let mut received_body_len = 0u64;
    let check_partial_body_len = |chunk_len: usize, received: &mut u64| {
        *received = received
            .checked_add(chunk_len as u64)
            .ok_or_else(|| "response body length overflow".to_string())?;
        if expected_partial_body_len.is_some_and(|expected| *received > expected) {
            return Err("response body exceeds Content-Range length".to_string());
        }
        Ok(())
    };

    match resp.body {
        HttpBody::Empty => {}
        HttpBody::Small(buf) => {
            if let Err(e) = check_partial_body_len(buf.len(), &mut received_body_len) {
                return finalize_sink(sink_opt, Err(e));
            }
            forward(buf.as_ref(), &mut sink_opt, &mut sink_active);
            if let Err(e) = file.write_all(buf.as_ref()).await {
                return finalize_sink(sink_opt, Err(format!("write chunk: {}", e)));
            }
        }
        HttpBody::Stream(mut rx_body) => {
            while let Some(chunk_res) = rx_body.recv().await {
                let bytes = match chunk_res {
                    Ok(b) => b,
                    Err(e) => return finalize_sink(sink_opt, Err(e)),
                };

                if let Err(e) = check_partial_body_len(bytes.len(), &mut received_body_len) {
                    return finalize_sink(sink_opt, Err(e));
                }
                forward(bytes.as_ref(), &mut sink_opt, &mut sink_active);

                if let Err(e) = file.write_all(bytes.as_ref()).await {
                    return finalize_sink(sink_opt, Err(format!("write chunk: {}", e)));
                }
            }
        }
    }

    if let Some(expected) = expected_partial_body_len
        && received_body_len != expected
    {
        return finalize_sink(
            sink_opt,
            Err(format!(
                "response body length {} does not match Content-Range length {}",
                received_body_len, expected
            )),
        );
    }

    if let Err(e) = file.flush().await {
        return finalize_sink(sink_opt, Err(format!("flush: {}", e)));
    }
    drop(file);

    if let Err(e) = tokio::fs::rename(&temp_path, dest).await {
        return finalize_sink(sink_opt, Err(format!("rename: {}", e)));
    }

    finalize_sink(sink_opt, Ok(()))
}

fn remaining_request_timeouts(
    timeouts: RequestTimeouts,
    deadline: Option<Instant>,
) -> Result<RequestTimeouts, String> {
    let request_timeout = match deadline {
        Some(deadline) => {
            let now = Instant::now();
            if now >= deadline {
                return Err("request timeout".to_string());
            }
            Some(deadline - now)
        }
        None => timeouts.request_timeout,
    };

    Ok(RequestTimeouts {
        request_timeout,
        connect_timeout: timeouts.connect_timeout,
    })
}

fn finalize_sink(
    sink_opt: Option<Box<dyn BodySink>>,
    res: Result<(), String>,
) -> Result<(), String> {
    if let Some(mut sink) = sink_opt {
        sink.close(&res);
    }
    res
}

fn build_download_request(
    url: &str,
    resume_offset: u64,
) -> Result<HttpRequest<http_body_util::combinators::BoxBody<Bytes, Error>>, String> {
    let mut builder = HttpRequest::builder()
        .method("GET")
        .uri(url)
        .header(header::ACCEPT, "*/*");
    if let Some(headers) = builder.headers_mut() {
        let ua = crate::get_user_agent();
        let value = header::HeaderValue::from_str(&ua)
            .map_err(|e| format!("invalid user agent header: {}", e))?;
        headers.insert(header::USER_AGENT, value);
        if resume_offset > 0 {
            let range = format!("bytes={}-", resume_offset);
            let value = header::HeaderValue::from_str(&range)
                .map_err(|e| format!("invalid range header: {}", e))?;
            headers.insert(header::RANGE, value);
        }
    }
    let empty = Full::new(Bytes::new())
        .map_err(|_| Error::other("body error"))
        .boxed();
    builder
        .body(empty)
        .map_err(|e| format!("build request: {}", e))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentRange {
    Bytes {
        start: u64,
        end: u64,
        complete_length: Option<u64>,
    },
    Unsatisfied {
        complete_length: Option<u64>,
    },
}

fn content_range(headers: &http::HeaderMap) -> Result<ContentRange, String> {
    let value = headers
        .get(header::CONTENT_RANGE)
        .ok_or_else(|| "missing Content-Range header".to_string())?
        .to_str()
        .map_err(|e| format!("invalid Content-Range header: {}", e))?;
    parse_content_range(value)
        .ok_or_else(|| format!("invalid Content-Range header value: {}", value))
}

fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes ")?;
    if let Some(length) = rest.strip_prefix("*/") {
        return Some(ContentRange::Unsatisfied {
            complete_length: parse_complete_length(length.trim())?,
        });
    }

    let (range, complete_length) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    if end < start {
        return None;
    }
    let complete_length = parse_complete_length(complete_length.trim())?;
    if let Some(length) = complete_length
        && end >= length
    {
        return None;
    }
    Some(ContentRange::Bytes {
        start,
        end,
        complete_length,
    })
}

fn parse_complete_length(value: &str) -> Option<Option<u64>> {
    if value == "*" {
        return Some(None);
    }
    value.parse::<u64>().ok().map(Some)
}

fn is_download_redirect(status: http::StatusCode) -> bool {
    matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308)
}

fn redirect_target(current_url: &str, headers: &http::HeaderMap) -> Result<String, String> {
    let location = headers
        .get(header::LOCATION)
        .ok_or_else(|| "redirect response missing Location header".to_string())?
        .to_str()
        .map_err(|e| format!("invalid redirect Location header: {}", e))?
        .trim();
    if location.is_empty() {
        return Err("redirect Location header is empty".to_string());
    }

    let base = url::Url::parse(current_url)
        .map_err(|e| format!("invalid current URL for redirect: {}", e))?;
    let next = base
        .join(location)
        .map_err(|e| format!("invalid redirect URL: {}", e))?;
    match next.scheme() {
        "http" | "https" => Ok(next.to_string()),
        scheme => Err(format!("unsupported redirect URL scheme: {}", scheme)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct DenyExampleGuard;

    impl crate::http::NetworkAccessGuard for DenyExampleGuard {
        fn check_access(&self, uri: &crate::http::Uri) -> Result<(), crate::http::HttpError> {
            if uri.host() == Some("denied.example.com") {
                return Err(crate::http::HttpError::access_denied(
                    "network access denied",
                ));
            }
            Ok(())
        }
    }

    // Pool starts lazily on first spawn/handle; nothing to do here.

    async fn spawn_file_server(content: &'static [u8]) -> std::net::SocketAddr {
        use axum::Router;
        use axum::routing::get;

        let app = Router::new().route("/file", get(move || async move { content }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    async fn spawn_slow_server(delay_ms: u64) -> std::net::SocketAddr {
        use axum::Router;
        use axum::routing::get;

        let app = Router::new().route(
            "/slow",
            get(move || async move {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                "data"
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    #[test]
    fn spawn_download_with_options_succeeds() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let content = b"hello download";
            let addr = spawn_file_server(content).await;
            let url = format!("http://{}/file", addr);

            let dest = std::env::temp_dir().join(format!(
                "rong_dl_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));

            let rx = spawn_download(
                DownloadOptions::new(&url, &dest)
                    .with_request_timeout(Duration::from_secs(5))
                    .with_connect_timeout(Duration::from_secs(1)),
                None,
            )
            .expect("should queue download");
            rx.await
                .expect("channel dropped")
                .expect("download should succeed");

            let written = tokio::fs::read(&dest).await.expect("file should exist");
            assert_eq!(written, content);
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn download_convenience_succeeds() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let content = b"hello direct download";
            let addr = spawn_file_server(content).await;
            let url = format!("http://{}/file", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_direct_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));

            download(DownloadOptions::new(&url, &dest), None)
                .await
                .expect("download should succeed");

            let written = tokio::fs::read(&dest).await.expect("file should exist");
            assert_eq!(written, content);
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn scoped_network_access_guard_blocks_spawn_download() {
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_denied_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));

            let err = crate::http::scope_network_access_guard(Arc::new(DenyExampleGuard), async {
                let rx = spawn_download(
                    DownloadOptions::new("http://denied.example.com/file", &dest),
                    None,
                )
                .expect("should queue download");
                rx.await
                    .expect("channel dropped")
                    .expect_err("download should be denied")
            })
            .await;

            assert_eq!(err, "network access denied");
            let _ = tokio::fs::remove_file(&dest).await;
            let _ = tokio::fs::remove_file(dest.with_extension("part")).await;
        });
    }

    /// Spawn a server that honors `Range: bytes=N-` with a 206 response
    /// containing the suffix of `content` starting at offset N.
    async fn spawn_range_aware_server(content: &'static [u8]) -> std::net::SocketAddr {
        use axum::Router;
        use axum::body::Body;
        use axum::http::{HeaderMap, StatusCode};
        use axum::response::Response;
        use axum::routing::get;

        let app = Router::new().route(
            "/file",
            get(move |headers: HeaderMap| async move {
                let mut start: usize = 0;
                if let Some(range) = headers.get(http::header::RANGE)
                    && let Ok(value) = range.to_str()
                    && let Some(spec) = value.strip_prefix("bytes=")
                    && let Some((begin, _end)) = spec.split_once('-')
                    && let Ok(n) = begin.parse::<usize>()
                {
                    start = n;
                }
                if start >= content.len() {
                    return Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header(
                            http::header::CONTENT_RANGE,
                            format!("bytes */{}", content.len()),
                        )
                        .body(Body::empty())
                        .unwrap();
                }
                let suffix = &content[start..];
                let mut builder = Response::builder();
                if start > 0 {
                    builder = builder.status(StatusCode::PARTIAL_CONTENT).header(
                        http::header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", start, content.len() - 1, content.len()),
                    );
                } else {
                    builder = builder.status(StatusCode::OK);
                }
                builder.body(Body::from(suffix.to_vec())).unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    async fn spawn_bad_206_server(content: &'static [u8]) -> std::net::SocketAddr {
        use axum::Router;
        use axum::body::Body;
        use axum::http::StatusCode;
        use axum::response::Response;
        use axum::routing::get;

        let app = Router::new().route(
            "/file",
            get(move || async move {
                Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(
                        http::header::CONTENT_RANGE,
                        format!("bytes 0-{}/{}", content.len() - 1, content.len()),
                    )
                    .body(Body::from(content.to_vec()))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    #[test]
    fn download_resume_appends_remaining_bytes() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let content: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            let addr = spawn_range_aware_server(content).await;
            let url = format!("http://{}/file", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_resume_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let part = dest.with_extension("part");

            // Pre-seed a partial download — first 10 bytes already on disk.
            tokio::fs::write(&part, &content[..10]).await.unwrap();

            download(DownloadOptions::new(&url, &dest).with_resume(), None)
                .await
                .expect("resume download should succeed");

            let written = tokio::fs::read(&dest).await.expect("file should exist");
            assert_eq!(written, content);
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn download_resume_restarts_on_416() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let content: &'static [u8] = b"shortbody";
            let addr = spawn_range_aware_server(content).await;
            let url = format!("http://{}/file", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_resume_416_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let part = dest.with_extension("part");
            // Seed a `.part` that is larger than the server's body so the
            // resume request gets back 416 Range Not Satisfiable.
            tokio::fs::write(&part, vec![0u8; content.len() * 2])
                .await
                .unwrap();

            let err = download(DownloadOptions::new(&url, &dest).with_resume(), None)
                .await
                .expect_err("resume should fail with 416");
            assert!(err.contains("range not satisfiable"), "got: {err}");
            // The stale `.part` should have been discarded so the next
            // attempt starts fresh.
            assert!(!part.exists(), "expected stale .part to be removed");
        });
    }

    #[test]
    fn download_resume_finalizes_complete_part_on_416() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let content: &'static [u8] = b"already complete";
            let addr = spawn_range_aware_server(content).await;
            let url = format!("http://{}/file", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_resume_complete_416_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let part = dest.with_extension("part");
            tokio::fs::write(&part, content).await.unwrap();

            download(DownloadOptions::new(&url, &dest).with_resume(), None)
                .await
                .expect("complete .part should be finalized");

            let written = tokio::fs::read(&dest).await.expect("file should exist");
            assert_eq!(written, content);
            assert!(!part.exists(), "expected .part to be renamed");
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn download_resume_rejects_mismatched_206_content_range() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let content: &'static [u8] = b"full remote body";
            let addr = spawn_bad_206_server(content).await;
            let url = format!("http://{}/file", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_resume_bad_206_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let part = dest.with_extension("part");
            tokio::fs::write(&part, b"partial").await.unwrap();

            let err = download(DownloadOptions::new(&url, &dest).with_resume(), None)
                .await
                .expect_err("mismatched 206 should fail");

            assert!(
                err.contains("resume content range starts"),
                "expected content range error, got: {err}"
            );
            let written = tokio::fs::read(&part).await.expect(".part should remain");
            assert_eq!(written, b"partial");
            assert!(!dest.exists(), "destination should not be finalized");
            let _ = tokio::fs::remove_file(&part).await;
        });
    }

    #[test]
    fn download_resume_rejects_truncated_206_body() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            use axum::Router;
            use axum::body::Body;
            use axum::http::StatusCode;
            use axum::response::Response;
            use axum::routing::get;

            let complete: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            let app = Router::new().route(
                "/file",
                get(move || async move {
                    Response::builder()
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(
                            http::header::CONTENT_RANGE,
                            format!("bytes 10-{}/{}", complete.len() - 1, complete.len()),
                        )
                        .body(Body::from(complete[10..15].to_vec()))
                        .unwrap()
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            let temp = tempfile::tempdir().expect("temporary download directory");
            let dest = temp.path().join("download.bin");
            let part = dest.with_extension("part");
            tokio::fs::write(&part, &complete[..10]).await.unwrap();

            let err = download(
                DownloadOptions::new(format!("http://{addr}/file"), &dest).with_resume(),
                None,
            )
            .await
            .expect_err("a truncated partial response must not be finalized as a valid download");
            assert!(
                err.contains("does not match Content-Range length"),
                "unexpected download error: {err}"
            );

            assert!(!dest.exists(), "corrupt download must not be finalized");
            assert!(
                part.exists(),
                "partial data should remain available for retry"
            );
        });
    }

    #[test]
    fn fresh_download_rejects_unsolicited_partial_response() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let complete: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            let addr = spawn_bad_206_server(&complete[..5]).await;
            let temp = tempfile::tempdir().expect("temporary download directory");
            let dest = temp.path().join("download.bin");

            let err = download(
                DownloadOptions::new(format!("http://{addr}/file"), &dest),
                None,
            )
            .await
            .expect_err("an unsolicited partial response must not be finalized");
            assert!(err.contains("without a resume range"));

            assert!(!dest.exists());
        });
    }

    #[test]
    fn resumed_download_rejects_range_that_ends_before_complete_length() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            use axum::Router;
            use axum::body::Body;
            use axum::http::StatusCode;
            use axum::response::Response;
            use axum::routing::get;

            let complete: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
            let app = Router::new().route(
                "/file",
                get(move || async move {
                    Response::builder()
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(
                            http::header::CONTENT_RANGE,
                            format!("bytes 10-14/{}", complete.len()),
                        )
                        .body(Body::from(complete[10..15].to_vec()))
                        .unwrap()
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            let temp = tempfile::tempdir().expect("temporary download directory");
            let dest = temp.path().join("download.bin");
            let part = dest.with_extension("part");
            tokio::fs::write(&part, &complete[..10]).await.unwrap();

            let err = download(
                DownloadOptions::new(format!("http://{addr}/file"), &dest).with_resume(),
                None,
            )
            .await
            .expect_err("an incomplete open-ended range must not be finalized");
            assert!(err.contains("ends before complete length"));

            assert!(!dest.exists());
            assert!(part.exists());
        });
    }

    #[test]
    fn redirect_target_resolves_relative_location() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            header::LOCATION,
            header::HeaderValue::from_static("/next.jpg"),
        );
        let target = redirect_target("https://example.com/a/start.jpg", &headers).unwrap();
        assert_eq!(target, "https://example.com/next.jpg");
    }

    #[test]
    fn redirect_target_rejects_non_http_scheme() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            header::LOCATION,
            header::HeaderValue::from_static("file:///tmp/a.jpg"),
        );
        let err = redirect_target("https://example.com/a/start.jpg", &headers).unwrap_err();
        assert!(err.contains("unsupported redirect URL scheme"));
    }

    #[test]
    fn download_follows_redirect() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            use axum::Router;
            use axum::response::IntoResponse;
            use axum::routing::get;

            let content = b"redirected download";
            let app = Router::new()
                .route("/file", get(move || async move { content }))
                .route(
                    "/redirect",
                    get(|| async {
                        (
                            http::StatusCode::FOUND,
                            [(http::header::LOCATION, "/file")],
                            "",
                        )
                            .into_response()
                    }),
                );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            let url = format!("http://{}/redirect", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_redirect_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));

            download(DownloadOptions::new(&url, &dest), None)
                .await
                .expect("download should follow redirect");

            let written = tokio::fs::read(&dest).await.expect("file should exist");
            assert_eq!(written, content);
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn download_abort_survives_redirect() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            use axum::Router;
            use axum::response::IntoResponse;
            use axum::routing::get;

            let app = Router::new()
                .route(
                    "/slow-file",
                    get(|| async {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        "redirected download"
                    }),
                )
                .route(
                    "/redirect",
                    get(|| async {
                        (
                            http::StatusCode::FOUND,
                            [(http::header::LOCATION, "/slow-file")],
                            "",
                        )
                            .into_response()
                    }),
                );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            let url = format!("http://{}/redirect", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_redirect_abort_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let (abort_tx, abort_rx) = oneshot::channel();

            let rx = spawn_download(DownloadOptions::new(&url, &dest), Some(abort_rx))
                .expect("should queue download");
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = abort_tx.send(());

            let err = rx
                .await
                .expect("channel dropped")
                .expect_err("download should abort");
            assert!(
                err.contains("aborted"),
                "expected abort error, got: {}",
                err
            );
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn download_redirects_share_request_timeout_budget() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            use axum::Router;
            use axum::response::IntoResponse;
            use axum::routing::get;

            let app = Router::new()
                .route(
                    "/slow-file",
                    get(|| async {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        "redirected download"
                    }),
                )
                .route(
                    "/redirect",
                    get(|| async {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        (
                            http::StatusCode::FOUND,
                            [(http::header::LOCATION, "/slow-file")],
                            "",
                        )
                            .into_response()
                    }),
                );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, app).await.unwrap();
            });

            let url = format!("http://{}/redirect", addr);
            let dest = std::env::temp_dir().join(format!(
                "rong_dl_redirect_timeout_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));

            let err = download(
                DownloadOptions::new(&url, &dest)
                    .with_request_timeout(Duration::from_millis(150))
                    .with_connect_timeout(Duration::from_secs(1)),
                None,
            )
            .await
            .expect_err("redirect chain should exhaust the timeout budget");

            assert!(
                err.contains("timeout"),
                "expected timeout error, got: {}",
                err
            );
            let _ = tokio::fs::remove_file(&dest).await;
        });
    }

    #[test]
    fn download_with_timeout_expires() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_slow_server(300).await;
            let url = format!("http://{}/slow", addr);
            let dest = std::env::temp_dir().join("rong_dl_timeout_test.bin");

            let rx = spawn_download(
                DownloadOptions::new(&url, &dest)
                    .with_request_timeout(Duration::from_millis(10))
                    .with_connect_timeout(Duration::from_secs(1)),
                None,
            )
            .expect("should queue download");
            let err = rx
                .await
                .expect("channel dropped")
                .expect_err("should time out");
            assert!(
                err.contains("timeout"),
                "expected timeout error, got: {}",
                err
            );
        });
    }
}
