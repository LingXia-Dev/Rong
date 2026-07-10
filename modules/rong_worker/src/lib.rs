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

use rong::{Source, spawn_local, *};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;
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

/// Serialize a JSValue to a JSON string using the context's JSON.stringify.
fn js_value_to_json(ctx: &JSContext, data: &JSValue) -> JSResult<String> {
    if let Some(obj) = data.clone().into_object() {
        obj.to_json_string()
    } else {
        let json_obj = ctx.global().get::<_, JSObject>("JSON")?;
        let stringify = json_obj.get::<_, JSFunc>("stringify")?;
        stringify.call::<_, String>(None, (data.clone(),))
    }
}

/// Messages from main thread → worker thread.
enum ToWorker {
    Message(String),
    Terminate,
}

/// Messages from worker thread → main thread.
enum FromWorker {
    Message(String),
    Error(String),
}

#[js_class]
pub struct Worker {
    /// Send commands to the worker thread.
    to_worker: mpsc::Sender<ToWorker>,
    /// Receive messages from the worker thread.
    from_worker: Arc<tokio::sync::Mutex<mpsc::Receiver<FromWorker>>>,
    /// JS callback for incoming messages.
    message_handler: Rc<RefCell<Option<JSFunc>>>,
    /// JS callback for errors.
    error_handler: Rc<RefCell<Option<JSFunc>>>,
    /// Whether terminate() has been called.
    terminated: Arc<AtomicBool>,
    /// Ensure the main-side polling loop starts only once.
    polling_started: Arc<AtomicBool>,
    /// Handle for the main-side polling loop so terminate/shutdown can abort it.
    polling_handle: Rc<RefCell<Option<tokio::task::JoinHandle<()>>>>,
    /// Thread join handle for forceful shutdown.
    thread_handle: Arc<std::sync::Mutex<Option<std::thread::JoinHandle<()>>>>,
}

#[js_class]
impl Worker {
    #[js_method(constructor)]
    fn new(_ctx: JSContext, path: String) -> JSResult<Self> {
        // main → worker
        let (to_worker_tx, to_worker_rx) = mpsc::channel::<ToWorker>(256);
        // worker → main
        let (from_worker_tx, from_worker_rx) = mpsc::channel::<FromWorker>(256);

        let script_path = if PathBuf::from(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&path)
        };

        let terminated = Arc::new(AtomicBool::new(false));
        let terminated_thread = terminated.clone();

        // Spawn a dedicated OS thread with its own tokio runtime + JS runtime.
        let thread_handle = std::thread::spawn(move || {
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
                    ))
                    .await;
            });
        });

        Ok(Worker {
            to_worker: to_worker_tx,
            from_worker: Arc::new(tokio::sync::Mutex::new(from_worker_rx)),
            message_handler: Rc::new(RefCell::new(None)),
            error_handler: Rc::new(RefCell::new(None)),
            terminated,
            polling_started: Arc::new(AtomicBool::new(false)),
            polling_handle: Rc::new(RefCell::new(None)),
            thread_handle: Arc::new(std::sync::Mutex::new(Some(thread_handle))),
        })
    }

    /// Send a message to the worker.
    #[js_method(rename = "postMessage")]
    fn post_message(&self, ctx: JSContext, data: JSValue) -> JSResult<()> {
        if self.terminated.load(Ordering::Acquire) {
            return Ok(());
        }

        let json = js_value_to_json(&ctx, &data)?;
        let tx = self.to_worker.clone();
        spawn_local(async move {
            let _ = tx.send(ToWorker::Message(json)).await;
        });
        Ok(())
    }

    /// Terminate the worker.
    #[js_method]
    fn terminate(&self) -> JSResult<()> {
        if self.terminated.swap(true, Ordering::AcqRel) {
            return Ok(());
        }

        if let Some(handle) = self.polling_handle.borrow_mut().take() {
            handle.abort();
        }

        let tx = self.to_worker.clone();
        spawn_local(async move {
            let _ = tx.send(ToWorker::Terminate).await;
        });

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
        let terminated = self.terminated.clone();
        let polling_handle_slot = self.polling_handle.clone();

        // This runs on the main thread's LocalSet via spawn_local.
        let polling_handle = spawn_local(async move {
            loop {
                if terminated.load(Ordering::Acquire) {
                    break;
                }

                let msg = {
                    let mut rx = from_worker.lock().await;
                    rx.recv().await
                };

                match msg {
                    Some(FromWorker::Message(json_str)) => {
                        if terminated.load(Ordering::Acquire) {
                            break;
                        }

                        match JSObject::from_json_string(&ctx, &json_str) {
                            Ok(value) => {
                                let handler = message_handler.borrow().clone();
                                if let Some(func) = handler {
                                    let event = JSObject::new(&ctx);
                                    event.set("data", value).ok();
                                    if let Err(e) = func.call_async::<_, ()>(None, (event,)).await {
                                        let err_handler = error_handler.borrow().clone();
                                        if let Some(err_fn) = err_handler {
                                            let err_message = worker_error_message(&ctx, e);
                                            let err_event =
                                                worker_error_event(&ctx, err_message.as_str());
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
                            let err_event = worker_error_event(&ctx, &message);
                            let _ = err_fn.call_async::<_, ()>(None, (err_event,)).await;
                        } else {
                            error!(target: "rong", message = %message, "worker emitted error event without handler");
                        }
                    }
                    None => break,
                }
            }
        });
        *polling_handle_slot.borrow_mut() = Some(polling_handle);
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

// ── Worker thread logic (runs on a separate OS thread) ─────────────

impl Worker {
    async fn run_worker_thread(
        script_path: PathBuf,
        mut to_worker_rx: mpsc::Receiver<ToWorker>,
        from_worker_tx: mpsc::Sender<FromWorker>,
        terminated: Arc<AtomicBool>,
    ) {
        let runtime = RongJS::runtime();
        let ctx = runtime.context();

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
        let post_ctx = ctx.clone();
        let post_message_fn = JSFunc::new(&ctx, move |data: JSValue| {
            let c = post_ctx.clone();
            let t = tx.clone();
            spawn_local(async move {
                match js_value_to_json(&c, &data) {
                    Ok(json) => {
                        let _ = t.send(FromWorker::Message(json)).await;
                    }
                    Err(e) => {
                        let _ = t
                            .send(FromWorker::Error(format!(
                                "postMessage serialization failed: {}",
                                worker_error_message(&c, e)
                            )))
                            .await;
                    }
                }
            });
        });
        ctx.global().set("postMessage", post_message_fn).ok();

        // close() and self
        let terminated_close = terminated.clone();
        let close_fn = JSFunc::new(&ctx, move || {
            terminated_close.store(true, Ordering::Release);
        });
        let global = ctx.global();
        global.set("close", close_fn).ok();
        global.set("self", global.clone()).ok();

        // Load and execute the worker script.
        match Source::from_path(&ctx, &script_path).await {
            Ok(source) => {
                if let Err(e) = ctx.eval_async::<()>(source).await {
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
        loop {
            if terminated.load(Ordering::Acquire) {
                break;
            }

            match to_worker_rx.recv().await {
                Some(ToWorker::Message(json_str)) => {
                    match JSObject::from_json_string(&ctx, &json_str) {
                        Ok(data) => {
                            if let Ok(handler) = ctx.global().get::<_, JSValue>("onmessage")
                                && let Ok(func) = handler.to_rust::<JSFunc>()
                            {
                                let event = JSObject::new(&ctx);
                                event.set("data", data).ok();
                                if let Err(e) = func.call_async::<_, ()>(None, (event,)).await {
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
                                    "JSON deserialization failed: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                }
                Some(ToWorker::Terminate) | None => break,
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
}
