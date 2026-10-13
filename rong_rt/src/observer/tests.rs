use super::*;
use crate::http::{
    HttpBody, RequestOptions, collect_body, post_json_bytes, send, send_stream,
    send_with_small_body_limit,
};
use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::response::Response as AxumResponse;
use axum::routing::{get, post, put};
use bytes::Bytes;
use http::{Method, StatusCode, Uri, Version, header};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use std::convert::Infallible;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::oneshot;

// The observer slot is process-wide; tests that install one take turns.
static OBSERVER_LOCK: Mutex<()> = Mutex::new(());

const LARGE_LEN: usize = 600 * 1024;

fn large_payload() -> Vec<u8> {
    (0..LARGE_LEN).map(|i| (i % 251) as u8).collect()
}

fn gzip_payload() -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(br#"{"gzipped":true,"padding":"aaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#)
        .unwrap();
    encoder.finish().unwrap()
}

const SSE_TEXT: &str = "id: 1\ndata: hello\n\nid: 2\ndata: world\n\n";

async fn spawn_server() -> SocketAddr {
    async fn large() -> AxumResponse {
        let payload = large_payload();
        let chunks: Vec<Result<Bytes, Infallible>> = payload
            .chunks(8 * 1024)
            .map(|c| Ok(Bytes::copy_from_slice(c)))
            .collect();
        AxumResponse::builder()
            .body(Body::from_stream(futures::stream::iter(chunks)))
            .unwrap()
    }

    async fn gzip() -> AxumResponse {
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONTENT_ENCODING, "gzip")
            .body(Body::from(gzip_payload()))
            .unwrap()
    }

    async fn slow(Path(n): Path<usize>) -> AxumResponse {
        let stream = futures::StreamExt::then(futures::stream::iter(0..n), |i| async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            Ok::<_, Infallible>(Bytes::from(format!("tick-{i};")))
        });
        AxumResponse::builder()
            .body(Body::from_stream(stream))
            .unwrap()
    }

    async fn redirect(Path(n): Path<u32>) -> AxumResponse {
        let location = if n == 0 {
            "/small".to_string()
        } else {
            format!("/redirect/{}", n - 1)
        };
        AxumResponse::builder()
            .status(302)
            .header(header::LOCATION, location)
            .body(Body::empty())
            .unwrap()
    }

    async fn sse() -> AxumResponse {
        let chunks = vec![
            Ok::<_, Infallible>("id: 1\ndata: hello\n\n"),
            Ok("id: 2\ndata: world\n\n"),
        ];
        AxumResponse::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .body(Body::from_stream(futures::stream::iter(chunks)))
            .unwrap()
    }

    let app = Router::new()
        .route("/small", get(|| async { "hello" }))
        .route("/large", get(large))
        .route("/gzip", get(gzip))
        .route("/slow/{n}", get(slow))
        .route("/redirect/{n}", get(redirect))
        .route("/sse", get(sse))
        .route("/echo", post(|body: Bytes| async move { body }))
        .route(
            "/upload",
            put(|body: Bytes| async move { format!("{}", body.len()) }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[derive(Default)]
struct Recording {
    events: Mutex<Vec<NetworkEvent>>,
}

impl NetworkObserver for Recording {
    fn on_event(&self, event: NetworkEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[derive(Debug, Default)]
struct Trace {
    start: Option<NetworkRequestStart>,
    head: Option<NetworkResponseHead>,
    request_body: Vec<u8>,
    body: Vec<u8>,
    chunks: usize,
    redirects: Vec<NetworkRedirect>,
    end: Option<NetworkRequestEnd>,
    /// Event kinds in arrival order, for ordering assertions.
    order: Vec<&'static str>,
}

impl Recording {
    fn traces(&self) -> Vec<Trace> {
        let events = self.events.lock().unwrap();
        let mut traces: Vec<(NetworkRequestId, Trace)> = Vec::new();
        fn slot(traces: &mut Vec<(NetworkRequestId, Trace)>, id: NetworkRequestId) -> &mut Trace {
            if let Some(pos) = traces.iter().position(|(i, _)| *i == id) {
                &mut traces[pos].1
            } else {
                traces.push((id, Trace::default()));
                &mut traces.last_mut().unwrap().1
            }
        }
        for event in events.iter() {
            match event {
                NetworkEvent::RequestStart(e) => {
                    let t = slot(&mut traces, e.id);
                    t.start = Some(e.clone());
                    t.order.push("start");
                }
                NetworkEvent::RequestBodyChunk { id, chunk } => {
                    let t = slot(&mut traces, *id);
                    t.request_body.extend_from_slice(chunk);
                    t.order.push("request_chunk");
                }
                NetworkEvent::ResponseHead(e) => {
                    let t = slot(&mut traces, e.id);
                    t.head = Some(e.clone());
                    t.order.push("head");
                }
                NetworkEvent::ResponseBodyChunk { id, chunk } => {
                    let t = slot(&mut traces, *id);
                    t.body.extend_from_slice(chunk);
                    t.chunks += 1;
                    t.order.push("chunk");
                }
                NetworkEvent::Redirect(e) => {
                    let t = slot(&mut traces, e.id);
                    t.redirects.push(e.clone());
                    t.order.push("redirect");
                }
                NetworkEvent::RequestEnd(e) => {
                    let t = slot(&mut traces, e.id);
                    t.end = Some(e.clone());
                    t.order.push("end");
                }
            }
        }
        traces.into_iter().map(|(_, t)| t).collect()
    }

    /// Traces whose request path starts with `path`, in start order.
    fn for_path(&self, path: &str) -> Vec<Trace> {
        self.traces()
            .into_iter()
            .filter(|t| {
                t.start
                    .as_ref()
                    .is_some_and(|s| s.uri.path().starts_with(path))
            })
            .collect()
    }

    fn for_owner(&self, owner: &str) -> Vec<Trace> {
        self.traces()
            .into_iter()
            .filter(|t| {
                t.start
                    .as_ref()
                    .and_then(|s| s.owner.as_ref())
                    .is_some_and(|o| o.as_str() == owner)
            })
            .collect()
    }

    /// Wait until `count` traces matching `select` have ended.
    async fn ended(&self, count: usize, select: impl Fn(&Self) -> Vec<Trace>) -> Vec<Trace> {
        for _ in 0..200 {
            let traces = select(self);
            if traces.len() >= count && traces.iter().all(|t| t.end.is_some()) {
                return traces;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("traces did not end: {:#?}", select(self));
    }
}

struct Installed {
    recording: Arc<Recording>,
    _lock: MutexGuard<'static, ()>,
}

impl Installed {
    fn new() -> Self {
        let lock = OBSERVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let recording = Arc::new(Recording::default());
        set_network_observer(Some(recording.clone()));
        Self {
            recording,
            _lock: lock,
        }
    }
}

impl Drop for Installed {
    fn drop(&mut self) {
        set_network_observer(None);
    }
}

fn get_request(url: &str) -> http::Request<BoxBody<Bytes, std::io::Error>> {
    http::Request::builder()
        .method("GET")
        .uri(url)
        .body(Full::new(Bytes::new()).map_err(|e| match e {}).boxed())
        .unwrap()
}

fn run<F: std::future::Future<Output = ()>>(f: F) {
    crate::RongExecutor::global().handle().block_on(f);
}

fn owner(name: &str) -> NetworkOwner {
    NetworkOwner::new(name)
}

#[test]
fn small_body_reports_full_lifecycle_in_order() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let url = format!("http://{addr}/small");
        let response = scope_network_owner(owner("small"), async {
            send(get_request(&url), RequestOptions::new()).await
        })
        .await
        .unwrap();
        assert!(matches!(response.body, HttpBody::Small(_)));
        assert_eq!(collect_body(response.body).await.unwrap(), "hello");

        let traces = installed.recording.ended(1, |r| r.for_owner("small")).await;
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        let start = t.start.as_ref().unwrap();
        assert_eq!(start.kind, NetworkRequestKind::Other);
        assert_eq!(start.method, Method::GET);
        assert_eq!(start.uri, url.parse::<Uri>().unwrap());
        assert_eq!(start.body_size, Some(0));
        assert_eq!(start.redirected_from, None);
        let head = t.head.as_ref().unwrap();
        assert_eq!(head.status, StatusCode::OK);
        assert_eq!(head.version, Version::HTTP_11);
        assert_eq!(head.headers[header::CONTENT_LENGTH], "5");
        assert_eq!(t.body, b"hello");
        let end = t.end.as_ref().unwrap();
        assert_eq!(end.outcome, NetworkOutcome::Completed);
        assert_eq!(end.response_bytes, 5);
        assert!(end.duration >= head.time_to_first_byte);
        assert_eq!(t.order.first(), Some(&"start"));
        assert_eq!(t.order.get(1), Some(&"head"));
        assert_eq!(t.order.last(), Some(&"end"));
    });
}

#[test]
fn large_streamed_body_is_teed_without_changing_app_bytes() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let url = format!("http://{addr}/large");
        for (label, coalesce) in [("large-coalesced", true), ("large-raw", false)] {
            let response = scope_network_owner(owner(label), async {
                if coalesce {
                    send_with_small_body_limit(get_request(&url), 256 * 1024, RequestOptions::new())
                        .await
                } else {
                    send_stream(get_request(&url), RequestOptions::new()).await
                }
            })
            .await
            .unwrap();
            assert!(matches!(response.body, HttpBody::Stream(_)));
            let app_bytes = collect_body(response.body).await.unwrap();
            assert_eq!(app_bytes.as_ref(), large_payload().as_slice());

            let traces = installed.recording.ended(1, |r| r.for_owner(label)).await;
            let t = &traces[0];
            assert_eq!(t.body, app_bytes.as_ref(), "{label}");
            assert!(t.chunks > 1, "{label}: expected many wire chunks");
            let end = t.end.as_ref().unwrap();
            assert_eq!(end.outcome, NetworkOutcome::Completed);
            assert_eq!(end.response_bytes, LARGE_LEN as u64);
        }
    });
}

#[test]
fn gzip_body_is_reported_as_wire_bytes() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let response = scope_network_owner(owner("gzip"), async {
            send(
                get_request(&format!("http://{addr}/gzip")),
                RequestOptions::new(),
            )
            .await
        })
        .await
        .unwrap();
        let app_bytes = collect_body(response.body).await.unwrap();
        // rong_rt never decodes; fetch does that above this layer.
        assert_eq!(app_bytes.as_ref(), gzip_payload().as_slice());

        let traces = installed.recording.ended(1, |r| r.for_owner("gzip")).await;
        let t = &traces[0];
        assert_eq!(t.body, gzip_payload());
        assert_eq!(&t.body[..2], &[0x1f, 0x8b]);
        assert_eq!(
            t.head.as_ref().unwrap().headers[header::CONTENT_ENCODING],
            "gzip"
        );
    });
}

#[test]
fn request_body_is_reported() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let payload = br#"{"hello":"observer"}"#;
        let response = scope_network_owner(owner("echo"), async {
            post_json_bytes(
                &format!("http://{addr}/echo"),
                payload,
                None,
                RequestOptions::new(),
            )
            .await
        })
        .await
        .unwrap();
        assert_eq!(response.body.as_ref(), payload);

        let traces = installed.recording.ended(1, |r| r.for_owner("echo")).await;
        let t = &traces[0];
        let start = t.start.as_ref().unwrap();
        assert_eq!(start.method, Method::POST);
        assert_eq!(start.body_size, Some(payload.len() as u64));
        assert_eq!(start.headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(t.request_body, payload);
        assert_eq!(t.body, payload);
    });
}

#[test]
fn connection_refused_reports_failure() {
    let installed = Installed::new();
    run(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let err = scope_network_owner(owner("refused"), async {
            send(
                get_request(&format!("http://{addr}/nothing")),
                RequestOptions::new(),
            )
            .await
        })
        .await
        .err()
        .expect("connection must be refused");

        let traces = installed
            .recording
            .ended(1, |r| r.for_owner("refused"))
            .await;
        let t = &traces[0];
        assert!(t.head.is_none());
        match &t.end.as_ref().unwrap().outcome {
            NetworkOutcome::Failed(message) => assert_eq!(message, err.message()),
            other => panic!("expected failure, got {other:?}"),
        }
    });
}

#[test]
fn abort_mid_stream_reports_aborted() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let (abort_tx, abort_rx) = oneshot::channel();
        let response = scope_network_owner(owner("abort"), async {
            send_stream(
                get_request(&format!("http://{addr}/slow/1000")),
                RequestOptions::new().with_abort(abort_rx),
            )
            .await
        })
        .await
        .unwrap();
        let HttpBody::Stream(mut rx) = response.body else {
            panic!("expected stream");
        };
        let first = rx.recv().await.unwrap().unwrap();
        assert!(first.starts_with(b"tick-0;"));
        abort_tx.send(()).unwrap();
        let mut saw_abort = false;
        while let Some(item) = rx.recv().await {
            if let Err(e) = item {
                assert_eq!(e, "aborted");
                saw_abort = true;
            }
        }
        assert!(saw_abort);

        let traces = installed.recording.ended(1, |r| r.for_owner("abort")).await;
        let end = traces[0].end.as_ref().unwrap();
        assert_eq!(end.outcome, NetworkOutcome::Aborted);
        assert!(end.response_bytes >= first.len() as u64);
    });
}

#[test]
fn dropping_the_response_reports_dropped() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let response = scope_network_owner(owner("dropped"), async {
            send_stream(
                get_request(&format!("http://{addr}/slow/1000")),
                RequestOptions::new(),
            )
            .await
        })
        .await
        .unwrap();
        let HttpBody::Stream(mut rx) = response.body else {
            panic!("expected stream");
        };
        rx.recv().await.unwrap().unwrap();
        drop(rx);

        let traces = installed
            .recording
            .ended(1, |r| r.for_owner("dropped"))
            .await;
        assert_eq!(
            traces[0].end.as_ref().unwrap().outcome,
            NetworkOutcome::Dropped
        );
    });
}

#[test]
fn sse_stream_reports_kind_owner_and_wire_chunks() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let options = crate::sse::SseConnectOptions::new(format!("http://{addr}/sse"))
            .unwrap()
            .with_reconnect(crate::sse::SseReconnectOptions {
                enabled: false,
                ..Default::default()
            });
        // Owner captured synchronously, before the worker hops to the executor.
        let mut conn =
            sync_scope_network_owner(owner("sse"), || crate::sse::connect_sse(options, None))
                .unwrap();
        conn.opened().await.unwrap();
        let first = conn.events.recv().await.unwrap().unwrap();
        let second = conn.events.recv().await.unwrap().unwrap();
        assert_eq!(
            (first.data.as_str(), second.data.as_str()),
            ("hello", "world")
        );

        let traces = installed.recording.ended(1, |r| r.for_owner("sse")).await;
        let t = &traces[0];
        let start = t.start.as_ref().unwrap();
        assert_eq!(start.kind, NetworkRequestKind::Sse);
        assert_eq!(start.headers[header::ACCEPT], "text/event-stream");
        assert_eq!(t.body, SSE_TEXT.as_bytes());
        assert_eq!(t.end.as_ref().unwrap().outcome, NetworkOutcome::Completed);
    });
}

#[test]
fn download_redirect_chain_reports_hops_in_order() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.txt");
        scope_network_owner(owner("download"), async {
            crate::download::download(
                crate::download::DownloadOptions::new(format!("http://{addr}/redirect/2"), &dest),
                None,
            )
            .await
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"hello");

        let traces = installed
            .recording
            .ended(4, |r| r.for_owner("download"))
            .await;
        let paths: Vec<_> = traces
            .iter()
            .map(|t| t.start.as_ref().unwrap().uri.path().to_string())
            .collect();
        assert_eq!(
            paths,
            ["/redirect/2", "/redirect/1", "/redirect/0", "/small"]
        );
        for (i, t) in traces.iter().enumerate() {
            let start = t.start.as_ref().unwrap();
            assert_eq!(start.kind, NetworkRequestKind::Download);
            let previous = (i > 0).then(|| traces[i - 1].start.as_ref().unwrap().id);
            assert_eq!(start.redirected_from, previous);
            if i < 3 {
                let redirect = &t.redirects[..];
                assert_eq!(redirect.len(), 1);
                assert_eq!(redirect[0].status, StatusCode::FOUND);
                assert_eq!(redirect[0].from, start.uri);
                assert_eq!(redirect[0].to, traces[i + 1].start.as_ref().unwrap().uri);
            } else {
                assert!(t.redirects.is_empty());
                assert_eq!(t.body, b"hello");
            }
        }
    });
}

#[test]
fn upload_reports_streamed_request_body() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 7) as u8).collect();
        std::fs::write(&path, &payload).unwrap();

        let response = scope_network_owner(owner("upload"), async {
            crate::upload::upload(
                crate::upload::UploadOptions::new(format!("http://{addr}/upload"), &path),
                None,
            )
            .await
        })
        .await
        .unwrap();
        assert_eq!(response.body, payload.len().to_string());

        let traces = installed
            .recording
            .ended(1, |r| r.for_owner("upload"))
            .await;
        let t = &traces[0];
        assert_eq!(t.start.as_ref().unwrap().kind, NetworkRequestKind::Upload);
        assert_eq!(t.request_body, payload);
    });
}

#[test]
fn concurrent_requests_keep_their_owners() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let tasks: Vec<_> = (0..16)
            .map(|i| {
                let url = format!("http://{addr}/small?n={i}");
                tokio::spawn(scope_network_owner(
                    owner(&format!("concurrent-{i}")),
                    async move {
                        let resp = send(get_request(&url), RequestOptions::new())
                            .await
                            .unwrap();
                        collect_body(resp.body).await.unwrap()
                    },
                ))
            })
            .collect();
        for task in tasks {
            assert_eq!(task.await.unwrap(), "hello");
        }

        let traces = installed
            .recording
            .ended(16, |r| {
                r.for_path("/small")
                    .into_iter()
                    .filter(|t| {
                        t.start
                            .as_ref()
                            .unwrap()
                            .owner
                            .as_ref()
                            .is_some_and(|o| o.as_str().starts_with("concurrent-"))
                    })
                    .collect()
            })
            .await;
        assert_eq!(traces.len(), 16);
        for t in &traces {
            let start = t.start.as_ref().unwrap();
            let n = start.uri.query().unwrap().trim_start_matches("n=");
            assert_eq!(
                start.owner.as_ref().unwrap().as_str(),
                format!("concurrent-{n}")
            );
            assert_eq!(t.body, b"hello");
        }
    });
}

#[test]
fn explicit_tag_overrides_scope_owner() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let mut request = get_request(&format!("http://{addr}/small?tagged"));
        let tag =
            NetworkRequestTag::new(NetworkRequestKind::Fetch).with_owner(Some(owner("tagged")));
        let id = tag.id;
        request.extensions_mut().insert(tag);
        scope_network_owner(owner("scope"), async {
            send(request, RequestOptions::new()).await.unwrap();
        })
        .await;

        let traces = installed
            .recording
            .ended(1, |r| r.for_owner("tagged"))
            .await;
        let start = traces[0].start.as_ref().unwrap();
        assert_eq!(start.id, id);
        assert_eq!(start.kind, NetworkRequestKind::Fetch);
        assert!(installed.recording.for_owner("scope").is_empty());
    });
}

#[test]
fn owner_scopes_nest_and_end() {
    assert_eq!(current_network_owner(), None);
    sync_scope_network_owner(owner("outer"), || {
        assert_eq!(current_network_owner(), Some(owner("outer")));
        sync_scope_network_owner(owner("inner"), || {
            assert_eq!(current_network_owner(), Some(owner("inner")));
        });
        assert_eq!(current_network_owner(), Some(owner("outer")));
    });
    assert_eq!(current_network_owner(), None);
}

#[test]
fn removed_observer_sees_nothing_and_behavior_is_unchanged() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let url = format!("http://{addr}/large?phase=");
        let before = send(get_request(&format!("{url}on")), RequestOptions::new())
            .await
            .unwrap();
        let before = collect_body(before.body).await.unwrap();
        installed
            .recording
            .ended(1, |r| {
                r.for_path("/large")
                    .into_iter()
                    .filter(|t| t.start.as_ref().unwrap().uri.port_u16() == Some(addr.port()))
                    .collect()
            })
            .await;

        set_network_observer(None);
        assert!(!network_observer_installed());
        let after = send(get_request(&format!("{url}off")), RequestOptions::new())
            .await
            .unwrap();
        let after = collect_body(after.body).await.unwrap();
        assert_eq!(before, after);
        assert_eq!(after.as_ref(), large_payload().as_slice());

        let traces: Vec<_> = installed
            .recording
            .for_path("/large")
            .into_iter()
            .filter(|t| {
                let uri = &t.start.as_ref().unwrap().uri;
                uri.port_u16() == Some(addr.port())
            })
            .collect();
        assert_eq!(traces.len(), 1);
        assert_eq!(
            traces[0].start.as_ref().unwrap().uri.query(),
            Some("phase=on")
        );
    });
}

#[test]
fn in_flight_request_keeps_reporting_after_observer_removal() {
    let installed = Installed::new();
    run(async {
        let addr = spawn_server().await;
        let response = scope_network_owner(owner("in-flight"), async {
            send_stream(
                get_request(&format!("http://{addr}/slow/5")),
                RequestOptions::new(),
            )
            .await
        })
        .await
        .unwrap();
        set_network_observer(None);
        let body = collect_body(response.body).await.unwrap();
        assert_eq!(body, "tick-0;tick-1;tick-2;tick-3;tick-4;");

        let traces = installed
            .recording
            .ended(1, |r| r.for_owner("in-flight"))
            .await;
        assert_eq!(traces[0].body, body.as_ref());
        assert_eq!(
            traces[0].end.as_ref().unwrap().outcome,
            NetworkOutcome::Completed
        );
    });
}
