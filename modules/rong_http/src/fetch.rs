use http::Request as HttpRequest;
use http::header;
use http_body::Frame;
use http_body_util::{BodyExt, Full, StreamBody, combinators::BoxBody};
use hyper::body::Bytes;
use rong::{function::Optional, *};
use std::io::Error;
use std::sync::Arc;
use tokio::sync::{Notify, oneshot};
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};

use crate::client;
use crate::formdata::FormData;
use crate::observer::FetchOwner;
use crate::request::{Request, RequestInit};
use crate::response::Response;
use crate::security::grant_network_access;
use rong_stream::ReadableStream;

// Convert Request to hyper::Request without cloning the whole Request value.
async fn to_hyper_request(request: &Request) -> JSResult<HttpRequest<BoxBody<Bytes, Error>>> {
    let user_agent = rong::get_user_agent();
    let mut builder = HttpRequest::builder()
        .method(request.method.clone())
        .uri(request.url.clone())
        .header(header::USER_AGENT, user_agent.as_str())
        .header(header::ACCEPT, "*/*")
        .header(header::ACCEPT_ENCODING, "gzip"); // TODO: "gzip, zstd"

    // Clone only the header map entries we need for the outbound request.
    if let Some(headers) = builder.headers_mut() {
        headers.extend(
            request
                .headers
                .as_header_map()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
    }

    // Create body
    let body: BoxBody<Bytes, Error> = if let Some(body) = &request.body {
        // Detect ReadableStream for streaming upload
        if let Some(obj) = body.0.clone().into_object() {
            if let Ok(rs) = obj.borrow::<ReadableStream>() {
                if let Some(rx) = rong_stream::readable_stream_take_receiver(&rs) {
                    let stream = ReceiverStream::new(rx).map(|item| match item {
                        Ok(bytes) => Ok(Frame::data(bytes)),
                        Err(e) => Err(Error::other(e)),
                    });
                    let sb = StreamBody::new(stream);
                    sb.boxed()
                } else {
                    return Err(HostError::new(
                        rong::error::E_INVALID_STATE,
                        "ReadableStream request body already used",
                    )
                    .with_name("TypeError")
                    .into());
                }
            } else {
                // Fallback to non-streaming conversion - convert once and reuse
                let (bytes, boundary) = body.to_bytes().await?;
                if let Some(boundary) = boundary
                    && let Some(headers) = builder.headers_mut()
                {
                    headers.insert(
                        header::CONTENT_TYPE,
                        FormData::content_type(&boundary).parse().map_err(|e| {
                            HostError::new(
                                rong::error::E_INTERNAL,
                                format!("Invalid content-type header: {}", e),
                            )
                        })?,
                    );
                }
                Full::new(bytes).map_err(|e| match e {}).boxed()
            }
        } else {
            // Non-object body (e.g., string) - convert once
            let (bytes, boundary) = body.to_bytes().await?;
            if let Some(boundary) = boundary
                && let Some(headers) = builder.headers_mut()
            {
                headers.insert(
                    header::CONTENT_TYPE,
                    FormData::content_type(&boundary).parse().map_err(|e| {
                        HostError::new(
                            rong::error::E_INTERNAL,
                            format!("Invalid content-type header: {}", e),
                        )
                    })?,
                );
            }
            Full::new(bytes).map_err(|e| match e {}).boxed()
        }
    } else {
        Full::new(Bytes::new()).map_err(|e| match e {}).boxed()
    };

    // builder.body() only fails if the headers are invalid, which we know they aren't
    // because we just created them from valid headers
    builder.body(body).map_err(|e| {
        HostError::new(
            rong::error::E_INVALID_ARG,
            format!("Failed to build request: {}", e),
        )
        .with_name("TypeError")
        .into()
    })
}

pub async fn fetch(
    ctx: JSContext,
    input: JSValue,
    init: Optional<RequestInit>,
) -> JSResult<Response> {
    // Create Request object from input and init
    let mut request = Request::new(input, init).map_err(|e| {
        HostError::new(rong::error::E_INVALID_ARG, e.to_string()).with_name("TypeError")
    })?;

    let mut observed = FetchOwner::new(&ctx);

    // Domain check for initial URL
    let domain = request.domain()?;
    if let Err(e) = grant_network_access(&domain) {
        observed.blocked(
            &request.method,
            &request.url,
            request.headers.as_header_map(),
            &e.to_string(),
        );
        return Err(e);
    }

    // Get abort signal if present
    let mut abort_receiver = request.abort_signal()?.map(|signal| signal.subscribe());

    let mut redirect_count = 0;
    const MAX_REDIRECTS: u32 = 20;

    loop {
        // Convert Request to hyper::Request without cloning the whole Request object.
        let mut hyper_request = to_hyper_request(&request).await?;
        observed.tag(&mut hyper_request);
        let orig_method = hyper_request.method().clone();
        let orig_uri = hyper_request.uri().clone();

        // Send via dedicated net service.
        // Note: transport treats oneshot sender-drop as "aborted", so the abort sender
        // must remain alive for the whole response stream lifetime. We keep it in a task, and only
        // stop it explicitly when we discard a redirect response and continue the loop.
        let (abort_bridge, abort_bridge_stop) = if let Some(r) = &mut abort_receiver {
            let (mut tx, rx) = oneshot::channel::<()>();
            let stop = Arc::new(Notify::new());
            let stop_wait = stop.clone();
            let mut abort_rx = r.clone();
            tokio::task::spawn_local(async move {
                // The transport dropping its receiver means the response is done
                // (or gone): nothing is left to abort, so the bridge ends too
                // instead of outliving every request that carried a signal.
                let aborted = tokio::select! {
                    _ = abort_rx.recv() => true,
                    _ = stop_wait.notified() => false,
                    _ = tx.closed() => false,
                };
                if aborted {
                    let _ = tx.send(());
                }
            });
            (Some(rx), Some(stop))
        } else {
            (None, None)
        };

        // Buffer responses up to 256KB in memory before streaming.
        let small_threshold = 256 * 1024; // 256KB

        // Race the network request with an early abort. If the abort wins, reject with its reason.
        let net_fut = client::send_fetch_request(hyper_request, small_threshold, abort_bridge);
        let net_resp = if let Some(early_abort) = &mut abort_receiver {
            tokio::select! {
                biased;
                reason = early_abort.recv() => {
                    return Err(RongJSError::from_thrown_value(reason));
                }
                res = net_fut => res.map_err(|e| {
                    HostError::new(rong::error::E_IO, "fetch failed")
                        .with_name("TypeError")
                        .with_data(rong::err_data!({ detail: (e.to_string()) }))
                })?,
            }
        } else {
            net_fut.await.map_err(|e| {
                HostError::new(rong::error::E_IO, "fetch failed")
                    .with_name("TypeError")
                    .with_data(rong::err_data!({ detail: (e.to_string()) }))
            })?
        };

        // Handle Redirects
        let status = net_resp.status.as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            match request.redirect() {
                "error" => {
                    if let Some(stop) = abort_bridge_stop {
                        stop.notify_one();
                    }
                    return Err(HostError::new(
                        rong::error::E_NETWORK,
                        "Redirects not allowed in 'error' mode",
                    )
                    .with_name("TypeError")
                    .into());
                }
                "manual" => {
                    // Fall through to return response
                }
                // `Request::redirect()` is backed by an enum, so only "follow" is possible here.
                // Keep `_` for forward compatibility, but assert in debug builds.
                _ => {
                    debug_assert_eq!(request.redirect(), "follow");
                    if redirect_count >= MAX_REDIRECTS {
                        if let Some(stop) = abort_bridge_stop {
                            stop.notify_one();
                        }
                        return Err(HostError::new(
                            rong::error::E_NETWORK,
                            "Maximum redirect count exceeded",
                        )
                        .with_name("NetworkError")
                        .into());
                    }

                    if let Some(location) = net_resp
                        .headers
                        .get("location")
                        .and_then(|v| v.to_str().ok())
                    {
                        redirect_count += 1;

                        // Resolve URL
                        let base = match url::Url::parse(&request.url.to_string()) {
                            Ok(v) => v,
                            Err(e) => {
                                if let Some(stop) = abort_bridge_stop.as_ref() {
                                    stop.notify_one();
                                }
                                return Err(HostError::new(
                                    rong::error::E_INTERNAL,
                                    format!("Invalid base URL: {}", e),
                                )
                                .into());
                            }
                        };

                        let next_url = match base.join(location) {
                            Ok(v) => v,
                            Err(e) => {
                                if let Some(stop) = abort_bridge_stop.as_ref() {
                                    stop.notify_one();
                                }
                                return Err(HostError::new(
                                    rong::error::E_NETWORK,
                                    format!("Invalid redirect URL: {}", e),
                                )
                                .into());
                            }
                        };

                        if next_url.scheme() != "http" && next_url.scheme() != "https" {
                            if let Some(stop) = abort_bridge_stop.as_ref() {
                                stop.notify_one();
                            }
                            return Err(HostError::new(
                                rong::error::E_NETWORK,
                                format!("Unsupported redirect URL scheme: {}", next_url.scheme()),
                            )
                            .with_name("TypeError")
                            .into());
                        }

                        let current_host = base.host_str();
                        let current_port = base.port_or_known_default();
                        let next_host = next_url.host_str();
                        let next_port = next_url.port_or_known_default();
                        let is_cross_host = current_host != next_host || current_port != next_port;
                        if is_cross_host {
                            request.headers.delete("authorization".to_string())?;
                            request.headers.delete("proxy-authorization".to_string())?;
                            request.headers.delete("cookie".to_string())?;
                            request.headers.delete("host".to_string())?;
                        }

                        // Check permission for new domain
                        let host = match next_url.host_str() {
                            Some(v) => v,
                            None => {
                                if let Some(stop) = abort_bridge_stop.as_ref() {
                                    stop.notify_one();
                                }
                                return Err(HostError::new(
                                    rong::error::E_NETWORK,
                                    "Redirect URL has no host",
                                )
                                .with_name("TypeError")
                                .into());
                            }
                        };
                        if let Err(e) = grant_network_access(host) {
                            if let Some(stop) = abort_bridge_stop.as_ref() {
                                stop.notify_one();
                            }
                            if let Ok(uri) = next_url.as_str().parse::<http::Uri>() {
                                observed.blocked(
                                    &request.method,
                                    &uri,
                                    request.headers.as_header_map(),
                                    &e.to_string(),
                                );
                            }
                            return Err(e);
                        }

                        // Update request URL
                        request.url = match next_url.to_string().parse::<http::Uri>() {
                            Ok(v) => v,
                            Err(e) => {
                                if let Some(stop) = abort_bridge_stop.as_ref() {
                                    stop.notify_one();
                                }
                                return Err(HostError::new(
                                    rong::error::E_INTERNAL,
                                    format!("Invalid URI: {}", e),
                                )
                                .into());
                            }
                        };

                        // Handle method/body changes for redirects
                        let should_switch_to_get = match status {
                            303 => {
                                request.method != http::Method::GET
                                    && request.method != http::Method::HEAD
                            }
                            301 | 302 => request.method == http::Method::POST,
                            _ => false,
                        };

                        if should_switch_to_get {
                            request.method = http::Method::GET;
                            request.body = None;
                            request.headers.delete("content-length".to_string())?;
                            request.headers.delete("content-type".to_string())?;
                            request.headers.delete("transfer-encoding".to_string())?;
                        }

                        // Discard the redirect response and continue.
                        // If it was streaming, notify the abort bridge to stop background reads.
                        if let Some(stop) = abort_bridge_stop {
                            stop.notify_one();
                        }

                        continue;
                    }
                }
            }
        }

        let body_kind = match net_resp.body {
            client::HttpBody::Small(bytes) => crate::body::BodyKind::Buffered(bytes),
            client::HttpBody::Stream(rx) => {
                crate::body::BodyKind::Channel(crate::body::HostBodyStream::from_receiver(rx))
            }
            client::HttpBody::Empty => crate::body::BodyKind::Buffered(Bytes::new()),
        };

        let type_ =
            if matches!(status, 301 | 302 | 303 | 307 | 308) && request.redirect() == "manual" {
                // We currently return a transparent 3xx response (headers/status accessible),
                // so this should remain "basic" rather than claiming browser-style opaqueredirect.
                "basic".to_string()
            } else {
                "basic".to_string()
            };

        return Ok(Response::from_meta(
            net_resp.status,
            net_resp.headers,
            body_kind,
            abort_receiver,
            orig_method,
            orig_uri,
            redirect_count > 0,
            type_,
        ));
    }
}
pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    let fetch_fn = JSFunc::new(ctx, fetch)?;
    ctx.global().set("fetch", fetch_fn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use futures::{StreamExt as FuturesStreamExt, stream};
    use rong_test::http::axum::body::Bytes as AxumBytes;
    use rong_test::http::axum::routing::{get, put};
    use rong_test::http::axum::{
        Router,
        body::Body,
        http::HeaderMap,
        response::{IntoResponse, Response as AxumResponse},
    };
    use rong_test::http::{axum, spawn_axum};
    use rong_test::*;
    use std::convert::Infallible;
    use std::io::Write;
    use std::net::SocketAddr;
    use tokio::time::{Duration, sleep};

    async fn test_ip() -> impl IntoResponse {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"origin": "127.0.0.1"}"#))
            .unwrap()
    }

    async fn test_gzip() -> impl IntoResponse {
        // Create gzipped JSON response
        let json = r#"{"gzipped": true, "method": "GET"}"#;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(json.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONTENT_ENCODING, "gzip")
            .body(Body::from(compressed))
            .unwrap()
    }

    async fn test_delay() -> impl IntoResponse {
        sleep(Duration::from_millis(100)).await;
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"delayed": true}"#))
            .unwrap()
    }

    async fn test_large() -> impl IntoResponse {
        // Add a tiny delay per chunk to ensure abort-on-read has a window to trigger,
        // while keeping the overall test quick.
        let stream = FuturesStreamExt::then(stream::iter(0..100), |i| async move {
            sleep(Duration::from_millis(6)).await;
            Ok::<_, Infallible>(format!("chunk_{:04}\n", i).repeat(1024))
        });

        // Convert the stream into a response
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from_stream(stream))
            .unwrap()
    }

    async fn test_headers(headers: HeaderMap) -> impl IntoResponse {
        // Create a response containing the received headers
        let mut headers_map = serde_json::Map::new();
        for (key, value) in headers.iter() {
            if let Ok(v) = value.to_str() {
                headers_map.insert(
                    key.as_str().to_string(),
                    serde_json::Value::String(v.to_string()),
                );
            }
        }
        let json = serde_json::Value::Object(headers_map);

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap()
    }

    async fn test_redirect(uri: axum::http::Uri) -> impl IntoResponse {
        // Parse query params (n=count)
        let query = uri.query().unwrap_or("");
        let count = query
            .split('&')
            .find(|p| p.starts_with("n="))
            .map(|p| p[2..].parse::<i32>().unwrap_or(1))
            .unwrap_or(1);

        if count > 0 {
            AxumResponse::builder()
                .status(302)
                .header("Location", format!("/redirect?n={}", count - 1))
                .body(Body::empty())
                .unwrap()
        } else {
            AxumResponse::builder()
                .status(302)
                .header("Location", "/ip")
                .body(Body::empty())
                .unwrap()
        }
    }

    async fn test_redirect_blocked() -> impl IntoResponse {
        AxumResponse::builder()
            .status(302)
            .header("Location", "http://blocked.invalid/next")
            .body(Body::empty())
            .unwrap()
    }

    async fn test_303() -> impl IntoResponse {
        AxumResponse::builder()
            .status(303)
            .header("Location", "/ip")
            .body(Body::empty())
            .unwrap()
    }

    async fn test_sse_basic() -> impl IntoResponse {
        let stream = stream::iter(vec![
            Ok::<_, Infallible>("id: 1\nevent: message\ndata: hello\n\n".to_string()),
            Ok::<_, Infallible>("id: 2\ndata: world\n\n".to_string()),
        ]);

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from_stream(stream))
            .unwrap()
    }

    async fn test_sse_reconnect(headers: HeaderMap) -> impl IntoResponse {
        let last_event_id = headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let payload = if last_event_id == "1" {
            "id: 2\ndata: second\n\n".to_string()
        } else {
            "retry: 20\nid: 1\ndata: first\n\n".to_string()
        };

        let stream = stream::iter(vec![Ok::<_, Infallible>(payload)]);
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from_stream(stream))
            .unwrap()
    }

    async fn test_sse_custom_events() -> impl IntoResponse {
        let stream = stream::iter(vec![
            Ok::<_, Infallible>("event: status\ndata: connected\n\n".to_string()),
            Ok::<_, Infallible>("event: progress\ndata: 50%\n\n".to_string()),
            Ok::<_, Infallible>("id: 3\ndata: default message\n\n".to_string()),
        ]);

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from_stream(stream))
            .unwrap()
    }

    async fn test_sse_many() -> impl IntoResponse {
        let events: Vec<Result<String, Infallible>> = (1..=5)
            .map(|i| Ok(format!("id: {i}\ndata: msg-{i}\n\n")))
            .collect();

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from_stream(stream::iter(events)))
            .unwrap()
    }

    async fn test_sse_live_small() -> impl IntoResponse {
        let initial =
            stream::once(async { Ok::<_, Infallible>("data: live-small\n\n".to_string()) });
        let keepalive = stream::once(async {
            sleep(Duration::from_millis(800)).await;
            Ok::<_, Infallible>(": keep-alive\n\n".to_string())
        });

        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from_stream(FuturesStreamExt::chain(
                initial, keepalive,
            )))
            .unwrap()
    }

    async fn test_sse_retry_control(headers: HeaderMap) -> impl IntoResponse {
        let last_event_id = headers
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let payload = if last_event_id == "1" {
            "id: 2\ndata: second\n\n".to_string()
        } else {
            "retry: 250\n\nid: 1\ndata: first\n\n".to_string()
        };

        let stream = stream::iter(vec![Ok::<_, Infallible>(payload)]);
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from_stream(stream))
            .unwrap()
    }

    async fn test_sse_not_event_stream() -> impl IntoResponse {
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from("not an event stream"))
            .unwrap()
    }

    async fn start_test_server() -> std::io::Result<SocketAddr> {
        let app = Router::new()
            .route("/ip", get(test_ip))
            .route("/gzip", get(test_gzip))
            .route("/delay", get(test_delay))
            .route("/large", get(test_large))
            .route("/headers", get(test_headers))
            .route("/redirect", get(test_redirect))
            .route("/redirect-blocked", get(test_redirect_blocked))
            .route("/303", put(test_303)) // PUT request receiving 303 -> GET /ip
            .route("/sse/basic", get(test_sse_basic))
            .route("/sse/reconnect", get(test_sse_reconnect))
            .route("/sse/custom", get(test_sse_custom_events))
            .route("/sse/many", get(test_sse_many))
            .route("/sse/live-small", get(test_sse_live_small))
            .route("/sse/retry-control", get(test_sse_retry_control))
            .route("/sse/not-event-stream", get(test_sse_not_event_stream))
            .route(
                "/upload",
                put(|bytes: AxumBytes| async move {
                    let total = bytes.len();
                    let json = serde_json::json!({"received": total});
                    axum::response::Response::builder()
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(json.to_string()))
                        .unwrap()
                }),
            );

        spawn_axum(app).await
    }

    #[test]
    fn test_fetch() {
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_url::init(&ctx)?;
            rong_timer::init(&ctx)?;
            rong_abort::init(&ctx)?;
            rong_exception::init(&ctx)?;
            // Needed for new ReadableStream(...) in tests
            rong_stream::init(&ctx)?;
            // FS needed for download-to-file streaming test
            rong_fs::init(&ctx)?;

            crate::header::init(&ctx)?;
            crate::request::init(&ctx)?;
            crate::response::init(&ctx)?;
            crate::init(&ctx)?;

            // Start test server
            let addr = match start_test_server().await {
                Ok(addr) => addr,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!(
                        "skipping rong_http fetch tests: cannot bind local TCP listener: {}",
                        e
                    );
                    return Ok(());
                }
                Err(e) => {
                    return Err(HostError::new(
                        rong::error::E_INTERNAL,
                        format!("failed to start test server: {}", e),
                    )
                    .into());
                }
            };
            let base_url = format!("http://{}", addr);

            // Set base URL for tests
            ctx.global().set("TEST_SERVER_URL", base_url)?;

            // Set WORKSPACE_ROOT for saving files in tests
            let workspace_root = std::env::current_dir()
                .map_err(|e| {
                    HostError::new(
                        rong::error::E_INTERNAL,
                        format!("Failed to get current dir: {}", e),
                    )
                })?
                .parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| {
                    HostError::new(rong::error::E_INTERNAL, "Failed to get workspace root")
                })?
                .to_string_lossy()
                .into_owned();
            ctx.global().set("WORKSPACE_ROOT", workspace_root)?;

            let passed = UnitJSRunner::load_script(&ctx, "fetch.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn test_sse() {
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_url::init(&ctx)?;
            rong_timer::init(&ctx)?;
            rong_abort::init(&ctx)?;
            rong_exception::init(&ctx)?;
            rong_stream::init(&ctx)?;

            crate::init(&ctx)?;

            let addr = match start_test_server().await {
                Ok(addr) => addr,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!(
                        "skipping rong_http sse tests: cannot bind local TCP listener: {}",
                        e
                    );
                    return Ok(());
                }
                Err(e) => {
                    return Err(HostError::new(
                        rong::error::E_INTERNAL,
                        format!("failed to start test server: {}", e),
                    )
                    .into());
                }
            };
            let base_url = format!("http://{}", addr);
            ctx.global().set("TEST_SERVER_URL", base_url)?;
            let has_sse: bool = ctx.eval(Source::from_bytes("typeof Rong.SSE === 'function'"))?;
            assert!(has_sse, "SSE should be initialized");

            let passed = UnitJSRunner::load_script(&ctx, "sse.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[cfg(feature = "network-observer")]
    mod observed {
        use rong_rt::http::{NetworkEvent, NetworkObserver, NetworkOutcome, NetworkRequestKind};
        use std::collections::BTreeMap;
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        pub(super) struct Recording(pub(super) Mutex<Vec<NetworkEvent>>);

        impl NetworkObserver for Recording {
            fn on_event(&self, event: NetworkEvent) {
                self.0.lock().unwrap().push(event);
            }
        }

        #[derive(Default)]
        struct Trace {
            owner: Option<String>,
            value: serde_json::Map<String, serde_json::Value>,
            body: Vec<u8>,
            chunks: usize,
            request_bytes: usize,
            /// Event ranks in arrival order: start 0, request chunk 1, head 2,
            /// response chunk 3, end 4.
            order: Vec<u8>,
        }

        /// Why `order` is not a prefix of start, request chunks, head,
        /// response chunks, end — or `None` if it is.
        fn order_error(order: &[u8]) -> Option<String> {
            let count = |rank| order.iter().filter(|r| **r == rank).count();
            if order.first() != Some(&0) || count(0) != 1 {
                return Some(format!("start must come first, once: {order:?}"));
            }
            if count(2) > 1 || count(4) > 1 {
                return Some(format!("duplicate head or end: {order:?}"));
            }
            if order.windows(2).any(|w| w[0] > w[1]) {
                return Some(format!("events out of order: {order:?}"));
            }
            None
        }

        /// Traces for `owner` ("" = no owner) whose URL contains `url_contains`,
        /// as JSON in request start order.
        pub(super) fn traces_json(
            recording: &Arc<Recording>,
            owner: &str,
            url_contains: &str,
        ) -> String {
            let events = recording.0.lock().unwrap();
            let mut traces: BTreeMap<u64, Trace> = BTreeMap::new();
            for event in events.iter() {
                match event {
                    NetworkEvent::RequestStart(e) => {
                        let t = traces.entry(e.id.as_u64()).or_default();
                        t.order.push(0);
                        t.owner = e.owner.as_ref().map(|o| o.as_str().to_string());
                        let kind = match e.kind {
                            NetworkRequestKind::Fetch => "fetch",
                            NetworkRequestKind::Sse => "sse",
                            NetworkRequestKind::Download => "download",
                            NetworkRequestKind::Upload => "upload",
                            _ => "other",
                        };
                        let v = &mut t.value;
                        v.insert("id".into(), e.id.as_u64().into());
                        v.insert("owner".into(), t.owner.clone().into());
                        v.insert("kind".into(), kind.into());
                        v.insert("method".into(), e.method.as_str().into());
                        v.insert("url".into(), e.uri.to_string().into());
                        v.insert(
                            "redirectedFrom".into(),
                            e.redirected_from.map(|id| id.as_u64()).into(),
                        );
                        v.insert("bodySize".into(), e.body_size.into());
                    }
                    NetworkEvent::RequestBodyChunk { id, chunk } => {
                        let t = traces.entry(id.as_u64()).or_default();
                        t.order.push(1);
                        t.request_bytes += chunk.len();
                    }
                    NetworkEvent::ResponseHead(e) => {
                        let t = traces.entry(e.id.as_u64()).or_default();
                        t.order.push(2);
                        let v = &mut t.value;
                        v.insert("status".into(), e.status.as_u16().into());
                        let header = |name: &str| {
                            e.headers
                                .get(name)
                                .and_then(|h| h.to_str().ok())
                                .map(String::from)
                        };
                        v.insert("contentEncoding".into(), header("content-encoding").into());
                        v.insert("location".into(), header("location").into());
                        v.insert(
                            "contentLength".into(),
                            header("content-length")
                                .and_then(|h| h.parse::<u64>().ok())
                                .into(),
                        );
                    }
                    NetworkEvent::ResponseBodyChunk { id, chunk } => {
                        let t = traces.entry(id.as_u64()).or_default();
                        t.order.push(3);
                        t.body.extend_from_slice(chunk);
                        t.chunks += 1;
                    }
                    NetworkEvent::RequestEnd(e) => {
                        let t = traces.entry(e.id.as_u64()).or_default();
                        t.order.push(4);
                        let v = &mut t.value;
                        let outcome = match &e.outcome {
                            NetworkOutcome::Completed => "completed".to_string(),
                            NetworkOutcome::Failed(message) => format!("failed: {message}"),
                            NetworkOutcome::Aborted => "aborted".to_string(),
                            NetworkOutcome::Dropped => "dropped".to_string(),
                            NetworkOutcome::Blocked(reason) => format!("blocked: {reason}"),
                            _ => "other".to_string(),
                        };
                        v.insert("outcome".into(), outcome.into());
                        v.insert("endBytes".into(), e.response_bytes.into());
                    }
                    _ => {}
                }
            }
            let wanted_owner = (!owner.is_empty()).then(|| owner.to_string());
            let list: Vec<_> = traces
                .into_values()
                .filter(|t| t.value.contains_key("url") && t.owner == wanted_owner)
                .filter(|t| {
                    t.value["url"]
                        .as_str()
                        .is_some_and(|url| url.contains(url_contains))
                })
                .map(|mut t| {
                    let v = &mut t.value;
                    for key in [
                        "status",
                        "outcome",
                        "contentEncoding",
                        "contentLength",
                        "location",
                        "endBytes",
                    ] {
                        v.entry(key).or_insert(serde_json::Value::Null);
                    }
                    v.insert("responseBytes".into(), t.body.len().into());
                    v.insert("chunks".into(), t.chunks.into());
                    v.insert("requestBytes".into(), t.request_bytes.into());
                    v.insert(
                        "firstBytes".into(),
                        t.body
                            .iter()
                            .take(2)
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()
                            .into(),
                    );
                    v.insert(
                        "bodyText".into(),
                        String::from_utf8_lossy(&t.body[..t.body.len().min(4096)])
                            .into_owned()
                            .into(),
                    );
                    v.insert("orderError".into(), order_error(&t.order).into());
                    serde_json::Value::Object(t.value)
                })
                .collect();
            serde_json::Value::Array(list).to_string()
        }
    }

    #[cfg(feature = "network-observer")]
    #[test]
    fn test_network_observer() {
        use rong_rt::http::{NetworkOwner, set_network_observer};

        let recording = std::sync::Arc::new(observed::Recording::default());
        set_network_observer(Some(recording.clone()));

        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_url::init(&ctx)?;
            rong_timer::init(&ctx)?;
            rong_abort::init(&ctx)?;
            rong_exception::init(&ctx)?;
            rong_stream::init(&ctx)?;
            crate::init(&ctx)?;

            let addr = match start_test_server().await {
                Ok(addr) => addr,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!("skipping network observer tests: cannot bind: {}", e);
                    return Ok(());
                }
                Err(e) => {
                    return Err(HostError::new(
                        rong::error::E_INTERNAL,
                        format!("failed to start test server: {}", e),
                    )
                    .into());
                }
            };
            let closed = std::net::TcpListener::bind("127.0.0.1:0")
                .and_then(|listener| listener.local_addr())
                .map_err(|e| HostError::new(rong::error::E_INTERNAL, e.to_string()))?;
            ctx.global()
                .set("TEST_SERVER_URL", format!("http://{}", addr))?;
            ctx.global()
                .set("CLOSED_URL", format!("http://{}/", closed))?;

            ctx.global().set(
                "__setNetworkOwner",
                JSFunc::new(&ctx, |ctx: JSContext, owner: String| {
                    crate::set_network_owner(
                        &ctx,
                        (!owner.is_empty()).then(|| NetworkOwner::new(owner)),
                    );
                })?,
            )?;
            struct DenyHost(String);
            impl crate::NetworkAccessGuard for DenyHost {
                fn check_access(&self, domain: &str) -> JSResult<()> {
                    if domain == self.0 {
                        Err(
                            HostError::new(rong::error::E_PERMISSION_DENIED, "denied by policy")
                                .into(),
                        )
                    } else {
                        Ok(())
                    }
                }
            }
            ctx.global().set(
                "__denyHost",
                JSFunc::new(&ctx, |host: String| {
                    crate::set_network_access_guard(Box::new(DenyHost(host)));
                })?,
            )?;
            let traces = recording.clone();
            ctx.global().set(
                "__observedRequests",
                JSFunc::new(&ctx, move |owner: String, url_contains: String| {
                    observed::traces_json(&traces, &owner, &url_contains)
                })?,
            )?;

            let passed = UnitJSRunner::load_script(&ctx, "network_observer.js")
                .await?
                .run()
                .await?;
            assert!(passed);
            Ok(())
        });

        set_network_observer(None);
    }

    #[test]
    fn test_network_access_guard() {
        use crate::security::{
            NetworkAccessGuard, grant_network_access, set_network_access_guard_scoped,
        };

        // Test default guard allows all domains
        let result = grant_network_access("httpbin.org");
        assert!(result.is_ok());

        // Define restricted network guard
        struct RestrictedNetworkGuard;

        impl NetworkAccessGuard for RestrictedNetworkGuard {
            fn check_access(&self, domain: &str) -> JSResult<()> {
                if domain == "allowed.example.com" {
                    Ok(())
                } else {
                    Err(
                        HostError::new(rong::error::E_PERMISSION_DENIED, "Network access denied")
                            .into(),
                    )
                }
            }
        }

        // Set restricted network guard (scoped so it can't leak into other tests on the same thread)
        let _scope = set_network_access_guard_scoped(Box::new(RestrictedNetworkGuard));

        // Test allowed domain - should succeed
        let allowed_result = grant_network_access("allowed.example.com");
        assert!(allowed_result.is_ok());

        // Test denied domain - should fail
        let denied_result = grant_network_access("denied.example.com");
        assert!(denied_result.is_err());
        if let Err(err) = denied_result {
            assert!(err.to_string().contains("Network access denied"));
        }
    }
}
