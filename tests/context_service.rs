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
