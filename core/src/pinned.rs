use crate::shared::{
    InflightGuard, MessageReceiver, TaskExecutionControl, TaskHandle, ensure_sync_bridge_allowed,
    spawn_local, task_timeout_error,
};
use crate::{HostError, JSEngine, JSResult, JSRuntime};
use futures::future::Aborted;
use std::any::Any;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Notify, mpsc, oneshot};
use tracing::{Instrument, Span, debug, error, info_span, warn};

type BoxedPinnedTaskFuture<S> =
    Pin<Box<dyn Future<Output = (JSResult<Box<dyn Any + Send>>, Option<S>)> + 'static>>;
type BoxedPinnedFutureFn<E, K, S> = Box<
    dyn FnOnce(
            JSRuntime<<E as JSEngine>::Runtime>,
            K,
            Option<S>,
            MessageReceiver,
        ) -> BoxedPinnedTaskFuture<S>
        + Send,
>;

struct PinnedAsyncTask<E: JSEngine + 'static, K, S> {
    key: K,
    future_fn: BoxedPinnedFutureFn<E, K, S>,
    message_receiver: MessageReceiver,
    result_tx: oneshot::Sender<JSResult<Box<dyn Any + Send>>>,
    execution: Arc<TaskExecutionControl>,
    parent_span: Span,
}

#[derive(Debug, Error)]
pub enum PinnedSpawnError {
    #[error("pinned worker {worker_id} queue is full at depth {depth}")]
    QueueFull { worker_id: usize, depth: usize },
    #[error("pinned worker {worker_id} stopped")]
    WorkerStopped { worker_id: usize },
}

pub struct PinnedWorker<E: JSEngine + 'static, K, S> {
    id: usize,
    task_tx: mpsc::Sender<PinnedAsyncTask<E, K, S>>,
    terminate_signal: Arc<Notify>,
    interrupt: crate::InterruptHandle,
    inflight_tasks: Arc<AtomicUsize>,
    idle_notify: Arc<Notify>,
    any_worker_idle: Arc<Notify>,
    message_queue_capacity: usize,
    thread_handle: Arc<StdMutex<Option<std::thread::JoinHandle<()>>>>,
}

impl<E: JSEngine + 'static, K, S> Clone for PinnedWorker<E, K, S> {
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

impl<E, K, S> PinnedWorker<E, K, S>
where
    E: JSEngine + 'static,
    K: Send + 'static,
    S: 'static,
{
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn state(&self) -> crate::shared::WorkerState {
        if self.inflight_tasks.load(Ordering::SeqCst) == 0 {
            crate::shared::WorkerState::Idle
        } else {
            crate::shared::WorkerState::Busy
        }
    }

    pub fn pending(&self) -> usize {
        self.inflight_tasks.load(Ordering::SeqCst)
    }

    fn decrement_inflight(&self) {
        if self.inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.idle_notify.notify_waiters();
            self.any_worker_idle.notify_one();
        }
    }

    async fn spawn_inner<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<TaskHandle<R>>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.inflight_tasks.fetch_add(1, Ordering::SeqCst);
        let mut inflight_guard = InflightGuard::new(
            &self.inflight_tasks,
            &self.idle_notify,
            &self.any_worker_idle,
        );

        let boxed_fn: BoxedPinnedFutureFn<E, K, S> = Box::new(
            move |runtime: JSRuntime<E::Runtime>,
                  key: K,
                  state: Option<S>,
                  receiver: MessageReceiver| {
                let user_fut = future_fn(runtime, key, state, receiver);
                let mapped = async move {
                    let (result, state) = user_fut.await;
                    (
                        result.map(|value| Box::new(value) as Box<dyn Any + Send>),
                        state,
                    )
                };
                Box::pin(mapped) as BoxedPinnedTaskFuture<S>
            },
        );

        let (message_tx, message_rx) = mpsc::channel(self.message_queue_capacity);
        let (result_tx, result_rx) = oneshot::channel();
        let execution = Arc::new(TaskExecutionControl::new(self.interrupt.clone()));
        let task = PinnedAsyncTask {
            key,
            future_fn: boxed_fn,
            message_receiver: MessageReceiver::new(message_rx),
            result_tx,
            execution: execution.clone(),
            parent_span: Span::current(),
        };

        if let Err(error) = self.task_tx.send(task).await {
            return Err(HostError::new(
                crate::error::E_INTERNAL,
                format!(
                    "Failed to queue pinned task on worker {}: {:?}",
                    self.id, error
                ),
            )
            .into());
        }
        inflight_guard.disarm();

        Ok(TaskHandle::new(self.id, message_tx, result_rx, execution))
    }

    fn try_spawn_inner<F, Fut, R>(
        &self,
        key: K,
        future_fn: F,
    ) -> Result<TaskHandle<R>, PinnedSpawnError>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        let pending_before = self.inflight_tasks.fetch_add(1, Ordering::SeqCst);

        let boxed_fn: BoxedPinnedFutureFn<E, K, S> = Box::new(
            move |runtime: JSRuntime<E::Runtime>,
                  key: K,
                  state: Option<S>,
                  receiver: MessageReceiver| {
                let user_fut = future_fn(runtime, key, state, receiver);
                let mapped = async move {
                    let (result, state) = user_fut.await;
                    (
                        result.map(|value| Box::new(value) as Box<dyn Any + Send>),
                        state,
                    )
                };
                Box::pin(mapped) as BoxedPinnedTaskFuture<S>
            },
        );

        let (message_tx, message_rx) = mpsc::channel(self.message_queue_capacity);
        let (result_tx, result_rx) = oneshot::channel();
        let execution = Arc::new(TaskExecutionControl::new(self.interrupt.clone()));
        let task = PinnedAsyncTask {
            key,
            future_fn: boxed_fn,
            message_receiver: MessageReceiver::new(message_rx),
            result_tx,
            execution: execution.clone(),
            parent_span: Span::current(),
        };

        match self.task_tx.try_send(task) {
            Ok(()) => Ok(TaskHandle::new(self.id, message_tx, result_rx, execution)),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.decrement_inflight();
                Err(PinnedSpawnError::QueueFull {
                    worker_id: self.id,
                    depth: pending_before,
                })
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.decrement_inflight();
                Err(PinnedSpawnError::WorkerStopped { worker_id: self.id })
            }
        }
    }

    pub async fn spawn<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<TaskHandle<R>>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.spawn_inner(key, future_fn).await
    }

    pub fn try_spawn<F, Fut, R>(
        &self,
        key: K,
        future_fn: F,
    ) -> Result<TaskHandle<R>, PinnedSpawnError>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.try_spawn_inner(key, future_fn)
    }

    pub async fn call<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.spawn(key, future_fn).await?.join().await
    }

    /// Run one pinned task with a wall-clock timeout covering queue and
    /// execution time.
    pub async fn call_with_timeout<F, Fut, R>(
        &self,
        timeout: Duration,
        key: K,
        future_fn: F,
    ) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        let started = Instant::now();
        let handle = tokio::time::timeout(timeout, self.spawn(key, future_fn))
            .await
            .map_err(|_| task_timeout_error(self.id, timeout))??;
        handle
            .join_with_timeout_reported(timeout.saturating_sub(started.elapsed()), timeout)
            .await
    }

    pub fn call_blocking<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("PinnedWorker::call_blocking")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call(key, future_fn))
    }

    /// Blocking counterpart to [`call_with_timeout`](Self::call_with_timeout).
    pub fn call_blocking_with_timeout<F, Fut, R>(
        &self,
        timeout: Duration,
        key: K,
        future_fn: F,
    ) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("PinnedWorker::call_blocking_with_timeout")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call_with_timeout(timeout, key, future_fn))
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

    /// Request a persistent hard interruption on this worker.
    pub fn interrupt(&self) {
        self.interrupt.interrupt();
    }

    /// Clear a persistent request made through [`interrupt`](Self::interrupt).
    pub fn clear_interrupt(&self) {
        self.interrupt.clear();
    }

    /// The interruption behavior of this worker's engine.
    pub fn interrupt_mode(&self) -> crate::InterruptMode {
        self.interrupt.mode()
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

struct PinnedRongInner<E: JSEngine + 'static, K: Eq + Hash + Clone + Send + 'static, S: 'static> {
    workers: Vec<PinnedWorker<E, K, S>>,
}

pub struct PinnedRong<E: JSEngine + 'static, K: Eq + Hash + Clone + Send + 'static, S: 'static> {
    inner: Arc<PinnedRongInner<E, K, S>>,
}

impl<E: JSEngine + 'static, K: Eq + Hash + Clone + Send + 'static, S: 'static> Clone
    for PinnedRong<E, K, S>
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E, K, S> PinnedRong<E, K, S>
where
    E: JSEngine + 'static,
    K: Eq + Hash + Clone + Send + 'static,
    S: 'static,
{
    pub(crate) fn build(
        workers: usize,
        task_queue_capacity: usize,
        message_queue_capacity: usize,
    ) -> Result<Self, crate::rong::RongBuildError> {
        if workers == 0 {
            return Err(crate::rong::RongBuildError::InvalidWorkers);
        }
        if task_queue_capacity == 0 {
            return Err(crate::rong::RongBuildError::InvalidTaskQueueCapacity);
        }
        if message_queue_capacity == 0 {
            return Err(crate::rong::RongBuildError::InvalidMessageQueueCapacity);
        }

        let any_worker_idle = Arc::new(Notify::new());
        let workers = initialize_pinned_workers::<E, K, S>(
            workers,
            task_queue_capacity,
            message_queue_capacity,
            any_worker_idle,
        )?;

        Ok(Self {
            inner: Arc::new(PinnedRongInner { workers }),
        })
    }

    pub fn total_workers_count(&self) -> usize {
        self.inner.workers.len()
    }

    pub fn worker_id_for_key(&self, key: &K) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.inner.workers.len()
    }

    pub fn worker_for_key(&self, key: &K) -> PinnedWorker<E, K, S> {
        self.inner.workers[self.worker_id_for_key(key)].clone()
    }

    pub async fn spawn<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<TaskHandle<R>>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.worker_for_key(&key).spawn(key, future_fn).await
    }

    pub fn try_spawn<F, Fut, R>(
        &self,
        key: K,
        future_fn: F,
    ) -> Result<TaskHandle<R>, PinnedSpawnError>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.worker_for_key(&key).try_spawn(key, future_fn)
    }

    pub async fn call<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.spawn(key, future_fn).await?.join().await
    }

    /// Dispatch one pinned task with a wall-clock timeout covering queue and
    /// execution time.
    pub async fn call_with_timeout<F, Fut, R>(
        &self,
        timeout: Duration,
        key: K,
        future_fn: F,
    ) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        self.worker_for_key(&key)
            .call_with_timeout(timeout, key, future_fn)
            .await
    }

    pub fn call_blocking<F, Fut, R>(&self, key: K, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("PinnedRong::call_blocking")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call(key, future_fn))
    }

    /// Blocking counterpart to [`call_with_timeout`](Self::call_with_timeout).
    pub fn call_blocking_with_timeout<F, Fut, R>(
        &self,
        timeout: Duration,
        key: K,
        future_fn: F,
    ) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, K, Option<S>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = (JSResult<R>, Option<S>)> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("PinnedRong::call_blocking_with_timeout")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call_with_timeout(timeout, key, future_fn))
    }

    pub async fn join(&self) -> JSResult<()> {
        futures::future::try_join_all(self.inner.workers.iter().map(PinnedWorker::join)).await?;
        Ok(())
    }

    pub fn shutdown(&self) -> JSResult<()> {
        for worker in &self.inner.workers {
            if let Err(err) = worker.terminate() {
                warn!(target: "rong", worker_id = worker.id(), error = ?err, "failed to terminate pinned worker");
            }
        }

        let mut workers = self.inner.workers.iter();
        crate::worker_thread::shutdown_worker_threads(
            move || {
                let worker = workers.next()?;
                crate::worker_thread::take_thread_handle(&worker.thread_handle)
                    .map(|handle| (worker.id(), handle))
            },
            "skipping join on current pinned worker thread during shutdown",
            "pinned worker thread panicked during shutdown",
        );

        Ok(())
    }
}

impl<E, K, S> Drop for PinnedRong<E, K, S>
where
    E: JSEngine + 'static,
    K: Eq + Hash + Clone + Send + 'static,
    S: 'static,
{
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            let _ = self.shutdown();
        }
    }
}

fn initialize_pinned_workers<E, K, S>(
    worker_count: usize,
    task_queue_capacity: usize,
    message_queue_capacity: usize,
    any_worker_idle: Arc<Notify>,
) -> Result<Vec<PinnedWorker<E, K, S>>, crate::rong::RongBuildError>
where
    E: JSEngine + 'static,
    K: Eq + Hash + Clone + Send + 'static,
    S: 'static,
{
    let mut workers = Vec::with_capacity(worker_count);

    for worker_id in 0..worker_count {
        let (task_tx, task_rx) = mpsc::channel(task_queue_capacity);
        let terminate_signal = crate::worker_thread::terminate_signal();
        let interrupt = crate::InterruptHandle::new();
        let inflight_tasks = Arc::new(AtomicUsize::new(0));
        let idle_notify = Arc::new(Notify::new());
        let thread_handle = Arc::new(StdMutex::new(None));
        let worker_span = info_span!("rong.pinned_worker", worker_id = worker_id);

        let worker = PinnedWorker {
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
            format!("pinned-worker-{worker_id}"),
            worker_span.clone(),
            "pinned worker thread started",
            "pinned worker thread stopped",
            ready_tx,
            move |ready_tx| async move {
                run_pinned_worker_loop::<E, K, S>(
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
                shutdown_pinned_workers(&workers);
                return Err(crate::rong::RongBuildError::WorkerStart { worker_id, reason });
            }
            Err(err) => {
                shutdown_pinned_workers(&workers);
                return Err(crate::rong::RongBuildError::WorkerStart {
                    worker_id,
                    reason: err.to_string(),
                });
            }
        }
    }

    Ok(workers)
}

fn shutdown_pinned_workers<E, K, S>(workers: &[PinnedWorker<E, K, S>])
where
    E: JSEngine + 'static,
    K: Send + 'static,
    S: 'static,
{
    for worker in workers {
        let _ = worker.terminate();
    }

    let mut workers = workers.iter();
    crate::worker_thread::shutdown_worker_threads(
        move || {
            let worker = workers.next()?;
            crate::worker_thread::take_thread_handle(&worker.thread_handle)
                .map(|handle| (worker.id(), handle))
        },
        "skipping join on current pinned worker thread during shutdown",
        "pinned worker thread panicked during shutdown",
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_pinned_worker_loop<E, K, S>(
    worker_id: usize,
    mut task_rx: mpsc::Receiver<PinnedAsyncTask<E, K, S>>,
    terminate_signal: Arc<Notify>,
    interrupt: crate::InterruptHandle,
    inflight_tasks: Arc<AtomicUsize>,
    idle_notify: Arc<Notify>,
    any_worker_idle: Arc<Notify>,
    worker_span: Span,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
) where
    E: JSEngine + 'static,
    K: Eq + Hash + Clone + Send + 'static,
    S: 'static,
{
    let local = tokio::task::LocalSet::new();

    local
        .run_until(async move {
            let js_runtime = E::runtime_with_interrupt(interrupt);
            let _ = ready_tx.send(Ok(()));
            let mut states = HashMap::<K, S>::new();

            let microtask_runner = if js_runtime.run_pending_jobs() >= 0 {
                let runtime = js_runtime.clone();
                let span =
                    info_span!(parent: &worker_span, "rong.pinned_microtasks", worker_id = worker_id);
                Some(spawn_local(
                    async move {
                        let mut interval =
                            tokio::time::interval(std::time::Duration::from_millis(50));
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

            type TaskJoinHandle<K, S> = tokio::task::JoinHandle<
                Result<(JSResult<Box<dyn Any + Send>>, K, Option<S>), Aborted>,
            >;

            let mut current_task_join: Option<TaskJoinHandle<K, S>> = None;
            let mut current_task_abort: Option<futures::future::AbortHandle> = None;
            let mut current_result_tx: Option<oneshot::Sender<JSResult<Box<dyn Any + Send>>>> = None;
            let mut current_execution: Option<Arc<TaskExecutionControl>> = None;
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
                                let PinnedAsyncTask {
                                    key,
                                    future_fn,
                                    message_receiver,
                                    result_tx,
                                    execution,
                                    parent_span,
                                } = task;
                                let task_span = info_span!(parent: &parent_span, "rong.pinned_task", worker_id = worker_id);

                                if execution.begin_start() {
                                    let state = states.remove(&key);
                                    let future = future_fn(js_runtime.clone(), key.clone(), state, message_receiver);
                                    let task_future = async move {
                                        let (result, state) = future.await;
                                        (result, key, state)
                                    }
                                    .instrument(task_span.clone());
                                    let (abortable_future, abort_handle) = futures::future::abortable(task_future);

                                    if execution.install_abort(abort_handle.clone()) {
                                        debug!(target: "rong", parent: &task_span, "pinned worker task started");
                                        current_task_abort = Some(abort_handle);
                                        current_result_tx = Some(result_tx);
                                        current_execution = Some(execution);
                                        current_task_span = Some(task_span.clone());
                                        current_task_join = Some(spawn_local(abortable_future.instrument(task_span)));
                                    } else {
                                        let timeout = execution.finish().expect("a cancelled starting pinned task has a timeout");
                                        let _ = result_tx.send(Err(task_timeout_error(worker_id, timeout)));
                                        if inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
                                            idle_notify.notify_waiters();
                                            any_worker_idle.notify_one();
                                        }
                                    }
                                } else {
                                    let timeout = execution.finish().expect("a cancelled queued pinned task has a timeout");
                                    let _ = result_tx.send(Err(task_timeout_error(worker_id, timeout)));
                                    if inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
                                        idle_notify.notify_waiters();
                                        any_worker_idle.notify_one();
                                    }
                                }
                            }
                            None => {
                                shutting_down = true;
                            }
                        }
                    }

                    task_result = async { current_task_join.as_mut().unwrap().await }, if current_task_join.is_some() => {
                        let timeout = current_execution
                            .take()
                            .and_then(|execution| execution.finish());
                        let final_result = if let Some(timeout) = timeout {
                            Err(task_timeout_error(worker_id, timeout))
                        } else {
                            match task_result {
                                Ok(Ok((inner, key, state))) => {
                                    if let Some(state) = state {
                                        states.insert(key, state);
                                    }
                                    inner
                                }
                                Ok(Err(_)) => Err(HostError::aborted(None).into()),
                                Err(join_error) => {
                                    if let Some(task_span) = current_task_span.as_ref() {
                                        error!(target: "rong", parent: task_span, worker_id = worker_id, error = ?join_error, "pinned task panicked or runtime dropped");
                                    } else {
                                        error!(target: "rong", parent: &worker_span, worker_id = worker_id, error = ?join_error, "pinned task panicked or runtime dropped");
                                    }
                                    Err(HostError::new(
                                        crate::error::E_INTERNAL,
                                        format!("Pinned task panicked or runtime dropped: {}", join_error),
                                    ).into())
                                }
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
                let result = match task.execution.finish() {
                    Some(timeout) => Err(task_timeout_error(worker_id, timeout)),
                    None => Err(HostError::aborted(None).into()),
                };
                let _ = task.result_tx.send(result);
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
