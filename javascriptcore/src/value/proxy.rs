use crate::{JSCContext, JSCValue};
use rong_core::{
    FromJSValue, JSContext, JSContextImpl, JSFunc, JSObject, JSProxyOps, JSTypeOf, JSValue,
    JSValueImpl, RongJSError,
};

#[derive(Clone)]
struct ProxyHelper(JSCValue);

const PROXY_HELPER_SOURCE: &[u8] = br#"
(() => {
    if (globalThis.__rongProxyHelper__ !== undefined) {
        return globalThis.__rongProxyHelper__;
    }

    const NativeProxy = globalThis.Proxy;
    const meta = new WeakMap();
    // Bound now, so a script replacing `WeakMap.prototype.has` (or `get`,
    // `set`, `Function.prototype.call`) later is never called from here.
    const call = Function.prototype.call;
    const weakHas = call.bind(WeakMap.prototype.has);
    const weakGet = call.bind(WeakMap.prototype.get);
    const weakSet = call.bind(WeakMap.prototype.set);

    function track(proxy, target, handler) {
        weakSet(meta, proxy, { target, handler });
        return proxy;
    }

    function TrackedProxy(target, handler) {
        if (new.target === undefined) {
            throw new TypeError("Constructor Proxy requires 'new'");
        }
        return track(new NativeProxy(target, handler), target, handler);
    }

    // Not `NativeProxy`'s heir: `Object.getPrototypeOf(Proxy)` would hand out
    // the constructor that makes untracked proxies.
    TrackedProxy.prototype = NativeProxy.prototype;
    TrackedProxy.revocable = function revocable(target, handler) {
        const revocable = NativeProxy.revocable(target, handler);
        track(revocable.proxy, target, handler);
        return revocable;
    };
    globalThis.Proxy = TrackedProxy;

    const helper = {
        create(target, handler) {
            return track(new NativeProxy(target, handler), target, handler);
        },
        isProxy(value) {
            return value !== null &&
                (typeof value === "object" || typeof value === "function") &&
                weakHas(meta, value);
        },
        target(value) {
            if (value === null || (typeof value !== "object" && typeof value !== "function") || !weakHas(meta, value)) {
                throw new TypeError("Not JS Proxy");
            }
            return weakGet(meta, value).target;
        }
    };

    Object.freeze(helper);
    Object.defineProperty(globalThis, "__rongProxyHelper__", {
        value: helper,
        configurable: false,
        enumerable: false,
        writable: false,
    });

    return helper;
})()
"#;

pub(crate) fn prime_proxy_helper(ctx: &JSCContext) -> JSCValue {
    ctx.eval_direct(PROXY_HELPER_SOURCE)
}

pub(crate) fn install_proxy_helper(ctx: &JSCContext) -> JSCValue {
    let host_ctx = JSContext::<JSCContext>::from_borrowed_raw_ptr(ctx.as_raw());
    if let Some(helper) = host_ctx.get_state::<ProxyHelper>() {
        return helper.0.clone();
    }

    let helper = prime_proxy_helper(ctx);
    if !helper.is_exception() {
        host_ctx.set_state(ProxyHelper(helper.clone()));
    }
    helper
}

fn get_proxy_helper(ctx: &JSCContext) -> JSCValue {
    install_proxy_helper(ctx)
}

fn thrown_or_undefined(ctx: &JSContext<JSCContext>, err: RongJSError) -> JSCValue {
    err.thrown_value(ctx)
        .unwrap_or_else(|| JSValue::undefined(ctx))
        .into_value()
}

fn helper_method(ctx: &JSCContext, name: &str) -> Result<JSFunc<JSCValue>, JSCValue> {
    let host_ctx = JSContext::<JSCContext>::from_borrowed_raw_ptr(ctx.as_raw());
    let helper = get_proxy_helper(ctx);
    if helper.is_exception() {
        return Err(helper);
    }

    let helper_obj = JSObject::from_js_value(&host_ctx, JSValue::from_raw(&host_ctx, helper))
        .map_err(|err: RongJSError| thrown_or_undefined(&host_ctx, err))?;

    helper_obj
        .get::<_, JSFunc<JSCValue>>(name)
        .map_err(|err: RongJSError| thrown_or_undefined(&host_ctx, err))
}

pub(crate) fn is_proxy(value: &JSCValue) -> bool {
    let ctx = JSCContext::from_borrowed_raw(*value.as_raw_context());
    let Ok(is_proxy) = helper_method(&ctx, "isProxy") else {
        return false;
    };
    // Called directly: a host call into script would run the pending promise
    // jobs afterwards, and asking whether a value is a Proxy must run nothing.
    let result = ctx.call(
        is_proxy.as_value(),
        JSCValue::create_undefined(&ctx),
        std::slice::from_ref(value),
    );
    !result.is_exception() && result.try_into().unwrap_or(false)
}

impl JSProxyOps for JSCValue {
    fn new_proxy(ctx: &Self::Context, target: Self, handler: Self) -> Result<Self, Self> {
        let create = helper_method(ctx, "create")?;
        let host_ctx = JSContext::<JSCContext>::from_borrowed_raw_ptr(ctx.as_raw());
        create
            .call::<_, JSValue<JSCValue>>(
                None,
                (
                    JSValue::from_raw(&host_ctx, target),
                    JSValue::from_raw(&host_ctx, handler),
                ),
            )
            .map(JSValue::into_value)
            .map_err(|err: RongJSError| thrown_or_undefined(&host_ctx, err))
    }

    fn proxy_target(&self) -> Result<Self, Self> {
        let ctx = JSCContext::from_borrowed_raw(*self.as_raw_context());
        let target = helper_method(&ctx, "target")?;
        let host_ctx = JSContext::<JSCContext>::from_borrowed_raw_ptr(ctx.as_raw());
        target
            .call::<_, JSValue<JSCValue>>(None, (JSValue::from_raw(&host_ctx, self.clone()),))
            .map(JSValue::into_value)
            .map_err(|err: RongJSError| thrown_or_undefined(&host_ctx, err))
    }
}
