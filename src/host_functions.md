Rust functions callable from JavaScript, and the types of their parameters.

# A host function is shared

Script may call a host function again while a call of it is still running:
the function calls itself, or it calls a callback that uses the same API. So
a function made by [`JSFunc::new`](crate::CoreJSFunc::new), and every method,
getter and setter registered on a [`ClassSetup`](crate::ClassSetup), is an
`Fn`: it reads what it captures and never has exclusive access to it.

State that changes goes in a `Cell` or a `RefCell`, and a `RefCell` borrow
ends before the function calls into script:

```no_run
use std::cell::{Cell, RefCell};

fn register(ctx: &rong::JSContext) -> rong::JSResult<()> {
    let calls = Cell::new(0u32);
    let names = RefCell::new(Vec::<String>::new());
    let visit = rong::JSFunc::new(
        ctx,
        move |name: String, callback: rong::JSFunc| -> rong::JSResult<u32> {
            calls.set(calls.get() + 1);
            names.borrow_mut().push(name); // released at the end of the statement
            callback.call::<_, ()>(None, ())?; // may call `visit` again
            Ok(calls.get())
        },
    )?;
    ctx.global().set("visit", visit)?;
    Ok(())
}
```

A closure that changes a capture directly does not compile; the error names
the variable:

```compile_fail,E0525
fn register(ctx: &rong::JSContext) -> rong::JSResult<()> {
    let mut calls = 0u32;
    let count = rong::JSFunc::new(ctx, move || {
        calls += 1;
        calls
    })?;
    ctx.global().set("count", count)?;
    Ok(())
}
```

```no_run
fn register(ctx: &rong::JSContext) -> rong::JSResult<()> {
    let calls = std::cell::Cell::new(0u32);
    let count = rong::JSFunc::new(ctx, move || {
        calls.set(calls.get() + 1);
        calls.get()
    })?;
    ctx.global().set("count", count)?;
    Ok(())
}
```

The same holds for a function that returns a future. Its closure only builds
the future, which owns what it needs; nothing of the function is held while
the future is pending.

```compile_fail,E0525
fn register(ctx: &rong::JSContext) -> rong::JSResult<()> {
    let mut calls = 0u32;
    let count = rong::JSFunc::new(ctx, move || {
        calls += 1;
        let seen = calls;
        async move { seen }
    })?;
    ctx.global().set("count", count)?;
    Ok(())
}
```

```no_run
fn register(ctx: &rong::JSContext) -> rong::JSResult<()> {
    let calls = std::cell::Cell::new(0u32);
    let count = rong::JSFunc::new(ctx, move || {
        calls.set(calls.get() + 1);
        let seen = calls.get();
        async move { seen }
    })?;
    ctx.global().set("count", count)?;
    Ok(())
}
```

And for a method registered by hand:

```compile_fail,E0525
fn setup(class: &rong::ClassSetup<rong::JSEngineValue>) -> rong::JSResult<()> {
    let mut calls = 0u32;
    class.method("count", move || {
        calls += 1;
        calls
    })
}
```

```no_run
fn setup(class: &rong::ClassSetup<rong::JSEngineValue>) -> rong::JSResult<()> {
    let calls = std::cell::Cell::new(0u32);
    class.method("count", move || {
        calls.set(calls.get() + 1);
        calls.get()
    })
}
```

[`JSFunc::new_once`](crate::CoreJSFunc::new_once) takes an `FnOnce`. A second
call, nested in the first or after it, throws `OnceFn had been called`.
