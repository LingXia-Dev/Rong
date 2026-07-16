use crate::formdata::FormData;
use bytes::Bytes;
use flate2::read::GzDecoder;
use http::HeaderMap;
use rong::*;
use rong_buffer::{Blob, File};
use rong_stream::{ReadableStream, readable_stream_take_receiver};
use rong_url::URLSearchParams;
use std::fmt;
use std::io::Read;
use std::sync::{Arc, Mutex};

type HostBodyChunk = Result<Bytes, String>;
type HostBodyReceiver = tokio::sync::mpsc::Receiver<HostBodyChunk>;

/// A single-consumer streaming HTTP body produced by the host runtime.
///
/// This handle is intentionally not `Clone`. A streaming host body has exactly
/// one consumption right, and duplicating the wrapper would misrepresent that
/// ownership model.
pub struct HostBodyStream {
    inner: Arc<Mutex<Option<HostBodyReceiver>>>,
}

impl HostBodyStream {
    pub fn from_receiver(receiver: tokio::sync::mpsc::Receiver<Result<Bytes, String>>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(receiver))),
        }
    }

    /// Consume the stream handle and return the underlying receiver.
    ///
    /// This is a single-consumer operation. Calling it after the stream has
    /// already been taken returns an error instead of yielding an empty body.
    pub fn into_receiver(
        self,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Bytes, String>>, String> {
        self.inner
            .lock()
            .map_err(|_| "failed to lock streaming body".to_owned())?
            .take()
            .ok_or_else(|| "streaming body already consumed".to_owned())
    }

    pub(crate) fn shared_slot(&self) -> Arc<Mutex<Option<HostBodyReceiver>>> {
        self.inner.clone()
    }

    pub(crate) fn from_shared_slot(inner: Arc<Mutex<Option<HostBodyReceiver>>>) -> Self {
        Self { inner }
    }

    pub(crate) fn is_consumed(&self) -> Result<bool, String> {
        self.inner
            .lock()
            .map_err(|_| "failed to lock streaming body".to_owned())
            .map(|guard| guard.is_none())
    }

    pub(crate) fn try_take_receiver(&self) -> Result<Option<HostBodyReceiver>, String> {
        self.inner
            .lock()
            .map_err(|_| "failed to lock streaming body".to_owned())
            .map(|mut guard| guard.take())
    }
}

impl fmt::Debug for HostBodyStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HostBodyStream(..)")
    }
}

/// Host-side HTTP body representation used when bridging between Rust and the
/// JavaScript Fetch API.
///
/// `HostBody::Stream` is single-consumer. Reading it as bytes or converting it
/// into a receiver consumes the body. Callers must not assume the same stream
/// can be read twice.
pub enum HostBody {
    Empty,
    Bytes(Bytes),
    Stream(HostBodyStream),
}

impl HostBody {
    pub fn empty() -> Self {
        Self::Empty
    }

    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self::Bytes(bytes.into())
    }

    pub fn from_stream(receiver: tokio::sync::mpsc::Receiver<Result<Bytes, String>>) -> Self {
        Self::Stream(HostBodyStream::from_receiver(receiver))
    }

    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Self::Empty => Some(Vec::new()),
            Self::Bytes(bytes) => Some(bytes.to_vec()),
            Self::Stream(_) => None,
        }
    }

    pub fn is_definitely_empty(&self) -> bool {
        match self {
            Self::Empty => true,
            Self::Bytes(bytes) => bytes.is_empty(),
            Self::Stream(_) => false,
        }
    }

    pub async fn into_bytes(self) -> Result<Bytes, String> {
        match self {
            Self::Empty => Ok(Bytes::new()),
            Self::Bytes(bytes) => Ok(bytes),
            Self::Stream(stream) => {
                let mut receiver = stream.into_receiver()?;
                let mut collected = Vec::new();
                while let Some(chunk) = receiver.recv().await {
                    match chunk {
                        Ok(bytes) => collected.extend_from_slice(&bytes),
                        Err(error) => return Err(error),
                    }
                }
                Ok(Bytes::from(collected))
            }
        }
    }
}

impl fmt::Debug for HostBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("HostBody::Empty"),
            Self::Bytes(bytes) => f
                .debug_tuple("HostBody::Bytes")
                .field(&format_args!("{} bytes", bytes.len()))
                .finish(),
            Self::Stream(_) => f.write_str("HostBody::Stream(..)"),
        }
    }
}

impl From<Bytes> for HostBody {
    fn from(value: Bytes) -> Self {
        Self::Bytes(value)
    }
}

impl From<Vec<u8>> for HostBody {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(Bytes::from(value))
    }
}

pub(crate) enum BodyKind {
    // Buffered, in-memory body as Bytes (Arc-backed, cheap clone, no aliasing issues)
    Buffered(Bytes),
    // Stream body via channel from net service (chunk or error)
    Channel(HostBodyStream),
    JS(HttpBody),
}

impl Clone for BodyKind {
    fn clone(&self) -> Self {
        match self {
            Self::Buffered(bytes) => Self::Buffered(bytes.clone()),
            Self::Channel(stream) => {
                Self::Channel(HostBodyStream::from_shared_slot(stream.shared_slot()))
            }
            Self::JS(body) => Self::JS(body.clone()),
        }
    }
}

#[derive(Clone)]
pub(crate) struct HttpBody(pub JSValue);

impl HttpBody {
    // Convert to bytes synchronously for hyper Body implementation
    pub async fn to_bytes(&self) -> JSResult<(Bytes, Option<String>)> {
        if let Some(obj) = self.0.clone().into_object() {
            let ctx = obj.context();

            // Handle URLSearchParams
            if let Ok(params) = obj.borrow::<URLSearchParams>() {
                return Ok((Bytes::from(params.to_string()), None));
            }

            // Handle TypedArray
            if let Some(typed_array) = AnyJSTypedArray::from_object(obj.clone())
                && let Some(bytes) = typed_array.as_bytes()
            {
                return Ok((Bytes::from(bytes.to_vec()), None));
            }

            // Handle ArrayBuffer
            if let Some(buffer) = JSArrayBuffer::from_object(obj.clone()) {
                return Ok((Bytes::from(buffer.as_bytes().to_vec()), None));
            }

            // Handle Blob
            if let Ok(blob) = obj.borrow::<Blob>() {
                return Ok((blob.bytes_ref().clone(), None));
            }

            // Handle File
            if let Ok(file) = obj.borrow::<File>() {
                return Ok((file.bytes_ref().clone(), None));
            }

            // Handle FormData
            let formdata = if let Ok(formdata) = obj.borrow::<FormData>() {
                Some(formdata.clone())
            } else {
                None
            };
            if let Some(formdata) = formdata {
                let (body, boundary) = formdata.serialize(ctx.clone()).await?;
                return Ok((Bytes::from(body), Some(boundary)));
            }

            // Handle ReadableStream by consuming its backing receiver.
            let stream_receiver = if let Ok(stream) = obj.borrow::<ReadableStream>() {
                Some(readable_stream_take_receiver(&stream))
            } else {
                None
            };
            if let Some(receiver) = stream_receiver {
                let mut receiver = receiver.ok_or_else(|| {
                    HostError::new(
                        rong::error::E_INVALID_STATE,
                        "ReadableStream body already used",
                    )
                    .with_name("TypeError")
                })?;
                let mut collected = Vec::new();
                while let Some(chunk) = receiver.recv().await {
                    match chunk {
                        Ok(bytes) => collected.extend_from_slice(&bytes),
                        Err(error) => {
                            return Err(HostError::new(rong::error::E_STREAM, error)
                                .with_name("TypeError")
                                .into());
                        }
                    }
                }
                return Ok((Bytes::from(collected), None));
            }

            // Invoke JavaScript's String conversion through the engine so a
            // user-defined conversion and any value it throws are preserved.
            let string_fn: JSFunc = ctx.global().get("String")?;
            let string: String = string_fn.call(None, (self.0.clone(),))?;
            return Ok((Bytes::from(string), None));
        }

        if self.0.is_null() || self.0.is_undefined() {
            return Ok((Bytes::new(), None));
        }

        // BodyInit falls back to JavaScript string conversion for values that
        // are not one of its supported binary or structured body types.
        let s = self.0.clone().to_rust::<String>()?;
        Ok((Bytes::from(s), None))
    }

    // Convert body to text
    pub async fn text(&self) -> JSResult<String> {
        // For most cases, we can just convert bytes to UTF-8 string
        let (bytes, _) = self.to_bytes().await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    // Convert body to bytes
    pub async fn bytes(&self) -> JSResult<Bytes> {
        Ok(self.to_bytes().await?.0)
    }
}

// Decompress bytes based on content-encoding header
pub(crate) fn decompress_bytes(bytes: Bytes, headers: &HeaderMap) -> JSResult<Bytes> {
    if let Some(encoding) = headers.get(http::header::CONTENT_ENCODING) {
        match encoding.to_str() {
            Ok("gzip") => {
                let mut decoder = GzDecoder::new(&bytes[..]);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed).map_err(|e| {
                    HostError::new(
                        rong::error::E_IO,
                        format!("Failed to decompress gzip: {}", e),
                    )
                })?;
                Ok(Bytes::from(decompressed))
            }
            Ok(encoding) => Err(HostError::new(
                rong::error::E_NOT_SUPPORTED,
                format!("Unsupported content-encoding: {}", encoding),
            )
            .into()),
            Err(_) => Ok(bytes),
        }
    } else {
        Ok(bytes)
    }
}
