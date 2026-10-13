use bytes::Bytes;
use http::Request as HttpRequest;
use http::header;
use http::{HeaderValue, Method, StatusCode, header::HeaderName};
use http_body::Frame;
use http_body_util::{BodyExt, StreamBody, combinators::BoxBody};
use std::io::Error;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

use crate::client::{HttpBody, RequestTimeouts, send_request_with_timeout};
use crate::observer::{HopId, NetworkRequestKind, OwnerCapture, tag_request};

const UPLOAD_CHUNK_SIZE: usize = 64 * 1024;
const UPLOAD_BODY_CHAN_CAP: usize = 16;
const UPLOAD_EVENT_CHAN_CAP: usize = 128;

#[derive(Clone, Debug)]
pub struct UploadOptions {
    url: String,
    file_path: PathBuf,
    method: Method,
    headers: Vec<(String, String)>,
    content_type: Option<String>,
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
}

impl UploadOptions {
    /// Build upload options for a file path and destination URL.
    pub fn new(url: impl Into<String>, file_path: impl AsRef<Path>) -> Self {
        Self {
            url: url.into(),
            file_path: file_path.as_ref().to_path_buf(),
            method: Method::PUT,
            headers: Vec::new(),
            content_type: None,
            request_timeout: None,
            connect_timeout: None,
        }
    }

    /// Override the HTTP method used for the upload request.
    pub fn with_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    /// Add a request header to the upload request.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Set the `Content-Type` header for the upload request.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Override the request timeout for this upload.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Override the socket-connect timeout for this upload.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    fn timeouts(&self) -> RequestTimeouts {
        RequestTimeouts {
            request_timeout: self.request_timeout,
            connect_timeout: self.connect_timeout,
        }
    }
}

/// Terminal upload response returned after the request finishes.
#[derive(Clone, Debug)]
pub struct UploadResponse {
    pub status: StatusCode,
    pub body: Bytes,
}

/// Upload lifecycle events surfaced by the streaming upload API.
#[derive(Clone, Debug)]
pub enum UploadEvent {
    Progress {
        uploaded_bytes: u64,
        total_bytes: Option<u64>,
    },
    Success(UploadResponse),
}

/// Handle for a background upload operation.
pub struct UploadTask {
    pub events: mpsc::Receiver<Result<UploadEvent, String>>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

/// Start an upload and receive progress events from a background task.
pub fn spawn_upload(
    options: UploadOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<UploadTask, String> {
    request_upload(options, abort_rx)
}

/// Upload a file on the current task and return only the terminal response.
pub async fn upload(
    options: UploadOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<UploadResponse, String> {
    let mut task = request_upload(options, abort_rx)?;
    while let Some(event) = task.events.recv().await {
        match event? {
            UploadEvent::Progress { .. } => {}
            UploadEvent::Success(response) => return Ok(response),
        }
    }
    Err("upload task ended without a terminal response".to_string())
}

impl UploadTask {
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn into_parts(
        mut self,
    ) -> (
        mpsc::Receiver<Result<UploadEvent, String>>,
        Option<oneshot::Sender<()>>,
    ) {
        let (_dummy_tx, dummy_rx) = mpsc::channel(1);
        let events = std::mem::replace(&mut self.events, dummy_rx);
        (events, self.cancel_tx.take())
    }

    pub fn into_stream(self) -> UploadEventStream {
        let (events, cancel_tx) = self.into_parts();
        UploadEventStream {
            inner: ReceiverStream::new(events),
            cancel_tx,
        }
    }
}

impl Drop for UploadTask {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub struct UploadEventStream {
    inner: ReceiverStream<Result<UploadEvent, String>>,
    cancel_tx: Option<oneshot::Sender<()>>,
}

impl UploadEventStream {
    pub fn cancel(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for UploadEventStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl tokio_stream::Stream for UploadEventStream {
    type Item = Result<UploadEvent, String>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

pub fn request_upload(
    options: UploadOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<UploadTask, String> {
    let (events_tx, events_rx) =
        mpsc::channel::<Result<UploadEvent, String>>(UPLOAD_EVENT_CHAN_CAP);
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let network_access_guard = crate::http::current_network_access_guard();
    let owner = OwnerCapture::capture();

    crate::RongExecutor::global().spawn(async move {
        owner
            .scope(crate::http::scope_network_access_guard_opt(
                network_access_guard,
                run_upload_worker(options, abort_rx, cancel_rx, events_tx),
            ))
            .await;
    });

    Ok(UploadTask {
        events: events_rx,
        cancel_tx: Some(cancel_tx),
    })
}

async fn run_upload_worker(
    options: UploadOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
    cancel_rx: oneshot::Receiver<()>,
    events_tx: mpsc::Sender<Result<UploadEvent, String>>,
) {
    let (stop_tx, stop_rx) = watch::channel(false);

    let stop_tx_cancel = stop_tx.clone();
    tokio::task::spawn(async move {
        let _ = cancel_rx.await;
        let _ = stop_tx_cancel.send(true);
    });
    if let Some(abort_rx) = abort_rx {
        let stop_tx_abort = stop_tx.clone();
        tokio::task::spawn(async move {
            let _ = abort_rx.await;
            let _ = stop_tx_abort.send(true);
        });
    }

    let mut file = match tokio::fs::File::open(&options.file_path).await {
        Ok(f) => f,
        Err(e) => {
            let _ = events_tx
                .send(Err(format!(
                    "open upload file '{}': {}",
                    options.file_path.display(),
                    e
                )))
                .await;
            return;
        }
    };

    let total_bytes = match file.metadata().await {
        Ok(meta) => Some(meta.len()),
        Err(_) => None,
    };

    let (body_tx, body_rx) = mpsc::channel::<Result<Bytes, Error>>(UPLOAD_BODY_CHAN_CAP);
    let body_stream = ReceiverStream::new(body_rx).map(|item| item.map(Frame::data));
    let request_body: BoxBody<Bytes, Error> = StreamBody::new(body_stream).boxed();
    let mut request = match build_request(&options, total_bytes, request_body) {
        Ok(req) => req,
        Err(e) => {
            let _ = events_tx.send(Err(e)).await;
            return;
        }
    };
    tag_request(&mut request, NetworkRequestKind::Upload, HopId::FIRST);
    if let Err(err) = crate::http::check_current_network_access(&request) {
        let _ = events_tx
            .send(Err(format!("upload request failed: {}", err)))
            .await;
        return;
    }

    let progress_tx = events_tx.clone();
    let stop_rx_reader = stop_rx.clone();
    let reader_handle = tokio::task::spawn(async move {
        let mut uploaded: u64 = 0;
        let mut chunk = vec![0u8; UPLOAD_CHUNK_SIZE];
        loop {
            if *stop_rx_reader.borrow() {
                break;
            }
            let n = file
                .read(&mut chunk)
                .await
                .map_err(|e| format!("read upload file: {}", e))?;
            if n == 0 {
                break;
            }

            let bytes = Bytes::copy_from_slice(&chunk[..n]);
            uploaded = uploaded.saturating_add(n as u64);
            if body_tx.send(Ok(bytes)).await.is_err() {
                break;
            }

            let _ = progress_tx
                .try_send(Ok(UploadEvent::Progress {
                    uploaded_bytes: uploaded,
                    total_bytes,
                }))
                .ok();
        }
        Ok::<(), String>(())
    });

    let (net_abort_tx, net_abort_rx) = oneshot::channel::<()>();
    let mut stop_rx_net = stop_rx.clone();
    tokio::task::spawn(async move {
        loop {
            if *stop_rx_net.borrow() {
                let _ = net_abort_tx.send(());
                break;
            }
            if stop_rx_net.changed().await.is_err() {
                break;
            }
        }
    });

    let response =
        match send_request_with_timeout(request, 0, Some(net_abort_rx), options.timeouts()).await {
            Ok(resp) => resp,
            Err(e) => {
                let _ = events_tx
                    .send(Err(format!("upload request failed: {}", e)))
                    .await;
                return;
            }
        };

    match reader_handle.await {
        Err(join_err) => {
            let _ = events_tx
                .send(Err(format!("upload reader task failed: {}", join_err)))
                .await;
            return;
        }
        Ok(Err(err)) => {
            let _ = events_tx.send(Err(err)).await;
            return;
        }
        Ok(Ok(())) => {}
    }

    if *stop_rx.borrow() {
        let _ = events_tx.send(Err("upload aborted".to_string())).await;
        return;
    }

    let body = match collect_response_body(response.body).await {
        Ok(body) => body,
        Err(e) => {
            let _ = events_tx
                .send(Err(format!("read upload response body: {}", e)))
                .await;
            return;
        }
    };

    let event = UploadEvent::Success(UploadResponse {
        status: response.status,
        body,
    });
    let _ = events_tx.send(Ok(event)).await;
}

fn build_request(
    options: &UploadOptions,
    total_bytes: Option<u64>,
    body: BoxBody<Bytes, Error>,
) -> Result<HttpRequest<BoxBody<Bytes, Error>>, String> {
    let mut builder = HttpRequest::builder()
        .method(options.method.clone())
        .uri(&options.url)
        .header(header::ACCEPT, "*/*");
    if let Some(headers) = builder.headers_mut() {
        let user_agent = crate::get_user_agent();
        let user_agent = HeaderValue::from_str(&user_agent)
            .map_err(|e| format!("invalid user agent header: {}", e))?;
        headers.insert(header::USER_AGENT, user_agent);

        if let Some(content_type) = &options.content_type {
            let content_type = HeaderValue::from_str(content_type)
                .map_err(|e| format!("invalid content-type header: {}", e))?;
            headers.insert(header::CONTENT_TYPE, content_type);
        }

        if let Some(total) = total_bytes {
            let content_len = HeaderValue::from_str(&total.to_string())
                .map_err(|e| format!("invalid content-length header: {}", e))?;
            headers.insert(header::CONTENT_LENGTH, content_len);
        }

        for (name, value) in &options.headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| format!("invalid upload header name '{}': {}", name, e))?;
            let value = HeaderValue::from_str(value)
                .map_err(|e| format!("invalid upload header '{}' value: {}", name, e))?;
            headers.insert(name, value);
        }
    }

    builder
        .body(body)
        .map_err(|e| format!("build upload request: {}", e))
}

async fn collect_response_body(body: HttpBody) -> Result<Bytes, String> {
    match body {
        HttpBody::Empty => Ok(Bytes::new()),
        HttpBody::Small(bytes) => Ok(bytes),
        HttpBody::Stream(mut rx) => {
            let mut out = Vec::new();
            while let Some(chunk) = rx.recv().await {
                let bytes = chunk?;
                out.extend_from_slice(&bytes);
            }
            Ok(Bytes::from(out))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio_stream::StreamExt;

    // Pool starts lazily on first spawn/handle; nothing to do here.

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

    async fn spawn_upload_server() -> std::net::SocketAddr {
        use axum::Router;
        use axum::body::Bytes as AxumBytes;
        use axum::http::HeaderMap;
        use axum::routing::any;

        let app = Router::new().route(
            "/upload",
            any(
                |method: Method, headers: HeaderMap, body: AxumBytes| async move {
                    let len = body.len();
                    let tag = headers
                        .get("x-upload-tag")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    (
                        StatusCode::OK,
                        format!("method={},uploaded={},tag={}", method, len, tag),
                    )
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    #[test]
    fn spawn_upload_reports_progress_and_success() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_upload_server().await;
            let path = std::env::temp_dir().join(format!(
                "rong_upload_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let payload = vec![7u8; 100 * 1024 + 77];
            tokio::fs::write(&path, &payload).await.unwrap();

            let options = UploadOptions::new(format!("http://{}/upload", addr), &path)
                .with_content_type("application/octet-stream")
                .with_header("x-upload-tag", "spawn")
                .with_connect_timeout(Duration::from_secs(1));
            let task = spawn_upload(options, None).expect("upload task should start");
            let mut stream = task.into_stream();

            let mut saw_progress = false;
            let mut success = None;
            while let Some(item) = stream.next().await {
                match item.expect("upload event should be ok") {
                    UploadEvent::Progress { uploaded_bytes, .. } => {
                        saw_progress = true;
                        assert!(uploaded_bytes > 0);
                    }
                    UploadEvent::Success(resp) => {
                        success = Some(resp);
                        break;
                    }
                }
            }

            assert!(saw_progress, "expected at least one progress event");
            let resp = success.expect("success event expected");
            assert_eq!(resp.status, StatusCode::OK);
            assert_eq!(
                resp.body,
                Bytes::from(format!("method=PUT,uploaded={},tag=spawn", payload.len()))
            );
            let _ = tokio::fs::remove_file(&path).await;
        });
    }

    #[test]
    fn upload_convenience_returns_response() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_upload_server().await;
            let path = std::env::temp_dir().join(format!(
                "rong_upload_direct_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let payload = vec![3u8; 4096];
            tokio::fs::write(&path, &payload).await.unwrap();

            let response = upload(
                UploadOptions::new(format!("http://{}/upload", addr), &path)
                    .with_method(Method::POST)
                    .with_header("x-upload-tag", "direct")
                    .with_content_type("application/octet-stream")
                    .with_request_timeout(Duration::from_secs(5))
                    .with_connect_timeout(Duration::from_secs(1)),
                None,
            )
            .await
            .expect("upload should succeed");

            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(
                response.body,
                Bytes::from(format!("method=POST,uploaded={},tag=direct", payload.len()))
            );
            let _ = tokio::fs::remove_file(&path).await;
        });
    }

    #[test]
    fn scoped_network_access_guard_blocks_spawn_upload() {
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let path = std::env::temp_dir().join(format!(
                "rong_upload_denied_test_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            tokio::fs::write(&path, b"denied").await.unwrap();

            let err = crate::http::scope_network_access_guard(Arc::new(DenyExampleGuard), async {
                upload(
                    UploadOptions::new("http://denied.example.com/upload", &path),
                    None,
                )
                .await
                .expect_err("upload should be denied")
            })
            .await;

            assert_eq!(err, "upload request failed: network access denied");
            let _ = tokio::fs::remove_file(&path).await;
        });
    }
}
