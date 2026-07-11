use bytes::Bytes;
use rong::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::AsyncWrite;
use tokio::sync::{Mutex, mpsc, oneshot};

type ByteSender = mpsc::Sender<Bytes>;
type DoneReceiver = oneshot::Receiver<Result<(), String>>;
type SharedSenderSlot = Arc<StdMutex<Option<ByteSender>>>;
type SharedDoneSlot = Arc<StdMutex<Option<DoneReceiver>>>;
type SharedErrorSlot = Arc<StdMutex<Option<String>>>;
type SinkSlot = Rc<StdMutex<Option<JSObject>>>;
type WriterChannel = (ByteSender, Option<DoneReceiver>);

#[js_class]
pub struct WritableStream {
    // A single writer lock; getWriter() takes ownership.
    pub(crate) tx_slot: SharedSenderSlot,
    // Optional completion signal for async writer (Some when created via to_async_writer)
    pub(crate) done_slot: SharedDoneSlot,
    // Error slot to surface background write failures to the front-end
    pub(crate) err_slot: SharedErrorSlot,
    // For JS-underlying sink mode
    sink_slot: SinkSlot,
}

#[js_class]
pub struct WritableStreamDefaultWriter {
    // Reference back to the stream's slot to support releaseLock
    slot: SharedSenderSlot,
    // Sender owned while the writer is locked
    tx: Arc<Mutex<Option<ByteSender>>>,
    // Optional completion signal for close()
    done_rx: Arc<StdMutex<Option<DoneReceiver>>>,
    // Reference to stream's done slot to return it on releaseLock
    done_slot_ref: SharedDoneSlot,
    // Shared error slot to report background errors
    err_slot: SharedErrorSlot,
    // For JS sink mode: reference back to stream's sink slot and the held sink object
    sink_slot_ref: SinkSlot,
    sink_obj: RefCell<Option<JSObject>>,
}

impl WritableStream {
    pub fn to_sender(tx: ByteSender) -> Self {
        Self {
            tx_slot: Arc::new(StdMutex::new(Some(tx))),
            done_slot: Arc::new(StdMutex::new(None)),
            err_slot: Arc::new(StdMutex::new(None)),
            sink_slot: Rc::new(StdMutex::new(None)),
        }
    }

    pub fn to_async_writer<W>(mut writer: W) -> Self
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<Bytes>(16);
        let (done_tx, done_rx) = oneshot::channel::<Result<(), String>>();
        let err_slot: SharedErrorSlot = Arc::new(StdMutex::new(None));
        let err_slot_for_task = err_slot.clone();

        tokio::task::spawn(async move {
            let mut error: Option<String> = None;
            while let Some(chunk) = rx.recv().await {
                if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut writer, &chunk).await {
                    error = Some(e.to_string());
                    break;
                }
            }
            if let Err(e) = tokio::io::AsyncWriteExt::flush(&mut writer).await
                && error.is_none()
            {
                error = Some(e.to_string());
            }
            if let Some(e) = error.as_ref()
                && let Ok(mut g) = err_slot_for_task.lock()
            {
                *g = Some(e.clone());
            }
            let _ = done_tx.send(match error {
                Some(e) => Err(e),
                None => Ok(()),
            });
        });

        Self {
            tx_slot: Arc::new(StdMutex::new(Some(tx))),
            done_slot: Arc::new(StdMutex::new(Some(done_rx))),
            err_slot,
            sink_slot: Rc::new(StdMutex::new(None)),
        }
    }
}

#[js_class]
impl WritableStream {
    #[js_method(constructor)]
    fn new(underlying: function::Optional<JSValue>) -> JSResult<Self> {
        // If underlying sink provided, store it; else make a dummy sink without channels
        let sink = match underlying.0 {
            Some(v) => v.into_object(),
            None => None,
        };
        Ok(Self {
            tx_slot: Arc::new(StdMutex::new(None)),
            done_slot: Arc::new(StdMutex::new(None)),
            err_slot: Arc::new(StdMutex::new(None)),
            sink_slot: Rc::new(StdMutex::new(sink)),
        })
    }

    #[js_method(rename = "getWriter")]
    pub fn get_writer(&self) -> JSResult<WritableStreamDefaultWriter> {
        let mut guard = self
            .tx_slot
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        match guard.take() {
            Some(tx) => {
                // Take done_rx if any (only for async writer)
                let done_rx = {
                    let mut d = self.done_slot.lock().map_err(|_| {
                        HostError::new(rong::error::E_INTERNAL, "Stream is poisoned")
                    })?;
                    d.take()
                };
                Ok(WritableStreamDefaultWriter {
                    slot: self.tx_slot.clone(),
                    tx: Arc::new(Mutex::new(Some(tx))),
                    done_rx: Arc::new(StdMutex::new(done_rx)),
                    done_slot_ref: self.done_slot.clone(),
                    err_slot: self.err_slot.clone(),
                    sink_slot_ref: self.sink_slot.clone(),
                    sink_obj: RefCell::new(None),
                })
            }
            None => {
                // Fall back to JS sink mode
                let mut sink_guard = self
                    .sink_slot
                    .lock()
                    .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
                if sink_guard.is_none() {
                    return Err(HostError::new(
                        rong::error::E_INVALID_STATE,
                        "WritableStream is locked",
                    )
                    .with_name("TypeError")
                    .into());
                }
                let obj = sink_guard.take().ok_or_else(|| {
                    HostError::new(rong::error::E_INTERNAL, "WritableStream sink missing")
                })?;
                Ok(WritableStreamDefaultWriter {
                    slot: self.tx_slot.clone(),
                    tx: Arc::new(Mutex::new(None)),
                    done_rx: Arc::new(StdMutex::new(None)),
                    done_slot_ref: self.done_slot.clone(),
                    err_slot: self.err_slot.clone(),
                    sink_slot_ref: self.sink_slot.clone(),
                    sink_obj: RefCell::new(Some(obj)),
                })
            }
        }
    }

    #[js_method]
    fn abort(&self) -> JSResult<()> {
        let mut guard = self
            .tx_slot
            .lock()
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
        *guard = None;
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Ok(sink_slot) = self.sink_slot.lock()
            && let Some(sink) = sink_slot.as_ref()
        {
            mark_fn(sink.as_js_value());
        }
    }
}

#[js_class]
impl WritableStreamDefaultWriter {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor("Illegal constructor")
    }

    #[js_method]
    pub async fn write(&self, chunk: JSValue) -> JSResult<()> {
        // Surface background error if any
        if let Ok(err_guard) = self.err_slot.lock()
            && let Some(e) = err_guard.as_ref()
        {
            return Err(HostError::new(
                rong::error::E_STREAM,
                format!("WritableStream error: {}", e),
            )
            .into());
        }
        // If we have a JS sink, call its write method in JS thread
        let sink_obj = self.sink_obj.borrow().clone();
        if let Some(sink_obj) = sink_obj
            && let Ok(write_fn) = sink_obj.get::<_, JSFunc>("write")
        {
            // Await if it returns a Promise
            let _r: JSValue = write_fn.call_async(None, (chunk,)).await?;
            return Ok(());
        }

        // Channel mode
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
                    "write expects Uint8Array or ArrayBuffer",
                )
                .with_name("TypeError")
                .into());
            }
        } else {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "write expects a TypedArray or ArrayBuffer",
            )
            .with_name("TypeError")
            .into());
        };
        let mut slot = self.tx.lock().await;
        match slot.as_mut() {
            Some(tx) => tx.send(bytes).await.map_err(|_| {
                HostError::new(rong::error::E_INVALID_STATE, "WritableStream closed").into()
            }),
            None => Err(HostError::new(
                rong::error::E_INVALID_STATE,
                "Writer not acquired or closed",
            )
            .into()),
        }
    }

    #[js_method]
    pub async fn close(&self) -> JSResult<()> {
        // If JS sink has close
        let sink_obj = self.sink_obj.borrow().clone();
        if let Some(sink_obj) = sink_obj
            && let Ok(close_fn) = sink_obj.get::<_, JSFunc>("close")
        {
            let _r: JSValue = close_fn.call_async(None, ()).await?;
        }

        // Channel mode: drop sender and await completion if possible
        {
            let mut slot = self.tx.lock().await;
            *slot = None;
        }
        let rx_opt = {
            let mut d = self
                .done_rx
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
            d.take()
        };
        if let Some(rx) = rx_opt {
            match rx.await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(HostError::new(rong::error::E_STREAM, e).into()),
                Err(_) => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    #[js_method]
    pub async fn abort(&self) -> JSResult<()> {
        let sink_obj = self.sink_obj.borrow().clone();
        if let Some(sink_obj) = sink_obj
            && let Ok(abort_fn) = sink_obj.get::<_, JSFunc>("abort")
        {
            let _r: JSValue = abort_fn.call_async(None, ()).await?;
        }

        let mut slot = self.tx.lock().await;
        *slot = None;
        Ok(())
    }

    #[js_method(rename = "releaseLock")]
    pub async fn release_lock(&self) -> JSResult<()> {
        // Take JS sink (if any) first so we don't hold a RefCell borrow across await.
        let sink_opt = self.sink_obj.borrow_mut().take();

        // Take back sender and return it to the stream's slot
        let tx_opt = {
            let mut slot = self.tx.lock().await;
            slot.take()
        };
        if let Some(tx) = tx_opt {
            let mut guard = self
                .slot
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
            if guard.is_none() {
                *guard = Some(tx);
            }
        }
        // Also return done_rx if any so next writer can await close
        let done_opt = {
            let mut d = self
                .done_rx
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
            d.take()
        };
        if let Some(done) = done_opt {
            let mut g = self
                .done_slot_ref
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
            if g.is_none() {
                *g = Some(done);
            }
        }
        // Return JS sink to stream slot if present
        if let Some(sink_obj) = sink_opt {
            let mut slot = self
                .sink_slot_ref
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Stream is poisoned"))?;
            if slot.is_none() {
                *slot = Some(sink_obj);
            }
        }
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Some(sink_obj) = self.sink_obj.borrow().as_ref() {
            mark_fn(sink_obj.as_js_value());
        }
        if let Ok(sink_slot) = self.sink_slot_ref.lock()
            && let Some(sink_obj) = sink_slot.as_ref()
        {
            mark_fn(sink_obj.as_js_value());
        }
    }
}

// Internal helpers for native fast paths
impl WritableStreamDefaultWriter {
    // Expose the underlying channel sender and done signal for intra-crate optimizations
    pub(crate) fn take_channel(&mut self) -> Option<WriterChannel> {
        // Only available when the writer is channel-backed (not JS sink)
        if self.sink_obj.borrow().is_some() {
            return None;
        }
        // Take ownership of the sender
        let tx_opt = futures::executor::block_on(async { self.tx.lock().await.take() });
        if let Some(tx) = tx_opt {
            // Also take the done receiver if any
            let done_opt = self.done_rx.lock().ok().and_then(|mut g| g.take());
            Some((tx, done_opt))
        } else {
            None
        }
    }
}

// Public Rust helpers for other modules
pub fn writable_stream_to_sender(tx: mpsc::Sender<Bytes>) -> WritableStream {
    WritableStream::to_sender(tx)
}

pub fn writable_stream_to_sender_with_done(
    tx: ByteSender,
    done_rx: DoneReceiver,
) -> WritableStream {
    WritableStream {
        tx_slot: Arc::new(StdMutex::new(Some(tx))),
        done_slot: Arc::new(StdMutex::new(Some(done_rx))),
        err_slot: Arc::new(StdMutex::new(None)),
        sink_slot: Rc::new(StdMutex::new(None)),
    }
}

pub fn writable_stream_to_async_write<W>(writer: W) -> WritableStream
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    WritableStream::to_async_writer(writer)
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<WritableStream>()?;
    ctx.register_hidden_class::<WritableStreamDefaultWriter>()?;
    Ok(())
}

/// Wrapper helper for clearer semantics
#[derive(Clone)]
pub struct JSWritableStream(pub JSObject);

impl JSWritableStream {
    pub fn new(ctx: &JSContext, stream: WritableStream) -> JSResult<Self> {
        let obj = rong::Class::lookup::<WritableStream>(ctx)?.instance(stream);
        Ok(Self(obj))
    }

    pub fn from_async_writer<W>(ctx: &JSContext, writer: W) -> JSResult<Self>
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let stream = WritableStream::to_async_writer(writer);
        Self::new(ctx, stream)
    }

    pub fn into_object(self) -> JSObject {
        self.0
    }

    pub fn object(&self) -> JSObject {
        self.0.clone()
    }
}
