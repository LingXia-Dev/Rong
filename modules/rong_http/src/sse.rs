use rong::function::*;
use rong::*;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

use crate::security::grant_network_access;

fn type_error(message: impl Into<String>) -> RongJSError {
    HostError::new(rong::error::E_TYPE, message)
        .with_name("TypeError")
        .into()
}

type EventReceiver = mpsc::Receiver<Result<rong_rt::sse::SseEvent, String>>;
type OpenedReceiver = oneshot::Receiver<Result<String, String>>;

#[allow(clippy::upper_case_acronyms, clippy::type_complexity)]
#[js_class]
pub struct SSE {
    url: String,
    close_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    _rt_close_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    rx: Arc<Mutex<Option<EventReceiver>>>,
    opened_rx: Arc<Mutex<Option<OpenedReceiver>>>,
    state: Arc<Mutex<SseStreamState>>,
}

#[js_class]
impl SSE {
    #[js_method(constructor)]
    fn new(_ctx: JSContext, url: String, options: Optional<JSObject>) -> JSResult<Self> {
        let parsed =
            url::Url::parse(&url).map_err(|e| type_error(format!("invalid url: {}", e)))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| type_error("url must contain a valid host"))?;
        grant_network_access(host)?;

        let mut headers = Vec::new();
        let mut reconnect_opts = rong_rt::sse::SseReconnectOptions::default();
        let mut request_timeout = None;
        let mut abort_rx: Option<oneshot::Receiver<()>> = None;

        if let Some(opts) = options.0 {
            if opts.has_property("headers")? {
                let value = opts.get::<_, JSValue>("headers")?;
                if !value.is_undefined() && !value.is_null() {
                    let obj = value
                        .into_object()
                        .ok_or_else(|| type_error("options.headers must be an object"))?;
                    headers = obj.entries_as::<String, String>().map_err(|_| {
                        type_error("options.headers must be string key/value pairs")
                    })?;
                }
            }

            if opts.has_property("reconnect")?
                && let Ok(reconnect_obj) = opts.get::<_, JSObject>("reconnect")
            {
                reconnect_opts.enabled = reconnect_obj.get::<_, bool>("enabled").unwrap_or(true);
                if let Ok(v) = reconnect_obj.get::<_, f64>("baseDelayMs")
                    && v.is_finite()
                    && v >= 0.0
                {
                    reconnect_opts.base_delay = std::time::Duration::from_millis((v as u64).max(1));
                }
                if let Ok(v) = reconnect_obj.get::<_, f64>("maxDelayMs")
                    && v.is_finite()
                    && v >= 1.0
                {
                    reconnect_opts.max_delay = std::time::Duration::from_millis(v as u64);
                }
                reconnect_opts.max_retries = reconnect_obj
                    .get::<_, f64>("maxRetries")
                    .ok()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .map(|v| v as u32);
            }

            if let Ok(v) = opts.get::<_, f64>("requestTimeoutMs")
                && v.is_finite()
                && v > 0.0
            {
                request_timeout = Some(std::time::Duration::from_millis(v as u64));
            }

            // AbortSignal support
            if opts.has_property("signal")? {
                let signal_val = opts.get::<_, JSValue>("signal")?;
                if !signal_val.is_undefined() && !signal_val.is_null() {
                    let signal_obj = signal_val
                        .into_object()
                        .ok_or_else(|| type_error("options.signal must be an AbortSignal"))?;
                    let signal = signal_obj
                        .borrow::<rong_abort::AbortSignal>()
                        .map_err(|_| type_error("options.signal must be an AbortSignal"))?;
                    let mut receiver = signal.subscribe();
                    let (tx, rx) = oneshot::channel::<()>();
                    abort_rx = Some(rx);
                    tokio::task::spawn_local(async move {
                        receiver.recv().await;
                        let _ = tx.send(());
                    });
                }
            }
        }

        let mut rt_options = rong_rt::sse::SseConnectOptions::new(&url)
            .map_err(|e| type_error(format!("invalid url: {}", e)))?
            .with_reconnect(reconnect_opts);
        if let Some(request_timeout) = request_timeout {
            rt_options = rt_options.with_request_timeout(request_timeout);
        }
        for (name, value) in headers {
            rt_options = rt_options.with_header(name, value);
        }

        let (close_tx, close_rx) = oneshot::channel::<()>();

        // Merge abort_rx and close_rx into a single signal for rong_rt
        let merged_abort_rx = if let Some(abort_rx) = abort_rx {
            let (merged_tx, merged_rx) = oneshot::channel::<()>();
            tokio::task::spawn_local(async move {
                tokio::select! {
                    _ = close_rx => {}
                    _ = abort_rx => {}
                }
                let _ = merged_tx.send(());
            });
            Some(merged_rx)
        } else {
            Some(close_rx)
        };

        let rt_conn = rong_rt::sse::connect_sse(rt_options, merged_abort_rx).map_err(|e| {
            HostError::new(rong::error::E_IO, format!("failed to connect SSE: {}", e))
                .with_name("TypeError")
        })?;
        let (rx, rt_close_tx, opened_rx) = rt_conn.into_parts_with_open();

        Ok(Self {
            url,
            close_tx: Arc::new(Mutex::new(Some(close_tx))),
            _rt_close_tx: Arc::new(Mutex::new(rt_close_tx)),
            opened_rx: Arc::new(Mutex::new(Some(opened_rx))),
            rx: Arc::new(Mutex::new(Some(rx))),
            state: Arc::new(Mutex::new(SseStreamState::Opening)),
        })
    }

    #[js_method]
    fn close(&self) {
        if let Ok(mut guard) = self.close_tx.lock()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(());
        }
        if let Ok(mut guard) = self._rt_close_tx.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.rx.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.opened_rx.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.state.lock() {
            *guard = SseStreamState::Done;
        }
    }

    #[js_method(getter)]
    fn url(&self) -> String {
        self.url.clone()
    }

    #[js_method]
    async fn next(&self, ctx: JSContext) -> JSResult<JSObject> {
        loop {
            let opening = {
                let guard = self.state.lock().map_err(|_| {
                    HostError::new(rong::error::E_INTERNAL, "SSE state is poisoned")
                })?;
                matches!(&*guard, SseStreamState::Opening)
            };

            if opening {
                let opened_rx = self.opened_rx.lock().ok().and_then(|mut g| g.take());
                let Some(opened_rx) = opened_rx else {
                    if let Ok(mut guard) = self.state.lock() {
                        *guard = SseStreamState::Done;
                    }
                    return Self::done_result(&ctx);
                };

                match opened_rx.await {
                    Ok(Ok(origin)) => {
                        if let Ok(mut guard) = self.state.lock() {
                            *guard = SseStreamState::Streaming { origin };
                        }
                        continue;
                    }
                    Ok(Err(message)) => {
                        if let Ok(mut guard) = self.state.lock() {
                            *guard = SseStreamState::Done;
                        }
                        return Err(HostError::new(rong::error::E_IO, message).into());
                    }
                    Err(_) => {
                        if let Ok(mut guard) = self.state.lock() {
                            *guard = SseStreamState::Done;
                        }
                        return Err(
                            HostError::new(rong::error::E_IO, "SSE connection failed").into()
                        );
                    }
                }
            }

            let origin = {
                let guard = self.state.lock().map_err(|_| {
                    HostError::new(rong::error::E_INTERNAL, "SSE state is poisoned")
                })?;
                match &*guard {
                    SseStreamState::Streaming { origin } => origin.clone(),
                    SseStreamState::Done => return Self::done_result(&ctx),
                    SseStreamState::Opening => continue,
                }
            };

            let mut rx = {
                let mut guard = self.rx.lock().map_err(|_| {
                    HostError::new(rong::error::E_INTERNAL, "SSE stream is poisoned")
                })?;
                guard.take()
            };

            let Some(mut rx) = rx.take() else {
                if let Ok(mut guard) = self.state.lock() {
                    *guard = SseStreamState::Done;
                }
                return Self::done_result(&ctx);
            };

            match rx.recv().await {
                Some(Ok(evt)) => {
                    if let Ok(mut guard) = self.rx.lock()
                        && guard.is_none()
                    {
                        *guard = Some(rx);
                    }
                    return Self::value_result(&ctx, &origin, evt);
                }
                Some(Err(message)) => {
                    self.close();
                    return Err(HostError::new(rong::error::E_IO, message).into());
                }
                None => {
                    self.close();
                    return Self::done_result(&ctx);
                }
            }
        }
    }

    #[js_method(rename = "return")]
    async fn r#return(&self, ctx: JSContext) -> JSResult<JSObject> {
        self.close();
        Self::done_result(&ctx)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

enum SseStreamState {
    Opening,
    Streaming { origin: String },
    Done,
}

impl SSE {
    fn done_result(ctx: &JSContext) -> JSResult<JSObject> {
        let obj = JSObject::new(ctx);
        obj.set("done", true)?;
        obj.set("value", JSValue::undefined(ctx))?;
        Ok(obj)
    }

    fn value_result(
        ctx: &JSContext,
        origin: &str,
        evt: rong_rt::sse::SseEvent,
    ) -> JSResult<JSObject> {
        let obj = JSObject::new(ctx);
        obj.set("done", false)?;
        let value = JSObject::new(ctx);
        value.set("type", evt.event.as_str())?;
        value.set("data", evt.data.as_str())?;
        value.set("id", evt.id.as_deref().unwrap_or(""))?;
        value.set("origin", origin)?;
        obj.set("value", value)?;
        Ok(obj)
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_hidden_class::<SSE>()?;
    let ctor = Class::lookup::<SSE>(ctx)?.clone();
    let proto = Class::prototype::<SSE>(ctx)?;
    rong::install_async_iterator_symbol(ctx, &proto)?;
    ctx.host_namespace().set("SSE", ctor)?;
    Ok(())
}
