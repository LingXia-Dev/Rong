use crate::{
    FromJSValue, HostError, JSContext, JSContextImpl, JSContextService, JSFunc, JSObject,
    JSObjectOps, JSResult, JSRuntimeService, JSTypeOf, JSValue, JSValueImpl, JSValueMapper,
    Promise,
};
use futures::Future;
use std::{
    collections::{HashMap, VecDeque},
    pin::Pin,
    sync::Arc,
};
use tokio::sync::{Mutex, mpsc, oneshot};
use tracing::error;

/// Runtime-level gate: hard serialization at engine entrance.
#[derive(Clone, Default)]
pub struct JsInvokeGate(pub Arc<Mutex<()>>);
impl JSRuntimeService for JsInvokeGate {}

/// Soft invoke queue with priority and event coalescing, per JSContext.
#[derive(Clone)]
pub struct JsInvokeQueue {
    tx: mpsc::Sender<QueueItem>,
}

impl JSContextService for JsInvokeQueue {}

impl JsInvokeQueue {
    fn get_or_init<C>(ctx: &JSContext<C>) -> Self
    where
        C: JSContextImpl,
    {
        if let Some(queue) = ctx.get_service::<Self>() {
            return queue.clone();
        }

        let (tx, rx) = mpsc::channel::<QueueItem>(1024);
        let queue = Self { tx };
        ctx.set_service(queue.clone());
        ctx.spawn_task(Self::run(rx));
        queue
    }

    async fn run(mut rx: mpsc::Receiver<QueueItem>) {
        let mut q_high: VecDeque<QueueItem> = VecDeque::new();
        let mut q_norm: VecDeque<QueueItem> = VecDeque::new();
        let mut q_event: VecDeque<QueueItem> = VecDeque::new();
        let mut event_gen: HashMap<String, u64> = HashMap::new();
        let mut next_gen: u64 = 1;

        const DRAIN_BATCH_LIMIT: usize = 64;

        loop {
            // Phase 1: Drain available incoming items (non-blocking, capped).
            // Cap prevents starvation of Phase 2 under sustained high throughput.
            for _ in 0..DRAIN_BATCH_LIMIT {
                match rx.try_recv() {
                    Ok(item) => Self::enqueue_item(
                        item,
                        &mut q_high,
                        &mut q_norm,
                        &mut q_event,
                        &mut event_gen,
                        &mut next_gen,
                    ),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => return,
                }
            }

            // Phase 2: Process one item from priority queues.
            if let Some(item) =
                Self::dequeue_item(&mut q_high, &mut q_norm, &mut q_event, &event_gen)
            {
                // Clean up the generation entry now that the event is consumed.
                if let Some(key) = &item.dedup_key
                    && let Some(&latest) = event_gen.get(key)
                    && latest == item.generation
                {
                    event_gen.remove(key);
                }

                (item.cb)().await;
                // After processing, loop back to drain more incoming + process more.
                continue;
            }

            // Phase 3: Nothing queued — wait for the next incoming item.
            match rx.recv().await {
                Some(item) => Self::enqueue_item(
                    item,
                    &mut q_high,
                    &mut q_norm,
                    &mut q_event,
                    &mut event_gen,
                    &mut next_gen,
                ),
                None => break, // Channel closed.
            }
        }
    }

    /// Route an item into the correct priority queue.
    fn enqueue_item(
        mut item: QueueItem,
        q_high: &mut VecDeque<QueueItem>,
        q_norm: &mut VecDeque<QueueItem>,
        q_event: &mut VecDeque<QueueItem>,
        event_gen: &mut HashMap<String, u64>,
        next_gen: &mut u64,
    ) {
        match item.priority {
            JsInvokePriority::High => q_high.push_back(item),
            JsInvokePriority::Normal => q_norm.push_back(item),
            JsInvokePriority::Event => {
                if let Some(key) = item.dedup_key.clone() {
                    let generation = *next_gen;
                    *next_gen += 1;
                    event_gen.insert(key, generation);
                    item.generation = generation;
                }
                q_event.push_back(item);
            }
        }
    }

    /// Pop the highest-priority ready item. Events use last-wins dedup.
    fn dequeue_item(
        q_high: &mut VecDeque<QueueItem>,
        q_norm: &mut VecDeque<QueueItem>,
        q_event: &mut VecDeque<QueueItem>,
        event_gen: &HashMap<String, u64>,
    ) -> Option<QueueItem> {
        if let item @ Some(_) = q_high.pop_front() {
            return item;
        }
        if let item @ Some(_) = q_norm.pop_front() {
            return item;
        }
        // For events, skip stale entries (superseded by newer same-key events).
        while let Some(ev) = q_event.pop_front() {
            if let Some(key) = &ev.dedup_key
                && let Some(&latest) = event_gen.get(key)
                && ev.generation != latest
            {
                continue; // Stale — skip.
            }
            return Some(ev);
        }
        None
    }
}

/// Invocation priority
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsInvokePriority {
    High,
    Normal,
    Event,
}

type InvokeFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;
type InvokeFn = Box<dyn FnOnce() -> InvokeFuture + 'static>;

struct QueueItem {
    priority: JsInvokePriority,
    dedup_key: Option<String>,
    generation: u64,
    cb: InvokeFn,
}

/// Enqueue a JS function invocation with priority/coalescing.
pub async fn enqueue_js_invoke<C>(
    ctx: &JSContext<C>,
    func: JSFunc<C::Value>,
    this_obj: Option<JSObject<C::Value>>,
    args_obj: Option<JSObject<C::Value>>,
    priority: JsInvokePriority,
    dedup_key: Option<String>,
    must_reply: bool,
) -> JSResult<()>
where
    C: JSContextImpl + 'static,
    C::Value: JSValueImpl + JSObjectOps + JSTypeOf + 'static,
    C::Runtime: 'static,
{
    let queue = JsInvokeQueue::get_or_init(ctx);

    let (reply_tx, reply_rx) = if must_reply {
        let (tx, rx) = oneshot::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    let cb: InvokeFn = Box::new(move || {
        Box::pin(async move {
            if priority == JsInvokePriority::Event {
                let result = js_invoke_async::<C, ()>(func, this_obj, args_obj).await;
                if let Some(tx) = reply_tx {
                    let _ = tx.send(result);
                } else if let Err(err) = result {
                    error!(target: "rong", error = ?err, "queued js event invoke failed");
                }
                return;
            }

            match js_invoke_start::<C>(func, this_obj, args_obj).await {
                Ok(Some(promise)) => {
                    let task_ctx = promise.context();
                    if let Some(tx) = reply_tx {
                        task_ctx.spawn_task(async move {
                            let _ = tx.send(promise.into_future::<()>().await);
                        });
                    } else {
                        task_ctx.spawn_task(async move {
                            if let Err(err) = promise.into_future::<()>().await {
                                error!(target: "rong", error = ?err, "background js invoke failed");
                            }
                        });
                    }
                }
                Ok(None) => {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(Ok(()));
                    }
                }
                Err(err) => {
                    if let Some(tx) = reply_tx {
                        let _ = tx.send(Err(err));
                    } else {
                        error!(target: "rong", error = ?err, "queued js invoke failed");
                    }
                }
            }
        })
    });

    let item = QueueItem {
        priority,
        dedup_key,
        generation: 0,
        cb,
    };
    queue
        .tx
        .send(item)
        .await
        .map_err(|_| HostError::new(crate::error::E_INTERNAL, "invoke queue closed"))?;

    if let Some(rx) = reply_rx {
        rx.await.unwrap_or_else(|_| {
            Err(HostError::new(crate::error::E_INTERNAL, "invoke queue reply dropped").into())
        })
    } else {
        Ok(())
    }
}

async fn js_invoke_start<C>(
    func: JSFunc<C::Value>,
    this_obj: Option<JSObject<C::Value>>,
    args_obj: Option<JSObject<C::Value>>,
) -> JSResult<Option<Promise<C::Value>>>
where
    C: JSContextImpl,
    C::Value: JSValueImpl + JSObjectOps + JSTypeOf + 'static,
    C::Runtime: 'static,
{
    let gate = {
        let ctx = func.context();
        ctx.runtime().get_or_init_service::<JsInvokeGate>().clone()
    };
    let result = {
        let _guard = gate.0.lock().await;
        match args_obj {
            Some(obj) => {
                let argv = [obj.into_value()];
                func.invoke_with_argv(this_obj, &argv)
            }
            None => func.invoke_with_argv(this_obj, &[]),
        }
    };

    if result.is_promise() {
        let ctx = func.context();
        Promise::from_js_value(&ctx, JSValue::from_raw(&ctx, result)).map(Some)
    } else {
        result.try_convert::<()>()?;
        Ok(None)
    }
}

/// Dispatch a JS invocation immediately with hard gate (async form).
#[allow(dead_code)]
pub async fn js_invoke_async<C, R>(
    func: JSFunc<C::Value>,
    this_obj: Option<JSObject<C::Value>>,
    args_obj: Option<JSObject<C::Value>>,
) -> JSResult<R>
where
    C: JSContextImpl,
    C::Value: JSValueImpl + JSObjectOps + JSTypeOf + 'static,
    C::Runtime: 'static,
    R: crate::FromJSValue<C::Value> + 'static,
{
    let gate = {
        let ctx = func.context();
        ctx.runtime().get_or_init_service::<JsInvokeGate>().clone()
    };
    let result = {
        let _guard = gate.0.lock().await;
        match args_obj {
            Some(obj) => {
                let argv = [obj.into_value()];
                func.invoke_with_argv(this_obj, &argv)
            }
            None => func.invoke_with_argv(this_obj, &[]),
        }
    };

    if result.is_promise() {
        let promise = {
            let ctx = func.context();
            Promise::from_js_value(&ctx, JSValue::from_raw(&ctx, result))?
        };
        promise.into_future::<R>().await
    } else {
        result.try_convert::<R>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_item(priority: JsInvokePriority, dedup_key: Option<&str>) -> QueueItem {
        QueueItem {
            priority,
            dedup_key: dedup_key.map(str::to_string),
            generation: 0,
            cb: Box::new(|| Box::pin(async {})),
        }
    }

    #[test]
    fn dequeue_prefers_high_then_normal_then_latest_event() {
        let mut q_high = VecDeque::new();
        let mut q_norm = VecDeque::new();
        let mut q_event = VecDeque::new();
        let mut event_gen = HashMap::new();
        let mut next_gen = 1;

        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::Event, Some("view:update")),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );
        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::Normal, None),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );
        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::Event, Some("view:update")),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );
        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::High, None),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );

        let high = JsInvokeQueue::dequeue_item(&mut q_high, &mut q_norm, &mut q_event, &event_gen)
            .expect("high-priority item");
        assert_eq!(high.priority, JsInvokePriority::High);

        let normal =
            JsInvokeQueue::dequeue_item(&mut q_high, &mut q_norm, &mut q_event, &event_gen)
                .expect("normal item");
        assert_eq!(normal.priority, JsInvokePriority::Normal);

        let event = JsInvokeQueue::dequeue_item(&mut q_high, &mut q_norm, &mut q_event, &event_gen)
            .expect("latest event item");
        assert_eq!(event.priority, JsInvokePriority::Event);
        assert_eq!(event.dedup_key.as_deref(), Some("view:update"));
        assert_eq!(event.generation, 2);

        assert!(
            JsInvokeQueue::dequeue_item(&mut q_high, &mut q_norm, &mut q_event, &event_gen)
                .is_none(),
            "stale event should have been skipped"
        );
    }

    #[test]
    fn enqueue_event_updates_generation_per_key() {
        let mut q_high = VecDeque::new();
        let mut q_norm = VecDeque::new();
        let mut q_event = VecDeque::new();
        let mut event_gen = HashMap::new();
        let mut next_gen = 1;

        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::Event, Some("same-key")),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );
        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::Event, Some("same-key")),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );
        JsInvokeQueue::enqueue_item(
            noop_item(JsInvokePriority::Event, Some("other-key")),
            &mut q_high,
            &mut q_norm,
            &mut q_event,
            &mut event_gen,
            &mut next_gen,
        );

        assert_eq!(event_gen.get("same-key"), Some(&2));
        assert_eq!(event_gen.get("other-key"), Some(&3));
        assert_eq!(next_gen, 4);
        assert_eq!(q_event.len(), 3);
    }
}
