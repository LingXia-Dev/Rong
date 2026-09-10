use rong_test::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

struct DropMarker(Rc<Cell<bool>>);

impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

#[test]
fn test_rust_promise_with_callback() {
    run(|ctx| {
        let ctx_clone = ctx.clone();
        // Register a Rust function that returns a promise
        let timeout_fn = JSFunc::new(ctx, move |millis: i32| {
            let (promise, resolve, _reject) = ctx_clone.promise().unwrap();

            // Directly resolve the promise with the input value
            resolve.call::<_, ()>(None, (millis,)).unwrap();
            promise
        })?;

        // Create a JS context to test the promise
        let js_code = r#"
                let result=10;
                rustTimeout(101).then((timeout)=>result=timeout);
                result
        "#;

        // Register the Rust function in JS context
        ctx.global().set("rustTimeout", timeout_fn)?;

        // Execute the JS code
        ctx.eval::<()>(Source::from_bytes(js_code.as_bytes()))
            .unwrap();

        // Run pending jobs until the promise resolves
        ctx.runtime().run_pending_jobs();
        let result: i32 = ctx.eval(Source::from_bytes("result")).unwrap();

        assert_eq!(result, 101);
        Ok(())
    });
}

#[test]
fn test_rust_promise_with_resolve() {
    run(|ctx| {
        let (promise, resolve, _reject) = ctx.promise().unwrap();

        // Use Rc<RefCell> for single-threaded shared mutability
        let result = std::rc::Rc::new(std::cell::RefCell::new(None));
        let result_clone = result.clone();

        let cb = JSFunc::new(ctx, move |value: String| {
            println!("Callback received value: {}", value);
            *result_clone.borrow_mut() = Some(value);
        })?;

        let then = promise.then()?;
        then.call::<_, ()>(Some(promise.into_object()), (cb,))
            .unwrap();

        // Resolve the promise
        resolve.call::<_, ()>(None, ("success!",)).unwrap();

        // Run pending jobs to trigger the callback
        ctx.runtime().run_pending_jobs();

        // Now assert the result after jobs have run
        let final_result = result.borrow().clone().expect("Callback was not called");
        assert_eq!(final_result, "success!");
        Ok(())
    });
}

#[test]
fn test_rust_future_in_js() {
    async_run!(|ctx: JSContext| async move {
        let ctx2 = ctx.clone();
        // Register a Rust async function that returns a promise
        let async_fn = JSFunc::new(&ctx, move |delay: i32| {
            let future = async move {
                tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                format!("completed after {}ms", delay)
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;

        // Register the function in JS context
        ctx.global().set("rustAsync", async_fn)?;

        // Create JS code that uses the async function
        let js_code = r#"
            let result = 'pending';
            rustAsync(50)
                .then(msg => { result = msg; })
                .catch(err => { result = err; });
            result
        "#;

        // Execute the JS code
        ctx.eval::<()>(Source::from_bytes(js_code.as_bytes()))?;

        // Initial result should be 'pending'
        let initial: String = ctx.eval(Source::from_bytes("result"))?;
        assert_eq!(initial, "pending");

        // wait rustAsync finished
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Check the final result
        let current: String = ctx.eval(Source::from_bytes("result"))?;
        assert_eq!(current, "completed after 50ms");
        Ok(())
    })
}

#[test]
fn test_context_drop_cancels_pending_rust_future() {
    async_run!(|ctx: JSContext| async move {
        let started = Rc::new(Cell::new(false));
        let dropped = Rc::new(Cell::new(false));
        let started_for_task = started.clone();
        let dropped_for_task = dropped.clone();

        let pending_fn = JSFunc::new(&ctx, move || {
            let started = started_for_task.clone();
            let dropped = dropped_for_task.clone();
            async move {
                started.set(true);
                let _marker = DropMarker(dropped);
                std::future::pending::<()>().await;
            }
        })?;
        ctx.global().set("pendingRustFuture", pending_fn)?;
        ctx.eval::<()>(Source::from_bytes("pendingRustFuture()"))?;

        tokio::task::yield_now().await;
        assert!(started.get(), "pending Rust future did not start");

        drop(ctx);
        tokio::task::yield_now().await;

        assert!(
            dropped.get(),
            "dropping a JS context must cancel its pending Rust futures"
        );
        Ok(())
    })
}

#[test]
fn test_context_shutdown_drains_pending_rust_future() {
    async_run!(|ctx: JSContext| async move {
        let dropped = Rc::new(Cell::new(false));
        let dropped_for_task = dropped.clone();

        let pending_fn = JSFunc::new(&ctx, move || {
            let dropped = dropped_for_task.clone();
            async move {
                let _marker = DropMarker(dropped);
                std::future::pending::<()>().await;
            }
        })?;
        ctx.global().set("pendingRustFuture", pending_fn)?;
        ctx.eval::<()>(Source::from_bytes("pendingRustFuture()"))?;
        tokio::task::yield_now().await;

        ctx.shutdown_tasks().await;

        assert!(
            dropped.get(),
            "context shutdown must finish canceling pending Rust futures"
        );
        Ok(())
    })
}

#[test]
fn test_rust_future_error_in_js() {
    async_run!(|ctx: JSContext| async move {
        let ctx2 = ctx.clone();
        // Register a Rust async function that returns a rejected promise
        let async_fn = JSFunc::new(&ctx, move |_: i32| {
            let future = async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                RongJSError::from(HostError::new(
                    rong::error::E_ERROR,
                    "async operation failed",
                ))
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;

        // Register the function in JS context
        ctx.global().set("rustAsyncError", async_fn)?;

        // Create JS code that uses the async function with more debug info
        let js_code = br#"
            let result = 'pending';
            let errorMessage = '';
            rustAsyncError(0)
                .then((msg) => {
                    result = 'resolved';
                })
                .catch((err) => {
                    result = 'rejected';
                    errorMessage = err.message;
                });
            result
        "#;

        // Execute the JS code
        ctx.eval::<()>(Source::from_bytes(js_code))?;

        // Initial result should be 'pending'
        let initial: String = ctx.eval(Source::from_bytes("result"))?;
        assert_eq!(initial, "pending");

        // wait rustAsyncError finished
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Check the final result
        let result: String = ctx.eval(Source::from_bytes("result"))?;
        assert_eq!(result, "rejected");

        // Verify the error message
        let error_message: String = ctx.eval(Source::from_bytes("errorMessage"))?;
        assert_eq!(error_message, "async operation failed");
        Ok(())
    })
}

#[test]
fn test_promise_into_future_resolve() {
    async_run!(|ctx: JSContext| async move {
        let set_timeout = JSFunc::new(&ctx, |callback: JSFunc, delay: u32| {
            let future = async move {
                tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                callback.call::<_, ()>(None, ()).unwrap();
            };
            spawn_local(future);
        })?;
        ctx.global().set("setTimeout", set_timeout)?;

        // Create Promise in JavaScript
        let js_code = r#"
            new Promise((resolve) => {
                setTimeout(() => {
                    resolve(42);
                }, 100);
            })
        "#;

        let promise = ctx
            .eval::<Promise>(Source::from_bytes(js_code.as_bytes()))
            .unwrap();

        let result: i32 = promise.into_future().await.unwrap();
        assert_eq!(result, 42);
        Ok(())
    })
}

#[test]
fn test_promise_into_future_reject_error() {
    async_run!(|ctx: JSContext| async move {
        let set_timeout = JSFunc::new(&ctx, |callback: JSFunc, delay: u32| {
            let future = async move {
                tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                callback.call::<_, ()>(None, ()).unwrap();
            };
            spawn_local(future);
        })?;
        ctx.global().set("setTimeout", set_timeout)?;

        let js_code = r#"
            new Promise((resolve, reject) => {
                setTimeout(() => {
                    reject(new Error("reject error"));
                }, 100);
            })
        "#;

        let promise = ctx.eval::<Promise>(Source::from_bytes(js_code.as_bytes()))?;

        let err = promise.into_future::<i32>().await.unwrap_err();
        let message = thrown_error_message(&ctx, &err)?;
        assert_eq!(message, "reject error");
        Ok(())
    })
}

#[test]
fn test_promise_into_future_reject_exception() {
    async_run!(|ctx: JSContext| async move {
        let set_timeout = JSFunc::new(&ctx, |callback: JSFunc, delay: u32| async move {
            tokio::time::sleep(Duration::from_millis(delay as u64)).await;
            let _ = callback.call::<_, ()>(None, ());
        })?;

        ctx.global().set("setTimeout", set_timeout)?;

        let js_code = r#"
            new Promise((resolve, reject) => {
                setTimeout(() => {
                    try {
                        throw new Error("timeout failure");
                    } catch (err) {
                        reject(err);
                    }
                }, 100);
            })
        "#;

        let promise = ctx.eval::<Promise>(Source::from_bytes(js_code.as_bytes()))?;

        let err = promise.into_future::<i32>().await.unwrap_err();
        let message = thrown_error_message(&ctx, &err)?;
        assert_eq!(message, "timeout failure");
        Ok(())
    })
}

#[test]
fn test_promise_into_future_reject_primitive() {
    async_run!(|ctx: JSContext| async move {
        let js_code = r#"
            Promise.reject("reason")
        "#;

        let promise = ctx.eval::<Promise>(Source::from_bytes(js_code.as_bytes()))?;
        let err = promise.into_future::<i32>().await.unwrap_err();

        let thrown = thrown_js_value(&ctx, &err)?;
        let s: String = String::from_js_value(&ctx, thrown)?;
        assert_eq!(s, "reason");
        Ok(())
    })
}

#[test]
fn test_promise_resolve_error_object() {
    async_run!(|ctx: JSContext| async move {
        let promise =
            ctx.eval::<Promise>(Source::from_bytes(br#"Promise.resolve(new Error("x"))"#))?;
        let value: JSValue = promise.into_future::<JSValue>().await?;
        assert!(value.is_error());
        assert!(!value.is_exception());

        let obj = value
            .into_object()
            .expect("Expected resolved Error to be an object");
        let message: String = obj.get("message")?;
        assert_eq!(message, "x");
        Ok(())
    })
}

#[test]
fn test_rust_promise_with_mut_state() {
    run(|ctx| {
        let ctx_clone = ctx.clone();
        let mut counter = 0;

        // Register a function that captures mutable state
        let counter_fn = JSFunc::new(ctx, move || {
            let (promise, resolve, _) = ctx_clone.promise().unwrap();
            counter += 1;
            resolve.call::<_, ()>(None, (counter,)).unwrap();
            promise
        })?;

        ctx.global().set("getCounter", counter_fn)?;

        // Call the function multiple times and store results
        let js_code = r#"
            let result1, result2;
            getCounter()
                .then(val => { result1 = val; });
            getCounter()
                .then(val => { result2 = val; });
        "#;

        ctx.eval::<()>(Source::from_bytes(js_code)).unwrap();
        ctx.runtime().run_pending_jobs();

        // Check individual results
        let result1: i32 = ctx.eval(Source::from_bytes("result1")).unwrap();
        let result2: i32 = ctx.eval(Source::from_bytes("result2")).unwrap();
        assert_eq!(result1, 1);
        assert_eq!(result2, 2);
        Ok(())
    });
}

#[test]
fn test_rust_async_with_mut_state() {
    async_run!(|ctx: JSContext| async move {
        let ctx2 = ctx.clone();
        let mut counter = 0;

        let async_fn = JSFunc::new(&ctx, move || {
            counter += 1;
            let count = counter;

            let future = async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                format!("Counter: {}", count)
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;

        ctx.global().set("asyncCounter", async_fn)?;

        let js_code = r#"
            let result1, result2;
            asyncCounter().then(val => { result1 = val; });
            asyncCounter().then(val => { result2 = val; });
        "#;

        ctx.eval::<()>(Source::from_bytes(js_code))?;

        // Wait for promises to resolve
        tokio::time::sleep(Duration::from_millis(60)).await;

        let result1: String = ctx.eval(Source::from_bytes("result1"))?;
        let result2: String = ctx.eval(Source::from_bytes("result2"))?;
        assert_eq!(result1, "Counter: 1");
        assert_eq!(result2, "Counter: 2");
        Ok(())
    })
}

/// A context that serves several logical requests in sequence keeps one JS
/// context, so work a request starts and forgets stays queued on it. Without a
/// scope that work resumes during a later request and observes *its* state; the
/// scope is what lets the embedder abandon it at the boundary instead.
#[test]
fn test_task_scope_abandons_forgotten_work() {
    async_run!(|ctx: JSContext| async move {
        let ctx2 = ctx.clone();
        let async_fn = JSFunc::new(&ctx, move |delay: i32| {
            let future = async move {
                tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                format!("completed after {}ms", delay)
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;
        ctx.global().set("rustAsync", async_fn)?;

        // Request one starts work it never awaits, and ends.
        let first = ctx.begin_task_scope();
        ctx.eval::<()>(Source::from_bytes(
            br#"
            globalThis.observed = 'none';
            rustAsync(30).then(msg => { globalThis.observed = msg; });
            "#,
        ))?;
        assert_eq!(ctx.current_task_scope(), Some(first));
        assert!(ctx.cancel_task_scope(first));
        assert_eq!(
            ctx.current_task_scope(),
            None,
            "cancelling the current scope leaves none current"
        );

        // Request two runs long enough that the abandoned work would have
        // completed, and pumps microtasks the way a host does between requests.
        let second = ctx.begin_task_scope();
        tokio::time::sleep(Duration::from_millis(60)).await;
        ctx.runtime().run_pending_jobs();
        let observed: String = ctx.eval(Source::from_bytes(b"observed"))?;
        assert_eq!(
            observed, "none",
            "abandoned work must not resume inside a later scope"
        );

        // The second scope is unaffected: its own work still resolves.
        ctx.eval::<()>(Source::from_bytes(
            br#"rustAsync(10).then(msg => { globalThis.observed = msg; });"#,
        ))?;
        tokio::time::sleep(Duration::from_millis(40)).await;
        ctx.runtime().run_pending_jobs();
        let observed: String = ctx.eval(Source::from_bytes(b"observed"))?;
        assert_eq!(observed, "completed after 10ms");

        assert!(ctx.cancel_task_scope(second));
        assert!(
            !ctx.cancel_task_scope(second),
            "a scope is cancelled exactly once"
        );
        Ok(())
    })
}

/// Work that finished before its scope was cancelled has already queued its
/// continuation, so a host that wants those to run drains microtasks first.
#[test]
fn test_task_scope_keeps_work_that_already_completed() {
    async_run!(|ctx: JSContext| async move {
        let ctx2 = ctx.clone();
        let async_fn = JSFunc::new(&ctx, move |delay: i32| {
            let future = async move {
                tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                format!("completed after {}ms", delay)
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;
        ctx.global().set("rustAsync", async_fn)?;

        let scope = ctx.begin_task_scope();
        ctx.eval::<()>(Source::from_bytes(
            br#"
            globalThis.observed = 'none';
            rustAsync(10).then(msg => { globalThis.observed = msg; });
            "#,
        ))?;
        tokio::time::sleep(Duration::from_millis(40)).await;
        ctx.runtime().run_pending_jobs();
        ctx.cancel_task_scope(scope);

        let observed: String = ctx.eval(Source::from_bytes(b"observed"))?;
        assert_eq!(observed, "completed after 10ms");
        Ok(())
    })
}

/// Without a scope nothing changes: this is the shape every existing embedder
/// already has.
#[test]
fn test_promises_outside_a_scope_are_untouched() {
    async_run!(|ctx: JSContext| async move {
        let ctx2 = ctx.clone();
        let async_fn = JSFunc::new(&ctx, move |delay: i32| {
            let future = async move {
                tokio::time::sleep(Duration::from_millis(delay as u64)).await;
                "done".to_owned()
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;
        ctx.global().set("rustAsync", async_fn)?;
        assert_eq!(ctx.current_task_scope(), None);

        let scope = ctx.begin_task_scope();
        ctx.enter_task_scope(None);
        ctx.eval::<()>(Source::from_bytes(
            br#"
            globalThis.observed = 'none';
            rustAsync(10).then(msg => { globalThis.observed = msg; });
            "#,
        ))?;
        // Cancelling a scope this promise was never created in cannot touch it.
        ctx.cancel_task_scope(scope);
        tokio::time::sleep(Duration::from_millis(40)).await;
        ctx.runtime().run_pending_jobs();
        let observed: String = ctx.eval(Source::from_bytes(b"observed"))?;
        assert_eq!(observed, "done");
        Ok(())
    })
}

/// Cancelling a scope is supposed to release what its abandoned work still
/// holds — a connection, a buffer, an in-flight request. A future parked on
/// I/O is never polled again on its own, so nothing but aborting its task can
/// drop it.
#[test]
fn test_task_scope_releases_what_abandoned_work_holds() {
    async_run!(|ctx: JSContext| async move {
        let dropped = Rc::new(Cell::new(false));
        let ctx2 = ctx.clone();
        let marker = dropped.clone();
        let async_fn = JSFunc::new(&ctx, move |_: i32| {
            let held = DropMarker(marker.clone());
            let future = async move {
                // Reach the park through one real poll first: the interesting
                // case is work already waiting on an upstream, not work that
                // was cancelled before it ever ran. Nothing can wake it after
                // that, which is what a request waiting on an upstream that
                // never answers looks like.
                tokio::task::yield_now().await;
                std::future::pending::<()>().await;
                let _keep = held;
                "never".to_owned()
            };
            Promise::from_future(&ctx2, None, future).unwrap()
        })?;
        ctx.global().set("rustAsync", async_fn)?;

        let scope = ctx.begin_task_scope();
        ctx.eval::<()>(Source::from_bytes(b"rustAsync(0).then(() => {});"))?;
        // Let the task run until it parks.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!dropped.get(), "the future is parked, not finished");

        assert!(ctx.cancel_task_scope(scope));
        // Give the runtime a turn to actually drop the abandoned task.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(
            dropped.get(),
            "cancelling the scope left the abandoned future holding its resources"
        );
        Ok(())
    })
}

/// Entering a scope that no longer exists leaves no scope current, which is
/// the one way to end up with work nothing can abandon later. Pinning it here
/// because the failure is silent: everything keeps working until a request
/// forgets something.
#[test]
fn test_entering_a_dead_scope_leaves_no_scope_current() {
    async_run!(|ctx: JSContext| async move {
        let outer = ctx.begin_task_scope();
        let inner = ctx.begin_task_scope();
        assert_eq!(ctx.current_task_scope(), Some(inner));

        // Save and restore is the shape this is for.
        let saved = ctx.enter_task_scope(Some(outer));
        assert_eq!(saved, Some(inner));
        assert_eq!(ctx.current_task_scope(), Some(outer));
        ctx.enter_task_scope(saved);
        assert_eq!(ctx.current_task_scope(), Some(inner));

        assert!(ctx.cancel_task_scope(outer));
        ctx.enter_task_scope(Some(outer));
        assert_eq!(
            ctx.current_task_scope(),
            None,
            "a cancelled scope must not become current again"
        );

        assert!(ctx.cancel_task_scope(inner));
        Ok(())
    })
}
