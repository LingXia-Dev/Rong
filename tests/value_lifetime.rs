use rong_test::*;

#[test]
fn dropping_js_value_after_context_and_runtime_is_safe() {
    let value = {
        let runtime = RongJS::runtime();
        let context = runtime.context();

        // Clone the returned value so engines with explicit persistent handles
        // must release one after both of its original owners have been dropped.
        context
            .eval::<JSValue>(Source::from_bytes("({ retained: true })"))
            .expect("create retained JavaScript value")
            .clone()
    };

    // Encourage native context storage to be recycled. Calling Drop through a
    // stale context pointer can otherwise appear to work by accident.
    for _ in 0..32 {
        let runtime = RongJS::runtime();
        let context = runtime.context();
        context
            .eval::<()>(Source::from_bytes("({ replacement: true })"))
            .expect("exercise replacement context");
    }

    drop(value);
}
