use bytes::Bytes;
use http::{Method, Uri, header};

use rong::{function::Optional, *};

use crate::body::{BodyKind, HostBody, HostBodyStream, HttpBody};
use crate::formdata::FormData;
use crate::header::Headers;
use rong_abort::AbortReceiver;
use rong_buffer::Blob;
use rong_stream::{
    JSReadableStream, ReadableStream, readable_stream_is_locked, readable_stream_take_receiver,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct ResponseParts {
    pub url: String,
    pub method: String,
    pub status: u16,
    pub headers: http::HeaderMap<http::header::HeaderValue>,
    pub body: HostBody,
}

#[derive(Default)]
#[js_class]
pub struct Response {
    url: Uri,
    method: Method,
    headers: Headers,
    status: u16,
    status_text: String,
    body: Option<BodyKind>,
    // JS method bindings clone the Rust struct; share state so bodyUsed is consistent.
    consumed: Rc<Cell<bool>>,
    redirected: bool,
    type_: String,
    abort_receiver: Option<AbortReceiver>,
    // Cache a JS ReadableStream instance so repeated Response.body access
    // returns the same object and doesn't have side effects.
    body_stream: Rc<RefCell<Option<JSObject>>>,
}

#[derive(FromJSValue)]
struct InitOption {
    status: Option<u16>,
    status_text: Option<String>,
    headers: Option<Headers>,
}

impl TryFromJSValue for InitOption {
    fn try_from_js(value: JSValue) -> JSResult<Self> {
        let obj = value.into_object().ok_or_else(|| {
            HostError::new(rong::error::E_INVALID_ARG, "Invalid Response Option")
                .with_name("TypeError")
        })?;

        let status = obj.get::<_, u16>("status").ok();
        let status_text = obj.get::<_, String>("statusText").ok();
        let headers = obj
            .get::<_, JSValue>("headers")
            .map(|v| Headers::new(Optional(Some(v))).ok());

        Ok(Self {
            status,
            status_text,
            headers: headers.unwrap_or(None),
        })
    }
}

#[js_class]
impl Response {
    #[js_method(constructor)]
    fn new(body: Optional<JSValue>, init: Optional<InitOption>) -> JSResult<Self> {
        let mut response = Self {
            status: 200,
            status_text: "".to_string(),
            consumed: Rc::new(Cell::new(false)),
            type_: "default".to_string(),
            ..Default::default()
        };

        if let Some(body) = body.0 {
            response.body = Some(BodyKind::JS(HttpBody(body)));
        }

        if let Some(init) = init.0 {
            if let Some(status) = init.status {
                // Validate status code
                if !(200..=599).contains(&status) {
                    return Err(HostError::new(
                        rong::error::E_OUT_OF_RANGE,
                        format!("Invalid status code: {}", status),
                    )
                    .with_name("RangeError")
                    .into());
                }
                response.status = status;
            }

            if let Some(text) = init.status_text {
                if text.contains(['\r', '\n']) {
                    return Err(HostError::new(
                        rong::error::E_INVALID_ARG,
                        "statusText must not contain CR or LF characters",
                    )
                    .with_name("TypeError")
                    .into());
                }
                response.status_text = text;
            }

            if let Some(headers) = init.headers {
                response.headers = headers;
            }
        }

        let has_body = response.body.as_ref().is_some_and(|body| match body {
            BodyKind::JS(body) => !body.0.is_null() && !body.0.is_undefined(),
            BodyKind::Buffered(bytes) => !bytes.is_empty(),
            BodyKind::Channel(_) => true,
        });
        if has_body && matches!(response.status, 204 | 205 | 304) {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Response with status {} cannot have body", response.status),
            )
            .with_name("TypeError")
            .into());
        }
        if !has_body {
            response.body = None;
        }

        Ok(response)
    }

    #[js_method(getter)]
    fn ok(&self) -> bool {
        (200..=299).contains(&self.status)
    }

    #[js_method(getter)]
    fn status(&self) -> u16 {
        self.status
    }

    #[js_method(getter, rename = "statusText")]
    fn status_text(&self) -> String {
        self.status_text.clone()
    }

    #[js_method(getter)]
    fn headers(&self) -> Headers {
        self.headers.clone()
    }

    #[js_method(getter)]
    fn redirected(&self) -> bool {
        self.redirected
    }

    #[js_method(getter)]
    fn url(&self) -> String {
        self.url.to_string()
    }

    #[js_method(getter, rename = "bodyUsed")]
    fn body_used(&self) -> bool {
        if self.consumed.get() {
            return true;
        }
        // If we have materialized a ReadableStream, check whether it is locked
        if let Some(obj) = self.body_stream.borrow().as_ref()
            && let Ok(rs) = obj.borrow::<rong_stream::ReadableStream>()
        {
            return readable_stream_is_locked(&rs);
        }
        // Fallback: before materialization, channel not taken means not used
        match &self.body {
            Some(BodyKind::Channel(inner)) => inner.is_consumed().unwrap_or(true),
            _ => false,
        }
    }

    #[js_method(getter, rename = "type")]
    fn type_(&self) -> String {
        self.type_.clone()
    }

    fn has_streaming_body(&self) -> bool {
        if matches!(self.body, Some(BodyKind::Channel(_))) {
            return true;
        }
        match &self.body {
            Some(BodyKind::JS(body)) => body
                .0
                .clone()
                .into_object()
                .is_some_and(|obj| obj.borrow::<ReadableStream>().is_ok()),
            _ => false,
        }
    }

    fn clone_body_kind(&self) -> Option<BodyKind> {
        match &self.body {
            Some(BodyKind::Buffered(bytes)) => Some(BodyKind::Buffered(bytes.clone())),
            Some(BodyKind::JS(body)) => Some(BodyKind::JS(body.clone())),
            Some(BodyKind::Channel(_)) | None => None,
        }
    }

    #[js_method]
    fn clone(&self) -> JSResult<Self> {
        if self.body_used() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "Cannot clone a Response whose body is already used",
            )
            .with_name("TypeError")
            .into());
        }
        if self.has_streaming_body() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "Response.clone() does not support streaming bodies; tee the stream before cloning",
            )
            .with_name("TypeError")
            .into());
        }

        Ok(Self {
            url: self.url.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
            status: self.status,
            status_text: self.status_text.clone(),
            body: self.clone_body_kind(),
            consumed: Rc::new(Cell::new(self.consumed.get())),
            redirected: self.redirected,
            type_: self.type_.clone(),
            abort_receiver: self.abort_receiver.clone(),
            body_stream: Rc::new(RefCell::new(None)),
        })
    }

    #[js_method(getter)]
    fn body(&self, ctx: JSContext) -> Option<JSObject> {
        // Return cached stream if we already created one
        if let Some(obj) = self.body_stream.borrow().as_ref() {
            return Some(obj.clone());
        }

        // Create and cache a stream based on the current body kind
        match &self.body {
            Some(BodyKind::Channel(inner)) => {
                // Do not consume the receiver on property access; build a stream from the shared slot
                if inner.is_consumed().unwrap_or(true) {
                    return None;
                }
                if let Ok(jsrs) = JSReadableStream::from_shared_receiver(&ctx, inner.shared_slot())
                {
                    let obj = jsrs.into_object();
                    self.body_stream.replace(Some(obj.clone()));
                    Some(obj)
                } else {
                    None
                }
            }
            Some(BodyKind::Buffered(b)) => {
                let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(1);
                let bytes = b.clone();
                rong::spawn_local(async move {
                    let _ = tx.send(Ok(bytes)).await;
                });
                if let Ok(jsrs) = JSReadableStream::from_receiver(&ctx, rx) {
                    let obj = jsrs.into_object();
                    self.body_stream.replace(Some(obj.clone()));
                    Some(obj)
                } else {
                    None
                }
            }
            Some(BodyKind::JS(body)) => {
                // Materialize JS body into a one-shot stream
                let (tx, rx) = mpsc::channel::<Result<Bytes, String>>(1);
                let body_clone = body.clone();
                rong::spawn_local(async move {
                    match body_clone.bytes().await {
                        Ok(bytes) => {
                            let _ = tx.send(Ok(bytes)).await;
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!("{}", e))).await;
                        }
                    }
                });
                if let Ok(jsrs) = JSReadableStream::from_receiver(&ctx, rx) {
                    let obj = jsrs.into_object();
                    self.body_stream.replace(Some(obj.clone()));
                    Some(obj)
                } else {
                    None
                }
            }
            None => None,
        }
    }

    async fn body_to_bytes_parts(
        body: Option<&BodyKind>,
        headers: http::HeaderMap,
        body_stream: Option<JSObject>,
        abort_receiver: Option<AbortReceiver>,
    ) -> JSResult<Bytes> {
        match body {
            Some(BodyKind::JS(body)) => body.bytes().await,
            Some(BodyKind::Buffered(bytes)) => {
                crate::body::decompress_bytes(bytes.clone(), &headers)
            }
            Some(BodyKind::Channel(inner)) => {
                let mut collected = Vec::new();
                // Pre-reserve capacity using Content-Length when available
                if let Some(cl_val) = headers
                    .get(header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    collected.reserve(cl_val);
                }
                // Try to take receiver from cached ReadableStream first (if one exists)
                let mut rx_opt = if let Some(obj) = body_stream.as_ref() {
                    if let Ok(rs) = obj.borrow::<rong_stream::ReadableStream>() {
                        rong_stream::readable_stream_take_receiver(&rs)
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Fallback to internal channel if stream is not materialized
                if rx_opt.is_none() {
                    rx_opt = inner
                        .try_take_receiver()
                        .map_err(|error| HostError::new(rong::error::E_INTERNAL, error))?;
                }

                if let Some(mut rx) = rx_opt {
                    let mut abort_receiver = abort_receiver;
                    if let Some(receiver) = &mut abort_receiver {
                        loop {
                            tokio::select! {
                                biased;
                                abort_reason = receiver.recv() => {
                                    return Err(RongJSError::from_thrown_value(abort_reason));
                                }
                                chunk = rx.recv() => {
                                    match chunk {
                                        Some(Ok(bytes)) => collected.extend_from_slice(&bytes),
                                        Some(Err(e)) => {
                                            return Err(HostError::new(rong::error::E_IO, e).into());
                                        }
                                        None => break,
                                    }
                                }
                            }
                        }
                    } else {
                        while let Some(item) = rx.recv().await {
                            match item {
                                Ok(bytes) => collected.extend_from_slice(&bytes),
                                Err(e) => {
                                    return Err(HostError::new(rong::error::E_IO, e).into());
                                }
                            }
                        }
                    }
                    crate::body::decompress_bytes(Bytes::from(collected), &headers)
                } else {
                    Ok(Bytes::new())
                }
            }

            None => Ok(Bytes::new()),
        }
    }

    async fn body_to_bytes(&self) -> JSResult<Bytes> {
        let body_stream = {
            let body_stream = self.body_stream.borrow();
            body_stream.as_ref().cloned()
        };
        Self::body_to_bytes_parts(
            self.body.as_ref(),
            self.headers.as_header_map().clone(),
            body_stream,
            self.abort_receiver.clone(),
        )
        .await
    }

    #[js_method]
    async fn text(&self) -> JSResult<String> {
        if self.body_used() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "body used already for: text",
            )
            .with_name("TypeError")
            .into());
        }
        self.consumed.set(true);
        let bytes = self.body_to_bytes().await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    #[js_method]
    async fn json(&self, ctx: JSContext) -> JSResult<JSValue> {
        if self.body_used() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "body used already for: json",
            )
            .with_name("TypeError")
            .into());
        }
        self.consumed.set(true);
        let bytes = self.body_to_bytes().await?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        text.as_str().json_to_js_value(&ctx)
    }

    #[js_method]
    async fn blob(&self) -> JSResult<Blob> {
        if self.body_used() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "body used already for: blob",
            )
            .with_name("TypeError")
            .into());
        }
        self.consumed.set(true);
        let bytes = self.body_to_bytes().await?;
        let mime = self
            .headers
            .get("Content-Type".to_string())?
            .unwrap_or_else(|| "".to_string());
        Ok(Blob::from_parts(mime, bytes))
    }

    #[js_method(rename = "arrayBuffer")]
    async fn array_buffer(&self, ctx: JSContext) -> JSResult<JSArrayBuffer> {
        if self.body_used() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "body used already for: arrayBuffer",
            )
            .with_name("TypeError")
            .into());
        }
        self.consumed.set(true);
        let bytes = self.body_to_bytes().await?;
        JSArrayBuffer::from_bytes(&ctx, &bytes)
    }

    #[js_method(rename = "formData")]
    async fn form_data(&self, ctx: JSContext) -> JSResult<JSObject> {
        if self.body_used() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "body used already for: formData",
            )
            .with_name("TypeError")
            .into());
        }
        self.consumed.set(true);

        let bytes = self.body_to_bytes().await?;
        let content_type = self
            .headers
            .get("Content-Type".to_string())?
            .unwrap_or_default();
        let form = FormData::from_bytes(&bytes, &content_type)?;
        Ok(Class::lookup::<FormData>(&ctx)?.instance(form))
    }

    #[js_method]
    fn error() -> Self {
        Self {
            status: 0,
            status_text: String::new(),
            type_: "error".to_string(),
            ..Default::default()
        }
    }

    #[js_method]
    fn redirect(url: String, status: Optional<u16>) -> JSResult<Self> {
        let status = status.0.unwrap_or(302);

        // Validate redirect status
        if !matches!(status, 301 | 302 | 303 | 307 | 308) {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Invalid redirect status: {}", status),
            )
            .with_name("TypeError")
            .into());
        }

        url::Url::parse(&url).map_err(|_| {
            HostError::new(rong::error::E_INVALID_ARG, format!("Invalid URL: {}", url))
                .with_name("TypeError")
        })?;
        let uri = Uri::try_from(url.as_str()).map_err(|_| {
            HostError::new(rong::error::E_INVALID_ARG, format!("Invalid URL: {}", url))
                .with_name("TypeError")
        })?;

        let mut headers = Headers::default();
        headers.set("Location".to_string(), url)?;

        Ok(Self {
            url: uri,
            status,
            headers,
            redirected: true,
            type_: "default".to_string(),
            ..Default::default()
        })
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        // Mark any JS values reachable from Response so the GC keeps them alive
        // - BodyKind::JS holds an HttpBody which wraps a JSValue
        // - abort_receiver may hold a JSValue reason inside the watch channel
        // - Cached body_stream JSObject if created
        let mut mark_fn = mark_fn;
        if let Some(BodyKind::JS(js_body)) = &self.body {
            mark_fn(&js_body.0);
        }

        if let Some(receiver) = &self.abort_receiver {
            receiver.gc_mark_with(|v| mark_fn(v));
        }

        if let Some(obj) = self.body_stream.borrow().as_ref() {
            mark_fn(obj);
        }
    }
}

impl Response {
    fn from_response_parts(ctx: &JSContext, parts: ResponseParts) -> JSResult<JSObject> {
        let ResponseParts {
            url,
            method,
            status: status_code,
            headers,
            body,
        } = parts;
        let uri = Uri::try_from(url.as_str()).map_err(|_| {
            HostError::new(rong::error::E_INVALID_ARG, format!("Invalid URL: {}", url))
                .with_name("TypeError")
        })?;
        let http_method = Method::from_bytes(method.as_bytes()).map_err(|_| {
            HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Invalid method: {}", method),
            )
            .with_name("TypeError")
        })?;
        let status = http::StatusCode::from_u16(status_code).map_err(|_| {
            HostError::new(
                rong::error::E_INVALID_ARG,
                format!("Invalid status code: {}", status_code),
            )
            .with_name("RangeError")
        })?;

        let body = match body {
            HostBody::Empty => None,
            HostBody::Bytes(bytes) => Some(BodyKind::Buffered(bytes)),
            HostBody::Stream(slot) => {
                if slot.is_consumed().map_err(|error| {
                    HostError::new(rong::error::E_INVALID_STATE, error).with_name("TypeError")
                })? {
                    return Err(HostError::new(
                        rong::error::E_INVALID_STATE,
                        "streaming response body already consumed",
                    )
                    .with_name("TypeError")
                    .into());
                }
                Some(BodyKind::Channel(slot))
            }
        };

        let response = Response {
            url: uri,
            method: http_method,
            headers: Headers::from_header_map(headers),
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            body,
            consumed: Rc::new(Cell::new(false)),
            redirected: false,
            type_: "default".to_string(),
            abort_receiver: None,
            body_stream: Rc::new(RefCell::new(None)),
        };

        let class = Class::lookup::<Response>(ctx)?;
        Ok(class.instance(response))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_meta(
        status: http::StatusCode,
        headers_in: http::HeaderMap,
        body_kind: BodyKind,
        abort_receiver: Option<AbortReceiver>,
        method: Method,
        url: Uri,
        redirected: bool,
        type_: String,
    ) -> Self {
        // Convert hyper headers to Headers
        let mut headers = Headers::default();
        for (name, value) in headers_in.iter() {
            if let Ok(value_str) = value.to_str() {
                let _ = headers.set(name.to_string(), value_str.to_string());
            }
        }

        Self {
            url,
            method,
            headers,
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            body: Some(body_kind),
            consumed: Rc::new(Cell::new(false)),
            redirected,
            type_,
            abort_receiver,
            ..Default::default()
        }
    }
}

impl Response {
    async fn extract_response_parts(obj: &JSObject) -> JSResult<ResponseParts> {
        let (url, method, status, headers, body_kind, body_stream, abort_receiver) = {
            let response = obj.borrow::<Response>()?;
            (
                response.url.to_string(),
                response.method.to_string(),
                response.status,
                response.headers.as_header_map().clone(),
                response.body.clone(),
                response.body_stream.borrow().as_ref().cloned(),
                response.abort_receiver.clone(),
            )
        };

        let body = match body_kind.as_ref() {
            None => HostBody::Empty,
            Some(BodyKind::Buffered(bytes)) => HostBody::Bytes(bytes.clone()),
            Some(BodyKind::Channel(inner)) => {
                HostBody::Stream(HostBodyStream::from_shared_slot(inner.shared_slot()))
            }
            Some(BodyKind::JS(body)) => {
                if let Some(obj) = body.0.clone().into_object()
                    && let Ok(stream) = obj.borrow::<ReadableStream>()
                {
                    let receiver = readable_stream_take_receiver(&stream).ok_or_else(|| {
                        HostError::new(
                            rong::error::E_INVALID_STATE,
                            "ReadableStream response body already used",
                        )
                    })?;
                    HostBody::Stream(HostBodyStream::from_receiver(receiver))
                } else {
                    let bytes = Self::body_to_bytes_parts(
                        Some(&BodyKind::JS(body.clone())),
                        headers.clone(),
                        body_stream,
                        abort_receiver,
                    )
                    .await?;
                    HostBody::Bytes(bytes)
                }
            }
        };

        Ok(ResponseParts {
            url,
            method,
            status,
            headers,
            body,
        })
    }
}

impl ResponseParts {
    /// Construct a JS `Response` object directly from Rust-owned response parts.
    ///
    /// Stream bodies are single-consumer. If the supplied `HostBody` stream has
    /// already been taken, this returns an error instead of silently replacing
    /// the body with an empty stream.
    pub fn into_js_object(self, ctx: &JSContext) -> JSResult<JSObject> {
        Response::from_response_parts(ctx, self)
    }

    /// Extract Rust-owned response parts from a JS `Response` object.
    ///
    /// Buffered bodies are returned as `HostBody::Bytes`. Stream bodies are
    /// returned as `HostBody::Stream` and remain single-consumer.
    pub async fn from_js_object(obj: &JSObject) -> JSResult<Self> {
        Response::extract_response_parts(obj).await
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<Response>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_response() {
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_url::init(&ctx)?;

            rong_buffer::init(&ctx)?;
            crate::header::init(&ctx)?;
            crate::formdata::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "response.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
