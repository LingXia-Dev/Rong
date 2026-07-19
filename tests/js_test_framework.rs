use rong_test::*;

const CONFORMANCE_JS: &str = include_str!("unit/test-framework-conformance.js");

#[test]
fn js_test_framework_conformance() {
    async_run!(|ctx: JSContext| async move {
        let runner = UnitJSRunner::load_source(&ctx, CONFORMANCE_JS).await?;
        assert!(runner.run().await?);
        Ok(())
    });
}
