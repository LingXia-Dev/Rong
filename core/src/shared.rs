use crate::{HostError, JSEngine, JSResult, JSRuntime};
use std::any::Any;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::{Notify, mpsc, oneshot};
use tracing::{Instrument, Span, debug, error, info_span, warn};

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Idle,
    Busy,
}

#[derive(Debug)]
pub enum TaskMessage {
    String(String),
    Usize(usize),
    Custom(Box<dyn Any + Send>),
}

pub struct MessageReceiver {
    receiver: mpsc::Receiver<TaskMessage>,
}

impl MessageReceiver {
    pub(crate) fn new(receiver: mpsc::Receiver<TaskMessage>) -> Self {
        Self { receiver }
    }

    pub fn try_recv(&mut self) -> Result<TaskMessage, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }

    pub async fn recv(&mut self) -> Option<TaskMessage> {
        self.receiver.recv().await
    }
}

type BoxedTaskFuture = Pin<Box<dyn Future<Output = JSResult<Box<dyn Any + Send>>>>>;
type BoxedFutureFn<E> =
    Box<dyn FnOnce(JSRuntime<<E as JSEngine>::Runtime>, MessageReceiver) -> BoxedTaskFuture + Send>;

struct UserAsyncTask<E: JSEngine + 'static> {
    future_fn: BoxedFutureFn<E>,
    message_receiver: MessageReceiver,
    result_tx: oneshot::Sender<JSResult<Box<dyn Any + Send>>>,
    parent_span: Span,
}

pub struct TaskHandle<R> {
    worker_id: usize,
    message_tx: mpsc::Sender<TaskMessage>,
    result_rx: oneshot::Receiver<JSResult<Box<dyn Any + Send>>>,
    _marker: PhantomData<R>,
}

impl<R> TaskHandle<R>
where
    R: Send + 'static,
{
    pub(crate) fn new(
        worker_id: usize,
        message_tx: mpsc::Sender<TaskMessage>,
        result_rx: oneshot::Receiver<JSResult<Box<dyn Any + Send>>>,
    ) -> Self {
        Self {
            worker_id,
            message_tx,
            result_rx,
            _marker: PhantomData,
        }
    }

    pub fn worker_id(&self) -> usize {
        self.worker_id
    }

    pub async fn send(&self, message: TaskMessage) -> JSResult<()> {
        self.message_tx.send(message).await.map_err(|e| {
            HostError::new(
                crate::error::E_INTERNAL,
                format!(
                    "Failed to send task message to worker {}: {:?}",
                    self.worker_id, e
                ),
            )
            .into()
        })
    }

    pub async fn join(self) -> JSResult<R> {
        let result = self.result_rx.await.map_err(|e| {
            HostError::new(
                crate::error::E_INTERNAL,
                format!(
                    "Failed to receive task result from worker {}: {:?}",
                    self.worker_id, e
                ),
            )
        })??;

        result.downcast::<R>().map(|boxed| *boxed).map_err(|_| {
            HostError::new(
                crate::error::E_INTERNAL,
                "Downcast failed while reading task result",
            )
            .into()
        })
    }
}

pub struct Worker<E: JSEngine + 'static> {
    id: usize,
    task_tx: mpsc::Sender<UserAsyncTask<E>>,
    terminate_signal: Arc<Notify>,
    interrupt: crate::InterruptHandle,
    inflight_tasks: Arc<AtomicUsize>,
    idle_notify: Arc<Notify>,
    any_worker_idle: Arc<Notify>,
    message_queue_capacity: usize,
    thread_handle: Arc<StdMutex<Option<std::thread::JoinHandle<()>>>>,
}

impl<E: JSEngine + 'static> Worker<E> {
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn name(&self) -> String {
        format!("worker-{}", self.id)
    }

    pub fn state(&self) -> WorkerState {
        if self.inflight_tasks.load(Ordering::SeqCst) == 0 {
            WorkerState::Idle
        } else {
            WorkerState::Busy
        }
    }

    pub(crate) fn reserve_if_idle(&self) -> bool {
        self.inflight_tasks
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub(crate) fn increment_inflight(&self) {
        self.inflight_tasks.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn decrement_inflight(&self) {
        if self.inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.idle_notify.notify_waiters();
            self.any_worker_idle.notify_one();
        }
    }

    pub(crate) async fn spawn_inner<F, Fut, R>(
        &self,
        future_fn: F,
        already_reserved: bool,
    ) -> JSResult<TaskHandle<R>>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        if !already_reserved {
            self.increment_inflight();
        }

        let boxed_fn: BoxedFutureFn<E> = Box::new(
            move |runtime: JSRuntime<E::Runtime>, receiver: MessageReceiver| {
                let user_fut = future_fn(runtime, receiver);
                let mapped = async move {
                    user_fut
                        .await
                        .map(|value| Box::new(value) as Box<dyn Any + Send>)
                };
                Box::pin(mapped) as BoxedTaskFuture
            },
        );

        let (message_tx, message_rx) = mpsc::channel(self.message_queue_capacity);
        let (result_tx, result_rx) = oneshot::channel();
        let task = UserAsyncTask {
            future_fn: boxed_fn,
            message_receiver: MessageReceiver::new(message_rx),
            result_tx,
            parent_span: Span::current(),
        };

        if let Err(e) = self.task_tx.send(task).await {
            self.decrement_inflight();
            return Err(HostError::new(
                crate::error::E_INTERNAL,
                format!("Failed to queue task on worker {}: {:?}", self.id, e),
            )
            .into());
        }

        Ok(TaskHandle {
            worker_id: self.id,
            message_tx,
            result_rx,
            _marker: PhantomData,
        })
    }

    pub async fn spawn<F, Fut, R>(&self, future_fn: F) -> JSResult<TaskHandle<R>>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        self.spawn_inner(future_fn, false).await
    }

    pub async fn call<F, Fut, R>(&self, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        self.spawn(future_fn).await?.join().await
    }

    pub fn call_blocking<F, Fut, R>(&self, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("Worker::call_blocking")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call(future_fn))
    }

    pub async fn join(&self) -> JSResult<()> {
        loop {
            if self.inflight_tasks.load(Ordering::SeqCst) == 0 {
                return Ok(());
            }
            self.idle_notify.notified().await;
        }
    }

    pub fn terminate(&self) -> JSResult<()> {
        self.terminate_signal.notify_one();
        Ok(())
    }

    /// The `Send + Sync` handle other threads use to abort JavaScript running
    /// on this worker (and, until cleared, reject new evaluations). See
    /// [`crate::InterruptHandle`].
    ///
    /// Interruption is deliberately decoupled from [`terminate`](Self::terminate):
    /// a graceful shutdown lets the in-flight task finish at its next yield,
    /// while a caller that must break non-yielding JavaScript interrupts
    /// through this handle explicitly.
    pub fn interrupt_handle(&self) -> crate::InterruptHandle {
        self.interrupt.clone()
    }
}

impl<E: JSEngine + 'static> Clone for Worker<E> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            task_tx: self.task_tx.clone(),
            terminate_signal: self.terminate_signal.clone(),
            interrupt: self.interrupt.clone(),
            inflight_tasks: self.inflight_tasks.clone(),
            idle_notify: self.idle_notify.clone(),
            any_worker_idle: self.any_worker_idle.clone(),
            message_queue_capacity: self.message_queue_capacity,
            thread_handle: self.thread_handle.clone(),
        }
    }
}

struct RongInner<E: JSEngine + 'static> {
    workers: Vec<Worker<E>>,
    any_worker_idle: Arc<Notify>,
}

pub struct Rong<E: JSEngine + 'static> {
    inner: Arc<RongInner<E>>,
}

impl<E: JSEngine + 'static> Clone for Rong<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E: JSEngine + 'static> Rong<E> {
    pub fn worker(&self, id: usize) -> JSResult<Worker<E>> {
        self.inner.workers.get(id).cloned().ok_or_else(|| {
            HostError::new(crate::error::E_NOT_FOUND, format!("Worker {id} not found")).into()
        })
    }

    pub fn workers(&self) -> Vec<Worker<E>> {
        self.inner.workers.clone()
    }

    pub fn free_workers_count(&self) -> usize {
        self.inner
            .workers
            .iter()
            .filter(|worker| worker.state() == WorkerState::Idle)
            .count()
    }

    pub fn total_workers_count(&self) -> usize {
        self.inner.workers.len()
    }

    pub async fn spawn<F, Fut, R>(&self, future_fn: F) -> JSResult<TaskHandle<R>>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        loop {
            for worker in &self.inner.workers {
                if worker.reserve_if_idle() {
                    return worker.spawn_inner(future_fn, true).await;
                }
            }

            self.inner.any_worker_idle.notified().await;
        }
    }

    pub async fn call<F, Fut, R>(&self, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        self.spawn(future_fn).await?.join().await
    }

    pub fn call_blocking<F, Fut, R>(&self, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("Rong::call_blocking")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call(future_fn))
    }

    pub async fn join(&self) -> JSResult<()> {
        futures::future::try_join_all(self.inner.workers.iter().map(Worker::join)).await?;
        Ok(())
    }

    pub fn shutdown(&self) -> JSResult<()> {
        for worker in &self.inner.workers {
            if let Err(err) = worker.terminate() {
                warn!(target: "rong", worker_id = worker.id, error = ?err, "failed to terminate worker");
            }
        }

        let mut workers = self.inner.workers.iter();
        crate::worker_thread::shutdown_worker_threads(
            move || {
                let worker = workers.next()?;
                crate::worker_thread::take_thread_handle(&worker.thread_handle)
                    .map(|handle| (worker.id, handle))
            },
            "skipping join on current worker thread during shutdown",
            "worker thread panicked during shutdown",
        );

        Ok(())
    }
}

impl<E: JSEngine + 'static> Drop for Rong<E> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            let _ = self.shutdown();
        }
    }
}

pub(crate) fn ensure_sync_bridge_allowed(api_name: &str) -> JSResult<()> {
    if crate::worker_thread::in_worker_thread() {
        return Err(HostError::new(
            crate::error::E_INTERNAL,
            format!("{api_name} cannot run from inside a Rong worker thread"),
        )
        .into());
    }

    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(HostError::new(
            crate::error::E_INTERNAL,
            format!(
                "{api_name} cannot run from inside an active Tokio runtime; use .await instead"
            ),
        )
        .into());
    }

    Ok(())
}

pub(crate) fn build_shared_workers<E: JSEngine + 'static>(
    worker_count: usize,
    task_queue_capacity: usize,
    message_queue_capacity: usize,
) -> Result<Rong<E>, crate::rong::RongBuildError> {
    let any_worker_idle = Arc::new(Notify::new());
    let workers = initialize_workers::<E>(
        worker_count,
        task_queue_capacity,
        message_queue_capacity,
        any_worker_idle.clone(),
    )?;

    Ok(Rong {
        inner: Arc::new(RongInner {
            workers,
            any_worker_idle,
        }),
    })
}

fn initialize_workers<E: JSEngine + 'static>(
    worker_count: usize,
    task_queue_capacity: usize,
    message_queue_capacity: usize,
    any_worker_idle: Arc<Notify>,
) -> Result<Vec<Worker<E>>, crate::rong::RongBuildError> {
    let mut workers = Vec::with_capacity(worker_count);

    for worker_id in 0..worker_count {
        let (task_tx, task_rx) = mpsc::channel(task_queue_capacity);
        let terminate_signal = crate::worker_thread::terminate_signal();
        let interrupt = crate::InterruptHandle::new();
        let inflight_tasks = Arc::new(AtomicUsize::new(0));
        let idle_notify = Arc::new(Notify::new());
        let thread_handle = Arc::new(StdMutex::new(None));
        let worker_span = info_span!("rong.worker", worker_id = worker_id);

        let worker = Worker {
            id: worker_id,
            task_tx,
            terminate_signal: terminate_signal.clone(),
            interrupt: interrupt.clone(),
            inflight_tasks: inflight_tasks.clone(),
            idle_notify: idle_notify.clone(),
            any_worker_idle: any_worker_idle.clone(),
            message_queue_capacity,
            thread_handle: thread_handle.clone(),
        };

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

        let thread_any_worker_idle = any_worker_idle.clone();
        let run_span = worker_span.clone();
        let handle = crate::worker_thread::spawn_js_worker_thread(
            worker_id,
            format!("worker-{worker_id}"),
            worker_span.clone(),
            "worker thread started",
            "worker thread stopped",
            ready_tx,
            move |ready_tx| async move {
                run_worker_loop::<E>(
                    worker_id,
                    task_rx,
                    terminate_signal,
                    interrupt,
                    inflight_tasks,
                    idle_notify,
                    thread_any_worker_idle,
                    run_span,
                    ready_tx,
                )
                .await;
            },
        );

        *thread_handle.lock().unwrap() = Some(handle);

        match ready_rx.recv() {
            Ok(Ok(())) => workers.push(worker),
            Ok(Err(reason)) => {
                shutdown_workers(&workers);
                return Err(crate::rong::RongBuildError::WorkerStart { worker_id, reason });
            }
            Err(err) => {
                shutdown_workers(&workers);
                return Err(crate::rong::RongBuildError::WorkerStart {
                    worker_id,
                    reason: err.to_string(),
                });
            }
        }
    }

    Ok(workers)
}

fn shutdown_workers<E: JSEngine + 'static>(workers: &[Worker<E>]) {
    for worker in workers {
        let _ = worker.terminate();
    }

    let mut workers = workers.iter();
    crate::worker_thread::shutdown_worker_threads(
        move || {
            let worker = workers.next()?;
            crate::worker_thread::take_thread_handle(&worker.thread_handle)
                .map(|handle| (worker.id, handle))
        },
        "skipping join on current worker thread during shutdown",
        "worker thread panicked during shutdown",
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_worker_loop<E: JSEngine + 'static>(
    worker_id: usize,
    mut task_rx: mpsc::Receiver<UserAsyncTask<E>>,
    terminate_signal: Arc<Notify>,
    interrupt: crate::InterruptHandle,
    inflight_tasks: Arc<AtomicUsize>,
    idle_notify: Arc<Notify>,
    any_worker_idle: Arc<Notify>,
    worker_span: Span,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
) {
    let local = tokio::task::LocalSet::new();

    local
        .run_until(async move {
            let js_runtime = E::runtime_with_interrupt(interrupt);
            let _ = ready_tx.send(Ok(()));

            let microtask_runner = if js_runtime.run_pending_jobs() >= 0 {
                let runtime = js_runtime.clone();
                let span = info_span!(parent: &worker_span, "rong.microtasks", worker_id = worker_id);
                Some(spawn_local(
                    async move {
                        let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
                        loop {
                            interval.tick().await;
                            runtime.run_pending_jobs();
                        }
                    }
                    .instrument(span),
                ))
            } else {
                None
            };

            type TaskJoinHandle = tokio::task::JoinHandle<
                Result<JSResult<Box<dyn Any + Send>>, futures::future::Aborted>,
            >;

            let mut current_task_join: Option<TaskJoinHandle> = None;
            let mut current_task_abort: Option<futures::future::AbortHandle> = None;
            let mut current_result_tx: Option<oneshot::Sender<JSResult<Box<dyn Any + Send>>>> = None;
            let mut current_task_span: Option<Span> = None;
            let mut shutting_down = false;

            loop {
                tokio::select! {
                    biased;

                    _ = terminate_signal.notified(), if !shutting_down => {
                        shutting_down = true;
                        if let Some(abort_handle) = current_task_abort.take() {
                            abort_handle.abort();
                        }
                    }

                    maybe_task = task_rx.recv(), if current_task_join.is_none() && !shutting_down => {
                        match maybe_task {
                            Some(task) => {
                                let task_span = info_span!(parent: &task.parent_span, "rong.task", worker_id = worker_id);
                                debug!(target: "rong", parent: &task_span, "worker task started");

                                let future = (task.future_fn)(js_runtime.clone(), task.message_receiver)
                                    .instrument(task_span.clone());
                                let (abortable_future, abort_handle) = futures::future::abortable(future);

                                current_task_abort = Some(abort_handle);
                                current_result_tx = Some(task.result_tx);
                                current_task_span = Some(task_span.clone());
                                current_task_join = Some(spawn_local(abortable_future.instrument(task_span)));
                            }
                            None => {
                                shutting_down = true;
                            }
                        }
                    }

                    task_result = async { current_task_join.as_mut().unwrap().await }, if current_task_join.is_some() => {
                        let final_result = match task_result {
                            Ok(Ok(inner)) => inner,
                            Ok(Err(_)) => Err(HostError::aborted(None).into()),
                            Err(join_error) => {
                                if let Some(task_span) = current_task_span.as_ref() {
                                    error!(target: "rong", parent: task_span, worker_id = worker_id, error = ?join_error, "user task panicked or runtime dropped");
                                } else {
                                    error!(target: "rong", parent: &worker_span, worker_id = worker_id, error = ?join_error, "user task panicked or runtime dropped");
                                }
                                Err(HostError::new(
                                    crate::error::E_INTERNAL,
                                    format!("User task panicked or runtime dropped: {}", join_error),
                                ).into())
                            }
                        };

                        if let Some(result_tx) = current_result_tx.take() {
                            let _ = result_tx.send(final_result);
                        }

                        current_task_join = None;
                        current_task_abort = None;
                        current_task_span = None;
                        if inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
                            idle_notify.notify_waiters();
                            any_worker_idle.notify_one();
                        }
                    }
                }

                if shutting_down && current_task_join.is_none() {
                    break;
                }
            }

            while let Ok(task) = task_rx.try_recv() {
                let _ = task.result_tx.send(Err(HostError::aborted(None).into()));
                if inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
                    idle_notify.notify_waiters();
                    any_worker_idle.notify_one();
                }
            }

            if let Some(handle) = microtask_runner {
                handle.abort();
            }

            if inflight_tasks.load(Ordering::SeqCst) == 0 {
                idle_notify.notify_waiters();
                any_worker_idle.notify_one();
            }
        })
        .await;
}

pub fn spawn_local<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + 'static,
{
    tokio::task::spawn_local(future)
}
