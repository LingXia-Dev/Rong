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
