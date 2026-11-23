//! Re-entrancy behavior when a class method calls into JavaScript and script
//! calls a method on the same instance.

use rong::JSEngineValue;
use rong::function::{JSClassRef, ThisMut};
use rong_macro::{js_class, js_method};
use rong_test::*;

#[js_class]
struct Emitter {
    count: i32,
}

#[js_class]
impl Emitter {
    #[js_method(constructor)]
    fn new() -> Self {
        Self { count: 0 }
    }

    #[js_method]
    fn count(&self) -> i32 {
        self.count
    }

    #[js_method]
    fn inc(&mut self) -> i32 {
        self.count += 1;
        self.count
    }

    #[js_method]
    fn emit_mut(&mut self, callback: JSFunc) -> JSResult<JSValue> {
        self.count += 1;
        callback.call(None, ())
    }

    #[js_method]
    fn emit_ref(&self, callback: JSFunc) -> JSResult<JSValue> {
        callback.call(None, ())
    }
}

#[js_class]
struct ScopedEmitter {
    count: i32,
}

impl JSClass<JSEngineValue> for ScopedEmitter {
    const NAME: &'static str = "ScopedEmitter";

    fn data_constructor() -> Constructor<JSEngineValue> {
        Constructor::new(|| Self { count: 0 })
    }

    fn class_setup(class: &ClassSetup<JSEngineValue>) -> JSResult<()> {
        class.method("count", |this: This<JSClassRef<Self>>| -> JSResult<i32> {
            Ok(this.borrow()?.count)
        })?;
        class.method(
            "emit",
            |this: ThisMut<Self>, callback: JSFunc| -> JSResult<JSValue> {
                {
                    let mut guard = this.borrow_mut()?;
                    guard.count += 1;
                }
                callback.call(None, ())
            },
        )?;
        Ok(())
    }
}

#[test]
fn mut_self_reentry_reports_invalid_state_with_class_name() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.register_class::<Emitter>()?;

    let mut_into_mut: String = ctx.eval(Source::from_bytes(
        r#"
        const e = new Emitter();
        let err;
        try {
            e.emit_mut(() => e.inc());
        } catch (ex) {
            err = ex.code + '|' + ex.message;
        }
        err + '|after:' + e.inc();
        "#,
    ))?;
    assert!(
        mut_into_mut.starts_with("E_INVALID_STATE|"),
        "expected E_INVALID_STATE, got: {mut_into_mut}"
    );
    assert!(
        mut_into_mut.contains("Emitter"),
        "expected class name Emitter in error, got: {mut_into_mut}"
    );
    assert!(
        mut_into_mut.contains("&mut self"),
        "expected `&mut self` mention in error, got: {mut_into_mut}"
    );
    assert!(
        mut_into_mut.ends_with("|after:2"),
        "expected instance to remain usable after error, got: {mut_into_mut}"
    );

    let mut_into_ref: String = ctx.eval(Source::from_bytes(
        r#"
        const e = new Emitter();
        let err;
        try {
            e.emit_mut(() => e.count());
        } catch (ex) {
            err = ex.code + '|' + ex.message;
        }
        err + '|after:' + e.count();
        "#,
    ))?;
    assert!(
        mut_into_ref.starts_with("E_INVALID_STATE|"),
        "expected E_INVALID_STATE, got: {mut_into_ref}"
    );
    assert!(
        mut_into_ref.contains("Emitter"),
        "expected class name Emitter in error, got: {mut_into_ref}"
    );
    assert!(
        mut_into_ref.contains("&mut self"),
        "expected `&mut self` mention in error, got: {mut_into_ref}"
    );
    assert!(
        mut_into_ref.ends_with("|after:1"),
        "expected instance to remain usable after error, got: {mut_into_ref}"
    );

    Ok(())
}

#[test]
fn ref_self_reentry_allows_ref_and_rejects_mut_with_invalid_state() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.register_class::<Emitter>()?;

    let ref_into_ref: i32 = ctx.eval(Source::from_bytes(
        r#"
        const e = new Emitter();
        e.inc();
        e.emit_ref(() => e.count());
        "#,
    ))?;
    assert_eq!(ref_into_ref, 1);

    let ref_into_mut: String = ctx.eval(Source::from_bytes(
        r#"
        const e = new Emitter();
        let err;
        try {
            e.emit_ref(() => e.inc());
        } catch (ex) {
            err = ex.code + '|' + ex.message;
        }
        err;
        "#,
    ))?;
    assert!(
        ref_into_mut.starts_with("E_INVALID_STATE|"),
        "expected E_INVALID_STATE, got: {ref_into_mut}"
    );
    assert!(
        ref_into_mut.contains("Emitter"),
        "expected class name Emitter in error, got: {ref_into_mut}"
    );
    assert!(
        ref_into_mut.contains("&self"),
        "expected `&self` mention in error, got: {ref_into_mut}"
    );

    Ok(())
}

#[test]
fn scoped_this_mut_drop_allows_reentry() -> JSResult<()> {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    ctx.register_class::<ScopedEmitter>()?;

    let count: i32 = ctx.eval(Source::from_bytes(
        r#"
        const e = new ScopedEmitter();
        e.emit(() => e.emit(() => e.count()));
        "#,
    ))?;
    assert_eq!(count, 2);

    Ok(())
}
