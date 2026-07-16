use crate::writable::WritableStream;
use bytes::{Bytes, BytesMut};
use rong::function::{JSClassRef, This};
use rong::*;
use rong_abort::AbortSignal;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::AsyncRead;
use tokio::sync::mpsc;

type StreamChunk = Result<Bytes, String>;
type StreamReceiver = mpsc::Receiver<StreamChunk>;
type ControlSender = mpsc::UnboundedSender<StreamChunk>;
type ControlReceiver = mpsc::UnboundedReceiver<StreamChunk>;
type SharedReceiverSlot = Arc<StdMutex<Option<StreamReceiver>>>;
type SharedReceiver = Arc<StdMutex<Option<StreamReceiver>>>;
type SharedSender = Arc<StdMutex<Option<ControlSender>>>;
type StreamInitializer = Rc<RefCell<Option<Box<dyn FnOnce() -> StreamReceiver>>>>;

#[derive(Clone, Default)]
struct AsyncReaderTaskRegistry {
    handles: Rc<RefCell<Vec<tokio::task::JoinHandle<()>>>>,
}

impl AsyncReaderTaskRegistry {
    fn track(&self, handle: tokio::task::JoinHandle<()>) {
        let mut handles = self.handles.borrow_mut();
        handles.retain(|task| !task.is_finished());
        handles.push(handle);
    }
}

impl JSRuntimeService for AsyncReaderTaskRegistry {
    fn on_shutdown(&self) {
        for handle in self.handles.borrow_mut().drain(..) {
            handle.abort();
        }
    }
}

#[js_class]
pub struct ReadableStream {
    // A single-consumer source guarded by a lock; getReader() takes ownership.
    pub(crate) rx_slot: SharedReceiverSlot,
    initializer: StreamInitializer,
}

#[js_class]
pub struct ReadableStreamDefaultReader {
    // Reference to the owning stream's slot so releaseLock can return ownership.
    slot: SharedReceiverSlot,
    // Receiver owned by the reader while locked.
    rx: SharedReceiver,
    canceled: Arc<AtomicBool>,
}

#[js_class]
pub struct ReadableStreamDefaultController {
    tx: SharedSender,
}

impl ReadableStream {
    pub fn from_receiver(rx: StreamReceiver) -> Self {
        Self {
            rx_slot: Arc::new(StdMutex::new(Some(rx))),
            initializer: Rc::new(RefCell::new(None)),
        }
    }

    fn from_lazy_receiver<F>(init: F) -> Self
    where
        F: FnOnce() -> StreamReceiver + 'static,
    {
        Self {
            rx_slot: Arc::new(StdMutex::new(None)),
            initializer: Rc::new(RefCell::new(Some(Box::new(init)))),
        }
    }
}

// Options for pipeTo
#[derive(FromJSObject, Default, Clone)]
struct PipeToOptions {
    #[js_name = "preventClose"]
    prevent_close: Option<bool>,
    #[js_name = "preventAbort"]
    prevent_abort: Option<bool>,
    #[js_name = "preventCancel"]
    prevent_cancel: Option<bool>,
    signal: Option<AbortSignal>,
}

#[js_class]
impl ReadableStream {
    #[js_method(constructor)]
    fn constructor(ctx: JSContext, underlying: function::Optional<JSValue>) -> JSResult<JSObject> {
        // Create a basic channel-backed stream
        let (tx, rx) = mpsc::channel::<StreamChunk>(16);
        let (control_tx, mut control_rx): (ControlSender, ControlReceiver) =
            mpsc::unbounded_channel();
        let stream = Self::from_receiver(rx);
        let obj = JSReadableStream::new(&ctx, stream)?.into_object();

        // Forward controller writes to the bounded stream channel in-order.
        let tx_forwards = tx.clone();
        rong::spawn_local(async move {
            while let Some(item) = control_rx.recv().await {
                if tx_forwards.send(item).await.is_err() {
                    break;
                }
            }
        });

        // If an underlying source is provided, call start(controller) if present
        if let Some(v) = underlying.0
            && let Some(src) = v.into_object()
        {
            let controller = ReadableStreamDefaultController {
                tx: Arc::new(StdMutex::new(Some(control_tx))),
            };
            if let Ok(start) = src.get::<_, JSFunc>("start") {
                start.call::<_, JSValue>(Some(src.clone()), (controller,))?;
            }
        }
        Ok(obj)
    }

    #[js_method(rename = "getReader")]
    pub fn get_reader(&self) -> JSResult<ReadableStreamDefaultReader> {
        let mut guard = self
            .rx_slot
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        if guard.is_none()
            && let Some(init) = self.initializer.borrow_mut().take()
        {
            *guard = Some(init());
        }
        match guard.take() {
            Some(rx) => Ok(ReadableStreamDefaultReader {
                slot: self.rx_slot.clone(),
                rx: Arc::new(StdMutex::new(Some(rx))),
                canceled: Arc::new(AtomicBool::new(false)),
            }),
            None => Err(
                HostError::new(rong::error::E_INVALID_STATE, "ReadableStream is locked")
                    .with_name("TypeError")
                    .into(),
            ),
        }
    }

    #[js_method]
    fn cancel(&self) -> JSResult<()> {
        let mut guard = self
            .rx_slot
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        *guard = None;
        let _ = self.initializer.borrow_mut().take();
        Ok(())
    }

    /// Minimal pipeTo implementation: pumps from this ReadableStream into a WritableStream.
    /// Honors preventClose/preventAbort/preventCancel. Basic signal support (pre-check).
    #[js_method(rename = "pipeTo")]
    async fn pipe_to(
        &self,
        ctx: JSContext,
        dest: JSClassRef<WritableStream>,
        options: function::Optional<PipeToOptions>,
    ) -> JSResult<()> {
        let reader = self.get_reader()?;
        let writer = dest.borrow()?.get_writer()?;
        Self::pipe_to_with_handles(ctx, reader, writer, options.0).await
    }

    async fn pipe_to_with_handles(
        ctx: JSContext,
        reader: ReadableStreamDefaultReader,
        mut writer: crate::writable::WritableStreamDefaultWriter,
        opts: Option<PipeToOptions>,
    ) -> JSResult<()> {
        let opts = opts.unwrap_or_default();
        let prevent_close = opts.prevent_close.unwrap_or(false);
        let prevent_abort = opts.prevent_abort.unwrap_or(false);
        let prevent_cancel = opts.prevent_cancel.unwrap_or(false);
        let mut pipe_err = if let Some(sig) = &opts.signal
            && sig.aborted()
        {
            Some(RongJSError::from_thrown_value(sig.get_reason()))
        } else {
            None
        };
        let pre_aborted = pipe_err.is_some();

        // Fast path: if this ReadableStream is channel-backed and the writer is channel-backed,
        // forward bytes directly between channels without constructing JS ArrayBuffers.
        if pipe_err.is_none()
            && let Some((tx, done_rx)) =
                crate::writable::WritableStreamDefaultWriter::take_channel(&mut writer)
            && let Some(mut rx) = reader.take_channel()
        {
            let mut abort_rx = opts.signal.as_ref().map(|s| s.subscribe());

            // Pump in Rust
            loop {
                if let Some(arx) = &mut abort_rx {
                    tokio::select! {
                        item = rx.recv() => {
                            match item {
                                Some(Ok(bytes)) => {
                                    if tx.send(bytes).await.is_err() { break; }
                                }
                                Some(Err(e)) => {
                                    pipe_err = Some(HostError::new(rong::error::E_STREAM, e).into());
                                    break;
                                }
                                None => break,
                            }
                        }
                        reason = arx.recv() => {
                            pipe_err = Some(RongJSError::from_thrown_value(reason));
                            break;
                        }
                    }
                } else {
                    match rx.recv().await {
                        Some(Ok(bytes)) => {
                            if tx.send(bytes).await.is_err() {
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            pipe_err = Some(HostError::new(rong::error::E_STREAM, e).into());
                            break;
                        }
                        None => break,
                    }
                }
            }

            // Close writer on normal completion
            if pipe_err.is_none() && !prevent_close {
                // Drop sender and wait for writer flush if available
                drop(tx);
                if let Some(rx) = done_rx {
                    let _ = rx.await;
                }
            }

            if let Some(e) = pipe_err {
                if !prevent_abort && !pre_aborted {
                    let _ = writer.abort().await;
                }
                let _ = reader.release_lock();
                let _ = writer.release_lock().await;
                return Err(e);
            }
            let _ = reader.release_lock();
            let _ = writer.release_lock().await;
            return Ok(());
        }

        // Fallback: JS-level pump
        let mut abort_rx = opts.signal.as_ref().map(|s| s.subscribe());
        while pipe_err.is_none() {
            // Race read vs abort (if provided)
            let res_obj = if let Some(rx) = &mut abort_rx {
                tokio::select! {
                    r = reader.read(ctx.clone()) => { Some(r?) }
                    reason = rx.recv() => { pipe_err = Some(RongJSError::from_thrown_value(reason)); None }
                }
            } else {
                Some(reader.read(ctx.clone()).await?)
            };
            if let Some(r) = res_obj {
                let done = r.get::<_, bool>("done")?;
                if done {
                    break;
                }
                let chunk = r.get::<_, JSValue>("value")?;
                // Also honor abort between read and write
                if let Some(sig) = &opts.signal
                    && sig.aborted()
                {
                    pipe_err = Some(RongJSError::from_thrown_value(sig.get_reason()));
                    break;
                }
                if let Err(e) = writer.write(chunk).await {
                    if !prevent_cancel {
                        let _ = reader.cancel().await;
                    }
                    pipe_err = Some(e);
                    break;
                }
                continue;
            } else {
                break;
            }
        }
        if let Some(e) = pipe_err {
            if !prevent_abort && !pre_aborted {
                let _ = writer.abort().await;
            }
            let _ = reader.release_lock();
            let _ = writer.release_lock().await;
            return Err(e);
        }
        if !prevent_close {
            let _ = writer.close().await;
        }
        let _ = reader.release_lock();
        let _ = writer.release_lock().await;
        Ok(())
    }

    #[js_method(rename = "pipeThrough")]
    fn pipe_through(
        &self,
        ctx: JSContext,
        transform: JSObject,
        options: function::Optional<PipeToOptions>,
    ) -> JSResult<JSObject> {
        let readable = transform.get::<_, JSObject>("readable").map_err(|_| {
            HostError::new(
                rong::error::E_TYPE,
                "pipeThrough expects an object with readable and writable",
            )
            .with_name("TypeError")
        })?;
        if readable.borrow::<ReadableStream>().is_err() {
            return Err(HostError::new(
                rong::error::E_TYPE,
                "transform.readable must be a ReadableStream",
            )
            .with_name("TypeError")
            .into());
        }

        let writable = transform
            .get::<_, JSClassRef<WritableStream>>("writable")
            .map_err(|_| {
                HostError::new(
                    rong::error::E_TYPE,
                    "pipeThrough expects an object with readable and writable",
                )
                .with_name("TypeError")
            })?;

        let pipe_ctx = ctx.clone();
        let pipe_options = options.0;
        let reader = self.get_reader()?;
        let writer = writable.borrow()?.get_writer()?;
        rong::spawn_local(async move {
            let _ = Self::pipe_to_with_handles(pipe_ctx, reader, writer, pipe_options).await;
        });

        Ok(readable)
    }

    /// Split this ReadableStream into two identical branches.
    /// The original stream becomes locked/disturbed; consumers must use the returned branches.
    #[js_method]
    fn tee(&self, ctx: JSContext) -> JSResult<JSArray> {
        // Take ownership of the underlying receiver. If none, the stream is locked.
        let rx = match readable_stream_take_receiver(self) {
            Some(rx) => rx,
            None => {
                return Err(HostError::new(
                    rong::error::E_INVALID_STATE,
                    "ReadableStream is locked",
                )
                .with_name("TypeError")
                .into());
            }
        };

        // Create two branches and a task to fan out chunks
        let (tx1, rx1) = mpsc::channel::<StreamChunk>(16);
        let (tx2, rx2) = mpsc::channel::<StreamChunk>(16);

        rong::spawn_local(async move {
            let mut src = rx;
            let mut tx1 = Some(tx1);
            let mut tx2 = Some(tx2);
            while let Some(item) = src.recv().await {
                match item {
                    Ok(bytes) => {
                        // Clone bytes for each branch (Bytes is cheap to clone)
                        if let Some(t1) = tx1.as_mut()
                            && t1.send(Ok(bytes.clone())).await.is_err()
                        {
                            tx1 = None;
                        }
                        if let Some(t2) = tx2.as_mut()
                            && t2.send(Ok(bytes)).await.is_err()
                        {
                            tx2 = None;
                        }
                        if tx1.is_none() && tx2.is_none() {
                            break;
                        }
                    }
                    Err(e) => {
                        if let Some(t1) = tx1.as_mut() {
                            let _ = t1.send(Err(e.clone())).await;
                        }
                        if let Some(t2) = tx2.as_mut() {
                            let _ = t2.send(Err(e)).await;
                        }
                        break;
                    }
                }
            }
            // Dropping tx1/tx2 closes the branches
        });

        // Wrap the two receivers as ReadableStream instances
        let b1 = JSReadableStream::from_receiver(&ctx, rx1)?.into_object();
        let b2 = JSReadableStream::from_receiver(&ctx, rx2)?.into_object();

        let arr = JSArray::new(&ctx)?;
        arr.set(0, b1)?;
        arr.set(1, b2)?;
        Ok(arr)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

#[js_class]
impl ReadableStreamDefaultReader {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Illegal constructor")
    }

    #[js_method]
    pub async fn read(&self, ctx: JSContext) -> JSResult<JSObject> {
        // Take the receiver out to avoid holding the lock across await
        let mut rx_opt = {
            let mut slot = self
                .rx
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Reader is poisoned"))?;
            slot.take()
        };

        // If already released or stream canceled
        let mut rx = match rx_opt.take() {
            Some(rx) => rx,
            None => {
                let out = JSObject::new(&ctx);
                out.set("done", true)?;
                return Ok(out);
            }
        };

        let next = rx.recv().await;
        // Put the receiver back
        {
            let mut slot = self
                .rx
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Reader is poisoned"))?;
            if self.canceled.load(Ordering::Acquire) {
                *slot = None;
                // Drop rx by not restoring it.
            } else {
                *slot = Some(rx);
            }
        }

        if self.canceled.load(Ordering::Acquire) {
            let out = JSObject::new(&ctx);
            out.set("done", true)?;
            return Ok(out);
        }

        match next {
            Some(Ok(bytes)) => {
                let out = JSObject::new(&ctx);
                out.set("done", false)?;
                let ab = JSArrayBuffer::from_bytes_owned(&ctx, bytes.to_vec())?;
                out.set("value", ab)?;
                Ok(out)
            }
            Some(Err(e)) => Err(HostError::new(rong::error::E_STREAM, e).into()),
            None => {
                // closed
                let out = JSObject::new(&ctx);
                out.set("done", true)?;
                Ok(out)
            }
        }
    }

    #[js_method(rename = "releaseLock")]
    pub fn release_lock(&self) -> JSResult<()> {
        // Take receiver out
        let rx_opt = {
            let mut slot = self
                .rx
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Reader is poisoned"))?;
            slot.take()
        };
        // Return it back to the stream's slot so another reader can be acquired
        if let Some(rx) = rx_opt
            && !self.canceled.load(Ordering::Acquire)
        {
            let mut guard = self
                .slot
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
            if guard.is_none() {
                *guard = Some(rx);
            }
        }
        Ok(())
    }

    #[js_method]
    async fn cancel(&self) -> JSResult<()> {
        self.canceled.store(true, Ordering::Release);
        let mut slot = self
            .rx
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Reader is poisoned"))?;
        *slot = None; // if a read is in-flight, it will observe `canceled` and drop on restore
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl ReadableStreamDefaultReader {
    pub(crate) fn take_channel(&self) -> Option<StreamReceiver> {
        self.rx.lock().ok()?.take()
    }
}

#[js_class]
impl ReadableStreamDefaultController {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Illegal constructor")
    }

    #[js_method]
    fn enqueue(&mut self, chunk: JSValue) -> JSResult<()> {
        // Support Uint8Array or ArrayBuffer
        let bytes: Bytes = if let Some(obj) = chunk.clone().into_object() {
            if let Some(ta) = AnyJSTypedArray::from_object(obj.clone()) {
                if let Some(b) = ta.as_bytes() {
                    Bytes::copy_from_slice(b)
                } else {
                    return Err(
                        HostError::new(rong::error::E_INVALID_ARG, "Invalid TypedArray")
                            .with_name("TypeError")
                            .into(),
                    );
                }
            } else if let Some(ab) = JSArrayBuffer::from_object(obj) {
                Bytes::copy_from_slice(ab.as_bytes())
            } else {
                return Err(HostError::new(
                    rong::error::E_INVALID_ARG,
                    "enqueue expects Uint8Array or ArrayBuffer",
                )
                .with_name("TypeError")
                .into());
            }
        } else {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "enqueue expects a TypedArray or ArrayBuffer",
            )
            .with_name("TypeError")
            .into());
        };

        let mut slot = self
            .tx
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        if let Some(tx) = slot.as_mut() {
            tx.send(Ok(bytes)).map_err(|_| {
                HostError::new(rong::error::E_INVALID_STATE, "ReadableStream is closed").into()
            })
        } else {
            Err(HostError::new(rong::error::E_INVALID_STATE, "ReadableStream is closed").into())
        }
    }

    #[js_method]
    fn close(&mut self) -> JSResult<()> {
        let mut slot = self
            .tx
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        *slot = None;
        Ok(())
    }

    #[js_method]
    fn error(&mut self, reason: function::Optional<JSValue>) -> JSResult<()> {
        let msg = reason
            .0
            .map(|v| v.to_string())
            .unwrap_or_else(|| "ReadableStream error".to_string());
        let mut slot = self
            .tx
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        if let Some(tx) = slot.as_mut() {
            let _ = tx.send(Err(msg));
        }
        *slot = None;
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

/// Take the underlying receiver from a ReadableStream. This consumes the stream's
/// readable side and returns the channel for Rust consumers.
pub fn readable_stream_take_receiver(rs: &ReadableStream) -> Option<StreamReceiver> {
    let mut guard = rs.rx_slot.lock().ok()?;
    if guard.is_none()
        && let Some(init) = rs.initializer.borrow_mut().take()
    {
        *guard = Some(init());
    }
    guard.take()
}

/// Check if a ReadableStream is locked (a reader has been acquired)
pub fn readable_stream_is_locked(rs: &ReadableStream) -> bool {
    let Ok(guard) = rs.rx_slot.lock() else {
        return true;
    };
    if guard.is_some() {
        return false;
    }
    rs.initializer
        .try_borrow()
        .map(|initializer| initializer.is_none())
        .unwrap_or(true)
}

// Install instance-level async iterator: stream implements next() and [Symbol.asyncIterator]
fn install_instance_async_iter(ctx: &JSContext, obj: &JSObject) -> JSResult<()> {
    // Custom next() that borrows `this` to access ReadableStream's internal rx_slot
    let next_fn = JSFunc::new(
        ctx,
        move |ctx: JSContext, this: This<JSObject>| async move {
            let rx_slot = match (*this).borrow::<ReadableStream>() {
                Ok(rs) => rs.rx_slot.clone(),
                Err(_) => {
                    return Err(HostError::new(rong::error::E_TYPE, "Not ReadableStream")
                        .with_name("TypeError")
                        .into());
                }
            };

            let mut rx = {
                let mut guard = rx_slot
                    .lock()
                    .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
                guard.take()
            };

            let Some(mut rx) = rx.take() else {
                let out = JSObject::new(&ctx);
                out.set("done", true).ok();
                out.set("value", JSValue::undefined(&ctx)).ok();
                return Ok(out);
            };

            let item = rx.recv().await;
            match item {
                Some(Ok(bytes)) => {
                    if let Ok(mut slot) = rx_slot.lock()
                        && slot.is_none()
                    {
                        *slot = Some(rx);
                    }
                    let out = JSObject::new(&ctx);
                    out.set("done", false).ok();
                    if let Ok(ab) = JSArrayBuffer::from_bytes(&ctx, &bytes) {
                        out.set("value", ab).ok();
                    }
                    Ok(out)
                }
                Some(Err(e)) => Err(HostError::new(rong::error::E_STREAM, e).into()),
                None => {
                    let out = JSObject::new(&ctx);
                    out.set("done", true).ok();
                    out.set("value", JSValue::undefined(&ctx)).ok();
                    Ok(out)
                }
            }
        },
    )?;
    obj.set("next", next_fn)?;

    rong::install_async_iterator_symbol(ctx, obj)
}

// Wrapper helper for clearer semantics
#[derive(Clone)]
pub struct JSReadableStream(pub JSObject);

impl JSReadableStream {
    pub fn new(ctx: &JSContext, stream: ReadableStream) -> JSResult<Self> {
        let obj = rong::Class::lookup::<ReadableStream>(ctx)?.instance(stream);
        install_instance_async_iter(ctx, &obj)?;
        Ok(Self(obj))
    }

    pub fn from_receiver(ctx: &JSContext, rx: StreamReceiver) -> JSResult<Self> {
        let stream = ReadableStream::from_receiver(rx);
        Self::new(ctx, stream)
    }

    /// Construct a ReadableStream from a shared receiver slot.
    /// The slot is an Arc<Mutex<Option<Receiver>>> managed by another owner.
    /// This does not consume the channel until the stream is locked via getReader/iteration.
    pub fn from_shared_receiver(ctx: &JSContext, slot: SharedReceiverSlot) -> JSResult<Self> {
        let stream = ReadableStream {
            rx_slot: slot,
            initializer: Rc::new(RefCell::new(None)),
        };
        Self::new(ctx, stream)
    }

    pub fn from_async_reader<R>(ctx: &JSContext, reader: R, chunk_size: usize) -> JSResult<Self>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let registry = ctx
            .runtime()
            .get_or_init_service::<AsyncReaderTaskRegistry>()
            .clone();
        let stream = ReadableStream::from_lazy_receiver(move || {
            let (tx, rx) = mpsc::channel::<StreamChunk>(16);
            let mut reader = reader;
            let task = rong::spawn_local(async move {
                let mut buf = BytesMut::with_capacity(chunk_size.max(1));
                loop {
                    buf.clear();
                    match tokio::io::AsyncReadExt::read_buf(&mut reader, &mut buf).await {
                        Ok(0) => break,
                        Ok(_) => {
                            if tx.send(Ok(buf.split().freeze())).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string())).await;
                            break;
                        }
                    }
                }
            });
            registry.track(task);
            rx
        });
        Self::new(ctx, stream)
    }

    pub fn into_object(self) -> JSObject {
        self.0
    }

    pub fn object(&self) -> JSObject {
        self.0.clone()
    }
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<ReadableStream>()?;
    ctx.register_hidden_class::<ReadableStreamDefaultReader>()?;
    ctx.register_hidden_class::<ReadableStreamDefaultController>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn lazy_stream_can_be_consumed_before_a_reader_is_created() {
        let stream = ReadableStream::from_lazy_receiver(|| {
            let (_tx, rx) = mpsc::channel(1);
            rx
        });

        assert!(!readable_stream_is_locked(&stream));
        assert!(readable_stream_take_receiver(&stream).is_some());
        assert!(readable_stream_is_locked(&stream));
    }

    fn run_stream_suite(unit: &str) {
        let unit = unit.to_string();
        async_run!(|ctx: JSContext| async move {
            rong_assert::init(&ctx)?;
            rong_abort::init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_encoding::init(&ctx)?;
            rong_timer::init(&ctx)?;
            crate::init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, &unit).await?.run().await?;
            assert!(passed);
            Ok(())
        });
    }

    #[test]
    fn test_stream_js() {
        run_stream_suite("stream.js");
    }

    #[test]
    fn test_stream_compression_js() {
        run_stream_suite("stream_compression.js");
    }
}
