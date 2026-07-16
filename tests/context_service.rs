use rong_test::*;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct TestContextService {
    flag: Arc<AtomicBool>,
}

impl JSContextService for TestContextService {
    fn on_shutdown(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
}

struct ReplaceableContextService {
    dropped: Arc<AtomicBool>,
}

impl Drop for ReplaceableContextService {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl JSContextService for ReplaceableContextService {}

#[test]
fn context_service_shutdown_is_called_on_drop() {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    {
        let flag = shutdown_flag.clone();
        run(|ctx| {
            let service = TestContextService { flag };
            ctx.set_service::<TestContextService>(service);
            Ok(())
        });
        // JSContext is dropped at the end of run(), which should trigger on_shutdown.
    }
    assert!(
        shutdown_flag.load(Ordering::SeqCst),
        "JSContextService::on_shutdown was not called on context drop"
    );
}

#[test]
fn replacing_context_service_must_not_invalidate_live_reference() {
    let first_dropped = Arc::new(AtomicBool::new(false));
    run(|ctx| {
        ctx.set_service(ReplaceableContextService {
            dropped: first_dropped.clone(),
        });
        let first = ctx
            .get_service::<ReplaceableContextService>()
            .expect("first service should be registered");

        ctx.set_service(ReplaceableContextService {
            dropped: Arc::new(AtomicBool::new(false)),
        });

        assert!(
            !first_dropped.load(Ordering::SeqCst),
            "set_service dropped a service while get_service still exposed a live reference"
        );
        let _keep_reference_live = first;
        Ok(())
    });
}

#[test]
fn borrowed_context_does_not_delay_owner_shutdown() {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    async_run!(|ctx: JSContext| async move {
        ctx.set_service(TestContextService {
            flag: shutdown_flag.clone(),
        });
        let borrowed = ctx.global().context();

        drop(ctx);
        tokio::task::yield_now().await;

        assert!(
            shutdown_flag.load(Ordering::SeqCst),
            "borrowed context handles must not delay owner shutdown"
        );
        drop(borrowed);
        Ok(())
    });
}

#[test]
fn owned_context_clone_keeps_lifecycle_alive() {
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    async_run!(|ctx: JSContext| async move {
        ctx.set_service(TestContextService {
            flag: shutdown_flag.clone(),
        });
        let owner = ctx.clone();

        drop(ctx);
        tokio::task::yield_now().await;
        assert!(
            !shutdown_flag.load(Ordering::SeqCst),
            "an owned context clone must preserve the context lifecycle"
        );

        drop(owner);
        tokio::task::yield_now().await;
        assert!(shutdown_flag.load(Ordering::SeqCst));
        Ok(())
    });
}
