//! Process-wide network observer (`network-observer` feature).
//!
//! Every request sent through `rong_rt`'s shared client (`http::send*`, SSE,
//! download, upload, and `fetch` in `rong_http`) reports its lifecycle to the
//! installed [`NetworkObserver`]. Without the feature, the hooks compile to
//! nothing; with it but no observer installed, each request pays one atomic load.

#[cfg(feature = "network-observer")]
pub use enabled::*;

#[cfg(feature = "network-observer")]
pub(crate) use enabled::{HopId, OwnerCapture, Recorder, redirect, tag_request};

#[cfg(not(feature = "network-observer"))]
pub(crate) use disabled::{HopId, OwnerCapture, Recorder, redirect, tag_request};

/// Which runtime API issued a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[cfg_attr(not(feature = "network-observer"), allow(dead_code))]
pub enum NetworkRequestKind {
    Fetch,
    Sse,
    Download,
    Upload,
    Other,
}

#[cfg(feature = "network-observer")]
mod enabled {
    use super::NetworkRequestKind;
    use bytes::Bytes;
    use http::{HeaderMap, Method, StatusCode, Uri, Version};
    use http_body::{Body, Frame, SizeHint};
    use http_body_util::{BodyExt, combinators::BoxBody};
    use std::fmt;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, RwLock};
    use std::task::{Context, Poll};
    use std::time::{Duration, Instant};

    tokio::task_local! {
        static NETWORK_OWNER: NetworkOwner;
    }

    static INSTALLED: AtomicBool = AtomicBool::new(false);
    static OBSERVER: RwLock<Option<Arc<dyn NetworkObserver>>> = RwLock::new(None);
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    /// Receives network events from every request in the process.
    ///
    /// Callbacks run inline on the thread that drives the request (usually the
    /// runtime's I/O executor) and must be cheap and non-blocking: hand the
    /// event to a channel with `try_send` and do the work elsewhere.
    pub trait NetworkObserver: Send + Sync + 'static {
        fn on_event(&self, event: NetworkEvent);
    }

    /// Install (`Some`) or remove (`None`) the process-wide observer.
    ///
    /// A request keeps reporting to the observer that saw it start, so its
    /// events stay paired even if the observer is swapped mid-flight.
    pub fn set_network_observer(observer: Option<Arc<dyn NetworkObserver>>) {
        let mut slot = OBSERVER.write().unwrap_or_else(|e| e.into_inner());
        INSTALLED.store(observer.is_some(), Ordering::Release);
        *slot = observer;
    }

    /// Whether an observer is currently installed.
    #[inline]
    pub fn network_observer_installed() -> bool {
        INSTALLED.load(Ordering::Acquire)
    }

    fn current_observer() -> Option<Arc<dyn NetworkObserver>> {
        if !network_observer_installed() {
            return None;
        }
        OBSERVER
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .cloned()
    }

    /// Process-unique request id.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct NetworkRequestId(u64);

    impl NetworkRequestId {
        pub fn next() -> Self {
            Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
        }

        pub fn as_u64(self) -> u64 {
            self.0
        }
    }

    /// Opaque attribution label (an app, a context, a subsystem). Cheap to clone.
    #[derive(Clone, PartialEq, Eq, Hash)]
    pub struct NetworkOwner(Arc<str>);

    impl NetworkOwner {
        pub fn new(owner: impl Into<Arc<str>>) -> Self {
            Self(owner.into())
        }

        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    impl From<&str> for NetworkOwner {
        fn from(value: &str) -> Self {
            Self::new(value)
        }
    }

    impl From<String> for NetworkOwner {
        fn from(value: String) -> Self {
            Self::new(value)
        }
    }

    impl fmt::Debug for NetworkOwner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Debug::fmt(&*self.0, f)
        }
    }

    impl fmt::Display for NetworkOwner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    /// Attribute requests sent from `future` (and tasks it starts through
    /// `rong_rt`) to `owner`.
    pub async fn scope_network_owner<F, T>(owner: NetworkOwner, future: F) -> T
    where
        F: Future<Output = T>,
    {
        NETWORK_OWNER.scope(owner, future).await
    }

    /// Synchronous form of [`scope_network_owner`], for APIs such as
    /// `sse::connect_sse` that capture the owner when called.
    pub fn sync_scope_network_owner<F, T>(owner: NetworkOwner, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        NETWORK_OWNER.sync_scope(owner, f)
    }

    /// The owner set by the innermost enclosing scope, if any.
    pub fn current_network_owner() -> Option<NetworkOwner> {
        NETWORK_OWNER.try_with(Clone::clone).ok()
    }

    /// Per-request attribution, carried in `http::Request::extensions()`.
    ///
    /// Requests without one get a fresh id, kind `Other`, and the owner of the
    /// scope that sends them.
    #[derive(Clone, Debug)]
    pub struct NetworkRequestTag {
        pub id: NetworkRequestId,
        pub owner: Option<NetworkOwner>,
        pub kind: NetworkRequestKind,
        /// The previous hop when this request follows a redirect.
        pub redirected_from: Option<NetworkRequestId>,
    }

    impl NetworkRequestTag {
        /// New tag with a fresh id and the current scope's owner.
        pub fn new(kind: NetworkRequestKind) -> Self {
            Self {
                id: NetworkRequestId::next(),
                owner: current_network_owner(),
                kind,
                redirected_from: None,
            }
        }

        pub fn with_owner(mut self, owner: Option<NetworkOwner>) -> Self {
            self.owner = owner;
            self
        }

        pub fn with_redirected_from(mut self, from: Option<NetworkRequestId>) -> Self {
            self.redirected_from = from;
            self
        }
    }

    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub struct NetworkRequestStart {
        pub id: NetworkRequestId,
        pub owner: Option<NetworkOwner>,
        pub kind: NetworkRequestKind,
        pub redirected_from: Option<NetworkRequestId>,
        pub method: Method,
        pub uri: Uri,
        pub headers: HeaderMap,
        /// Exact request body length, or `None` for a streamed body.
        pub body_size: Option<u64>,
        pub started_at: Instant,
    }

    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub struct NetworkResponseHead {
        pub id: NetworkRequestId,
        pub status: StatusCode,
        pub version: Version,
        pub headers: HeaderMap,
        pub time_to_first_byte: Duration,
    }

    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub struct NetworkRedirect {
        /// The hop that answered with the redirect.
        pub id: NetworkRequestId,
        pub status: StatusCode,
        pub from: Uri,
        pub to: Uri,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum NetworkOutcome {
        /// The response body was read to its end.
        Completed,
        Failed(String),
        /// The caller's abort signal fired.
        Aborted,
        /// The caller dropped the response before its end.
        Dropped,
    }

    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub struct NetworkRequestEnd {
        pub id: NetworkRequestId,
        pub outcome: NetworkOutcome,
        /// Response body bytes received, as they came off the connection
        /// (before any content decoding).
        pub response_bytes: u64,
        pub duration: Duration,
    }

    #[derive(Clone, Debug)]
    #[non_exhaustive]
    pub enum NetworkEvent {
        RequestStart(NetworkRequestStart),
        /// Request body bytes as the connection pulls them.
        RequestBodyChunk {
            id: NetworkRequestId,
            chunk: Bytes,
        },
        ResponseHead(NetworkResponseHead),
        /// Response body bytes as the connection delivers them, before the
        /// caller sees them and before any content decoding.
        ResponseBodyChunk {
            id: NetworkRequestId,
            chunk: Bytes,
        },
        Redirect(NetworkRedirect),
        /// Always the last event for a started request.
        RequestEnd(NetworkRequestEnd),
    }

    /// Report a followed redirect. Callers that run their own redirect loop
    /// (such as `fetch`) emit this between hops.
    pub fn notify_network_redirect(id: NetworkRequestId, status: StatusCode, from: &Uri, to: &Uri) {
        if let Some(observer) = current_observer() {
            observer.on_event(NetworkEvent::Redirect(NetworkRedirect {
                id,
                status,
                from: from.clone(),
                to: to.clone(),
            }));
        }
    }

    /// Hop id threaded through crate-internal redirect loops.
    #[derive(Clone, Copy)]
    pub(crate) struct HopId(Option<NetworkRequestId>);

    impl HopId {
        /// A request that does not follow a redirect.
        pub(crate) const FIRST: Self = Self(None);
    }

    pub(crate) fn tag_request<B>(
        request: &mut http::Request<B>,
        kind: NetworkRequestKind,
        redirected_from: HopId,
    ) -> HopId {
        if !network_observer_installed() {
            return HopId::FIRST;
        }
        let tag = NetworkRequestTag::new(kind).with_redirected_from(redirected_from.0);
        let id = tag.id;
        request.extensions_mut().insert(tag);
        HopId(Some(id))
    }

    pub(crate) fn redirect(hop: HopId, status: StatusCode, from: &str, to: &str) {
        let Some(id) = hop.0 else { return };
        if let (Ok(from), Ok(to)) = (from.parse::<Uri>(), to.parse::<Uri>()) {
            notify_network_redirect(id, status, &from, &to);
        }
    }

    /// Owner captured on the calling side, re-entered on the executor.
    pub(crate) struct OwnerCapture(Option<NetworkOwner>);

    impl OwnerCapture {
        pub(crate) fn capture() -> Self {
            Self(current_network_owner())
        }

        pub(crate) async fn scope<F: Future>(self, future: F) -> F::Output {
            match self.0 {
                Some(owner) => scope_network_owner(owner, future).await,
                None => future.await,
            }
        }
    }

    struct Active {
        observer: Arc<dyn NetworkObserver>,
        id: NetworkRequestId,
        started: Instant,
        response_bytes: u64,
    }

    /// Lifecycle reporter for one request; emits `RequestEnd` exactly once.
    pub(crate) struct Recorder(Option<Active>);

    impl Recorder {
        /// Emits `RequestStart` and tees the request body. Must run on the
        /// calling side so the scope owner is still visible.
        pub(crate) fn start(request: &mut http::Request<BoxBody<Bytes, std::io::Error>>) -> Self {
            let Some(observer) = current_observer() else {
                return Self(None);
            };
            let tag = request
                .extensions()
                .get::<NetworkRequestTag>()
                .cloned()
                .unwrap_or_else(|| NetworkRequestTag::new(NetworkRequestKind::Other));
            let started = Instant::now();
            let id = tag.id;
            observer.on_event(NetworkEvent::RequestStart(NetworkRequestStart {
                id,
                owner: tag.owner,
                kind: tag.kind,
                redirected_from: tag.redirected_from,
                method: request.method().clone(),
                uri: request.uri().clone(),
                headers: request.headers().clone(),
                body_size: request.body().size_hint().exact(),
                started_at: started,
            }));
            let body = std::mem::replace(
                request.body_mut(),
                http_body_util::Empty::new().map_err(|e| match e {}).boxed(),
            );
            *request.body_mut() = ObservedBody {
                inner: body,
                observer: observer.clone(),
                id,
            }
            .boxed();
            Self(Some(Active {
                observer,
                id,
                started,
                response_bytes: 0,
            }))
        }

        pub(crate) fn response_head(&self, parts: &http::response::Parts) {
            if let Some(active) = &self.0 {
                active
                    .observer
                    .on_event(NetworkEvent::ResponseHead(NetworkResponseHead {
                        id: active.id,
                        status: parts.status,
                        version: parts.version,
                        headers: parts.headers.clone(),
                        time_to_first_byte: active.started.elapsed(),
                    }));
            }
        }

        pub(crate) fn chunk(&mut self, chunk: &Bytes) {
            if let Some(active) = &mut self.0 {
                active.response_bytes += chunk.len() as u64;
                active.observer.on_event(NetworkEvent::ResponseBodyChunk {
                    id: active.id,
                    chunk: chunk.clone(),
                });
            }
        }

        pub(crate) fn complete(&mut self) {
            self.end(NetworkOutcome::Completed);
        }

        pub(crate) fn fail(&mut self, message: &str) {
            self.end(NetworkOutcome::Failed(message.to_string()));
        }

        pub(crate) fn abort(&mut self) {
            self.end(NetworkOutcome::Aborted);
        }

        fn end(&mut self, outcome: NetworkOutcome) {
            if let Some(active) = self.0.take() {
                active
                    .observer
                    .on_event(NetworkEvent::RequestEnd(NetworkRequestEnd {
                        id: active.id,
                        outcome,
                        response_bytes: active.response_bytes,
                        duration: active.started.elapsed(),
                    }));
            }
        }
    }

    impl Drop for Recorder {
        fn drop(&mut self) {
            self.end(NetworkOutcome::Dropped);
        }
    }

    struct ObservedBody {
        inner: BoxBody<Bytes, std::io::Error>,
        observer: Arc<dyn NetworkObserver>,
        id: NetworkRequestId,
    }

    impl Body for ObservedBody {
        type Data = Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
            let poll = Pin::new(&mut self.inner).poll_frame(cx);
            if let Poll::Ready(Some(Ok(frame))) = &poll
                && let Some(data) = frame.data_ref()
                && !data.is_empty()
            {
                self.observer.on_event(NetworkEvent::RequestBodyChunk {
                    id: self.id,
                    chunk: data.clone(),
                });
            }
            poll
        }

        fn is_end_stream(&self) -> bool {
            self.inner.is_end_stream()
        }

        fn size_hint(&self) -> SizeHint {
            self.inner.size_hint()
        }
    }
}

#[cfg(not(feature = "network-observer"))]
mod disabled {
    use super::NetworkRequestKind;
    use bytes::Bytes;
    use http_body_util::combinators::BoxBody;
    use std::future::Future;

    #[derive(Clone, Copy)]
    pub(crate) struct HopId;

    impl HopId {
        pub(crate) const FIRST: Self = Self;
    }

    #[inline(always)]
    pub(crate) fn tag_request<B>(
        _request: &mut http::Request<B>,
        _kind: NetworkRequestKind,
        _redirected_from: HopId,
    ) -> HopId {
        HopId
    }

    #[inline(always)]
    pub(crate) fn redirect(_hop: HopId, _status: http::StatusCode, _from: &str, _to: &str) {}

    pub(crate) struct OwnerCapture;

    impl OwnerCapture {
        #[inline(always)]
        pub(crate) fn capture() -> Self {
            Self
        }

        #[inline(always)]
        pub(crate) async fn scope<F: Future>(self, future: F) -> F::Output {
            future.await
        }
    }

    pub(crate) struct Recorder;

    impl Recorder {
        #[inline(always)]
        pub(crate) fn start(_request: &mut http::Request<BoxBody<Bytes, std::io::Error>>) -> Self {
            Self
        }
        #[inline(always)]
        pub(crate) fn response_head(&self, _parts: &http::response::Parts) {}
        #[inline(always)]
        pub(crate) fn chunk(&mut self, _chunk: &Bytes) {}
        #[inline(always)]
        pub(crate) fn complete(&mut self) {}
        #[inline(always)]
        pub(crate) fn fail(&mut self, _message: &str) {}
        #[inline(always)]
        pub(crate) fn abort(&mut self) {}
    }
}

#[cfg(all(test, feature = "network-observer"))]
mod tests;
