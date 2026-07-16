use futures::stream;
use rong_test::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
struct ShutdownFlag(Arc<AtomicBool>);

impl JSContextService for ShutdownFlag {
    fn on_shutdown(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn iterator_sync() {
    run(|ctx: &JSContext| {
        ctx.global()
            .set("print", JSFunc::new(ctx, |msg: String| println!("{}", msg)))?;

        let data = vec![1, 2, 3, 4, 5];
        let iterator = JSFunc::new(ctx, move |ctx: JSContext| data.clone().to_js_iter(&ctx));

        ctx.global().set("iterator", iterator)?;
        let result: i32 = ctx.eval(Source::from_bytes(
            r#"
            for (const n of iterator()) {
                 print(n.toString());
            }
            let sum=0;
            for (const n of iterator()) {
                 print(n.toString());
                 sum+=n;
            }
            sum
            "#,
        ))?;
        assert_eq!(result, 15);
        Ok(())
    });
}

#[test]
fn sync_iterator_does_not_keep_context_alive() {
    let shutdown = Arc::new(AtomicBool::new(false));
    async_run!(|ctx: JSContext| async move {
        ctx.set_service(ShutdownFlag(shutdown.clone()));
        let iterator = vec![1, 2, 3].to_js_iter(&ctx)?;
        ctx.global().set("heldIterator", iterator)?;

        drop(ctx);
        tokio::task::yield_now().await;

        assert!(
            shutdown.load(Ordering::SeqCst),
            "a synchronous iterator must not own its JS context"
        );
        Ok(())
    });
}

#[test]
fn iterator_async() {
    async_run!(async |ctx: JSContext| {
        ctx.global().set(
            "print",
            JSFunc::new(&ctx, |msg: String| println!("{}", msg)),
        )?;

        let data = stream::iter(1..=5);
        let iterator = JSFunc::new(&ctx, move |ctx: JSContext| {
            data.clone().to_js_async_iter(&ctx)
        })?;

        ctx.global().set("iterator", iterator)?;
        let result: i32 = ctx
            .eval_async(Source::from_bytes(
                r#"
            print(typeof iterator()[Symbol.asyncIterator]);
            (async function () {
                for await (const n of iterator()) {
                   print(n.toString());
                }
                let sum=0;
                for await (const n of iterator()) {
                    print(n.toString());
                    sum+=n;
                }
                return sum;
            })()
            "#,
            ))
            .await?;
        assert_eq!(result, 15);
        Ok(())
    });
}

#[test]
fn iterator_async_rejects_with_error_object() {
    async_run!(async |ctx: JSContext| {
        let bad_iterator = JSFunc::new(&ctx, move |ctx: JSContext| {
            stream::iter(vec![Err::<i32, _>(rong::RongJSError::from(
                rong::HostError::new(rong::error::E_INTERNAL, "boom"),
            ))])
            .to_js_async_iter(&ctx)
        })?;

        ctx.global().set("badIterator", bad_iterator)?;
        let result: bool = ctx
            .eval_async(Source::from_bytes(
                r#"
                (async function () {
                    try {
                        await badIterator().next();
                        return false;
                    } catch (e) {
                        return (
                            typeof e === "object" &&
                            e !== null &&
                            e.message === "boom" &&
                            e.code === "E_INTERNAL"
                        );
                    }
                })()
                "#,
            ))
            .await?;
        assert!(result);
        Ok(())
    });
}

#[test]
fn pending_async_iterator_does_not_keep_context_alive() {
    let shutdown = Arc::new(AtomicBool::new(false));
    async_run!(|ctx: JSContext| async move {
        ctx.set_service(ShutdownFlag(shutdown.clone()));

        let iterator = JSFunc::new(&ctx, move |ctx: JSContext| {
            stream::pending::<i32>().to_js_async_iter(&ctx)
        })?;
        ctx.global().set("pendingIterator", iterator)?;
        ctx.eval::<()>(Source::from_bytes(
            r#"
            const pending = pendingIterator();
            pending.next();
            pending.return();
            "#,
        ))?;
        tokio::task::yield_now().await;

        drop(ctx);
        tokio::task::yield_now().await;

        assert!(
            shutdown.load(Ordering::SeqCst),
            "pending async iterator tasks must not keep their JS context alive"
        );
        Ok(())
    });
}
