use rong_test::*;
use std::cell::Cell;
use std::rc::Rc;

// Observe every continuation path, including recovery chains and async/await.
// Catch the promise returned by finally so rejection tests cannot introduce
// unrelated unhandled rejections.
const OBSERVE: &str = r#"
    globalThis.events = [];
    globalThis.observe = promise => {
        promise.then(
            value => events.push(['then', value]),
            error => events.push(['reject', String(error)])
        ).then(() => events.push(['chain']));
        promise.catch(error => events.push(['catch', String(error)]));
        promise.finally(() => events.push(['finally'])).catch(() => {});
        (async () => {
            try {
                const value = await promise;
                events.push(['await', value]);
            } catch (error) {
                events.push(['await-catch', String(error)]);
            } finally {
                events.push(['await-finally']);
            }
        })();
    };
"#;

struct Dropped(Rc<Cell<bool>>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

fn cancellation_during_poll<const REJECT: bool, const PENDING: bool>() {
    async_run!(|ctx: JSContext| async move {
        ctx.eval::<()>(Source::from_bytes(OBSERVE))?;
        let scope = ctx.begin_task_scope();
        let cancel_ctx = ctx.clone();
        ctx.global().set(
            "cancelScope",
            JSFunc::new(&ctx, move || cancel_ctx.cancel_task_scope(scope))?,
        )?;

        let dropped = Rc::new(Cell::new(false));
        let held = Dropped(dropped.clone());
        let future_ctx = ctx.clone();
        let (polled_tx, polled_rx) = tokio::sync::oneshot::channel();
        let promise = Promise::from_future(&ctx, None, async move {
            let _held = held;
            // Cancellation reenters from JS while the task is being polled.
            future_ctx.eval::<()>(Source::from_bytes(
                "if (!cancelScope()) throw new Error('scope was not cancelled');",
            ))?;
            polled_tx.send(()).unwrap();
            if PENDING {
                std::future::pending::<()>().await;
            }
            if REJECT {
                Err(HostError::new("E_TEST", "cancelled failure").into())
            } else {
                Ok::<i32, RongJSError>(42)
            }
        })?;
        ctx.global().set("cancelledPromise", promise)?;
        ctx.eval::<()>(Source::from_bytes("observe(cancelledPromise)"))?;

        // Synchronize on the actual poll, with no timing-dependent sleeps.
        polled_rx.await.unwrap();
        ctx.runtime().run_pending_jobs();
        assert!(dropped.get(), "cancelled future kept its resources");
        assert_eq!(ctx.current_task_scope(), None);

        // Pump real host work in the next request. Every continuation attached
        // to the cancelled promise must remain silent in that request too.
        let next = ctx.begin_task_scope();
        let healthy = Promise::from_future(&ctx, None, async { 7i32 })?;
        ctx.global().set("healthyPromise", healthy.clone())?;
        ctx.eval::<()>(Source::from_bytes(
            "globalThis.healthyValue = 0; healthyPromise.then(v => { healthyValue = v; });",
        ))?;
        assert_eq!(healthy.into_future::<i32>().await?, 7);
        ctx.runtime().run_pending_jobs();
        ctx.eval::<()>(Source::from_bytes(
            r#"
            if (healthyValue !== 7) throw new Error('next request did not complete');
            if (events.length !== 0) {
                throw new Error('cancelled continuations ran: ' + JSON.stringify(events));
            }
            "#,
        ))?;
        assert!(ctx.cancel_task_scope(next));
        Ok(())
    });
}

#[test]
fn cancellation_during_successful_poll_suppresses_js_continuations() {
    cancellation_during_poll::<false, false>();
}

#[test]
fn cancellation_during_failed_poll_suppresses_js_continuations() {
    cancellation_during_poll::<true, false>();
}

#[test]
fn cancellation_during_pending_poll_drops_future() {
    cancellation_during_poll::<false, true>();
}

#[test]
fn foreign_scope_cannot_enter_or_cancel_another_context() {
    async_run!(|ctx: JSContext| async move {
        let other = ctx.runtime().context();
        ctx.eval::<()>(Source::from_bytes(OBSERVE))?;
        other.eval::<()>(Source::from_bytes(OBSERVE))?;
        let own = ctx.begin_task_scope();
        let foreign = other.begin_task_scope();
        assert_ne!(own, foreign);

        let (own_tx, own_rx) = tokio::sync::oneshot::channel();
        let (other_tx, other_rx) = tokio::sync::oneshot::channel();
        let own_promise = Promise::from_future(&ctx, None, async { own_rx.await.unwrap() })?;
        let other_promise = Promise::from_future(&other, None, async { other_rx.await.unwrap() })?;
        ctx.global().set("hostPromise", own_promise)?;
        other.global().set("hostPromise", other_promise.clone())?;
        ctx.eval::<()>(Source::from_bytes("observe(hostPromise)"))?;
        other.eval::<()>(Source::from_bytes("observe(hostPromise)"))?;

        assert!(!ctx.cancel_task_scope(foreign));
        assert!(!other.cancel_task_scope(own));
        assert_eq!(ctx.current_task_scope(), Some(own));
        assert_eq!(other.current_task_scope(), Some(foreign));
        assert_eq!(ctx.enter_task_scope(Some(foreign)), Some(own));
        assert_eq!(ctx.current_task_scope(), None);
        assert_eq!(other.enter_task_scope(Some(own)), Some(foreign));
        assert_eq!(other.current_task_scope(), None);
        ctx.enter_task_scope(Some(own));
        other.enter_task_scope(Some(foreign));

        assert!(ctx.cancel_task_scope(own));
        let _ = own_tx.send(99i32);
        other_tx.send(7i32).unwrap();
        assert_eq!(other_promise.into_future::<i32>().await?, 7);
        ctx.runtime().run_pending_jobs();
        ctx.eval::<()>(Source::from_bytes(
            "if (events.length) throw new Error('cancelled context ran continuations');",
        ))?;
        other.eval::<()>(Source::from_bytes(
            r#"
            const expected = ['await', 'await-finally', 'chain', 'finally', 'then'];
            const actual = events.map(event => event[0]).sort();
            if (JSON.stringify(actual) !== JSON.stringify(expected)) {
                throw new Error('live context lost continuations: ' + JSON.stringify(events));
            }
            for (const [kind, value] of events) {
                if ((kind === 'then' || kind === 'await') && value !== 7) {
                    throw new Error('wrong context value: ' + value);
                }
            }
            "#,
        ))?;
        assert!(other.cancel_task_scope(foreign));
        assert!(!ctx.cancel_task_scope(own));
        Ok(())
    });
}

#[test]
fn scope_ids_are_not_reused_across_runtimes() {
    let first = {
        let runtime = RongJS::runtime();
        let ctx = runtime.context();
        ctx.begin_task_scope()
    };
    let runtime = RongJS::runtime();
    let ctx = runtime.context();
    let second = ctx.begin_task_scope();
    assert_ne!(first, second);
    assert!(!ctx.cancel_task_scope(first));
    assert_eq!(ctx.current_task_scope(), Some(second));
    assert!(ctx.cancel_task_scope(second));
}
