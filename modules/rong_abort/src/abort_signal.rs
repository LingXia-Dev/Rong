use rong::{function::*, *};
use rong_event::{Emitter, EmitterExt, EventEmitter, EventKey};
use rong_exception::{DOMException, DOMExceptionName};
use std::rc::Rc;
use std::sync::Mutex;
use tokio::sync::watch;

// The AbortSignal interface represents a signal object that allows you to communicate
// with an asynchronous operation (such as a fetch request) and abort it if required
// via an AbortController object
#[js_class(clone)]
pub struct AbortSignal {
    inner: Rc<Mutex<AbortSignalInner>>,
}

struct AbortSignalInner {
    aborted: bool,

    // The reason why the operation was aborted, which can be any JavaScript value
    // default value is UNDEFINED
    reason: JSValue,

    emitter: EventEmitter,
    sender: watch::Sender<JSValue>,
}

/// A structure with one sender and multiple receivers
/// Uses watch channel to implement, where the sender can send abort signals and
/// multiple receivers can subscribe and receive the signal
#[derive(Clone, Debug)]
pub struct AbortReceiver {
    inner: watch::Receiver<JSValue>,
}

impl AbortSignal {
    fn lock_inner(&self) -> std::sync::MutexGuard<'_, AbortSignalInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn new(ctx: &JSContext) -> Self {
        let (sender, _) = watch::channel(JSValue::undefined(ctx));
        Self {
            inner: Rc::new(Mutex::new(AbortSignalInner {
                emitter: EventEmitter::new(),
                aborted: false,
                reason: JSValue::undefined(ctx),
                sender,
            })),
        }
    }

    /// Creates a new receiver
    /// Each receiver subscribes to the same sender, implementing a one-to-many
    /// communication pattern
    #[must_use]
    pub fn subscribe(&self) -> AbortReceiver {
        let inner = self.lock_inner();
        let recv = inner.sender.subscribe();
        if inner.aborted {
            let reason = inner.reason.clone();
            let _ = inner.sender.send(reason);
        }
        AbortReceiver { inner: recv }
    }

    /// Sends an abort signal
    /// If the signal hasn't been sent yet, it will be sent to all subscribed receivers
    /// Uses watch channel to ensure all receivers receive the same signal
    pub fn notify_abort(&self, abort: JSValue) -> JSResult<()> {
        let inner = self.lock_inner();
        // Always try to send the signal if there are active receivers
        if inner.sender.receiver_count() > 0 {
            let _ = inner.sender.send(abort);
        }
        Ok(())
    }
}

impl AbortReceiver {
    /// Receives an abort signal
    pub async fn recv(&mut self) -> JSValue {
        loop {
            // get the current value in the channel. However, this value might still
            // be the initial undefined value if no abort signal has been sent yet.
            let value = self.inner.borrow().clone();
            if !value.is_undefined() {
                return value;
            }
            // waits for the next change to the value
            let _ = self.inner.changed().await;
        }
    }

    /// Mark the current abort reason (if any) for the GC
    pub fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        let borrow = self.inner.borrow();
        if !borrow.is_undefined() {
            // Do not clone during GC marking; pass the borrowed JSValue reference
            mark_fn(&borrow);
        }
    }
}

#[js_class]
impl AbortSignal {
    #[js_method(constructor)]
    fn constructor() -> JSResult<()> {
        Err(HostError::new(
            rong::error::E_ILLEGAL_CONSTRUCTOR,
            "Failed to construct 'AbortSignal': Illegal constructor",
        )
        .with_name("TypeError")
        .into())
    }

    #[js_method(getter, enumerable, rename = "onabort")]
    fn get_on_abort(&self) -> Option<JSFunc> {
        let inner = self.inner.lock().ok()?;
        inner.emitter.get_listener(&EventKey::from("abort"))
    }

    #[js_method(setter, rename = "onabort")]
    fn set_on_abort(&self, this: This<JSObject>, listener: JSFunc) -> JSResult<()> {
        let key = EventKey::from("abort");
        Self::add_event_listener(this, key, listener, false, false)?;
        Ok(())
    }

    #[js_method(getter, enumerable)]
    pub fn aborted(&self) -> bool {
        self.lock_inner().aborted
    }

    #[js_method(getter, enumerable, rename = "reason")]
    pub fn get_reason(&self) -> JSValue {
        let inner = self.lock_inner();
        inner.reason.clone()
    }

    #[js_method(setter, rename = "reason")]
    pub(crate) fn set_reason(&self, reason: Optional<JSValue>) {
        let mut inner = self.lock_inner();
        if let Some(r) = reason.0 {
            inner.reason = r;
        }
    }

    #[js_method(rename = "throwIfAborted")]
    fn throw_if_aborted(&self, ctx: JSContext) -> JSValue {
        let inner = self.lock_inner();
        if inner.aborted && !inner.reason.is_undefined() {
            ctx.throw(inner.reason.clone())
        } else {
            JSValue::undefined(&ctx)
        }
    }

    /// static method takes an iterable of abort signals and returns an AbortSignal.
    /// The returned abort signal is aborted when any of the input iterable abort
    /// signals are aborted. The abort reason will be set to the reason of the first
    /// signal that is aborted. If any of the given abort signals are already aborted
    /// then so will be the returned AbortSignal.
    #[js_method]
    fn any(ctx: JSContext, signals: JSArray) -> JSResult<JSObject> {
        let new_signal = AbortSignal::new(&ctx);
        let class = Class::lookup::<AbortSignal>(&ctx)?;
        let mut unaborted_signals = Vec::with_capacity(signals.len()? as usize);

        for item in signals.iter::<JSObject>()? {
            let signal = item?;
            let borrow = signal.borrow_mut::<AbortSignal>()?;

            if borrow.aborted() {
                {
                    let mut inner = new_signal.lock_inner();
                    inner.aborted = true;
                    inner.reason = borrow.get_reason();
                }
                let new_signal = class.instance::<AbortSignal>(new_signal);
                return Ok(new_signal);
            } else {
                drop(borrow);
                unaborted_signals.push(signal);
            }
        }

        let new_signal = class.instance::<AbortSignal>(new_signal);

        for signal in unaborted_signals {
            let to_abort = new_signal.clone();
            let ctx_for_closure = ctx.clone();

            let notifier = JSFunc::new_once(&ctx, move |signal: This<JSObject>| -> JSResult<()> {
                let signal_obj = signal.borrow::<AbortSignal>()?;
                let reason = signal_obj.get_reason();
                drop(signal_obj);

                let to_abort_obj = to_abort.borrow_mut::<AbortSignal>()?;
                {
                    let mut inner = to_abort_obj.lock_inner();
                    inner.aborted = true;
                    inner.reason = reason;
                }
                drop(to_abort_obj);

                Self::broadcast_abort(&ctx_for_closure, This(to_abort))
            })?;
            Self::add_event_listener(This(signal), EventKey::from("abort"), notifier, false, true)?;
        }
        Ok(new_signal)
    }

    /// static method returns an AbortSignal that is already set as aborted, and
    /// which does not trigger an abort event
    #[js_method]
    fn abort(ctx: JSContext, reason: Optional<JSValue>) -> JSResult<AbortSignal> {
        let signal = Self::new(&ctx);
        signal.set_reason(reason);
        {
            let mut inner = signal
                .inner
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "AbortSignal is poisoned"))?;
            inner.aborted = true;
        }
        Ok(signal)
    }

    /// static method returns an AbortSignal that will automatically abort after a specified time
    /// The signal aborts with a TimeoutError DOMException on timeout.
    /// The "active" time in milliseconds before the returned AbortSignal will abort
    #[js_method]
    fn timeout(ctx: JSContext, time: u64) -> JSResult<JSObject> {
        let signal = Self::new(&ctx);
        let timeout_error = get_reason_or_dom_exception(&ctx, None, DOMExceptionName::TIMEOUT_ERR)?;
        {
            let mut inner = signal.lock_inner();
            inner.reason = timeout_error;
        }

        let instance = Class::lookup::<AbortSignal>(&ctx)?.instance(signal);
        let instance_clone = instance.clone();

        ctx.spawn_task(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(time)).await;
            // Route through broadcast_abort so Rust subscribers on the
            // internal watch channel (via `signal.subscribe()`) are woken
            // alongside JS event listeners. Setting `inner.aborted = true`
            // without `notify_abort` leaves Rust receivers permanently
            // pending.
            let task_ctx = instance_clone.context();
            let _ = Self::broadcast_abort(&task_ctx, This(instance_clone));
        });

        Ok(instance)
    }

    /// send abort signal to this
    pub(crate) fn broadcast_abort(ctx: &JSContext, this: This<JSObject>) -> JSResult<()> {
        let borrow = this.borrow_mut::<AbortSignal>()?;
        let reason = get_reason_or_dom_exception(
            ctx,
            Some(borrow.lock_inner().reason.clone()),
            DOMExceptionName::ABORT_ERR,
        )?;

        // Update observable state before emitting the event so handlers see `aborted === true`.
        {
            let mut inner = borrow.lock_inner();
            inner.aborted = true;
            inner.reason = reason.clone();
        }

        borrow.notify_abort(reason)?;

        // Drop borrow before emitting event to allow event handlers to access AbortSignal
        drop(borrow);

        Self::do_emit(This(this.0.clone()), EventKey::from("abort"), Rest(vec![]))?;
        Ok(())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Ok(inner) = self.inner.lock() {
            if !inner.reason.is_undefined() {
                mark_fn(&inner.reason);
            }
            inner.emitter.gc_mark_with(mark_fn);
        }
    }
}

fn get_reason_or_dom_exception(
    ctx: &JSContext,
    reason: Option<JSValue>,
    name: DOMExceptionName,
) -> JSResult<JSValue> {
    let reason = match reason {
        Some(r) if !r.is_undefined() => r,
        _ => DOMException::create(ctx, "", name)?.into_js_value(),
    };
    Ok(reason)
}

impl Emitter for AbortSignal {
    fn get_event_emitter(&self) -> EventEmitter {
        self.lock_inner().emitter.clone()
    }
}
