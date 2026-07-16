use rong_test::*;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static RUNTIME_SERVICE_SHUTDOWNS: AtomicUsize = AtomicUsize::new(0);

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

#[derive(Default)]
struct CountingRuntimeService;

impl JSRuntimeService for CountingRuntimeService {
    fn on_shutdown(&self) {
        RUNTIME_SERVICE_SHUTDOWNS.fetch_add(1, Ordering::SeqCst);
    }
}

struct ServiceStateCollision;

impl JSContextService for ServiceStateCollision {}

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
fn runtime_service_shutdown_is_called_once() {
    RUNTIME_SERVICE_SHUTDOWNS.store(0, Ordering::SeqCst);
    {
        let runtime = RongJS::runtime();
        let _ = runtime.get_or_init_service::<CountingRuntimeService>();
    }
    assert_eq!(RUNTIME_SERVICE_SHUTDOWNS.load(Ordering::SeqCst), 1);
}

#[test]
fn context_services_and_plain_state_have_distinct_type_namespaces() {
    run(|ctx| {
        ctx.set_state(ServiceStateCollision);
        assert!(ctx.get_service::<ServiceStateCollision>().is_none());

        ctx.set_service(ServiceStateCollision);
        assert!(ctx.get_state::<ServiceStateCollision>().is_some());
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
