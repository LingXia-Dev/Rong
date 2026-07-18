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
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{error, warn};

type WorkerInitializer = Box<dyn Fn(&JSContext) -> JSResult<()> + Send + Sync>;

static WORKER_INITIALIZER: OnceLock<WorkerInitializer> = OnceLock::new();
static ACTIVE_DETACHED_WORKER_THREADS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_DETACHED_WORKER_THREADS: AtomicUsize = AtomicUsize::new(0);

const WORKER_TERMINATION_GRACE: Duration = Duration::from_secs(1);

/// Process-level accounting for worker threads that outlived their termination
/// grace period.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkerTerminationStats {
    /// Detached worker threads that have not exited yet.
    pub active_detached_threads: usize,
    /// Worker threads detached since process start, including ones that later exited.
    pub total_detached_threads: usize,
}

/// Return process-level JS Worker termination statistics.
pub fn termination_stats() -> WorkerTerminationStats {
    WorkerTerminationStats {
        active_detached_threads: ACTIVE_DETACHED_WORKER_THREADS.load(Ordering::Acquire),
        total_detached_threads: TOTAL_DETACHED_WORKER_THREADS.load(Ordering::Acquire),
    }
}

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
    termination_started: AtomicBool,
    interrupt: InterruptHandle,
    polling_handle: Rc<RefCell<Option<tokio::task::AbortHandle>>>,
    thread_handle: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
    thread_stopped: Arc<AtomicBool>,
    thread_detached: Arc<AtomicBool>,
    thread_exit: watch::Receiver<bool>,
}

struct WorkerThreadExit {
    stopped: Arc<AtomicBool>,
    detached: Arc<AtomicBool>,
    exit_tx: watch::Sender<bool>,
}

impl Drop for WorkerThreadExit {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let was_detached = self.detached.swap(false, Ordering::AcqRel);
        let active_detached_threads = was_detached.then(decrement_active_detached_worker_threads);
        self.exit_tx.send_replace(true);

        if let Some(active_detached_threads) = active_detached_threads {
            warn!(
                target: "rong",
                active_detached_threads,
                "detached JS Worker thread eventually exited"
            );
        }
    }
}

fn decrement_active_detached_worker_threads() -> usize {
    ACTIVE_DETACHED_WORKER_THREADS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            count.checked_sub(1)
        })
        .map(|previous| previous - 1)
        .unwrap_or(0)
}

fn mark_worker_thread_detached(
    stopped: &AtomicBool,
    detached: &AtomicBool,
) -> Option<WorkerTerminationStats> {
    ACTIVE_DETACHED_WORKER_THREADS.fetch_add(1, Ordering::AcqRel);
    if detached
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        decrement_active_detached_worker_threads();
        return None;
    }

    // The worker can exit immediately before or after it is marked detached.
    // Let exactly one side undo the active count in either race ordering.
    if stopped.load(Ordering::Acquire) {
        if detached.swap(false, Ordering::AcqRel) {
            decrement_active_detached_worker_threads();
        }
        return None;
    }

    TOTAL_DETACHED_WORKER_THREADS.fetch_add(1, Ordering::AcqRel);
    Some(termination_stats())
}

async fn wait_for_worker_thread_exit(mut exit_rx: watch::Receiver<bool>) -> bool {
    if *exit_rx.borrow() {
        return true;
    }

    loop {
        match exit_rx.changed().await {
            Ok(()) if *exit_rx.borrow() => return true,
            Ok(()) => {}
            Err(_) => return *exit_rx.borrow(),
        }
    }
}

fn spawn_worker_thread_reaper(
    handle: std::thread::JoinHandle<()>,
    stopped: Arc<AtomicBool>,
    detached: Arc<AtomicBool>,
    exit_rx: watch::Receiver<bool>,
    interrupt: InterruptHandle,
    grace: Duration,
) -> tokio::task::JoinHandle<()> {
    let executor = RongExecutor::global();
    let reaper_executor = executor.clone();
    executor.spawn(async move {
        let stopped_in_time = stopped.load(Ordering::Acquire)
            || tokio::time::timeout(grace, wait_for_worker_thread_exit(exit_rx))
                .await
                .unwrap_or(false);

        if !stopped_in_time {
            if let Some(stats) = mark_worker_thread_detached(&stopped, &detached) {
                warn!(
                    target: "rong",
                    mode = ?interrupt.mode(),
                    grace_ms = u64::try_from(grace.as_millis()).unwrap_or(u64::MAX),
                    active_detached_threads = stats.active_detached_threads,
                    total_detached_threads = stats.total_detached_threads,
                    "JS Worker thread did not stop within the termination grace; detaching it"
                );
            }
            drop(handle);
            return;
        }

        match reaper_executor.spawn_blocking(move || handle.join()).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                warn!(target: "rong", error = ?err, "JS Worker thread panicked during shutdown");
            }
            Err(err) => {
                warn!(target: "rong", error = ?err, "failed to join stopped JS Worker thread");
            }
        }
    })
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
        if self.termination_started.swap(true, Ordering::AcqRel) {
            return;
        }

        self.interrupt.interrupt();
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
            drop(spawn_worker_thread_reaper(
                handle,
                self.thread_stopped.clone(),
                self.thread_detached.clone(),
                self.thread_exit.clone(),
                self.interrupt.clone(),
                WORKER_TERMINATION_GRACE,
            ));
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
        let interrupt = InterruptHandle::new();
        let interrupt_worker = interrupt.clone();
        let thread_stopped = Arc::new(AtomicBool::new(false));
        let thread_stopped_worker = thread_stopped.clone();
        let thread_detached = Arc::new(AtomicBool::new(false));
        let thread_detached_worker = thread_detached.clone();
        let (thread_exit_tx, thread_exit) = watch::channel(false);

        // Spawn a dedicated OS thread with its own tokio runtime + JS runtime.
        let thread_handle = std::thread::spawn(move || {
            let _exit = WorkerThreadExit {
                stopped: thread_stopped_worker,
                detached: thread_detached_worker,
                exit_tx: thread_exit_tx,
            };
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
                        interrupt_worker,
                    ))
                    .await;
            });
        });

        let lifecycle = WorkerLifecycle(Rc::new(WorkerLifecycleInner {
            to_worker: to_worker_tx,
            terminate_tx,
            terminated,
            termination_started: AtomicBool::new(false),
            interrupt,
            polling_handle: Rc::new(RefCell::new(None)),
            thread_handle: Arc::new(std::sync::Mutex::new(Some(thread_handle))),
            thread_stopped,
            thread_detached,
            thread_exit,
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
        interrupt: InterruptHandle,
    ) {
        let runtime = RongJS::runtime_with_interrupt(interrupt);
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
                    if terminated.load(Ordering::Acquire) {
                        return;
                    }
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
                                if terminated.load(Ordering::Acquire) {
                                    break 'message_loop;
                                }
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
    use std::sync::Once;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc as std_mpsc;
    use std::time::Duration;

    static TEST_INITIALIZER: Once = Once::new();
    static TEST_WORKER_STARTED: std::sync::Mutex<Option<std_mpsc::Sender<()>>> =
        std::sync::Mutex::new(None);

    fn install_test_initializer() {
        TEST_INITIALIZER.call_once(|| {
            set_initializer(|ctx| {
                rong_console::init(ctx)?;
                rong_timer::init(ctx)?;
                let started = JSFunc::new(ctx, || {
                    let sender = TEST_WORKER_STARTED
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .clone();
                    if let Some(sender) = sender {
                        let _ = sender.send(());
                    }
                })?;
                ctx.global().set("__rongWorkerTestStarted", started)?;
                Ok(())
            });
        });
    }

    fn set_test_worker_started_sender(sender: Option<std_mpsc::Sender<()>>) {
        *TEST_WORKER_STARTED
            .lock()
            .unwrap_or_else(|err| err.into_inner()) = sender;
    }

    fn worker_fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/unit")
            .join(name)
            .canonicalize()
            .expect("worker fixture")
    }

    async fn wait_for_worker_started(
        receiver: &std_mpsc::Receiver<()>,
        errors: &std_mpsc::Receiver<String>,
    ) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match receiver.try_recv() {
                    Ok(()) => return,
                    Err(std_mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                    Err(std_mpsc::TryRecvError::Disconnected) => {
                        panic!("worker start notifier disconnected")
                    }
                }
                match errors.try_recv() {
                    Ok(error) => panic!("worker fixture failed before busy loop: {error}"),
                    Err(std_mpsc::TryRecvError::Empty) => {}
                    Err(std_mpsc::TryRecvError::Disconnected) => {
                        panic!("worker error notifier disconnected")
                    }
                }
            }
        })
        .await
        .expect("worker should enter the busy-loop fixture");
    }

    async fn wait_for_worker_stopped(lifecycle: &WorkerLifecycle) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !lifecycle.thread_stopped.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker thread should stop");
    }

    #[derive(Clone)]
    struct ShutdownFlag(Arc<AtomicBool>);

    impl JSContextService for ShutdownFlag {
        fn on_shutdown(&self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn test_worker() {
        install_test_initializer();
        // Set cwd to workspace root so relative paths in worker scripts resolve correctly
        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        std::env::set_current_dir(&workspace_root).expect("set cwd");

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
        install_test_initializer();
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

    #[test]
    fn terminate_stops_non_yielding_worker_scripts() {
        let runtime = RongJS::runtime();
        if !runtime.interrupt_handle().mode().is_preemptive() {
            eprintln!("skipping busy-loop Worker test: engine is cooperative-only");
            return;
        }
        drop(runtime);

        install_test_initializer();
        async_run!(|ctx: JSContext| async move {
            for (fixture, send_message) in [
                ("worker-busy-entry.js", false),
                ("worker-busy-message.js", true),
            ] {
                let (started_tx, started_rx) = std_mpsc::channel();
                let (error_tx, error_rx) = std_mpsc::channel();
                set_test_worker_started_sender(Some(started_tx));

                let worker = Worker::new(
                    ctx.global().context(),
                    worker_fixture(fixture).to_string_lossy().into_owned(),
                )?;
                let lifecycle = worker.lifecycle.clone();
                worker.set_onerror(
                    ctx.global().context(),
                    JSFunc::new(&ctx, move |event: JSObject| {
                        let message = event
                            .get::<_, String>("message")
                            .unwrap_or_else(|_| "unknown worker error".to_string());
                        let _ = error_tx.send(message);
                    })?,
                )?;
                if send_message {
                    worker.post_message(ctx.global().context(), JSValue::undefined(&ctx))?;
                }

                wait_for_worker_started(&started_rx, &error_rx).await;
                assert_eq!(lifecycle.interrupt.mode(), InterruptMode::Preemptive);

                worker.terminate()?;
                worker.terminate()?;
                wait_for_worker_stopped(&lifecycle).await;
                assert!(!lifecycle.thread_detached.load(Ordering::Acquire));
                assert!(matches!(
                    error_rx.try_recv(),
                    Err(std_mpsc::TryRecvError::Empty)
                ));

                set_test_worker_started_sender(None);
            }
            Ok(())
        });
    }

    #[test]
    fn terminate_is_idempotent_during_worker_startup() {
        install_test_initializer();
        async_run!(|ctx: JSContext| async move {
            for _ in 0..8 {
                let worker = Worker::new(
                    ctx.global().context(),
                    worker_fixture("worker-pending.js")
                        .to_string_lossy()
                        .into_owned(),
                )?;
                let lifecycle = worker.lifecycle.clone();
                worker.terminate()?;
                worker.terminate()?;
                wait_for_worker_stopped(&lifecycle).await;
                assert!(lifecycle.termination_started.load(Ordering::Acquire));
                assert!(!lifecycle.thread_detached.load(Ordering::Acquire));
            }
            Ok(())
        });
    }

    #[test]
    fn reaper_detaches_after_bounded_grace() {
        let stats_before = termination_stats();
        let stopped = Arc::new(AtomicBool::new(false));
        let detached = Arc::new(AtomicBool::new(false));
        let (exit_tx, exit_rx) = watch::channel(false);
        let exit_after_detach = exit_rx.clone();
        let (release_tx, release_rx) = std_mpsc::channel();
        let thread_stopped = stopped.clone();
        let thread_detached = detached.clone();
        let handle = std::thread::spawn(move || {
            let _exit = WorkerThreadExit {
                stopped: thread_stopped,
                detached: thread_detached,
                exit_tx,
            };
            let _ = release_rx.recv();
        });

        let reaper = spawn_worker_thread_reaper(
            handle,
            stopped.clone(),
            detached.clone(),
            exit_rx,
            InterruptHandle::new(),
            Duration::from_millis(20),
        );
        RongExecutor::global().handle().block_on(async {
            tokio::time::timeout(Duration::from_secs(1), reaper)
                .await
                .expect("worker reaper should respect its grace period")
                .expect("worker reaper task should complete");
        });

        let detached_stats = termination_stats();
        assert!(detached.load(Ordering::Acquire));
        assert_eq!(
            detached_stats.active_detached_threads,
            stats_before.active_detached_threads + 1
        );
        assert_eq!(
            detached_stats.total_detached_threads,
            stats_before.total_detached_threads + 1
        );

        release_tx.send(()).expect("release detached worker thread");
        RongExecutor::global().handle().block_on(async {
            let exited = tokio::time::timeout(
                Duration::from_secs(2),
                wait_for_worker_thread_exit(exit_after_detach),
            )
            .await
            .expect("detached worker thread should eventually exit");
            assert!(exited);
        });
        assert!(stopped.load(Ordering::Acquire));
        assert!(!detached.load(Ordering::Acquire));
        assert_eq!(
            termination_stats().active_detached_threads,
            stats_before.active_detached_threads
        );
    }
}
