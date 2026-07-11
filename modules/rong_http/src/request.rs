use http::{Method, Uri};
use rong::{function::Optional, *};

use crate::body::{HostBody, HttpBody};
use crate::formdata::FormData;
use crate::header::Headers;
use rong_abort::AbortSignal;
use rong_stream::{JSReadableStream, ReadableStream, readable_stream_is_locked};
use rong_url::URL;
use std::cell::Cell;

#[derive(Debug)]
pub struct RequestParts {
    pub url: String,
    pub method: String,
    pub headers: http::HeaderMap<http::header::HeaderValue>,
    pub body: HostBody,
}

#[js_class]
pub struct Request {
    pub(crate) url: Uri,
    pub(crate) method: Method,
    pub(crate) headers: Headers,
    pub(crate) body: Option<HttpBody>,
    redirect: RequestRedirect,
    signal: Option<AbortSignal>, // AbortSignal
    consumed: Cell<bool>,
}

impl Request {
    pub(crate) fn abort_signal(&self) -> Option<&AbortSignal> {
        self.signal.as_ref()
    }

    fn has_streaming_body(&self) -> bool {
        self.body
            .as_ref()
            .and_then(|body| body.0.clone().into_object())
            .is_some_and(|obj| obj.borrow::<ReadableStream>().is_ok())
    }

    fn try_clone(&self) -> JSResult<Self> {
        if self.has_streaming_body() {
            return Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "Request.clone() does not support streaming bodies; tee the stream before cloning",
            )
            .with_name("TypeError")
            .into());
        }

        Ok(Self {
            method: self.method.clone(),
            url: self.url.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            redirect: self.redirect.clone(),
            signal: self.signal.clone(),
            consumed: Cell::new(self.consumed.get()),
        })
    }

    /// Extract domain from the request URL
    pub(crate) fn domain(&self) -> JSResult<String> {
        let host = self.url.host().ok_or_else(|| {
            HostError::new(rong::error::E_INVALID_ARG, "URL has no host").with_name("TypeError")
        })?;
        Ok(host.to_string())
    }
}

#[derive(Default, Clone)]
enum RequestRedirect {
    #[default]
    Follow,
    Error,
    Manual,
}

#[derive(FromJSValue, Default)]
pub(crate) struct RequestInit {
    method: Option<Method>,
    headers: Option<Headers>,
    body: Option<JSValue>,
    redirect: Option<RequestRedirect>,
    signal: Option<AbortSignal>, // AbortSignal
}

impl RequestInit {
    fn assign_request(self, request: &mut Request) {
        if let Some(method) = self.method {
            request.method = method;
        }
        if let Some(headers) = self.headers {
            request.headers = headers;
        }
        if let Some(body) = self.body {
            request.body = Some(HttpBody(body));
        }
        if let Some(redirect) = self.redirect {
            request.redirect = redirect;
        }
        if let Some(signal) = self.signal {
            request.signal = Some(signal);
        }
    }
}

impl TryFromJSValue for RequestInit {
    fn try_from_js(value: JSValue) -> JSResult<Self> {
        let mut request = Self::default();

        let obj = value.into_object().ok_or_else(|| {
            HostError::new(rong::error::E_INVALID_ARG, "Invalid RequestInit").with_name("TypeError")
        })?;

        // Method
        if let Ok(method_str) = obj.get::<_, String>("method") {
            request.method = Some(Method::from_bytes(method_str.as_bytes()).map_err(|_| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    format!("Invalid method: {}", method_str),
                )
                .with_name("TypeError")
            })?);
        }

        // Headers
        if let Ok(headers_init) = obj.get::<_, JSValue>("headers") {
            request.headers = Some(Headers::new(Optional(Some(headers_init)))?);
        }

        // Body
        if let Ok(body) = obj.get::<_, JSValue>("body") {
            request.body = Some(body);
        }

        // Redirect
        if let Ok(redirect_str) = obj.get::<_, String>("redirect") {
            let redirect = match redirect_str.as_str() {
                "follow" => RequestRedirect::Follow,
                "error" => RequestRedirect::Error,
                "manual" => RequestRedirect::Manual,
                _ => {
                    return Err(HostError::new(
                        rong::error::E_INVALID_ARG,
                        format!("Invalid redirect: {}", redirect_str),
                    )
                    .with_name("TypeError")
                    .into());
                }
            };
            request.redirect = Some(redirect);
        }

        // Signal
        if let Ok(signal_obj) = obj.get::<_, JSObject>("signal")
            && let Ok(signal) = signal_obj.borrow::<AbortSignal>()
        {
            request.signal = Some(signal.clone());
        }
        Ok(request)
    }
}

#[js_class]
impl Request {
    #[js_method(constructor)]
    pub(crate) fn new(input: JSValue, request_init: Optional<RequestInit>) -> JSResult<Self> {
        // Parse input - can be a URL string or another Request object
        let mut request = if let Ok(url_str) = input.clone().to_rust::<String>() {
            let url = Uri::try_from(url_str.as_str()).map_err(|_| {
                HostError::new(
                    rong::error::E_INVALID_ARG,
                    format!("Invalid URL: {}", url_str),
                )
                .with_name("TypeError")
            })?;

            Self {
                url,
                ..Default::default()
            }
        } else if let Some(obj) = input.into_object() {
            if let Ok(req) = obj.borrow::<Request>() {
                req.try_clone()?
            } else if let Ok(url) = obj.borrow::<URL>() {
                // Convert URL to string first, then parse as Uri
                let url_str = url.to_string();
                let uri = Uri::try_from(url_str.as_str()).map_err(|_| {
                    HostError::new(
                        rong::error::E_INVALID_ARG,
                        format!("Invalid URL: {}", url_str),
                    )
                    .with_name("TypeError")
                })?;
                Self {
                    url: uri,
                    ..Default::default()
                }
            } else {
                Self::default()
            }
        } else {
            Self::default()
        };

        // Process init object if provided
        if let Some(init) = request_init.0 {
            init.assign_request(&mut request);
        }

        // make sure body is None for Get
        if request.method == Method::GET {
            request.body = None;
        }

        Ok(request)
    }

    #[js_method(getter)]
    fn method(&self) -> String {
        self.method.as_str().to_string()
    }

    #[js_method(getter)]
    fn url(&self) -> String {
        self.url.to_string()
    }

    #[js_method(getter)]
    fn headers(&self) -> Headers {
        self.headers.clone()
    }

    #[js_method(getter)]
    fn cache(&self) -> &'static str {
        "no-cache"
    }

    #[js_method(getter)]
    pub(crate) fn redirect(&self) -> &'static str {
        match self.redirect {
            RequestRedirect::Follow => "follow",
            RequestRedirect::Error => "error",
            RequestRedirect::Manual => "manual",
        }
    }

    #[js_method(getter)]
    fn keepalive(&self) -> bool {
        true
    }

    #[js_method(getter)]
    fn signal(&self) -> Option<AbortSignal> {
        self.signal.clone()
    }

    #[js_method(getter)]
    fn body(&self) -> Option<JSValue> {
        self.body.clone().map(|b| b.0)
    }

    #[js_method(getter, rename = "bodyUsed")]
    fn body_used(&self) -> bool {
        if self.consumed.get() {
            return true;
        }
        if let Some(body) = &self.body
            && let Some(obj) = body.0.clone().into_object()
            && let Ok(rs) = obj.borrow::<ReadableStream>()
        {
            return readable_stream_is_locked(&rs);
        }
        false
    }

    #[js_method]
    fn clone(&self) -> JSResult<Self> {
        self.try_clone()
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
        if let Some(body) = &self.body {
            body.text().await
        } else {
            Ok(String::new())
        }
    }

    #[js_method]
    async fn json(&self, ctx: JSContext) -> JSResult<JSValue> {
        let text = self.text().await?;
        text.as_str().json_to_js_value(&ctx)
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
        if let Some(body) = &self.body {
            let bytes = body.bytes().await?;
            JSArrayBuffer::from_bytes(&ctx, &bytes)
        } else {
            JSArrayBuffer::from_bytes(&ctx, &[])
        }
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

        if let Some(body) = &self.body
            && let Some(obj) = body.0.clone().into_object()
            && let Ok(formdata) = obj.borrow::<FormData>()
        {
            return Ok(Class::lookup::<FormData>(&ctx)?.instance(formdata.clone()));
        }

        let bytes = if let Some(body) = &self.body {
            body.bytes().await?
        } else {
            bytes::Bytes::new()
        };

        let content_type = self
            .headers
            .get("Content-Type".to_string())?
            .unwrap_or_default();
        let form = FormData::from_bytes(&bytes, &content_type)?;
        Ok(Class::lookup::<FormData>(&ctx)?.instance(form))
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Some(signal) = &self.signal {
            signal.gc_mark_with(mark_fn);
        }
    }
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: Method::GET,
            url: Uri::from_static("about:blank"),
            headers: Headers::default(),
            body: None,
            redirect: RequestRedirect::default(),
            signal: None,
            consumed: Cell::new(false),
        }
    }
}

impl Request {
    fn from_request_parts(ctx: &JSContext, parts: RequestParts) -> JSResult<JSObject> {
        let RequestParts {
            url,
            method,
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

        let http_body = match body {
            HostBody::Empty => None,
            HostBody::Bytes(bytes) => Some(HttpBody(
                JSArrayBuffer::from_bytes(ctx, bytes.as_ref())?.into_js_value(ctx),
            )),
            HostBody::Stream(slot) => {
                if slot.is_consumed().map_err(|error| {
                    HostError::new(rong::error::E_INVALID_STATE, error).with_name("TypeError")
                })? {
                    return Err(HostError::new(
                        rong::error::E_INVALID_STATE,
                        "streaming request body already consumed",
                    )
                    .with_name("TypeError")
                    .into());
                }
                Some(HttpBody(
                    JSReadableStream::from_shared_receiver(ctx, slot.shared_slot())?
                        .into_object()
                        .into_js_value(),
                ))
            }
        };

        let request = Request {
            url: uri,
            method: http_method,
            headers: Headers::from_header_map(headers),
            body: http_body,
            redirect: RequestRedirect::default(),
            signal: None,
            consumed: Cell::new(false),
        };

        let class = Class::lookup::<Request>(ctx)?;
        Ok(class.instance(request))
    }
}

impl RequestParts {
    /// Construct a JS `Request` object directly from Rust-owned request parts.
    ///
    /// Stream bodies are single-consumer. If the supplied `HostBody` already
    /// had its stream taken, this returns an error instead of silently
    /// replacing the body with an empty stream.
    pub fn into_js_object(self, ctx: &JSContext) -> JSResult<JSObject> {
        Request::from_request_parts(ctx, self)
    }
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<Request>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_request() {
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_url::init(&ctx)?;

            crate::header::init(&ctx)?;
            init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "request.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }
}
