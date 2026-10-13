use bytes::Bytes;
use http::Request as HttpRequest;
use http::header;
use http::{HeaderValue, header::HeaderName};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use std::cmp;
use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::client::{self, HttpBody, RequestTimeouts};
use crate::observer::{HopId, NetworkRequestKind, OwnerCapture, tag_request};

// SSE is latency-sensitive; forward frames immediately instead of waiting for body coalescing.
const SSE_STREAM_COALESCE_TARGET: usize = 0;
const SSE_EVENT_CHAN_CAP: usize = 128;

type SseConnectionPartsWithOpen = (
    mpsc::Receiver<Result<SseEvent, String>>,
    Option<oneshot::Sender<()>>,
    oneshot::Receiver<Result<String, String>>,
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseScheme {
    Http,
    Https,
}

impl SseScheme {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// Parsed destination for an SSE connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseDestination {
    pub scheme: SseScheme,
    pub target: String,
    pub path: String,
    pub query: Option<String>,
}

impl SseDestination {
    pub fn to_url(&self) -> Result<String, String> {
        let target = self.target.trim();
        if target.is_empty() {
            return Err("destination.target cannot be empty".to_string());
        }

        let mut path = self.path.trim().to_string();
        if path.is_empty() {
            path = "/".to_string();
        } else if !path.starts_with('/') {
            path = format!("/{}", path);
        }

        let mut url = format!("{}://{}{}", self.scheme.as_str(), target, path);
        if let Some(query) = &self.query {
            let q = query.trim_start_matches('?');
            if !q.is_empty() {
                url.push('?');
                url.push_str(q);
            }
        }
        Ok(url)
    }
}

/// Reconnect behavior for SSE connections.
#[derive(Clone, Debug)]
pub struct SseReconnectOptions {
    pub enabled: bool,
    pub max_retries: Option<u32>,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for SseReconnectOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: None,
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_millis(30_000),
        }
    }
}

/// Connection options for a new SSE session.
#[derive(Clone, Debug)]
pub struct SseConnectOptions {
    destination: SseDestination,
    headers: Vec<(String, String)>,
    last_event_id: Option<String>,
    reconnect: SseReconnectOptions,
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
}

impl SseConnectOptions {
    /// Build SSE options from a full `http://` or `https://` URL.
    pub fn new(url: impl AsRef<str>) -> Result<Self, String> {
        let destination = parse_destination(url.as_ref())?;
        Ok(Self {
            destination,
            headers: Vec::new(),
            last_event_id: None,
            reconnect: SseReconnectOptions::default(),
            request_timeout: None,
            connect_timeout: None,
        })
    }

    /// Add a request header sent during the SSE handshake.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Set the initial `Last-Event-ID` header.
    pub fn with_last_event_id(mut self, last_event_id: impl Into<String>) -> Self {
        self.last_event_id = Some(last_event_id.into());
        self
    }

    /// Override the reconnect policy for this connection.
    pub fn with_reconnect(mut self, reconnect: SseReconnectOptions) -> Self {
        self.reconnect = reconnect;
        self
    }

    /// Override the request timeout used for the SSE handshake and stream.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Override the socket-connect timeout used for the SSE handshake.
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

/// One parsed SSE event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
    pub id: Option<String>,
    pub retry_ms: Option<u64>,
    pub origin: String,
}

/// Active SSE connection with an event receiver and lifecycle controls.
pub struct SseConnection {
    pub events: mpsc::Receiver<Result<SseEvent, String>>,
    close_tx: Option<oneshot::Sender<()>>,
    opened_rx: Option<oneshot::Receiver<Result<String, String>>>,
}

impl SseConnection {
    /// Wait for the initial handshake result and return the connection origin.
    pub async fn opened(&mut self) -> Result<String, String> {
        match self.opened_rx.take() {
            Some(rx) => rx
                .await
                .map_err(|_| "sse open channel dropped".to_string())?,
            None => Err("sse open state already consumed".to_string()),
        }
    }

    pub fn close(&mut self) {
        if let Some(tx) = self.close_tx.take() {
            let _ = tx.send(());
        }
    }

    pub fn into_parts(
        mut self,
    ) -> (
        mpsc::Receiver<Result<SseEvent, String>>,
        Option<oneshot::Sender<()>>,
    ) {
        let (_dummy_tx, dummy_rx) = mpsc::channel(1);
        let events = std::mem::replace(&mut self.events, dummy_rx);
        (events, self.close_tx.take())
    }

    pub fn into_parts_with_open(mut self) -> SseConnectionPartsWithOpen {
        let (_dummy_tx, dummy_rx) = mpsc::channel(1);
        let events = std::mem::replace(&mut self.events, dummy_rx);
        let (_dummy_open_tx, dummy_open_rx) = oneshot::channel();
        let opened_rx = self.opened_rx.take().unwrap_or(dummy_open_rx);
        (events, self.close_tx.take(), opened_rx)
    }

    pub fn into_event_stream(self) -> SseEventStream {
        let (events, close_tx, _opened_rx) = self.into_parts_with_open();
        SseEventStream {
            inner: ReceiverStream::new(events),
            close_tx,
        }
    }

    pub fn into_stream(self) -> SseEventStream {
        self.into_event_stream()
    }
}

impl Drop for SseConnection {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct SseEventStream {
    inner: ReceiverStream<Result<SseEvent, String>>,
    close_tx: Option<oneshot::Sender<()>>,
}

impl SseEventStream {
    pub fn close(&mut self) {
        if let Some(tx) = self.close_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for SseEventStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl tokio_stream::Stream for SseEventStream {
    type Item = Result<SseEvent, String>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

#[derive(Default)]
struct PendingEvent {
    event: Option<String>,
    data_lines: Vec<String>,
    id: Option<String>,
    retry_ms: Option<u64>,
}

/// Establish a new SSE connection from explicit options.
pub fn connect_sse(
    options: SseConnectOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<SseConnection, String> {
    let (events_tx, events_rx) = mpsc::channel::<Result<SseEvent, String>>(SSE_EVENT_CHAN_CAP);
    let (close_tx, close_rx) = oneshot::channel::<()>();
    let (opened_tx, opened_rx) = oneshot::channel::<Result<String, String>>();
    let network_access_guard = crate::http::current_network_access_guard();
    let owner = OwnerCapture::capture();

    crate::RongExecutor::global().spawn(async move {
        owner
            .scope(crate::http::scope_network_access_guard_opt(
                network_access_guard,
                run_sse_worker(options, abort_rx, close_rx, events_tx, Some(opened_tx)),
            ))
            .await;
    });

    Ok(SseConnection {
        events: events_rx,
        close_tx: Some(close_tx),
        opened_rx: Some(opened_rx),
    })
}

/// Establish a new SSE connection from a URL string.
pub fn connect(
    url: impl AsRef<str>,
    abort_rx: Option<oneshot::Receiver<()>>,
) -> Result<SseConnection, String> {
    connect_sse(SseConnectOptions::new(url)?, abort_rx)
}

async fn run_sse_worker(
    options: SseConnectOptions,
    abort_rx: Option<oneshot::Receiver<()>>,
    close_rx: oneshot::Receiver<()>,
    events_tx: mpsc::Sender<Result<SseEvent, String>>,
    mut opened_tx: Option<oneshot::Sender<Result<String, String>>>,
) {
    let (stop_tx, mut stop_rx) = mpsc::unbounded_channel::<()>();

    let stop_tx_close = stop_tx.clone();
    tokio::task::spawn(async move {
        let _ = close_rx.await;
        let _ = stop_tx_close.send(());
    });

    if let Some(abort_rx) = abort_rx {
        let stop_tx_abort = stop_tx.clone();
        tokio::task::spawn(async move {
            let _ = abort_rx.await;
            let _ = stop_tx_abort.send(());
        });
    }

    let mut retries: u32 = 0;
    let mut last_event_id = options.last_event_id.clone();
    let mut reconnect_delay_ms = cmp::max(1, options.reconnect.base_delay.as_millis() as u64);
    let max_delay_ms = cmp::max(
        reconnect_delay_ms,
        options.reconnect.max_delay.as_millis() as u64,
    );

    loop {
        let mut req = match build_sse_request(&options, last_event_id.as_deref()) {
            Ok(v) => v,
            Err(e) => {
                complete_initial_open(&mut opened_tx, Err(e.clone()));
                let _ = events_tx.send(Err(e)).await;
                break;
            }
        };
        tag_request(&mut req, NetworkRequestKind::Sse, HopId::FIRST);
        if let Err(err) = crate::http::check_current_network_access(&req) {
            let message = format!("sse request failed: {}", err);
            complete_initial_open(&mut opened_tx, Err(message.clone()));
            let _ = events_tx.send(Err(message)).await;
            break;
        }

        let (attempt_abort_tx, attempt_abort_rx) = oneshot::channel::<()>();
        let send_fut = client::send_request_with_coalesce(
            req,
            0,
            Some(attempt_abort_rx),
            SSE_STREAM_COALESCE_TARGET,
            options.timeouts(),
        );
        tokio::pin!(send_fut);

        let response = tokio::select! {
            _ = stop_rx.recv() => {
                let _ = attempt_abort_tx.send(());
                break;
            }
            res = &mut send_fut => match res {
                Ok(resp) => resp,
                Err(e) => {
                    if !should_reconnect(&options.reconnect, retries) {
                        let message = format!("sse request failed: {}", e);
                        complete_initial_open(&mut opened_tx, Err(message.clone()));
                        let _ = events_tx.send(Err(message)).await;
                        break;
                    }
                    retries = retries.saturating_add(1);
                    let delay_ms = reconnect_delay_ms.min(max_delay_ms);
                    reconnect_delay_ms = cmp::min(delay_ms.saturating_mul(2), max_delay_ms);
                    if wait_reconnect_delay(&mut stop_rx, delay_ms).await {
                        break;
                    }
                    continue;
                }
            },
        };

        if response.status != http::StatusCode::OK {
            let message = format!("sse server returned status {}", response.status);
            complete_initial_open(&mut opened_tx, Err(message.clone()));
            let _ = events_tx.send(Err(message)).await;
            break;
        }

        let content_type = response
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.starts_with("text/event-stream") {
            let message = format!(
                "invalid sse content-type: {}",
                if content_type.is_empty() {
                    "<empty>"
                } else {
                    &content_type
                }
            );
            complete_initial_open(&mut opened_tx, Err(message.clone()));
            let _ = events_tx.send(Err(message)).await;
            break;
        }

        complete_initial_open(
            &mut opened_tx,
            Ok(format!(
                "{}://{}",
                options.destination.scheme.as_str(),
                options.destination.target
            )),
        );

        let mut body_rx = match response.body {
            HttpBody::Stream(rx) => rx,
            HttpBody::Small(bytes) => {
                let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(1);
                let _ = tx.send(Ok(bytes)).await;
                rx
            }
            HttpBody::Empty => {
                if !should_reconnect(&options.reconnect, retries) {
                    break;
                }
                retries = retries.saturating_add(1);
                let delay_ms = reconnect_delay_ms.min(max_delay_ms);
                reconnect_delay_ms = cmp::min(delay_ms.saturating_mul(2), max_delay_ms);
                if wait_reconnect_delay(&mut stop_rx, delay_ms).await {
                    break;
                }
                continue;
            }
        };

        retries = 0;
        reconnect_delay_ms = cmp::max(1, options.reconnect.base_delay.as_millis() as u64);
        let mut line_buf: Vec<u8> = Vec::new();
        let mut pending = PendingEvent::default();
        let mut stream_error: Option<String> = None;

        loop {
            tokio::select! {
                _ = stop_rx.recv() => {
                    let _ = attempt_abort_tx.send(());
                    return;
                }
                next = body_rx.recv() => {
                    match next {
                        Some(Ok(chunk)) => {
                            line_buf.extend_from_slice(&chunk);
                            if let Err(e) = parse_available_events(
                                &mut line_buf,
                                &mut pending,
                                &events_tx,
                                &mut last_event_id,
                                &mut reconnect_delay_ms,
                                max_delay_ms,
                                &options.destination,
                            ).await {
                                stream_error = Some(e);
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            stream_error = Some(e);
                            break;
                        }
                        None => {
                            break;
                        }
                    }
                }
            }
        }

        if !line_buf.is_empty() {
            // Treat trailing bytes without newline as one final line.
            line_buf.push(b'\n');
            let _ = parse_available_events(
                &mut line_buf,
                &mut pending,
                &events_tx,
                &mut last_event_id,
                &mut reconnect_delay_ms,
                max_delay_ms,
                &options.destination,
            )
            .await;
        }
        let _ = flush_pending_event(
            &mut pending,
            &events_tx,
            &mut last_event_id,
            &mut reconnect_delay_ms,
            max_delay_ms,
            &options.destination,
        )
        .await;

        if let Some(err) = stream_error
            && !should_reconnect(&options.reconnect, retries)
        {
            let _ = events_tx
                .send(Err(format!("sse stream failed: {}", err)))
                .await;
            break;
        }

        if !options.reconnect.enabled {
            break;
        }
        if let Some(max_retries) = options.reconnect.max_retries
            && retries >= max_retries
        {
            break;
        }
        retries = retries.saturating_add(1);

        let delay_ms = reconnect_delay_ms.min(max_delay_ms);
        reconnect_delay_ms = cmp::min(delay_ms.saturating_mul(2), max_delay_ms);
        if wait_reconnect_delay(&mut stop_rx, delay_ms).await {
            break;
        }
    }
}

fn should_reconnect(reconnect: &SseReconnectOptions, retries: u32) -> bool {
    if !reconnect.enabled {
        return false;
    }
    if let Some(max_retries) = reconnect.max_retries {
        retries < max_retries
    } else {
        true
    }
}

async fn wait_reconnect_delay(stop_rx: &mut mpsc::UnboundedReceiver<()>, delay_ms: u64) -> bool {
    let delay = tokio::time::sleep(Duration::from_millis(delay_ms.max(1)));
    tokio::pin!(delay);
    tokio::select! {
        _ = stop_rx.recv() => true,
        _ = &mut delay => false,
    }
}

fn build_sse_request(
    options: &SseConnectOptions,
    last_event_id: Option<&str>,
) -> Result<HttpRequest<BoxBody<Bytes, Error>>, String> {
    let uri = options.destination.to_url()?;
    let mut builder = HttpRequest::builder()
        .method("GET")
        .uri(&uri)
        .header(header::ACCEPT, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::USER_AGENT, crate::get_user_agent());

    if let Some(last_id) = last_event_id
        && !last_id.is_empty()
    {
        builder = builder.header("Last-Event-ID", last_id);
    }

    if let Some(headers) = builder.headers_mut() {
        for (name, value) in &options.headers {
            let key = HeaderName::from_bytes(name.as_bytes())
                .map_err(|e| format!("invalid sse header name '{}': {}", name, e))?;
            let val = HeaderValue::from_str(value)
                .map_err(|e| format!("invalid sse header '{}' value: {}", name, e))?;
            headers.insert(key, val);
        }
    }

    let body: BoxBody<Bytes, Error> = Full::new(Bytes::new()).map_err(|e| match e {}).boxed();
    builder
        .body(body)
        .map_err(|e| format!("failed to build sse request: {}", e))
}

fn parse_destination(url: &str) -> Result<SseDestination, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid sse url: {}", e))?;

    let scheme = match parsed.scheme() {
        "http" => SseScheme::Http,
        "https" => SseScheme::Https,
        other => return Err(format!("unsupported sse url scheme: {}", other)),
    };

    let host = parsed
        .host_str()
        .ok_or_else(|| "sse url must contain a host".to_string())?;
    let target = match parsed.port() {
        Some(port) => format!("{}:{}", host, port),
        None => host.to_string(),
    };

    Ok(SseDestination {
        scheme,
        target,
        path: parsed.path().to_string(),
        query: parsed.query().map(|q| q.to_string()),
    })
}

async fn parse_available_events(
    line_buf: &mut Vec<u8>,
    pending: &mut PendingEvent,
    events_tx: &mpsc::Sender<Result<SseEvent, String>>,
    last_event_id: &mut Option<String>,
    reconnect_delay_ms: &mut u64,
    max_delay_ms: u64,
    destination: &SseDestination,
) -> Result<(), String> {
    while let Some(pos) = line_buf.iter().position(|b| *b == b'\n') {
        let mut raw_line: Vec<u8> = line_buf.drain(..=pos).collect();
        if raw_line.last() == Some(&b'\n') {
            raw_line.pop();
        }
        if raw_line.last() == Some(&b'\r') {
            raw_line.pop();
        }

        let line = String::from_utf8_lossy(&raw_line).into_owned();
        if line.is_empty() {
            flush_pending_event(
                pending,
                events_tx,
                last_event_id,
                reconnect_delay_ms,
                max_delay_ms,
                destination,
            )
            .await?;
            continue;
        }
        if line.starts_with(':') {
            continue;
        }

        let (field, value) = if let Some((f, v)) = line.split_once(':') {
            (f, v.strip_prefix(' ').unwrap_or(v))
        } else {
            (line.as_str(), "")
        };

        match field {
            "event" => pending.event = Some(value.to_string()),
            "data" => pending.data_lines.push(value.to_string()),
            "id" if !value.contains('\0') => pending.id = Some(value.to_string()),
            "retry" => {
                if let Ok(v) = value.parse::<u64>() {
                    pending.retry_ms = Some(v);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

async fn flush_pending_event(
    pending: &mut PendingEvent,
    events_tx: &mpsc::Sender<Result<SseEvent, String>>,
    last_event_id: &mut Option<String>,
    reconnect_delay_ms: &mut u64,
    max_delay_ms: u64,
    destination: &SseDestination,
) -> Result<(), String> {
    if let Some(retry_ms) = pending.retry_ms.take() {
        *reconnect_delay_ms = retry_ms.clamp(1, max_delay_ms);
    }

    if let Some(id) = pending.id.take() {
        *last_event_id = Some(id);
    }

    if pending.data_lines.is_empty() {
        pending.event = None;
        return Ok(());
    }

    let event_id = last_event_id.clone();

    let evt = SseEvent {
        event: pending
            .event
            .take()
            .unwrap_or_else(|| "message".to_string()),
        data: pending.data_lines.join("\n"),
        id: event_id,
        retry_ms: Some(*reconnect_delay_ms),
        origin: format!("{}://{}", destination.scheme.as_str(), destination.target),
    };
    pending.data_lines.clear();

    events_tx
        .send(Ok(evt))
        .await
        .map_err(|_| "sse consumer dropped".to_string())
}

fn complete_initial_open(
    opened_tx: &mut Option<oneshot::Sender<Result<String, String>>>,
    result: Result<String, String>,
) {
    if let Some(tx) = opened_tx.take() {
        let _ = tx.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::HeaderMap;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use axum::{Router, response::Response as AxumResponse};
    use std::convert::Infallible;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_stream::{self as stream, wrappers::ReceiverStream};

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

    async fn spawn_sse_server() -> std::net::SocketAddr {
        async fn live_small() -> impl IntoResponse {
            let (tx, rx) = mpsc::channel(2);
            tokio::spawn(async move {
                let _ = tx
                    .send(Ok::<_, Infallible>("data: live-small\n\n".to_string()))
                    .await;
                tokio::time::sleep(Duration::from_millis(800)).await;
                let _ = tx
                    .send(Ok::<_, Infallible>(": keep-alive\n\n".to_string()))
                    .await;
            });

            AxumResponse::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from_stream(ReceiverStream::new(rx)))
                .unwrap()
        }

        async fn header_echo(headers: HeaderMap) -> impl IntoResponse {
            let tag = headers
                .get("x-test")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            let last_event_id = headers
                .get("last-event-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            let payload = format!("data: {}|{}\n\n", tag, last_event_id);
            let stream = stream::iter(vec![Ok::<_, Infallible>(payload)]);

            AxumResponse::builder()
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from_stream(stream))
                .unwrap()
        }

        let app = Router::new()
            .route("/live-small", get(live_small))
            .route("/headers", get(header_echo));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        addr
    }

    #[test]
    fn connect_convenience_opens_and_receives_small_event() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_sse_server().await;
            let url = format!("http://{}/live-small", addr);

            let mut connection = connect(&url, None).expect("sse connection should start");
            let opened = connection.opened().await.expect("sse should open");
            assert_eq!(opened, format!("http://{}", addr));

            let event = tokio::time::timeout(Duration::from_millis(600), connection.events.recv())
                .await
                .expect("event should arrive before timeout")
                .expect("event channel should stay open")
                .expect("event should be ok");
            assert_eq!(event.event, "message");
            assert_eq!(event.data, "live-small");

            let second_open = connection
                .opened()
                .await
                .expect_err("open state is one-shot");
            assert!(second_open.contains("already consumed"));
            connection.close();
        });
    }

    #[test]
    fn connect_options_builder_sends_headers_and_last_event_id() {
        let _guard = crate::client::test_guard();
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let addr = spawn_sse_server().await;
            let url = format!("http://{}/headers", addr);
            let options = SseConnectOptions::new(&url)
                .expect("valid url")
                .with_header("x-test", "builder")
                .with_last_event_id("41")
                .with_request_timeout(Duration::from_secs(1))
                .with_connect_timeout(Duration::from_secs(1))
                .with_reconnect(SseReconnectOptions {
                    enabled: false,
                    ..Default::default()
                });

            let mut connection = connect_sse(options, None).expect("sse connection should start");
            let event = connection
                .events
                .recv()
                .await
                .expect("event channel should stay open")
                .expect("event should be ok");
            assert_eq!(event.data, "builder|41");
            connection.close();
        });
    }

    #[test]
    fn scoped_network_access_guard_blocks_connect_sse() {
        let handle = crate::RongExecutor::global().handle();
        handle.block_on(async {
            let err = crate::http::scope_network_access_guard(Arc::new(DenyExampleGuard), async {
                let mut connection = connect("http://denied.example.com/events", None)
                    .expect("sse connection should start");
                connection.opened().await.expect_err("sse should be denied")
            })
            .await;

            assert_eq!(err, "sse request failed: network access denied");
        });
    }
}
