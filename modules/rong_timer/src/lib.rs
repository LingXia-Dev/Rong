//! Timer implementation
//!
//! This module provides timer functionality in two shapes:
//! - Callback timers mounted on the global object:
//!   - setTimeout/clearTimeout
//!   - setInterval/clearInterval
//! - Bun-style sleep helpers mounted on `globalThis.Rong`:
//!   - Rong.sleep
//!   - Rong.sleepSync
//!
//! # Limitations
//! - Unlike Web APIs, this implementation does not support passing additional arguments
//!   to the callback function. Only the callback function and delay are supported.
//! - Delay is in milliseconds and should be a positive number.

use rong::{
    JSContext, JSContextService, JSFunc, JSResult, JSValue, RongExecutor, function::Optional,
};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Duration;

mod sleep;

fn lock_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

struct TimerTick {
    id: u32,
    pending: watch::Sender<bool>,
}

#[derive(Clone)]
struct TimerCancellation {
    tx: watch::Sender<bool>,
}

impl TimerCancellation {
    fn new() -> (Self, watch::Receiver<bool>) {
        let (tx, rx) = watch::channel(false);
        (Self { tx }, rx)
    }

    fn cancel(&self) {
        self.tx.send_replace(true);
    }

    async fn cancelled(rx: &mut watch::Receiver<bool>) {
        if *rx.borrow_and_update() {
            return;
        }

        while rx.changed().await.is_ok() {
            if *rx.borrow_and_update() {
                return;
            }
        }
    }
}

struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone)]
pub struct TimerRegistry {
    inner: Rc<TimerRegistryInner>,
}

struct TimerEntry {
    cancel: TimerCancellation,
    // For callback-style timers (global setTimeout/setInterval).
    // Keep callbacks in the context registry so teardown drops them before the JS context.
    callback: Option<JSFunc>,
    repeat: bool,
}

struct TimerRegistryInner {
    next_id: AtomicU32,
    timers: Mutex<HashMap<u32, TimerEntry>>,
    callback_tx: mpsc::UnboundedSender<TimerTick>,
    callback_rx: RefCell<Option<mpsc::UnboundedReceiver<TimerTick>>>,
}

impl JSContextService for TimerRegistry {
    fn on_shutdown(&self) {
        self.shutdown();
    }
}

impl Default for TimerRegistry {
    fn default() -> Self {
        let (callback_tx, callback_rx) = mpsc::unbounded_channel();
        Self {
            inner: Rc::new(TimerRegistryInner {
                next_id: AtomicU32::new(0),
                timers: Mutex::new(HashMap::new()),
                callback_tx,
                callback_rx: RefCell::new(Some(callback_rx)),
            }),
        }
    }
}

impl TimerRegistry {
    fn ensure(ctx: &JSContext) -> Self {
        if let Some(registry) = ctx.get_service::<Self>() {
            return registry.clone();
        }

        let registry = Self::default();
        ctx.set_service(registry.clone());
        registry.start(ctx);
        registry
    }

    fn start(&self, ctx: &JSContext) {
        let Some(mut rx) = self.inner.callback_rx.borrow_mut().take() else {
            return;
        };
        let registry = self.clone();

        ctx.spawn_task(async move {
            while let Some(tick) = rx.recv().await {
                let callback = {
                    let mut timers = lock_poison(&registry.inner.timers);
                    let callback = timers
                        .get(&tick.id)
                        .map(|entry| (entry.callback.clone(), entry.repeat));
                    if matches!(callback, Some((_, false))) {
                        timers.remove(&tick.id);
                    }
                    callback
                };

                if let Some((Some(callback), repeat)) = callback {
                    let result = callback.call::<_, JSValue>(None, ());
                    if result.is_err() && repeat {
                        registry.cancel_timer(tick.id);
                    }
                }

                tick.pending.send_replace(false);
            }
        });
    }

    fn callback_tx(&self) -> mpsc::UnboundedSender<TimerTick> {
        self.inner.callback_tx.clone()
    }

    fn next_id(&self) -> u32 {
        self.inner.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn register_timer(&self, id: u32, cancel: TimerCancellation) {
        lock_poison(&self.inner.timers).insert(
            id,
            TimerEntry {
                cancel,
                callback: None,
                repeat: false,
            },
        );
    }

    fn register_timer_with_callback(
        &self,
        id: u32,
        cancel: TimerCancellation,
        callback: JSFunc,
        repeat: bool,
    ) {
        lock_poison(&self.inner.timers).insert(
            id,
            TimerEntry {
                cancel,
                callback: Some(callback),
                repeat,
            },
        );
    }

    fn cancel_timer(&self, id: u32) {
        if let Some(entry) = lock_poison(&self.inner.timers).remove(&id) {
            entry.cancel.cancel();
        }
    }

    fn shutdown(&self) {
        let timers = lock_poison(&self.inner.timers)
            .drain()
            .map(|(_, entry)| entry)
            .collect::<Vec<_>>();
        for entry in timers {
            entry.cancel.cancel();
        }
    }
}

struct TimerRegistration {
    registry: TimerRegistry,
    id: u32,
}

impl Drop for TimerRegistration {
    fn drop(&mut self) {
        self.registry.cancel_timer(self.id);
    }
}

fn set_timeout_with_repeat(
    ctx: &JSContext,
    registry: TimerRegistry,
    callback: JSFunc,
    delay: Optional<f64>,
    repeat: bool,
) -> u32 {
    let id = registry.next_id();
    let (cancel, mut cancel_rx) = TimerCancellation::new();
    let (pending, mut pending_rx) = watch::channel(false);
    let callback_tx = registry.callback_tx();

    registry.register_timer_with_callback(id, cancel.clone(), callback, repeat);
    let delay = delay.unwrap_or(0.0).max(0.0) as u64;
    let interval_duration = Duration::from_millis(delay.max(1));

    let run_timer = async move {
        let send_tick = || -> bool {
            if !pending.send_if_modified(|queued| {
                if *queued {
                    false
                } else {
                    *queued = true;
                    true
                }
            }) {
                return false;
            }

            if callback_tx
                .send(TimerTick {
                    id,
                    pending: pending.clone(),
                })
                .is_err()
            {
                pending.send_replace(false);
                return false;
            }
            true
        };

        if repeat {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval_duration) => {}
                    _ = TimerCancellation::cancelled(&mut cancel_rx) => break,
                }

                if !send_tick() {
                    continue;
                }

                while *pending_rx.borrow_and_update() {
                    tokio::select! {
                        result = pending_rx.changed() => {
                            if result.is_err() {
                                return;
                            }
                        }
                        _ = TimerCancellation::cancelled(&mut cancel_rx) => return,
                    }
                }
            }
            return;
        }

        if delay > 0 {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
                _ = TimerCancellation::cancelled(&mut cancel_rx) => return,
            }
        }
        send_tick();
    };

    let registration = TimerRegistration { registry, id };
    ctx.spawn_task(async move {
        let _registration = registration;
        let mut task = AbortOnDrop(RongExecutor::global().spawn(run_timer));
        let _ = (&mut task.0).await;
    });

    id
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    let registry = TimerRegistry::ensure(ctx);

    let global = ctx.global();

    let registry_clone = registry.clone();
    let set_timeout = JSFunc::new(
        ctx,
        move |ctx: JSContext, callback: JSFunc, delay: Optional<f64>| {
            set_timeout_with_repeat(&ctx, registry_clone.clone(), callback, delay, false)
        },
    );

    let registry_clone = registry.clone();
    let clear_timeout = JSFunc::new(ctx, move |id: JSValue| {
        if let Ok(id) = id.to_rust::<u32>() {
            registry_clone.cancel_timer(id);
        }
    });

    let registry_clone = registry.clone();
    let set_interval = JSFunc::new(
        ctx,
        move |ctx: JSContext, callback: JSFunc, delay: Optional<f64>| {
            set_timeout_with_repeat(&ctx, registry_clone.clone(), callback, delay, true)
        },
    );

    let clear_interval = JSFunc::new(ctx, move |id: JSValue| {
        if let Ok(id) = id.to_rust::<u32>() {
            registry.cancel_timer(id);
        }
    });

    global.set("setTimeout", set_timeout)?;
    global.set("clearTimeout", clear_timeout)?;
    global.set("setInterval", set_interval)?;
    global.set("clearInterval", clear_interval)?;

    sleep::init(ctx)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicI32, Ordering};
    use tokio::time::{Instant, sleep};

    fn run_unit_suite(unit: &str) {
        let unit = unit.to_string();
        async_run!(|ctx: JSContext| async move {
            init(&ctx).unwrap();

            rong_console::init(&ctx)?;
            rong_assert::init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, &unit).await?.run().await?;
            assert!(passed);

            Ok(())
        })
    }

    #[test]
    fn test_set_interval_without_cancel() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx).unwrap();

            let counter = Rc::new(AtomicI32::new(0));
            let counter_clone = counter.clone();

            let increment = JSFunc::new(&ctx, move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
            });
            ctx.global().set("increment", increment)?;

            // Keep the interval handle in scope
            let _interval_id: u32 = ctx
                .eval(Source::from_bytes("setInterval(increment, 50)"))
                .unwrap();

            let deadline = Instant::now() + Duration::from_millis(750);
            while counter.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
                sleep(Duration::from_millis(25)).await;
            }
            let count = counter.load(Ordering::SeqCst);
            assert!(
                count >= 3,
                "Expected at least 3 increments within 750ms, got {}",
                count
            );

            // without cancel explicitly, it should no panic!
            Ok(())
        })
    }

    #[test]
    fn test_timer() {
        for unit in ["timer.js", "sleep.js"] {
            run_unit_suite(unit);
        }
    }

    #[test]
    fn context_drop_cancels_registered_timers() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            let registry = TimerRegistry::ensure(&ctx);

            ctx.eval::<u32>(Source::from_bytes("setInterval(() => {}, 60_000)"))?;
            assert_eq!(lock_poison(&registry.inner.timers).len(), 1);

            drop(ctx);

            assert!(
                lock_poison(&registry.inner.timers).is_empty(),
                "dropping a JS context must cancel its registered timers"
            );
            Ok(())
        });
    }

    #[test]
    fn context_task_shutdown_unregisters_pending_sleep() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            let registry = TimerRegistry::ensure(&ctx);

            ctx.eval::<JSValue>(Source::from_bytes("Rong.sleep(60_000)"))?;
            tokio::task::yield_now().await;
            assert_eq!(lock_poison(&registry.inner.timers).len(), 1);

            ctx.shutdown_tasks().await;

            assert!(
                lock_poison(&registry.inner.timers).is_empty(),
                "canceling Rong.sleep must unregister its timer"
            );
            Ok(())
        });
    }

    #[test]
    fn context_task_shutdown_unregisters_callback_timer() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            let registry = TimerRegistry::ensure(&ctx);

            ctx.eval::<u32>(Source::from_bytes("setInterval(() => {}, 60_000)"))?;
            tokio::task::yield_now().await;
            assert_eq!(lock_poison(&registry.inner.timers).len(), 1);

            ctx.shutdown_tasks().await;

            assert!(
                lock_poison(&registry.inner.timers).is_empty(),
                "context task shutdown must unregister callback timers"
            );
            Ok(())
        });
    }

    #[test]
    fn timer_cancellation_before_wait_is_observed() {
        async_run!(|_ctx: JSContext| async move {
            let (cancel, mut cancel_rx) = TimerCancellation::new();
            cancel.cancel();

            tokio::time::timeout(
                Duration::from_millis(100),
                TimerCancellation::cancelled(&mut cancel_rx),
            )
            .await
            .expect("timer cancellation must remain observable before the waiter starts");
            Ok(())
        });
    }

    #[test]
    fn timer_registries_are_isolated_by_context() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            let second_ctx = ctx.runtime().context();
            init(&second_ctx)?;

            let first = TimerRegistry::ensure(&ctx);
            let second = TimerRegistry::ensure(&second_ctx);

            assert!(
                !Rc::ptr_eq(&first.inner, &second.inner),
                "each JS context must own an independent timer registry"
            );
            Ok(())
        });
    }
}
