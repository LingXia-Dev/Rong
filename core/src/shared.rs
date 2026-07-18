use crate::{HostError, JSEngine, JSResult, JSRuntime};
use std::any::Any;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskExecutionPhase {
    Queued,
    Starting,
    Running,
    TimedOut,
    Finished,
}

struct TaskExecutionState {
    phase: TaskExecutionPhase,
    abort_handle: Option<futures::future::AbortHandle>,
    timeout: Option<Duration>,
    timeout_interrupt: Option<crate::InterruptGuard>,
}

pub(crate) struct TaskExecutionControl {
    interrupt: crate::InterruptHandle,
    state: StdMutex<TaskExecutionState>,
}

impl TaskExecutionControl {
    pub(crate) fn new(interrupt: crate::InterruptHandle) -> Self {
        Self {
            interrupt,
            state: StdMutex::new(TaskExecutionState {
                phase: TaskExecutionPhase::Queued,
                abort_handle: None,
                timeout: None,
                timeout_interrupt: None,
            }),
        }
    }

    /// Claim a queued task before invoking its future factory.
    pub(crate) fn begin_start(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        if state.phase != TaskExecutionPhase::Queued {
            return false;
        }
        state.phase = TaskExecutionPhase::Starting;
        true
    }

    /// Publish the abort handle once the task future has been constructed.
    pub(crate) fn install_abort(&self, abort_handle: futures::future::AbortHandle) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        if state.phase == TaskExecutionPhase::Starting {
            state.phase = TaskExecutionPhase::Running;
            state.abort_handle = Some(abort_handle);
            true
        } else {
            debug_assert_eq!(state.phase, TaskExecutionPhase::TimedOut);
            abort_handle.abort();
            false
        }
    }

    /// Win the deadline race and cancel only this task.
    fn request_timeout(&self, timeout: Duration) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        match state.phase {
            TaskExecutionPhase::Queued => {
                state.phase = TaskExecutionPhase::TimedOut;
                state.timeout = Some(timeout);
                true
            }
            TaskExecutionPhase::Starting | TaskExecutionPhase::Running => {
                state.phase = TaskExecutionPhase::TimedOut;
                state.timeout = Some(timeout);
                state.timeout_interrupt = Some(self.interrupt.interrupt_scoped());
                if let Some(abort_handle) = state.abort_handle.as_ref() {
                    abort_handle.abort();
                }
                true
            }
            TaskExecutionPhase::TimedOut => true,
            TaskExecutionPhase::Finished => false,
        }
    }

    /// Close the execution boundary and release this task's interrupt request.
    pub(crate) fn finish(&self) -> Option<Duration> {
        let (timeout, timeout_interrupt) = {
            let mut state = self.state.lock().unwrap_or_else(|err| err.into_inner());
            state.phase = TaskExecutionPhase::Finished;
            state.abort_handle = None;
            (state.timeout.take(), state.timeout_interrupt.take())
        };
        drop(timeout_interrupt);
        timeout
    }
}

pub(crate) fn task_timeout_error(worker_id: usize, timeout: Duration) -> crate::RongJSError {
    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    HostError::new(
        crate::error::E_TIMEOUT,
        format!("Worker task on worker {worker_id} timed out after {timeout:?}"),
    )
    .with_name("TimeoutError")
    .with_data(crate::err_data!({ worker_id: worker_id, timeout_ms: timeout_ms }))
    .into()
}

fn pool_timeout_error(timeout: Duration) -> crate::RongJSError {
    let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    HostError::new(
        crate::error::E_TIMEOUT,
        format!("Worker pool task timed out after {timeout:?}"),
    )
    .with_name("TimeoutError")
    .with_data(crate::err_data!({ timeout_ms: timeout_ms }))
    .into()
}

pub(crate) struct InflightGuard<'a> {
    inflight_tasks: &'a AtomicUsize,
    idle_notify: &'a Notify,
    any_worker_idle: &'a Notify,
    armed: bool,
}

impl<'a> InflightGuard<'a> {
    pub(crate) fn new(
        inflight_tasks: &'a AtomicUsize,
        idle_notify: &'a Notify,
        any_worker_idle: &'a Notify,
    ) -> Self {
        Self {
            inflight_tasks,
            idle_notify,
            any_worker_idle,
            armed: true,
        }
    }

    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        if self.armed && self.inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.idle_notify.notify_waiters();
            self.any_worker_idle.notify_one();
        }
    }
}

struct UserAsyncTask<E: JSEngine + 'static> {
    future_fn: BoxedFutureFn<E>,
    message_receiver: MessageReceiver,
    result_tx: oneshot::Sender<JSResult<Box<dyn Any + Send>>>,
    execution: Arc<TaskExecutionControl>,
    parent_span: Span,
}

pub struct TaskHandle<R> {
    worker_id: usize,
    message_tx: mpsc::Sender<TaskMessage>,
    result_rx: oneshot::Receiver<JSResult<Box<dyn Any + Send>>>,
    execution: Arc<TaskExecutionControl>,
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
        execution: Arc<TaskExecutionControl>,
    ) -> Self {
        Self {
            worker_id,
            message_tx,
            result_rx,
            execution,
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
        Self::decode_result(self.worker_id, self.result_rx.await)
    }

    /// Wait for the task, cancelling it if the timeout elapses first.
    ///
    /// The timeout includes time spent in the worker queue. A queued task is
    /// cancelled without interrupting the task currently using that worker. A
    /// running task has both its Rust future aborted and its JavaScript runtime
    /// interrupted. The worker releases that scoped interruption before it
    /// starts another task.
    ///
    /// On a cooperative-only engine, non-yielding synchronous JavaScript cannot
    /// be preempted; this method still returns `E_TIMEOUT`, but that worker stays
    /// busy until the engine call returns.
    pub async fn join_with_timeout(self, timeout: Duration) -> JSResult<R> {
        self.join_with_timeout_reported(timeout, timeout).await
    }

    pub(crate) async fn join_with_timeout_reported(
        self,
        wait_timeout: Duration,
        reported_timeout: Duration,
    ) -> JSResult<R> {
        let worker_id = self.worker_id;
        let execution = self.execution;
        let mut result_rx = self.result_rx;

        match tokio::time::timeout(wait_timeout, &mut result_rx).await {
            Ok(result) => Self::decode_result(worker_id, result),
            Err(_) if execution.request_timeout(reported_timeout) => {
                Err(task_timeout_error(worker_id, reported_timeout))
            }
            Err(_) => Self::decode_result(worker_id, result_rx.await),
        }
    }

    fn decode_result(
        worker_id: usize,
        result: Result<JSResult<Box<dyn Any + Send>>, oneshot::error::RecvError>,
    ) -> JSResult<R> {
        let result = result.map_err(|e| {
            HostError::new(
                crate::error::E_INTERNAL,
                format!(
                    "Failed to receive task result from worker {}: {:?}",
                    worker_id, e
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
        let mut inflight_guard = InflightGuard::new(
            &self.inflight_tasks,
            &self.idle_notify,
            &self.any_worker_idle,
        );

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
        let execution = Arc::new(TaskExecutionControl::new(self.interrupt.clone()));
        let task = UserAsyncTask {
            future_fn: boxed_fn,
            message_receiver: MessageReceiver::new(message_rx),
            result_tx,
            execution: execution.clone(),
            parent_span: Span::current(),
        };

        if let Err(e) = self.task_tx.send(task).await {
            return Err(HostError::new(
                crate::error::E_INTERNAL,
                format!("Failed to queue task on worker {}: {:?}", self.id, e),
            )
            .into());
        }
        inflight_guard.disarm();

        Ok(TaskHandle {
            worker_id: self.id,
            message_tx,
            result_rx,
            execution,
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

    /// Run one task with a wall-clock timeout covering queue and execution time.
    pub async fn call_with_timeout<F, Fut, R>(&self, timeout: Duration, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        let started = Instant::now();
        let handle = tokio::time::timeout(timeout, self.spawn(future_fn))
            .await
            .map_err(|_| task_timeout_error(self.id, timeout))??;
        handle
            .join_with_timeout_reported(timeout.saturating_sub(started.elapsed()), timeout)
            .await
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

    /// Blocking counterpart to [`call_with_timeout`](Self::call_with_timeout).
    pub fn call_blocking_with_timeout<F, Fut, R>(
        &self,
        timeout: Duration,
        future_fn: F,
    ) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("Worker::call_blocking_with_timeout")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call_with_timeout(timeout, future_fn))
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
    ///
    /// Call [`clear_interrupt`](Self::clear_interrupt) before reusing it. For a
    /// task-scoped deadline, prefer [`call_with_timeout`](Self::call_with_timeout).
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

    /// Dispatch one task with a wall-clock timeout covering queue and execution
    /// time.
    pub async fn call_with_timeout<F, Fut, R>(&self, timeout: Duration, future_fn: F) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        let started = Instant::now();
        let handle = tokio::time::timeout(timeout, self.spawn(future_fn))
            .await
            .map_err(|_| pool_timeout_error(timeout))??;
        handle
            .join_with_timeout_reported(timeout.saturating_sub(started.elapsed()), timeout)
            .await
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

    /// Blocking counterpart to [`call_with_timeout`](Self::call_with_timeout).
    pub fn call_blocking_with_timeout<F, Fut, R>(
        &self,
        timeout: Duration,
        future_fn: F,
    ) -> JSResult<R>
    where
        F: FnOnce(JSRuntime<E::Runtime>, MessageReceiver) -> Fut + Send + 'static,
        Fut: Future<Output = JSResult<R>> + 'static,
        R: Send + 'static,
    {
        ensure_sync_bridge_allowed("Rong::call_blocking_with_timeout")?;
        rong_rt::RongExecutor::global()
            .handle()
            .block_on(self.call_with_timeout(timeout, future_fn))
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
                                let UserAsyncTask {
                                    future_fn,
                                    message_receiver,
                                    result_tx,
                                    execution,
                                    parent_span,
                                } = task;
                                let task_span = info_span!(parent: &parent_span, "rong.task", worker_id = worker_id);

                                if execution.begin_start() {
                                    let future = future_fn(js_runtime.clone(), message_receiver)
                                        .instrument(task_span.clone());
                                    let (abortable_future, abort_handle) = futures::future::abortable(future);

                                    if execution.install_abort(abort_handle.clone()) {
                                        debug!(target: "rong", parent: &task_span, "worker task started");
                                        current_task_abort = Some(abort_handle);
                                        current_result_tx = Some(result_tx);
                                        current_execution = Some(execution);
                                        current_task_span = Some(task_span.clone());
                                        current_task_join = Some(spawn_local(abortable_future.instrument(task_span)));
                                    } else {
                                        let timeout = execution.finish().expect("a cancelled starting task has a timeout");
                                        let _ = result_tx.send(Err(task_timeout_error(worker_id, timeout)));
                                        if inflight_tasks.fetch_sub(1, Ordering::SeqCst) == 1 {
                                            idle_notify.notify_waiters();
                                            any_worker_idle.notify_one();
                                        }
                                    }
                                } else {
                                    let timeout = execution.finish().expect("a cancelled queued task has a timeout");
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

pub fn spawn_local<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + 'static,
{
    tokio::task::spawn_local(future)
}
