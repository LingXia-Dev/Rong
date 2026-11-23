//! Promise rejections that nothing handles, reported to the embedder once the
//! job queue has drained.
//!
//! Engines report two facts as they happen: a promise was rejected with no
//! handler, and a handler was added to a promise rejected without one. A
//! rejection that gets its handler before the drain ends is dropped; the rest
//! reach the context's tracker in the order they happened.

use crate::{JSContext, JSContextImpl, JSObject, JSResult, JSRuntime, JSValue, JSValueImpl};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Which rejection events an engine can report, or a tracker asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RejectionEvents {
    /// A promise was rejected and had no handler once the job queue drained.
    pub unhandled: bool,
    /// A promise still unhandled when an earlier drain ended got a handler.
    pub handled_later: bool,
}

impl RejectionEvents {
    pub const NONE: Self = Self {
        unhandled: false,
        handled_later: false,
    };
    pub const UNHANDLED: Self = Self {
        unhandled: true,
        handled_later: false,
    };
    pub const ALL: Self = Self {
        unhandled: true,
        handled_later: true,
    };

    pub(crate) fn covers(self, wanted: Self) -> bool {
        (self.unhandled || !wanted.unhandled) && (self.handled_later || !wanted.handled_later)
    }
}

/// One event delivered to a rejection tracker.
pub enum RejectionEvent<V: JSValueImpl> {
    /// `promise` was rejected with `reason` and nothing handled it this turn.
    Unhandled {
        promise: JSObject<V>,
        reason: JSValue<V>,
    },
    /// `promise`, unhandled when an earlier drain ended, has a handler now.
    /// Delivered whether or not the tracker asked for `Unhandled`.
    HandledLater {
        promise: JSObject<V>,
        reason: JSValue<V>,
    },
}

/// What an engine's hook saw; see [`record`].
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectionHook {
    /// Rejected while it had no handler.
    Rejected,
    /// A handler was added to a promise rejected without one.
    HandlerAdded,
}

pub(crate) type Tracker<C> =
    Rc<dyn Fn(&JSContext<C>, RejectionEvent<<C as JSContextImpl>::Value>) -> JSResult<()>>;

pub(crate) struct ContextTracker<C: JSContextImpl> {
    pub(crate) events: RejectionEvents,
    pub(crate) tracker: Tracker<C>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Unhandled,
    HandledLater,
}

struct Pending<V> {
    context: u64,
    kind: Kind,
    promise: V,
    reason: V,
}

/// Rejections of one runtime waiting for the end of the drain.
pub(crate) struct RejectionQueue<V> {
    pending: RefCell<Vec<Pending<V>>>,
    delivering: Cell<bool>,
}

impl<V> Default for RejectionQueue<V> {
    fn default() -> Self {
        Self {
            pending: RefCell::new(Vec::new()),
            delivering: Cell::new(false),
        }
    }
}

impl<V> RejectionQueue<V> {
    /// Drops what a closing context left, while its values can still be freed.
    pub(crate) fn forget_context(&self, context: u64) {
        let dropped: Vec<_> = {
            let mut pending = self.pending.borrow_mut();
            let (gone, kept) = std::mem::take(&mut *pending)
                .into_iter()
                .partition(|p| p.context == context);
            *pending = kept;
            gone
        };
        drop(dropped);
    }

    pub(crate) fn clear(&self) {
        let dropped = std::mem::take(&mut *self.pending.borrow_mut());
        drop(dropped);
    }
}

thread_local! {
    // Trackers registered on this thread, so an engine whose hook is always
    // installed pays one read per rejection while there are none.
    static TRACKERS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) fn tracker_added() {
    TRACKERS.with(|n| n.set(n.get() + 1));
}

pub(crate) fn tracker_removed() {
    TRACKERS.with(|n| n.set(n.get().saturating_sub(1)));
}

/// Whether any context on this thread has a rejection tracker.
#[doc(hidden)]
pub fn tracking_on_this_thread() -> bool {
    TRACKERS.with(Cell::get) > 0
}

/// Called by an engine's rejection hook, from inside the engine, as it sees a
/// rejection or a late handler. Only queues; delivery waits for the drain.
#[doc(hidden)]
pub fn record<C>(ctx: &JSContext<C>, hook: RejectionHook, promise: C::Value, reason: C::Value)
where
    C: JSContextImpl,
{
    if !tracking_on_this_thread() {
        return;
    }
    let context = ctx.serial();
    let tracked = ctx.has_rejection_tracker();
    let runtime = ctx.runtime();
    let mut pending = runtime.rejections.pending.borrow_mut();
    let kind = match hook {
        RejectionHook::Rejected => Kind::Unhandled,
        RejectionHook::HandlerAdded => {
            // Handled before it was reported, possibly from another context of
            // the runtime: neither event is due. Values drop after the borrow
            // ends, since freeing one can run engine code.
            if let Some(i) = pending
                .iter()
                .position(|p| p.kind == Kind::Unhandled && p.promise == promise)
            {
                let handled = pending.remove(i);
                drop(pending);
                drop(handled);
                return;
            }
            Kind::HandledLater
        }
    };
    if tracked {
        pending.push(Pending {
            context,
            kind,
            promise,
            reason,
        });
    }
}

/// Delivers what is queued for `runtime`. Called where the outermost job drain
/// ends. Rejections recorded while trackers run wait for the next drain, so a
/// tracker that rejects again cannot keep this loop going.
pub(crate) fn deliver<C>(runtime: &JSRuntime<C::Runtime>)
where
    C: JSContextImpl,
    C::Value: crate::JSObjectOps,
{
    let queue = &runtime.rejections;
    if queue.delivering.get() || queue.pending.borrow().is_empty() {
        return;
    }
    queue.delivering.set(true);
    let _reset = Reset(&queue.delivering);
    let batch = std::mem::take(&mut *queue.pending.borrow_mut());
    for item in batch {
        let Some(target) = JSContext::<C>::of_runtime(runtime, item.context) else {
            continue;
        };
        let Some((events, tracker)) = target.rejection_tracker() else {
            continue;
        };
        let wanted = match item.kind {
            Kind::Unhandled => events.unhandled,
            Kind::HandledLater => events.handled_later,
        };
        if !wanted {
            continue;
        }
        let Some(promise) = JSValue::from_raw(&target, item.promise).into_object() else {
            continue;
        };
        let reason = JSValue::from_raw(&target, item.reason);
        let event = match item.kind {
            Kind::Unhandled => RejectionEvent::Unhandled { promise, reason },
            Kind::HandledLater => RejectionEvent::HandledLater { promise, reason },
        };
        if let Err(err) = tracker(&target, event) {
            tracing::warn!(target: "rong", error = %err, "promise rejection tracker failed");
        }
    }
}

struct Reset<'a>(&'a Cell<bool>);

impl Drop for Reset<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
