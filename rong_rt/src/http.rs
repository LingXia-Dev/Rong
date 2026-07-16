use bytes::Bytes;
use http::Request as HttpRequest;
use http::header;
use http::{HeaderMap, StatusCode};
use http::{HeaderValue, Method, header::HeaderName};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt;
use std::future::Future;
use std::io::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::client::{self, RequestTimeouts};

pub use crate::client::{HttpBody, HttpResponse};
pub use http::Uri;

tokio::task_local! {
    static NETWORK_ACCESS_GUARD: Arc<dyn NetworkAccessGuard>;
}

/// Transport and response-decoding failures surfaced by the high-level HTTP API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpErrorKind {
    Transport,
    Json,
    AccessDenied,
}

/// Error returned by the public HTTP helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError {
    kind: HttpErrorKind,
    message: String,
}

impl HttpError {
    pub fn kind(&self) -> HttpErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn json(err: impl fmt::Display) -> Self {
        Self {
            kind: HttpErrorKind::Json,
            message: err.to_string(),
        }
    }

    pub fn access_denied(message: impl Into<String>) -> Self {
        Self {
            kind: HttpErrorKind::AccessDenied,
            message: message.into(),
        }
    }
}

impl From<String> for HttpError {
    fn from(message: String) -> Self {
        Self {
            kind: HttpErrorKind::Transport,
            message,
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for HttpError {}

/// Per-request network access policy.
///
/// The runtime owns only the hook point. Embedders define the actual policy,
/// such as an app-specific domain allowlist.
pub trait NetworkAccessGuard: Send + Sync {
    fn check_access(&self, uri: &Uri) -> Result<(), HttpError>;
}

pub async fn scope_network_access_guard<F, T>(guard: Arc<dyn NetworkAccessGuard>, future: F) -> T
where
    F: Future<Output = T>,
{
    NETWORK_ACCESS_GUARD.scope(guard, future).await
}

pub(crate) fn current_network_access_guard() -> Option<Arc<dyn NetworkAccessGuard>> {
    NETWORK_ACCESS_GUARD.try_with(Clone::clone).ok()
}

pub(crate) async fn scope_network_access_guard_opt<F, T>(
    guard: Option<Arc<dyn NetworkAccessGuard>>,
    future: F,
) -> T
where
    F: Future<Output = T>,
{
    if let Some(guard) = guard {
        scope_network_access_guard(guard, future).await
    } else {
        future.await
    }
}

/// Per-request behavior overrides for the high-level HTTP helpers.
#[derive(Default)]
pub struct RequestOptions {
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    abort_rx: Option<oneshot::Receiver<()>>,
    network_access_guard: Option<Arc<dyn NetworkAccessGuard>>,
}

impl fmt::Debug for RequestOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestOptions")
            .field("request_timeout", &self.request_timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("has_abort", &self.abort_rx.is_some())
            .field(
                "has_network_access_guard",
                &self.network_access_guard.is_some(),
            )
            .finish()
    }
}

impl RequestOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Override only the socket-connect phase timeout for this request.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    pub fn with_abort(mut self, abort_rx: oneshot::Receiver<()>) -> Self {
        self.abort_rx = Some(abort_rx);
        self
    }

    pub fn with_network_access_guard(mut self, guard: Arc<dyn NetworkAccessGuard>) -> Self {
        self.network_access_guard = Some(guard);
        self
    }

    #[doc(hidden)]
    pub fn with_abort_opt(mut self, abort_rx: Option<oneshot::Receiver<()>>) -> Self {
        self.abort_rx = abort_rx;
        self
    }

    fn into_parts(
        self,
    ) -> (
        RequestTimeouts,
        Option<oneshot::Receiver<()>>,
        Option<Arc<dyn NetworkAccessGuard>>,
    ) {
        (
            RequestTimeouts {
                request_timeout: self.request_timeout,
                connect_timeout: self.connect_timeout,
            },
            self.abort_rx,
            self.network_access_guard,
        )
    }
}

fn check_network_access(
    request: &HttpRequest<BoxBody<Bytes, Error>>,
    guard: Option<&dyn NetworkAccessGuard>,
) -> Result<(), HttpError> {
    if let Some(guard) = guard {
        guard.check_access(request.uri())?;
    }
    Ok(())
}

pub(crate) fn check_current_network_access(
    request: &HttpRequest<BoxBody<Bytes, Error>>,
) -> Result<(), HttpError> {
    let guard = current_network_access_guard();
    check_network_access(request, guard.as_deref())
}

/// Fully collected HTTP response body.
#[derive(Debug)]
pub struct BytesResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

/// JSON-decoded HTTP response body.
#[derive(Debug)]
pub struct JsonResponse<T> {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: T,
}

/// Configure a process-wide HTTP proxy for subsequent requests.
pub fn set_proxy(proxy_url: &str) -> Result<(), HttpError> {
    client::set_proxy(proxy_url).map_err(Into::into)
}

/// Clear the process-wide HTTP proxy override.
pub fn clear_proxy() {
    client::clear_proxy();
}

/// Read the current process-wide HTTP proxy URL, if any.
pub fn proxy() -> Option<String> {
    client::get_proxy()
}

/// Send a request and let the runtime decide whether to buffer or stream the body.
pub async fn send(
    request: HttpRequest<BoxBody<Bytes, Error>>,
    options: RequestOptions,
) -> Result<HttpResponse, HttpError> {
    let (timeouts, abort_rx, guard) = options.into_parts();
    let guard = guard.or_else(current_network_access_guard);
    check_network_access(&request, guard.as_deref())?;
    client::send_request_with_timeout(
        request,
        client::DEFAULT_BLOCKING_BODY_LIMIT,
        abort_rx,
        timeouts,
    )
    .await
    .map_err(Into::into)
}

#[doc(hidden)]
pub async fn send_with_small_body_limit(
    request: HttpRequest<BoxBody<Bytes, Error>>,
    small_body_limit: usize,
    options: RequestOptions,
) -> Result<HttpResponse, HttpError> {
    let (timeouts, abort_rx, guard) = options.into_parts();
    let guard = guard.or_else(current_network_access_guard);
    check_network_access(&request, guard.as_deref())?;
    client::send_request_with_timeout(request, small_body_limit, abort_rx, timeouts)
        .await
        .map_err(Into::into)
}

/// Send a request while forcing response delivery through the streaming body path.
pub async fn send_stream(
    request: HttpRequest<BoxBody<Bytes, Error>>,
    options: RequestOptions,
) -> Result<HttpResponse, HttpError> {
    let (timeouts, abort_rx, guard) = options.into_parts();
    let guard = guard.or_else(current_network_access_guard);
    check_network_access(&request, guard.as_deref())?;
    client::send_request_with_coalesce(request, 0, abort_rx, 0, timeouts)
        .await
        .map_err(Into::into)
}

/// Collect an `HttpBody` into a single `Bytes` buffer.
pub async fn collect_body(body: HttpBody) -> Result<Bytes, HttpError> {
    match body {
        HttpBody::Empty => Ok(Bytes::new()),
        HttpBody::Small(bytes) => Ok(bytes),
        HttpBody::Stream(mut rx) => {
            let mut out = Vec::new();
            while let Some(chunk) = rx.recv().await {
                let chunk = chunk.map_err(HttpError::from)?;
                out.extend_from_slice(&chunk);
            }
            Ok(Bytes::from(out))
        }
    }
}

/// Send a request and always return a fully collected body.
pub async fn send_bytes(
    request: HttpRequest<BoxBody<Bytes, Error>>,
    options: RequestOptions,
) -> Result<BytesResponse, HttpError> {
    let response = send(request, options).await?;
    let body = collect_body(response.body).await?;
    Ok(BytesResponse {
        status: response.status,
        headers: response.headers,
        body,
    })
}

/// Send a request, fully collect the body, and decode it as JSON.
pub async fn send_json<T>(
    request: HttpRequest<BoxBody<Bytes, Error>>,
    options: RequestOptions,
) -> Result<JsonResponse<T>, HttpError>
where
    T: DeserializeOwned,
{
    let response = send_bytes(request, options).await?;
    let body = serde_json::from_slice::<T>(&response.body).map_err(HttpError::json)?;
    Ok(JsonResponse {
        status: response.status,
        headers: response.headers,
        body,
    })
}

fn build_json_request(
    method: Method,
    url: &str,
    body: &[u8],
    extra_headers: Option<&[(&str, &str)]>,
) -> Result<HttpRequest<BoxBody<Bytes, Error>>, HttpError> {
    let mut builder = HttpRequest::builder()
        .method(method)
        .uri(url)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json");

    if let Some(headers) = builder.headers_mut() {
        let user_agent = crate::get_user_agent();
        let user_agent = HeaderValue::from_str(&user_agent)
            .map_err(|e| HttpError::from(format!("invalid user agent header: {}", e)))?;
        headers.insert(header::USER_AGENT, user_agent);

        if let Some(extra_headers) = extra_headers {
            for (name, value) in extra_headers {
                let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| {
                    HttpError::from(format!("invalid header name '{}': {}", name, e))
                })?;
                let header_value = HeaderValue::from_str(value).map_err(|e| {
                    HttpError::from(format!("invalid header '{}' value: {}", name, e))
                })?;
                headers.insert(header_name, header_value);
            }
        }
    }

    builder
        .body(
            Full::new(Bytes::copy_from_slice(body))
                .map_err(|_| Error::other("body error"))
                .boxed(),
        )
        .map_err(|e| HttpError::from(format!("build request: {}", e)))
}

/// Send a JSON request with a pre-serialized body and fully collect the response.
pub async fn send_json_bytes(
    method: Method,
    url: &str,
    body: &[u8],
    extra_headers: Option<&[(&str, &str)]>,
    options: RequestOptions,
) -> Result<BytesResponse, HttpError> {
    let request = build_json_request(method, url, body, extra_headers)?;
    send_bytes(request, options).await
}

/// Send a POST JSON request with a pre-serialized body and fully collect the response.
pub async fn post_json_bytes(
    url: &str,
    body: &[u8],
    extra_headers: Option<&[(&str, &str)]>,
    options: RequestOptions,
) -> Result<BytesResponse, HttpError> {
    send_json_bytes(Method::POST, url, body, extra_headers, options).await
}

/// Send a JSON request using a serializable body and decode the response as JSON.
pub async fn send_json_request<TReq, TResp>(
    method: Method,
    url: &str,
    body: &TReq,
    extra_headers: Option<&[(&str, &str)]>,
    options: RequestOptions,
) -> Result<JsonResponse<TResp>, HttpError>
where
    TReq: Serialize + ?Sized,
    TResp: DeserializeOwned,
{
    let body = serde_json::to_vec(body).map_err(HttpError::json)?;
    let request = build_json_request(method, url, &body, extra_headers)?;
    send_json(request, options).await
}

/// Send a POST JSON request using a serializable body and decode the response as JSON.
pub async fn post_json<TReq, TResp>(
    url: &str,
    body: &TReq,
    extra_headers: Option<&[(&str, &str)]>,
    options: RequestOptions,
) -> Result<JsonResponse<TResp>, HttpError>
where
    TReq: Serialize + ?Sized,
    TResp: DeserializeOwned,
{
    send_json_request(Method::POST, url, body, extra_headers, options).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header;
    use http_body_util::{BodyExt, Full};

    async fn spawn_server() -> std::net::SocketAddr {
        use axum::Router;
        use axum::body::Body;
        use axum::http::HeaderMap as AxumHeaderMap;
        use axum::http::Method as AxumMethod;
        use axum::routing::get;
        use axum::routing::post;
        use std::convert::Infallible;
        use tokio_stream as stream;

        async fn bytes() -> impl axum::response::IntoResponse {
            (
                [(header::CONTENT_TYPE, "text/plain")],
                axum::body::Body::from("hello"),
            )
        }

        async fn json() -> impl axum::response::IntoResponse {
            (
                [(header::CONTENT_TYPE, "application/json")],
                axum::body::Body::from(r#"{"ok":true,"value":7}"#),
            )
        }

        async fn stream_body() -> impl axum::response::IntoResponse {
            let chunks = stream::iter(vec![
                Ok::<_, Infallible>("he".to_string()),
                Ok::<_, Infallible>("llo".to_string()),
            ]);
            (
                [(header::CONTENT_TYPE, "text/plain")],
                Body::from_stream(chunks),
            )
        }

        async fn delayed_stream_body() -> impl axum::response::IntoResponse {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let _ = tx.send(Ok::<_, Infallible>("late".to_string())).await;
            });
            (
                [(header::CONTENT_TYPE, "text/plain")],
                Body::from_stream(tokio_stream::wrappers::ReceiverStream::new(rx)),
            )
        }

        async fn echo_json(
            method: AxumMethod,
            headers: AxumHeaderMap,
            body: axum::body::Bytes,
        ) -> impl axum::response::IntoResponse {
            let tag = headers
                .get("x-test")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            let content_type = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            let accept = headers
                .get(header::ACCEPT)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            (
                [(header::CONTENT_TYPE, "application/json")],
                format!(
                    r#"{{"method":"{}","tag":"{}","content_type":"{}","accept":"{}","body":{}}}"#,
                    method,
                    tag,
                    content_type,
                    accept,
                    String::from_utf8_lossy(&body)
                ),
            )
        }

        let app = Router::new()
            .route("/bytes", get(bytes))
            .route("/json", get(json))
            .route("/stream", get(stream_body))
            .route("/delayed-stream", get(delayed_stream_body))
            .route("/echo-json", post(echo_json));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    fn empty_request(url: &str) -> HttpRequest<BoxBody<Bytes, Error>> {
        HttpRequest::builder()
            .method("GET")
            .uri(url)
            .body(
                Full::new(Bytes::new())
                    .map_err(|_| Error::other("body error"))
                    .boxed(),
            )
            .unwrap()
    }

    #[derive(Debug, serde::Deserialize)]
    struct TestJson {
        ok: bool,
        value: u32,
    }

    #[derive(Debug, serde::Deserialize)]
    struct EchoJsonResponse {
        method: String,
        tag: String,
        content_type: String,
        accept: String,
        body: serde_json::Value,
    }

    #[derive(Debug, serde::Serialize)]
    struct TestRequest<'a> {
        hello: &'a str,
    }

    #[test]
    fn send_bytes_collects_body() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_server().await;
            let url = format!("http://{}/bytes", addr);
            let response = send_bytes(empty_request(&url), RequestOptions::new())
                .await
                .expect("bytes response");
            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(response.body.as_ref(), b"hello");
        });
    }

    #[test]
    fn send_json_parses_body() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_server().await;
            let url = format!("http://{}/json", addr);
            let response = send_json::<TestJson>(empty_request(&url), RequestOptions::new())
                .await
                .expect("json response");
            assert_eq!(response.status, StatusCode::OK);
            assert!(response.body.ok);
            assert_eq!(response.body.value, 7);
        });
    }

    #[test]
    fn send_stream_collects_streaming_body() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_server().await;
            let url = format!("http://{}/stream", addr);
            let response = send_stream(empty_request(&url), RequestOptions::new())
                .await
                .expect("stream response");
            let body = collect_body(response.body).await.expect("stream body");
            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(body.as_ref(), b"hello");
        });
    }

    #[test]
    fn request_timeout_includes_response_body() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_server().await;
            let url = format!("http://{}/delayed-stream", addr);
            let err = send_bytes(
                empty_request(&url),
                RequestOptions::new().with_request_timeout(Duration::from_millis(100)),
            )
            .await
            .expect_err("the whole response must observe request_timeout");
            assert_eq!(err.kind(), HttpErrorKind::Transport);
            assert!(err.message().contains("request timeout"));
        });
    }

    #[test]
    fn post_json_bytes_sends_json_headers_and_body() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_server().await;
            let url = format!("http://{}/echo-json", addr);
            let response = post_json_bytes(
                &url,
                br#"{"hello":"world"}"#,
                Some(&[("x-test", "bytes")]),
                RequestOptions::new(),
            )
            .await
            .expect("json bytes response");
            let body: EchoJsonResponse = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(body.method, "POST");
            assert_eq!(body.tag, "bytes");
            assert_eq!(body.content_type, "application/json");
            assert_eq!(body.accept, "application/json");
            assert_eq!(body.body["hello"], "world");
        });
    }

    #[test]
    fn post_json_serializes_request_and_decodes_response() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_server().await;
            let url = format!("http://{}/echo-json", addr);
            let response = post_json::<_, EchoJsonResponse>(
                &url,
                &TestRequest { hello: "typed" },
                Some(&[("x-test", "typed")]),
                RequestOptions::new(),
            )
            .await
            .expect("typed json response");
            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(response.body.method, "POST");
            assert_eq!(response.body.tag, "typed");
            assert_eq!(response.body.content_type, "application/json");
            assert_eq!(response.body.accept, "application/json");
            assert_eq!(response.body.body["hello"], "typed");
        });
    }

    #[derive(Debug)]
    struct DenyExampleGuard;

    impl NetworkAccessGuard for DenyExampleGuard {
        fn check_access(&self, uri: &Uri) -> Result<(), HttpError> {
            if uri.host() == Some("denied.example.com") {
                return Err(HttpError::access_denied("network access denied"));
            }
            Ok(())
        }
    }

    #[test]
    fn network_access_guard_blocks_request_before_transport() {
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let request = empty_request("http://denied.example.com/");
            let err = send_bytes(
                request,
                RequestOptions::new().with_network_access_guard(Arc::new(DenyExampleGuard)),
            )
            .await
            .unwrap_err();

            assert_eq!(err.kind(), HttpErrorKind::AccessDenied);
            assert_eq!(err.message(), "network access denied");
        });
    }

    #[test]
    fn network_access_guard_scope_applies_to_default_request_options() {
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let request = empty_request("http://denied.example.com/");
            let err = scope_network_access_guard(Arc::new(DenyExampleGuard), async move {
                send_bytes(request, RequestOptions::new())
                    .await
                    .unwrap_err()
            })
            .await;

            assert_eq!(err.kind(), HttpErrorKind::AccessDenied);
        });
    }
}
