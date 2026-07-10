use rong_macro::FromJSObject;
use rong_test::function::JSParameterType;
use rong_test::*;
use tokio::time::{Duration, sleep};

#[derive(FromJSObject)]
struct NestedQueryOptions {
    query: Option<JSObject>,
}

#[test]
fn function_with_optional() {
    run(|ctx| {
        let func = JSFunc::new(ctx, |a: i32, b: Optional<i32>| match *b {
            Some(val) => a + val,
            None => a,
        })?
        .name("add_optional")?;
        ctx.global().set("add_optional", func)?;

        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_optional(7)"))
                .unwrap(),
            7
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_optional(7, 3)"))
                .unwrap(),
            10
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_optional.length"))
                .unwrap(),
            1
        );
        Ok(())
    });
}

#[test]
fn function_with_rest() {
    run(|ctx| {
        let func = JSFunc::new(ctx, |init: i32, rest: Rest<i32>| {
            let sum: i32 = rest.iter().sum();
            init + sum
        })?
        .name("add")?;
        ctx.global().set("add_rest", func)?;

        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_rest(1)")).unwrap(),
            1
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_rest(1, 2)"))
                .unwrap(),
            3
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_rest(1, 2, 3, 4)"))
                .unwrap(),
            10
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add_rest.length"))
                .unwrap(),
            1
        );
        Ok(())
    });
}

#[test]
fn function_with_optional_and_rest() {
    run(|ctx| {
        let func = JSFunc::new(ctx, |a: i32, b: Optional<i32>, rest: Rest<i32>| {
            let base = match *b {
                Some(val) => a + val,
                None => a,
            };
            let sum: i32 = rest.iter().sum();
            base + sum
        })?
        .name("complex_add")?;
        ctx.global().set("complex_add", func)?;

        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"complex_add(1)"))
                .unwrap(),
            1
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"complex_add(1, 2)"))
                .unwrap(),
            3
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"complex_add(1, 2, 3)"))
                .unwrap(),
            6
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"complex_add(1, 2, 3, 4)"))
                .unwrap(),
            10
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"complex_add.length"))
                .unwrap(),
            1
        );
        Ok(())
    });
}

#[test]
fn test_jsfunc_call() {
    run(|ctx| {
        // Test 1: Rust-created JS function
        let rust_func = JSFunc::new(ctx, |a: i32, b: i32| a + b)?;
        let result: i32 = rust_func.call(None, (2, 3)).unwrap();
        assert_eq!(result, 5);

        // Test 2: JavaScript-created function
        let js_func: JSFunc = ctx
            .eval(Source::from_bytes(b"(function(a, b) { return a * b; })"))
            .unwrap();
        let result: i32 = js_func.call(None, (4, 5)).unwrap();
        assert_eq!(result, 20);

        // Test 3: error. Rust clousre set the lenght of function.
        let result: Result<i32, _> = rust_func.call(None, ());
        assert!(result.is_err());
        Ok(())
    });
}

#[test]
fn test_jsfunc_call_macro() {
    run(|ctx| {
        // Test 1: 2 arguments
        let rust_func = JSFunc::new(ctx, |a: i32, b: i32| a + b)?;
        let result: i32 = rust_func.call(None, (2, 3)).unwrap();
        assert_eq!(result, 5);

        // Test 2: 0 argument
        let rust_func = JSFunc::new(ctx, || 8)?;
        let result: i32 = rust_func.call(None, ()).unwrap();
        assert_eq!(result, 8);
        Ok(())
    });
}

#[test]
fn test_jsfunc_as_argument() {
    run(|ctx| {
        // Register a function that takes a JS function as argument
        let func = JSFunc::new(ctx, |callback: JSFunc| {
            // Call the JS function with some arguments
            let result: i32 = callback.call(None, (2, 3)).unwrap();
            result * 2
        })?
        .name("call_and_double")?;

        ctx.global().set("call_and_double", func)?;

        // Test with a simple addition function
        let result: i32 = ctx
            .eval(Source::from_bytes(
                b"call_and_double(function(a, b) { return a + b; })",
            ))
            .unwrap();
        assert_eq!(result, 10); // (2 + 3) * 2

        // Test with a multiplication function
        let result: i32 = ctx
            .eval(Source::from_bytes(
                b"call_and_double(function(a, b) { return a * b; })",
            ))
            .unwrap();
        assert_eq!(result, 12); // (2 * 3) * 2
        Ok(())
    });
}

#[test]
fn test_async_rust_fn_resolve() {
    async_run!(|ctx: JSContext| async move {
        let async_func = JSFunc::new(&ctx, |a: i32, b: i32| async move {
            sleep(Duration::from_millis(100)).await;
            a + b
        })?;
        ctx.global().set("add", async_func)?;

        let result: i32 = ctx
            .eval::<Promise>(Source::from_bytes(
                b"add(2,6).then(result=>{return result;})",
            ))?
            .into_future()
            .await?;

        assert_eq!(result, 8);
        Ok(())
    });
}

#[test]
fn test_async_rust_fn_reject() {
    async_run!(|ctx: JSContext| async move {
        let async_func = JSFunc::new(&ctx, |_a: i32, _b: i32| async move {
            sleep(Duration::from_millis(100)).await;
            RongJSError::from(HostError::new(
                rong::error::E_ERROR,
                "Failed to perform add",
            ))
        })?; // PromiseResolver help call reject to propagate error to JS catch
        ctx.global().set("add", async_func)?;

        // catch trigger rust resolver callback
        let result = ctx
            .eval::<Promise>(Source::from_bytes(
                br#"add(2,6)
                .then((resolve) => {return resolve;})
                .catch(err =>{ throw new Error(err+"!");})
                "#,
            ))?
            .into_future::<i32>()
            .await;

        let err = result.unwrap_err();
        let message = thrown_error_message(&ctx, &err)?;
        assert!(message.contains("Failed to perform add!"));
        Ok(())
    });
}

#[test]
fn test_async_rust_fn_preserves_nested_jsobject_after_await() {
    async_run!(|ctx: JSContext| async move {
        let stringify_query = JSFunc::new(&ctx, |options: NestedQueryOptions| async move {
            let query = options.query.ok_or_else(|| {
                HostError::new(rong::error::E_INVALID_ARG, "missing query").with_name("TypeError")
            })?;

            sleep(Duration::from_millis(50)).await;
            query.to_json_string()
        })?;
        ctx.global()
            .set("stringifyQueryAfterAwait", stringify_query)?;

        let result: String = ctx
            .eval::<Promise>(Source::from_bytes(
                br#"
                stringifyQueryAfterAwait({
                    page: "file",
                    query: { section: "openFile" }
                })
                "#,
            ))?
            .into_future()
            .await?;

        assert_eq!(result, r#"{"section":"openFile"}"#);
        Ok(())
    });
}

#[test]
fn test_new_once() {
    run(|ctx| {
        // Create a function that can only be called once
        let func = JSFunc::new_once(ctx, |x: i32| x + 1)?;
        ctx.global().set("once", func)?;

        // catch trigger rust resolver callback
        let result = ctx.eval::<i32>(Source::from_bytes("once(2); once(3)"));

        let err = result.unwrap_err();
        let message = thrown_error_message(ctx, &err)?;
        assert!(message.contains("OnceFn had been called"));

        Ok(())
    });
}

#[test]
fn test_new_once_async() {
    async_run!(|ctx: JSContext| async move {
        let set_timeout = JSFunc::new_once(&ctx, |callback: JSFunc, delay: u32| async move {
            tokio::time::sleep(Duration::from_millis(delay as u64)).await;
            callback.call::<_, ()>(None, ()).unwrap();
        })?;
        ctx.global().set("setTimeout", set_timeout)?;

        // Create Promise in JavaScript
        let js_code = r#"
            new Promise((resolve) => {
                setTimeout(() => {
                    resolve(42);
                }, 100);
                setTimeout(() => {
                   resolve(42);
                }, 100);

            })
        "#;

        let promise = ctx
            .eval::<Promise>(Source::from_bytes(js_code.as_bytes()))
            .unwrap();

        let result: JSResult<i32> = promise.into_future().await;
        let err = result.unwrap_err();
        let message = thrown_error_message(&ctx, &err)?;
        assert!(message.contains("OnceFn had been called"));

        tokio::time::sleep(Duration::from_millis(110)).await;
        Ok(())
    })
}

#[test]
fn test_call_async_sync_return() {
    async_run!(|ctx: JSContext| async move {
        // Test regular synchronous function with call_async
        let js_func: JSFunc = ctx
            .eval(Source::from_bytes(b"(function(a, b) { return a + b; })"))
            .unwrap();

        let result: i32 = js_func.call_async(None, (2, 3)).await?;
        assert_eq!(result, 5);

        Ok(())
    });
}

#[test]
fn test_call_async_promise_return() {
    async_run!(|ctx: JSContext| async move {
        // Create a sleep function using Rust
        let sleep_fn = JSFunc::new(&ctx, |ms: u32| async move {
            sleep(Duration::from_millis(ms as u64)).await;
        })?;
        ctx.global().set("sleep", sleep_fn)?;

        // Test async function that returns a Promise
        let js_func: JSFunc = ctx
            .eval(Source::from_bytes(
                b"(async function(a, b) {
                    await sleep(100);
                    return a * b;
                })",
            ))
            .unwrap();

        let result: i32 = js_func.call_async(None, (4, 5)).await?;
        assert_eq!(result, 20);

        Ok(())
    });
}

#[test]
fn test_call_async_with_this() {
    async_run!(|ctx: JSContext| async move {
        // Create a sleep function using Rust
        let sleep_fn = JSFunc::new(&ctx, |ms: u32| async move {
            sleep(Duration::from_millis(ms as u64)).await;
        })?;
        ctx.global().set("sleep", sleep_fn)?;

        // Create an object with a method that uses 'this'
        let obj: JSObject = ctx
            .eval(Source::from_bytes(
                b"({
                    value: 10,
                    asyncMethod: async function(x) {
                        await sleep(50);
                        return this.value + x;
                    }
                })",
            ))
            .unwrap();

        let method: JSFunc = obj.get("asyncMethod")?;

        // Call with 'this' context
        let result: i32 = method.call_async(Some(obj.clone()), (5,)).await?;
        assert_eq!(result, 15); // 10 + 5

        Ok(())
    });
}

#[test]
fn test_call_async_error_handling() {
    async_run!(|ctx: JSContext| async move {
        // Create a sleep function using Rust
        let sleep_fn = JSFunc::new(&ctx, |ms: u32| async move {
            sleep(Duration::from_millis(ms as u64)).await;
        })?;
        ctx.global().set("sleep", sleep_fn)?;

        // Function that throws an error after a delay
        let js_func: JSFunc = ctx
            .eval(Source::from_bytes(
                b"(async function() {
                    await sleep(50);
                    throw new Error('test error');
                })",
            ))
            .unwrap();

        let result: Result<i32, _> = js_func.call_async(None, ()).await;
        assert!(result.is_err());

        Ok(())
    });
}

#[test]
fn function_return_vec_is_js_array() {
    run(|ctx| {
        let func = JSFunc::new(ctx, || -> Vec<i32> { vec![1, 2, 3] })?;
        ctx.global().set("make_vec", func)?;

        let ok: bool = ctx.eval(Source::from_bytes(
            br#"
            (function () {
                const arr = make_vec();
                return Array.isArray(arr)
                    && arr.length === 3
                    && arr[0] === 1
                    && arr[1] === 2
                    && arr[2] === 3;
            })()
        "#,
        ))?;
        assert!(ok);
        Ok(())
    });
}

#[test]
fn function_param_vec_js_array() {
    run(|ctx| {
        let func = JSFunc::new(ctx, |nums: Vec<i32>| -> i32 { nums.into_iter().sum() })?;
        ctx.global().set("sum_vec", func)?;

        let result: i32 = ctx.eval(Source::from_bytes(b"sum_vec([1,2,3,4])")).unwrap();
        assert_eq!(result, 10);
        Ok(())
    });
}

#[derive(Clone)]
struct Job {
    id: i32,
}

impl IntoJSValue<JSEngineValue> for Job {
    fn into_js_value(self, ctx: &JSContext) -> JSValue {
        let obj = JSObject::new(ctx);
        obj.set("id", self.id).unwrap();
        obj.into_js_value()
    }
}

impl FromJSValue<JSEngineValue> for Job {
    fn from_js_value(ctx: &JSContext, value: JSValue) -> JSResult<Self> {
        let obj = JSObject::from_js_value(ctx, value)?;
        let id: i32 = obj.get("id")?;
        Ok(Job { id })
    }
}

impl JSParameterType for Job {}

// General free function: context + Optional<Vec<custom>>
fn sum_optional_jobs(_ctx: JSContext, items: Optional<Vec<Job>>) -> i32 {
    match items.0 {
        Some(v) => v.into_iter().map(|j| j.id).sum(),
        None => 0,
    }
}

#[test]
fn function_param_context_optional_vec() {
    run(|ctx| {
        let f = JSFunc::new(ctx, sum_optional_jobs)?;
        ctx.global().set("sum_optional_jobs", f)?;

        let r0: i32 = ctx.eval(Source::from_bytes(b"sum_optional_jobs()"))?;
        assert_eq!(r0, 0);

        let r1: i32 = ctx.eval(Source::from_bytes(b"sum_optional_jobs([{id:3},{id:4}])"))?;
        assert_eq!(r1, 7);
        Ok(())
    });
}

#[test]
fn function_param_custom_struct() {
    run(|ctx| {
        // Rust function takes custom class and optional primitive
        let f = JSFunc::new(ctx, |j: Job, opt: Optional<i32>| -> i32 {
            j.id + (*opt).unwrap_or_default()
        })?;
        ctx.global().set("use_job", f)?;

        let r0: i32 = ctx.eval(Source::from_bytes(b"use_job({id:7})"))?;
        assert_eq!(r0, 7);
        let r1: i32 = ctx.eval(Source::from_bytes(b"use_job({id:7}, 5)"))?;
        assert_eq!(r1, 12);
        Ok(())
    });
}

#[test]
fn function_param_vec_of_custom_struct() {
    run(|ctx| {
        let sum = JSFunc::new(ctx, |items: Vec<Job>| -> i32 {
            items.into_iter().map(|j| j.id).sum()
        })?;
        ctx.global().set("sum_jobs", sum)?;

        let res: i32 = ctx.eval(Source::from_bytes(b"sum_jobs([{id:1}, {id:2}, {id:3}])"))?;
        assert_eq!(res, 6);
        Ok(())
    });
}

#[test]
fn host_function_inherits_function_prototype() {
    run(|ctx| {
        let add = JSFunc::new(ctx, |a: i32, b: i32| a + b)?;
        ctx.global().set("add_host", add)?;

        let t_call: String = ctx.eval(Source::from_bytes(b"typeof add_host.call"))?;
        let t_apply: String = ctx.eval(Source::from_bytes(b"typeof add_host.apply"))?;
        let t_bind: String = ctx.eval(Source::from_bytes(b"typeof add_host.bind"))?;
        assert_eq!(t_call, "function");
        assert_eq!(t_apply, "function");
        assert_eq!(t_bind, "function");

        let r1: i32 = ctx.eval(Source::from_bytes(b"add_host.call(null, 1, 2)"))?;
        assert_eq!(r1, 3);
        let r2: i32 = ctx.eval(Source::from_bytes(b"add_host.apply(null, [10, 5])"))?;
        assert_eq!(r2, 15);
        let r3: i32 = ctx.eval(Source::from_bytes(b"add_host.bind(null, 7)(8)"))?;
        assert_eq!(r3, 15);
        Ok(())
    });
}

#[test]
fn host_async_function_call_apply() {
    async_run!(|ctx: JSContext| async move {
        let fa = JSFunc::new(&ctx, |a: i32| async move { a * 2 })?;
        ctx.global().set("fa_host", fa)?;

        let out1: i32 = ctx
            .eval_async(Source::from_bytes(b"fa_host.call(null, 6)"))
            .await?;
        assert_eq!(out1, 12);

        let out2: i32 = ctx
            .eval_async(Source::from_bytes(b"fa_host.apply(null, [9])"))
            .await?;
        assert_eq!(out2, 18);
        Ok(())
    });
}
