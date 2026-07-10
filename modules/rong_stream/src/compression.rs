use bytes::Bytes;
use flate2::Compression;
use flate2::write::{
    DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder,
};
use rong::{HostError, JSContext, JSObject, JSResult, JSValue, js_class, js_method};
use std::io::{self, Write};
use tokio::sync::mpsc;

use crate::{JSReadableStream, JSWritableStream, writable_stream_to_sender};

type StreamChunk = Result<Bytes, String>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompressionFormat {
    Gzip,
    Deflate,
    DeflateRaw,
}

impl CompressionFormat {
    fn parse(raw: &str) -> JSResult<Self> {
        match raw {
            "gzip" => Ok(Self::Gzip),
            "deflate" => Ok(Self::Deflate),
            "deflate-raw" => Ok(Self::DeflateRaw),
            _ => Err(HostError::new(
                rong::error::E_TYPE,
                format!("Unsupported compression format: {raw}"),
            )
            .with_name("TypeError")
            .into()),
        }
    }
}

enum EncoderState {
    Gzip(GzEncoder<Vec<u8>>),
    Deflate(ZlibEncoder<Vec<u8>>),
    DeflateRaw(DeflateEncoder<Vec<u8>>),
}

impl EncoderState {
    fn new(format: CompressionFormat) -> Self {
        match format {
            CompressionFormat::Gzip => {
                Self::Gzip(GzEncoder::new(Vec::new(), Compression::default()))
            }
            CompressionFormat::Deflate => {
                Self::Deflate(ZlibEncoder::new(Vec::new(), Compression::default()))
            }
            CompressionFormat::DeflateRaw => {
                Self::DeflateRaw(DeflateEncoder::new(Vec::new(), Compression::default()))
            }
        }
    }

    fn write_all(&mut self, chunk: &[u8]) -> io::Result<()> {
        match self {
            Self::Gzip(inner) => inner.write_all(chunk),
            Self::Deflate(inner) => inner.write_all(chunk),
            Self::DeflateRaw(inner) => inner.write_all(chunk),
        }
    }

    fn try_finish(&mut self) -> io::Result<()> {
        match self {
            Self::Gzip(inner) => inner.try_finish(),
            Self::Deflate(inner) => inner.try_finish(),
            Self::DeflateRaw(inner) => inner.try_finish(),
        }
    }

    fn take_output(&mut self) -> Option<Bytes> {
        let buffer = match self {
            Self::Gzip(inner) => inner.get_mut(),
            Self::Deflate(inner) => inner.get_mut(),
            Self::DeflateRaw(inner) => inner.get_mut(),
        };
        if buffer.is_empty() {
            return None;
        }
        Some(Bytes::from(std::mem::take(buffer)))
    }
}

enum DecoderState {
    Gzip(GzDecoder<Vec<u8>>),
    Deflate(ZlibDecoder<Vec<u8>>),
    DeflateRaw(DeflateDecoder<Vec<u8>>),
}

impl DecoderState {
    fn new(format: CompressionFormat) -> Self {
        match format {
            CompressionFormat::Gzip => Self::Gzip(GzDecoder::new(Vec::new())),
            CompressionFormat::Deflate => Self::Deflate(ZlibDecoder::new(Vec::new())),
            CompressionFormat::DeflateRaw => Self::DeflateRaw(DeflateDecoder::new(Vec::new())),
        }
    }

    fn write_chunk(&mut self, chunk: &[u8]) -> io::Result<()> {
        let mut offset = 0;
        while offset < chunk.len() {
            let written = match self {
                Self::Gzip(inner) => inner.write(&chunk[offset..])?,
                Self::Deflate(inner) => inner.write(&chunk[offset..])?,
                Self::DeflateRaw(inner) => inner.write(&chunk[offset..])?,
            };
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Trailing data after end of compressed stream",
                ));
            }
            offset += written;
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Gzip(inner) => inner.flush(),
            Self::Deflate(inner) => inner.flush(),
            Self::DeflateRaw(inner) => inner.flush(),
        }
    }

    fn try_finish(&mut self) -> io::Result<()> {
        match self {
            Self::Gzip(inner) => inner.try_finish(),
            Self::Deflate(inner) => inner.try_finish(),
            Self::DeflateRaw(inner) => inner.try_finish(),
        }
    }

    fn take_output(&mut self) -> Option<Bytes> {
        let buffer = match self {
            Self::Gzip(inner) => inner.get_mut(),
            Self::Deflate(inner) => inner.get_mut(),
            Self::DeflateRaw(inner) => inner.get_mut(),
        };
        if buffer.is_empty() {
            return None;
        }
        Some(Bytes::from(std::mem::take(buffer)))
    }
}

fn spawn_compression_task(
    mut input_rx: mpsc::Receiver<Bytes>,
    output_tx: mpsc::Sender<StreamChunk>,
    format: CompressionFormat,
) {
    rong::spawn_local(async move {
        let mut encoder = EncoderState::new(format);

        while let Some(chunk) = input_rx.recv().await {
            let result = encoder.write_all(&chunk).map_err(|error| error.to_string());

            match result {
                Ok(()) => {
                    if let Some(bytes) = encoder.take_output()
                        && output_tx.send(Ok(bytes)).await.is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    let _ = output_tx.send(Err(error)).await;
                    return;
                }
            }
        }

        match encoder.try_finish() {
            Ok(()) => {
                if let Some(bytes) = encoder.take_output() {
                    let _ = output_tx.send(Ok(bytes)).await;
                }
            }
            Err(error) => {
                let _ = output_tx.send(Err(error.to_string())).await;
            }
        }
    });
}

fn spawn_decompression_task(
    mut input_rx: mpsc::Receiver<Bytes>,
    output_tx: mpsc::Sender<StreamChunk>,
    format: CompressionFormat,
) {
    rong::spawn_local(async move {
        let mut decoder = DecoderState::new(format);

        while let Some(chunk) = input_rx.recv().await {
            let result = decoder
                .write_chunk(&chunk)
                .and_then(|_| decoder.flush())
                .map_err(|error| error.to_string());

            match result {
                Ok(()) => {
                    if let Some(bytes) = decoder.take_output()
                        && output_tx.send(Ok(bytes)).await.is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    let _ = output_tx.send(Err(error)).await;
                    return;
                }
            }
        }

        match decoder.try_finish() {
            Ok(()) => {
                if let Some(bytes) = decoder.take_output() {
                    let _ = output_tx.send(Ok(bytes)).await;
                }
            }
            Err(error) => {
                let _ = output_tx.send(Err(error.to_string())).await;
            }
        }
    });
}

fn build_transform_pair(
    ctx: &JSContext,
    format: CompressionFormat,
    decode: bool,
) -> JSResult<(JSObject, JSObject)> {
    let (input_tx, input_rx) = mpsc::channel::<Bytes>(16);
    let (output_tx, output_rx) = mpsc::channel::<StreamChunk>(16);

    if decode {
        spawn_decompression_task(input_rx, output_tx, format);
    } else {
        spawn_compression_task(input_rx, output_tx, format);
    }

    let writable = JSWritableStream::new(ctx, writable_stream_to_sender(input_tx))?.into_object();
    let readable = JSReadableStream::from_receiver(ctx, output_rx)?.into_object();

    Ok((readable, writable))
}

#[js_class]
pub struct CompressionStream {
    readable: JSObject,
    writable: JSObject,
}

#[js_class]
impl CompressionStream {
    #[js_method(constructor)]
    fn constructor(ctx: JSContext, format: String) -> JSResult<Self> {
        let format = CompressionFormat::parse(&format)?;
        let (readable, writable) = build_transform_pair(&ctx, format, false)?;
        Ok(Self { readable, writable })
    }

    #[js_method(getter)]
    fn readable(&self) -> JSObject {
        self.readable.clone()
    }

    #[js_method(getter)]
    fn writable(&self) -> JSObject {
        self.writable.clone()
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        mark_fn(self.readable.as_js_value());
        mark_fn(self.writable.as_js_value());
    }
}

#[js_class]
pub struct DecompressionStream {
    readable: JSObject,
    writable: JSObject,
}

#[js_class]
impl DecompressionStream {
    #[js_method(constructor)]
    fn constructor(ctx: JSContext, format: String) -> JSResult<Self> {
        let format = CompressionFormat::parse(&format)?;
        let (readable, writable) = build_transform_pair(&ctx, format, true)?;
        Ok(Self { readable, writable })
    }

    #[js_method(getter)]
    fn readable(&self) -> JSObject {
        self.readable.clone()
    }

    #[js_method(getter)]
    fn writable(&self) -> JSObject {
        self.writable.clone()
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        mark_fn(self.readable.as_js_value());
        mark_fn(self.writable.as_js_value());
    }
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<CompressionStream>()?;
    ctx.register_class::<DecompressionStream>()?;
    Ok(())
}
