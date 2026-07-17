//! Web Worker implementation for Rong
//!
//! Provides a Web Worker-like API for running JavaScript in **separate OS threads**.
//! Each worker runs its own isolated JavaScript runtime and tokio event loop.
//!
//! # Customizing Worker Initialization
//!
//! By default, workers are initialized with only the `console` module. Customize
//! which modules are available in worker contexts using [`set_initializer`]:
//!
//! ```rust,no_run
//! rong_worker::set_initializer(|ctx| {
//!     rong_console::init(ctx)?;
//!     rong_timer::init(ctx)?;
//!     Ok(())
//! });
//! ```

use rong::{Source, *};
use std::cell::RefCell;
use std::ops::Deref;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, watch};
use tracing::{error, warn};

type WorkerInitializer = Box<dyn Fn(&JSContext) -> JSResult<()> + Send + Sync>;

static WORKER_INITIALIZER: OnceLock<WorkerInitializer> = OnceLock::new();

/// Set a custom initializer for worker contexts.
///
/// Called once when each worker context is created. If not set, workers
/// only get `console` by default. Must be called before any workers are created.
pub fn set_initializer<F>(f: F)
where
    F: Fn(&JSContext) -> JSResult<()> + Send + Sync + 'static,
{
    let _ = WORKER_INITIALIZER.set(Box::new(f));
}

/// Register the `Worker` constructor in the given JavaScript context.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    ctx.register_class::<Worker>()?;
    Ok(())
}

/// Serialized payload passed between isolated worker runtimes.
enum SerializedWorkerValue {
    Json(String),
    Undefined,
    BigInt(String),
    Number(f64),
}

/// Serialize a JSValue for transfer to another runtime.
fn serialize_worker_value(ctx: &JSContext, data: &JSValue) -> JSResult<SerializedWorkerValue> {
    if data.is_undefined() {
        return Ok(SerializedWorkerValue::Undefined);
    }
    if data.is_bigint() {
        return Ok(SerializedWorkerValue::BigInt(
            data.clone().to_rust::<String>()?,
        ));
    }
    if data.is_number() {
        let number = data.clone().to_rust::<f64>()?;
        if !number.is_finite() || (number == 0.0 && number.is_sign_negative()) {
            return Ok(SerializedWorkerValue::Number(number));
        }
    }

    if let Some(obj) = data.clone().into_object() {
        obj.to_json_string().map(SerializedWorkerValue::Json)
    } else {
        let json_obj = ctx.global().get::<_, JSObject>("JSON")?;
        let stringify = json_obj.get::<_, JSFunc>("stringify")?;
        stringify
            .call::<_, String>(None, (data.clone(),))
            .map(SerializedWorkerValue::Json)
    }
}

fn deserialize_worker_value(ctx: &JSContext, data: SerializedWorkerValue) -> JSResult<JSValue> {
    match data {
        SerializedWorkerValue::Json(json) => json.as_str().json_to_js_value(ctx),
        SerializedWorkerValue::Undefined => Ok(JSValue::undefined(ctx)),
        SerializedWorkerValue::BigInt(value) => {
            let constructor = ctx.global().get::<_, JSFunc>("BigInt")?;
            constructor.call(None, (value,))
        }
        SerializedWorkerValue::Number(value) => Ok(JSValue::from_rust(ctx, value)),
    }
}

/// Messages from main thread → worker thread.
enum ToWorker {
    Message(SerializedWorkerValue),
}

/// Messages from worker thread → main thread.
enum FromWorker {
    Message(SerializedWorkerValue),
    Error(String),
}

#[derive(Clone)]
struct WorkerLifecycle(Rc<WorkerLifecycleInner>);

struct WorkerLifecycleInner {
    to_worker: mpsc::Sender<ToWorker>,
    terminate_tx: watch::Sender<bool>,
    terminated: Arc<AtomicBool>,
    polling_handle: Rc<RefCell<Option<tokio::task::AbortHandle>>>,
    thread_handle: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
    thread_stopped: Arc<AtomicBool>,
}

struct WorkerThreadExit(Arc<AtomicBool>);

impl Drop for WorkerThreadExit {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Deref for WorkerLifecycle {
    type Target = WorkerLifecycleInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl WorkerLifecycle {
    fn terminate(&self) {
        self.terminated.store(true, Ordering::Release);
        if !self.thread_stopped.load(Ordering::Acquire) {
            self.terminate_tx.send_replace(true);
        }

        if let Some(handle) = self.polling_handle.borrow_mut().take() {
            handle.abort();
        }

        let thread_handle = self
            .thread_handle
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());
        if let Some(handle) = thread_handle {
            RongExecutor::global().spawn_blocking(move || {
                let _ = handle.join();
            });
        }
    }
}

#[derive(Clone, Default)]
struct WorkerContextRegistry {
    workers: Rc<RefCell<Vec<Weak<WorkerLifecycleInner>>>>,
}

impl WorkerContextRegistry {
    fn get_or_init(ctx: &JSContext) -> Self {
        if let Some(registry) = ctx.get_service::<Self>() {
            return registry.clone();
        }

        let registry = Self::default();
        ctx.set_service(registry.clone());
        registry
    }

    fn register(&self, lifecycle: &WorkerLifecycle) {
        let mut workers = self.workers.borrow_mut();
        workers.retain(|worker| worker.strong_count() > 0);
        workers.push(Rc::downgrade(&lifecycle.0));
    }
}

impl JSContextService for WorkerContextRegistry {
    fn on_shutdown(&self) {
        for worker in self.workers.borrow_mut().drain(..) {
            if let Some(worker) = worker.upgrade() {
                WorkerLifecycle(worker).terminate();
            }
        }
    }
}

#[js_class]
pub struct Worker {
    lifecycle: WorkerLifecycle,
    /// Receive messages from the worker thread.
    from_worker: Arc<tokio::sync::Mutex<mpsc::Receiver<FromWorker>>>,
    /// JS callback for incoming messages.
    message_handler: Rc<RefCell<Option<JSFunc>>>,
    /// JS callback for errors.
    error_handler: Rc<RefCell<Option<JSFunc>>>,
    /// Ensure the main-side polling loop starts only once.
    polling_started: Arc<AtomicBool>,
}

#[js_class]
impl Worker {
    #[js_method(constructor)]
    fn new(ctx: JSContext, path: String) -> JSResult<Self> {
        // main → worker
        let (to_worker_tx, to_worker_rx) = mpsc::channel::<ToWorker>(256);
        // worker → main
        let (from_worker_tx, from_worker_rx) = mpsc::channel::<FromWorker>(256);
        let (terminate_tx, terminate_rx) = watch::channel(false);

        let script_path = if PathBuf::from(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&path)
        };

        let terminated = Arc::new(AtomicBool::new(false));
        let terminated_thread = terminated.clone();
        let terminate_thread_tx = terminate_tx.clone();
        let thread_stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped_worker = thread_stopped.clone();

        // Spawn a dedicated OS thread with its own tokio runtime + JS runtime.
        let thread_handle = std::thread::spawn(move || {
            let _exit = WorkerThreadExit(thread_stopped_worker);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create worker tokio runtime");

            rt.block_on(async {
                let local = tokio::task::LocalSet::new();
                local
                    .run_until(Self::run_worker_thread(
                        script_path,
                        to_worker_rx,
                        from_worker_tx,
                        terminated_thread,
                        terminate_thread_tx,
                        terminate_rx,
                    ))
                    .await;
            });
        });

        let lifecycle = WorkerLifecycle(Rc::new(WorkerLifecycleInner {
            to_worker: to_worker_tx,
            terminate_tx,
            terminated,
            polling_handle: Rc::new(RefCell::new(None)),
            thread_handle: Arc::new(std::sync::Mutex::new(Some(thread_handle))),
            thread_stopped,
        }));
        WorkerContextRegistry::get_or_init(&ctx).register(&lifecycle);

        Ok(Worker {
            lifecycle,
            from_worker: Arc::new(tokio::sync::Mutex::new(from_worker_rx)),
            message_handler: Rc::new(RefCell::new(None)),
            error_handler: Rc::new(RefCell::new(None)),
            polling_started: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Send a message to the worker.
    #[js_method(rename = "postMessage")]
    fn post_message(&self, ctx: JSContext, data: JSValue) -> JSResult<()> {
        if self.lifecycle.terminated.load(Ordering::Acquire) {
            return Ok(());
        }

        let data = serialize_worker_value(&ctx, &data)?;
        let tx = self.lifecycle.to_worker.clone();
        ctx.spawn_task(async move {
            let _ = tx.send(ToWorker::Message(data)).await;
        });
        Ok(())
    }

    /// Terminate the worker.
    #[js_method]
    fn terminate(&self) -> JSResult<()> {
        self.lifecycle.terminate();
        Ok(())
    }

    /// Set the onmessage handler. Also starts the main-side polling loop
    /// that reads messages from the worker thread and dispatches to JS.
    #[js_method(setter, rename = "onmessage")]
    fn set_onmessage(&self, ctx: JSContext, handler: JSFunc) -> JSResult<()> {
        *self.message_handler.borrow_mut() = Some(handler);

        self.ensure_polling(ctx);
        Ok(())
    }

    #[js_method(setter, rename = "onerror")]
    fn set_onerror(&self, ctx: JSContext, handler: JSFunc) -> JSResult<()> {
        *self.error_handler.borrow_mut() = Some(handler);
        self.ensure_polling(ctx);
        Ok(())
    }

    fn ensure_polling(&self, ctx: JSContext) {
        // Only start polling once.
        if self.polling_started.swap(true, Ordering::AcqRel) {
            return;
        }

        let from_worker = self.from_worker.clone();
        let message_handler = self.message_handler.clone();
        let error_handler = self.error_handler.clone();
        let terminated = self.lifecycle.terminated.clone();
        let polling_handle_slot = self.lifecycle.polling_handle.clone();
        let polling_ctx = ctx.clone();

        let polling_handle = ctx.spawn_task_with_handle(async move {
            loop {
                if terminated.load(Ordering::Acquire) {
                    break;
                }

                let msg = {
                    let mut rx = from_worker.lock().await;
                    rx.recv().await
                };

                match msg {
                    Some(FromWorker::Message(data)) => {
                        if terminated.load(Ordering::Acquire) {
                            break;
                        }

                        match deserialize_worker_value(&polling_ctx, data) {
                            Ok(value) => {
                                let handler = message_handler.borrow().clone();
                                if let Some(func) = handler {
                                    let event = JSObject::new(&polling_ctx);
                                    event.set("data", value).ok();
                                    if let Err(e) = func.call_async::<_, ()>(None, (event,)).await {
                                        let err_handler = error_handler.borrow().clone();
                                        if let Some(err_fn) = err_handler {
                                            let err_message = worker_error_message(&polling_ctx, e);
                                            let err_event =
                                                worker_error_event(&polling_ctx, err_message.as_str());
                                            let _ = err_fn
                                                .call_async::<_, ()>(None, (err_event,))
                                                .await;
                                        } else {
                                            error!(target: "rong", error = ?e, "worker onmessage handler failed");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(target: "rong", error = ?e, "worker failed to deserialize JSON message");
                            }
                        }
                    }
                    Some(FromWorker::Error(message)) => {
                        let err_handler = error_handler.borrow().clone();
                        if let Some(err_fn) = err_handler {
                            let err_event = worker_error_event(&polling_ctx, &message);
                            let _ = err_fn.call_async::<_, ()>(None, (err_event,)).await;
                        } else {
                            error!(target: "rong", message = %message, "worker emitted error event without handler");
                        }
                    }
                    None => break,
                }
            }
        });
        *polling_handle_slot.borrow_mut() = polling_handle;
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        for slot in [&self.message_handler, &self.error_handler] {
            if let Some(handler) = slot.borrow().clone() {
                mark_fn(handler.as_js_value());
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.lifecycle.terminate();
    }
}

// ── Worker thread logic (runs on a separate OS thread) ─────────────

impl Worker {
    async fn run_worker_thread(
        script_path: PathBuf,
        mut to_worker_rx: mpsc::Receiver<ToWorker>,
        from_worker_tx: mpsc::Sender<FromWorker>,
        terminated: Arc<AtomicBool>,
        terminate_tx: watch::Sender<bool>,
        mut terminate_rx: watch::Receiver<bool>,
    ) {
        let runtime = RongJS::runtime();
        let ctx = runtime.context();

        if *terminate_rx.borrow() {
            return;
        }

        // Initialize worker context.
        if let Some(initializer) = WORKER_INITIALIZER.get() {
            if let Err(e) = initializer(&ctx) {
                let _ = from_worker_tx
                    .send(FromWorker::Error(format!(
                        "initializer failed: {}",
                        worker_error_message(&ctx, e)
                    )))
                    .await;
                return;
            }
        } else {
            rong_console::init(&ctx).ok();
        }

        // postMessage: worker → main  (sends JSON over the channel)
        let tx = from_worker_tx.clone();
        let post_message_fn = JSFunc::new(&ctx, move |ctx: JSContext, data: JSValue| {
            let t = tx.clone();
            let message = match serialize_worker_value(&ctx, &data) {
                Ok(data) => FromWorker::Message(data),
                Err(e) => FromWorker::Error(format!(
                    "postMessage serialization failed: {}",
                    worker_error_message(&ctx, e)
                )),
            };
            ctx.spawn_task(async move {
                let _ = t.send(message).await;
            });
        });
        ctx.global().set("postMessage", post_message_fn).ok();

        // close() and self
        let terminated_close = terminated.clone();
        let terminate_close = terminate_tx.clone();
        let close_fn = JSFunc::new(&ctx, move || {
            terminated_close.store(true, Ordering::Release);
            terminate_close.send_replace(true);
        });
        let global = ctx.global();
        global.set("close", close_fn).ok();
        global.set("self", global.clone()).ok();

        // Load and execute the worker script.
        let source = tokio::select! {
            _ = terminate_rx.changed() => return,
            source = Source::from_path(&ctx, &script_path) => source,
        };
        match source {
            Ok(source) => {
                let result = tokio::select! {
                    _ = terminate_rx.changed() => return,
                    result = ctx.eval_async::<()>(source) => result,
                };
                if let Err(e) = result {
                    let _ = from_worker_tx
                        .send(FromWorker::Error(format!(
                            "script error in {:?}: {}",
                            script_path,
                            worker_error_message(&ctx, e)
                        )))
                        .await;
                    return;
                }
            }
            Err(e) => {
                let _ = from_worker_tx
                    .send(FromWorker::Error(format!(
                        "failed to load {:?}: {}",
                        script_path, e
                    )))
                    .await;
                return;
            }
        }

        // Message loop: receive from main, dispatch to onmessage.
        'message_loop: loop {
            if terminated.load(Ordering::Acquire) {
                break;
            }

            let message = tokio::select! {
                changed = terminate_rx.changed() => {
                    if changed.is_err() || *terminate_rx.borrow() {
                        break;
                    }
                    continue;
                }
                message = to_worker_rx.recv() => message,
            };

            match message {
                Some(ToWorker::Message(data)) => match deserialize_worker_value(&ctx, data) {
                    Ok(data) => {
                        if let Ok(handler) = ctx.global().get::<_, JSValue>("onmessage")
                            && let Ok(func) = handler.to_rust::<JSFunc>()
                        {
                            let event = JSObject::new(&ctx);
                            event.set("data", data).ok();
                            let result = tokio::select! {
                                _ = terminate_rx.changed() => break 'message_loop,
                                result = func.call_async::<_, ()>(None, (event,)) => result,
                            };
                            if let Err(e) = result {
                                let _ = from_worker_tx
                                    .send(FromWorker::Error(format!(
                                        "worker onmessage handler error: {}",
                                        worker_error_message(&ctx, e)
                                    )))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = from_worker_tx
                            .send(FromWorker::Error(format!(
                                "worker message deserialization failed: {}",
                                e
                            )))
                            .await;
                    }
                },
                None => break,
            }
        }
    }
}

fn worker_error_message(ctx: &JSContext, err: RongJSError) -> String {
    err.into_host_in(ctx)
        .into_host_error()
        .map(|host| host.message)
        .unwrap_or_else(|| "Worker error".to_string())
}

fn worker_error_event(ctx: &JSContext, message: &str) -> JSObject {
    let event = JSObject::new(ctx);
    let _ = event.set("type", "error");
    let _ = event.set("message", message);
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Clone)]
    struct ShutdownFlag(Arc<AtomicBool>);

    impl JSContextService for ShutdownFlag {
        fn on_shutdown(&self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_worker() {
        // Set cwd to workspace root so relative paths in worker scripts resolve correctly
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        std::env::set_current_dir(&workspace_root).expect("set cwd");

        set_initializer(|ctx| {
            rong_console::init(ctx)?;
            rong_timer::init(ctx)?;
            Ok(())
        });

        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;

            rong_console::init(&ctx)?;
            rong_assert::init(&ctx)?;
            rong_timer::init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "worker.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        })
    }

    #[test]
    fn worker_polling_stops_with_context() {
        let shutdown = Arc::new(AtomicBool::new(false));
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            ctx.set_service(ShutdownFlag(shutdown.clone()));

            let script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/unit/worker-pending.js")
                .canonicalize()
                .expect("worker fixture");
            let callback_ctx = ctx.global().context();
            let worker = Worker::new(callback_ctx, script.to_string_lossy().into_owned())?;
            let lifecycle = worker.lifecycle.clone();
            let messages = Arc::new(AtomicUsize::new(0));
            let messages_for_callback = messages.clone();
            worker.set_onmessage(
                ctx.global().context(),
                JSFunc::new(&ctx, move |_event: JSObject| {
                    messages_for_callback.fetch_add(1, Ordering::SeqCst);
                })?,
            )?;
            worker.post_message(ctx.global().context(), JSValue::from_rust(&ctx, "start"))?;
            let worker = Class::lookup::<Worker>(&ctx)?.instance(worker);
            ctx.global().set("heldWorker", worker)?;
            tokio::time::timeout(Duration::from_secs(2), async {
                while messages.load(Ordering::SeqCst) < 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("worker should enter its pending onmessage handler");

            drop(ctx);
            tokio::time::timeout(Duration::from_secs(2), async {
                while !lifecycle.thread_stopped.load(Ordering::Acquire) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("context shutdown should stop a pending worker thread");

            assert!(shutdown.load(Ordering::SeqCst));
            assert!(
                lifecycle.terminated.load(Ordering::Acquire),
                "context shutdown must terminate registered workers"
            );
            Ok(())
        });
    }
}
