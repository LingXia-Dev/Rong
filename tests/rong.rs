use rong::{
    JSContextService, JSFunc, JSObject, JSResult, JsInvokePriority, Rong, RongJS, Source,
    TaskMessage, enqueue_js_invoke,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct ShutdownFlag(Arc<AtomicBool>);

impl JSContextService for ShutdownFlag {
    fn on_shutdown(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn test_call_simple() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let result = rong
        .call(|_runtime, _receiver| async {
            let value = 10 + 20;
            Ok(value)
        })
        .await?;
    assert_eq!(result, 30);
    Ok(())
}

#[tokio::test]
async fn test_send_usize_message() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let worker = rong.worker(0)?;
    let received = Arc::new(Mutex::new(None::<usize>));
    let received_clone = received.clone();

    let task = worker
        .spawn::<_, _, ()>(async move |_runtime, mut receiver| {
            if let Some(TaskMessage::Usize(value)) = receiver.recv().await {
                *received_clone.lock().unwrap() = Some(value);
            }
            Ok(())
        })
        .await?;

    task.send(TaskMessage::Usize(123)).await?;
    task.join().await?;
    rong.join().await?;

    assert_eq!(*received.lock().unwrap(), Some(123));
    Ok(())
}

#[tokio::test]
async fn test_send_string_message() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let worker = rong.worker(0)?;
    let received = Arc::new(Mutex::new(None::<String>));
    let received_clone = received.clone();

    let task = worker
        .spawn::<_, _, ()>(async move |_runtime, mut receiver| {
            if let Some(TaskMessage::String(value)) = receiver.recv().await {
                *received_clone.lock().unwrap() = Some(value);
            }
            Ok(())
        })
        .await?;

    let expected = "hello worker".to_string();
    task.send(TaskMessage::String(expected.clone())).await?;
    task.join().await?;
    rong.join().await?;

    assert_eq!(received.lock().unwrap().clone(), Some(expected));
    Ok(())
}

#[tokio::test]
async fn test_send_custom_message() -> JSResult<()> {
    #[derive(Debug, PartialEq, Clone)]
    struct MyCustomData {
        id: i32,
        name: String,
    }

    let rong = Rong::<RongJS>::builder().shared().build()?;
    let worker = rong.worker(0)?;
    let received = Arc::new(Mutex::new(None::<MyCustomData>));
    let received_clone = received.clone();

    let task = worker
        .spawn::<_, _, ()>(async move |_runtime, mut receiver| {
            if let Some(TaskMessage::Custom(value)) = receiver.recv().await
                && let Ok(value) = value.downcast::<MyCustomData>()
            {
                *received_clone.lock().unwrap() = Some(*value);
            }
            Ok(())
        })
        .await?;

    let expected = MyCustomData {
        id: 99,
        name: "test data".to_string(),
    };
    task.send(TaskMessage::Custom(Box::new(expected.clone())))
        .await?;
    task.join().await?;
    rong.join().await?;

    assert_eq!(received.lock().unwrap().clone(), Some(expected));
    Ok(())
}

#[tokio::test]
async fn test_worker_termination() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let worker = rong.worker(0)?;
    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();

    let task = worker
        .spawn::<_, _, ()>(async move |_runtime, _receiver| {
            started_clone.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        })
        .await?;

    while !started.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    worker.terminate()?;

    let err = tokio::time::timeout(Duration::from_secs(2), task.join())
        .await
        .expect("task join timed out")
        .expect_err("terminated task should not complete successfully");
    assert!(err.to_string().contains("aborted"));

    rong.join().await?;
    assert_eq!(worker.state(), rong::WorkerState::Idle);
    Ok(())
}

#[tokio::test]
async fn test_enqueue_js_invoke_queue() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    rong.call(|runtime, _receiver| async move {
        let ctx = runtime.context();
        let script = r#"(() => {
            globalThis.__invoke_counter = 0;
            return function () {
                globalThis.__invoke_counter += 1;
                return globalThis.__invoke_counter;
            };
        })()"#;
        let js_fn: JSFunc = ctx.eval(Source::from_bytes(script))?;

        enqueue_js_invoke(
            &ctx,
            js_fn.clone(),
            None,
            None,
            JsInvokePriority::Normal,
            None,
            true,
        )
        .await?;
        enqueue_js_invoke(
            &ctx,
            js_fn.clone(),
            None,
            None,
            JsInvokePriority::High,
            None,
            true,
        )
        .await?;
        enqueue_js_invoke(
            &ctx,
            js_fn,
            None,
            None,
            JsInvokePriority::Normal,
            None,
            true,
        )
        .await?;

        let final_value: i32 = ctx.global().get("__invoke_counter")?;
        assert_eq!(final_value, 3);
        Ok(())
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn pending_invoke_does_not_keep_context_alive() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicBool::new(false));
    let shutdown_for_task = shutdown.clone();
    let started_for_task = started.clone();

    rong.call(move |runtime, _receiver| async move {
        let ctx = runtime.context();
        ctx.set_service(ShutdownFlag(shutdown_for_task.clone()));
        ctx.global().set(
            "markInvokeStarted",
            JSFunc::new(&ctx, move || {
                started_for_task.store(true, Ordering::SeqCst);
            })?,
        )?;
        let pending: JSFunc = ctx.eval(Source::from_bytes(
            r#"(async function () {
                markInvokeStarted();
                await new Promise(() => {});
            })"#,
        ))?;

        enqueue_js_invoke(
            &ctx,
            pending,
            None,
            None,
            JsInvokePriority::Event,
            None,
            false,
        )
        .await?;
        tokio::time::timeout(Duration::from_secs(1), async {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending invoke did not start");

        drop(ctx);
        tokio::task::yield_now().await;

        assert!(
            shutdown_for_task.load(Ordering::SeqCst),
            "pending invoke work must not keep its JS context alive"
        );
        Ok(())
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_spawn_waits_for_idle_worker() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().workers(1).build()?;

    let first = rong
        .spawn::<_, _, ()>(async move |_runtime, _receiver| {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        })
        .await?;

    let second = tokio::time::timeout(
        Duration::from_secs(1),
        rong.spawn::<_, _, ()>(async move |_runtime, _receiver| Ok(())),
    )
    .await
    .expect("Rong::spawn timed out")?;

    assert_eq!(first.worker_id(), second.worker_id());
    first.join().await?;
    second.join().await?;
    rong.join().await?;
    Ok(())
}

#[tokio::test]
async fn test_reentrant_rong_call_blocking_returns_error() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let rong_clone = rong.clone();
    let nested_error = Arc::new(Mutex::new(None::<String>));
    let nested_error_clone = nested_error.clone();

    rong.call(move |_runtime, _receiver| async move {
        let err = rong_clone
            .call_blocking(|_runtime, _receiver| async move { Ok::<_, rong::RongJSError>(123) })
            .expect_err("reentrant Rong::call_blocking should fail");
        *nested_error_clone.lock().unwrap() = Some(err.to_string());
        Ok(())
    })
    .await?;

    let message = nested_error.lock().unwrap().clone().unwrap_or_default();
    assert!(
        message.contains("Rong::call_blocking") || message.contains("Rong worker thread"),
        "unexpected reentrant error: {message}"
    );

    Ok(())
}

#[tokio::test]
async fn test_reentrant_pinned_rong_call_blocking_returns_error() -> JSResult<()> {
    let workers = Rong::<RongJS>::builder()
        .pinned::<String, usize>()
        .workers(1)
        .build()?;
    let workers_clone = workers.clone();
    let nested_error = Arc::new(Mutex::new(None::<String>));
    let nested_error_clone = nested_error.clone();

    workers
        .call(
            "alpha".to_owned(),
            move |_runtime, _key, state, _receiver| async move {
                let err = workers_clone
                    .call_blocking(
                        "alpha".to_owned(),
                        |_runtime, _key, state, _receiver| async move {
                            (Ok::<_, rong::RongJSError>(state.unwrap_or(0) + 1), state)
                        },
                    )
                    .expect_err("reentrant PinnedRong::call_blocking should fail");
                *nested_error_clone.lock().unwrap() = Some(err.to_string());
                (Ok(()), state)
            },
        )
        .await?;

    let message = nested_error.lock().unwrap().clone().unwrap_or_default();
    assert!(
        message.contains("PinnedRong::call_blocking") || message.contains("Rong worker thread"),
        "unexpected reentrant error: {message}"
    );

    Ok(())
}

#[tokio::test]
async fn test_invoke_queue_makes_progress_under_burst_enqueue() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let observed = Arc::new(AtomicUsize::new(0));
    let observed_clone = observed.clone();

    rong.call(|runtime, _receiver| async move {
        let ctx = runtime.context();
        let script = r#"(() => {
            globalThis.__burst_counter = 0;
            return function () {
                globalThis.__burst_counter += 1;
                return globalThis.__burst_counter;
            };
        })()"#;
        let js_fn: JSFunc = ctx.eval(Source::from_bytes(script))?;

        for _ in 0..256 {
            enqueue_js_invoke(
                &ctx,
                js_fn.clone(),
                None,
                None,
                JsInvokePriority::Normal,
                None,
                false,
            )
            .await?;
        }

        tokio::time::timeout(
            Duration::from_secs(1),
            enqueue_js_invoke(
                &ctx,
                js_fn.clone(),
                None,
                None,
                JsInvokePriority::High,
                None,
                true,
            ),
        )
        .await
        .expect("high-priority invoke timed out")?;

        let value: i32 = ctx.global().get("__burst_counter")?;
        observed_clone.store(value as usize, Ordering::SeqCst);
        Ok(())
    })
    .await?;

    assert!(
        observed.load(Ordering::SeqCst) > 0,
        "invoke queue failed to make progress during burst enqueue"
    );

    Ok(())
}

#[tokio::test]
async fn test_invoke_queue_does_not_block_on_pending_js_promise() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;
    let started = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(AtomicUsize::new(0));
    let started_clone = started.clone();
    let observed_clone = observed.clone();

    rong.call(|runtime, _receiver| async move {
        let ctx = runtime.context();

        ctx.global().set(
            "sleepMs",
            JSFunc::new(&ctx, |ms: i32| async move {
                tokio::time::sleep(Duration::from_millis(ms as u64)).await;
            })?,
        )?;

        let slow: JSFunc = ctx.eval(Source::from_bytes(
            r#"(async function () {
                globalThis.__invoke_started = 1;
                await sleepMs(200);
                globalThis.__invoke_finished = 1;
            })"#,
        ))?;

        let fast = JSFunc::new(&ctx, move || {
            observed_clone.store(1, Ordering::SeqCst);
        })?;

        enqueue_js_invoke(
            &ctx,
            slow,
            None,
            None,
            JsInvokePriority::Normal,
            None,
            false,
        )
        .await?;

        tokio::time::timeout(Duration::from_millis(100), async {
            while started_clone.load(Ordering::SeqCst) == 0 {
                if let Ok(started_flag) = ctx.global().get::<_, i32>("__invoke_started")
                    && started_flag == 1
                {
                    started.store(1, Ordering::SeqCst);
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("slow invoke did not start");

        tokio::time::timeout(
            Duration::from_millis(50),
            enqueue_js_invoke(&ctx, fast, None, None, JsInvokePriority::High, None, true),
        )
        .await
        .expect("high-priority invoke timed out")?;

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(finished_flag) = ctx.global().get::<_, i32>("__invoke_finished")
                    && finished_flag == 1
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("slow invoke did not finish");

        Ok(())
    })
    .await?;

    assert_eq!(observed.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn test_invoke_queue_event_async_handler_keeps_last_wins_ordering() -> JSResult<()> {
    let rong = Rong::<RongJS>::builder().shared().build()?;

    rong.call(|runtime, _receiver| async move {
        let ctx = runtime.context();

        ctx.global().set(
            "sleepMs",
            JSFunc::new(&ctx, |ms: i32| async move {
                tokio::time::sleep(Duration::from_millis(ms as u64)).await;
            })?,
        )?;

        ctx.eval::<()>(Source::from_bytes(r#"globalThis.__event_value = "unset";"#))?;

        let handler: JSFunc = ctx.eval(Source::from_bytes(
            r#"(async function (event) {
                await sleepMs(event.delay);
                globalThis.__event_value = event.label;
            })"#,
        ))?;

        let old_event = JSObject::new(&ctx);
        old_event.set("label", "old")?;
        old_event.set("delay", 75)?;

        let new_event = JSObject::new(&ctx);
        new_event.set("label", "new")?;
        new_event.set("delay", 0)?;

        enqueue_js_invoke(
            &ctx,
            handler.clone(),
            None,
            Some(old_event),
            JsInvokePriority::Event,
            Some("view:update".to_string()),
            false,
        )
        .await?;

        enqueue_js_invoke(
            &ctx,
            handler,
            None,
            Some(new_event),
            JsInvokePriority::Event,
            Some("view:update".to_string()),
            true,
        )
        .await?;

        tokio::time::sleep(Duration::from_millis(120)).await;

        let final_value: String = ctx.global().get("__event_value")?;
        assert_eq!(final_value, "new");

        Ok(())
    })
    .await?;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pinned_rong_reuses_worker_state_for_the_same_key() -> JSResult<()> {
    let workers = Rong::<RongJS>::builder()
        .pinned::<String, usize>()
        .workers(2)
        .task_queue_capacity(4)
        .build()?;

    let first = workers
        .spawn("alpha".to_owned(), |_, _, state, _| async move {
            let next = state.unwrap_or(0) + 1;
            (Ok(next), Some(next))
        })
        .await?;
    let first_worker = first.worker_id();
    assert_eq!(first.join().await?, 1);

    let second = workers
        .spawn("alpha".to_owned(), |_, _, state, _| async move {
            let next = state.unwrap_or(0) + 1;
            (Ok(next), Some(next))
        })
        .await?;
    assert_eq!(second.worker_id(), first_worker);
    assert_eq!(second.join().await?, 2);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_pinned_rong_routes_other_keys_without_sharing_state() -> JSResult<()> {
    let workers = Rong::<RongJS>::builder()
        .pinned::<String, usize>()
        .workers(2)
        .task_queue_capacity(4)
        .build()?;

    let alpha = "alpha".to_owned();
    let alpha_worker = workers.worker_id_for_key(&alpha);
    let other = (0..64)
        .map(|index| format!("beta-{index}"))
        .find(|candidate| workers.worker_id_for_key(candidate) != alpha_worker)
        .expect("expected another key to map to a different worker");

    let alpha_task = workers
        .spawn(alpha.clone(), |_, _, state, _| async move {
            let next = state.unwrap_or(0) + 1;
            (Ok(next), Some(next))
        })
        .await?;
    assert_eq!(alpha_task.worker_id(), alpha_worker);
    assert_eq!(alpha_task.join().await?, 1);

    let beta_task = workers
        .spawn(other.clone(), |_, _, state, _| async move {
            let next = state.unwrap_or(100) + 1;
            (Ok(next), Some(next))
        })
        .await?;
    assert_ne!(beta_task.worker_id(), alpha_worker);
    assert_eq!(beta_task.join().await?, 101);

    let beta_again = workers
        .spawn(other, |_, _, state, _| async move {
            let next = state.unwrap_or(100) + 1;
            (Ok(next), Some(next))
        })
        .await?;
    assert_eq!(beta_again.join().await?, 102);

    Ok(())
}
