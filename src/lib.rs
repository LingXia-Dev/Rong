pub use rong_core::err_data;
pub use rong_core::{
    AnyJSTypedArray as CoreAnyJSTypedArray, Class as CoreClass, ClassSetup, FromJSValue, HostError,
    InterruptGuard, InterruptHandle, InterruptMode, IntoJSAsyncIteratorExt, IntoJSIteratorExt,
    IntoJSValue, JSArray as CoreJSArray, JSArrayBuffer as CoreJSArrayBuffer, JSAsyncIterator,
    JSBytes as CoreJSBytes, JSClass, JSContext as CoreJSContext, JSContextService,
    JSDate as CoreJSDate, JSEngine, JSException as CoreJSException, JSFunc as CoreJSFunc,
    JSIterator, JSObject as CoreJSObject, JSProxy as CoreJSProxy, JSResult,
    JSRuntime as CoreJSRuntime, JSRuntimeService, JSSymbol as CoreJSSymbol,
    JSTypedArray as CoreJSTypedArray, JSTypedArrayKind, JSValue as CoreJSValue, JSValueType,
    JsonToJSValue, Promise as CorePromise, PropertyDescriptor as CorePropertyDescriptor,
    RongJSError, Source, SourceKind, TaskScope, Uint8Clamped, error, illegal_constructor,
    install_async_iterator_symbol, install_iterator_symbol,
};
// Re-export selected runtime API from rong_core::rong so downstream crates use `rong::...`
pub use rong_core::rong::{
    Rong, RongBuildError, TaskHandle, TaskMessage, Worker, WorkerState, spawn_local,
};
pub use rong_core::{PinnedRong, PinnedSpawnError};

// Re-export user-agent helpers.
pub use rong_rt::sse;
pub use rong_rt::upload;
pub use rong_rt::{
    DEFAULT_USER_AGENT, InstallGlobalExecutorError, RongExecutor, RongExecutorBuildError,
    RongExecutorBuilder, get_user_agent, set_user_agent,
};
// Re-export selected JS invoke queue APIs (module remains internal to core)
pub use rong_core::{JsInvokePriority, enqueue_js_invoke};

pub mod function {
    pub use rong_core::function::{
        Constructor, FromParams, IntoJSCallable, IntoOnceJSCallable, JSParameterType, KAsyncFnMut,
        KAsyncFnOnce, KFnMut, KFnOnce, Optional, ParamsAccessor, Rest, This,
    };

    #[cfg(any(feature = "quickjs", feature = "jscore", feature = "arkjs"))]
    pub type ThisMut<T> = rong_core::function::ThisMut<T, crate::JSEngineValue>;

    #[cfg(any(feature = "quickjs", feature = "jscore", feature = "arkjs"))]
    pub type JSClassRef<T> = rong_core::function::JSClassRef<T, crate::JSEngineValue>;

    #[cfg(not(any(feature = "quickjs", feature = "jscore", feature = "arkjs")))]
    pub type ThisMut<T> = rong_core::function::ThisMut<T, ()>;

    #[cfg(not(any(feature = "quickjs", feature = "jscore", feature = "arkjs")))]
    pub type JSClassRef<T> = rong_core::function::JSClassRef<T, ()>;
}

/// The engine-typed API for one engine; `rong::quickjs::JSContext` and so on.
#[cfg(any(feature = "quickjs", feature = "jscore", feature = "arkjs"))]
macro_rules! engine_api {
    ($engine:ty) => {
        pub type RongJS = $engine;
        pub type JSEngineValue = <RongJS as $crate::JSEngine>::Value;
        pub type JSEngineContext = <RongJS as $crate::JSEngine>::Context;
        pub type JSContext = $crate::CoreJSContext<JSEngineContext>;
        pub type JSRuntime = $crate::CoreJSRuntime<<RongJS as $crate::JSEngine>::Runtime>;
        pub type JSValue = $crate::CoreJSValue<JSEngineValue>;
        pub type JSObject = $crate::CoreJSObject<JSEngineValue>;
        pub type JSProxy = $crate::CoreJSProxy<JSEngineValue>;
        pub type JSSymbol = $crate::CoreJSSymbol<JSEngineValue>;
        pub type JSFunc = $crate::CoreJSFunc<JSEngineValue>;
        pub type JSDate = $crate::CoreJSDate<JSEngineValue>;
        pub type Class = $crate::CoreClass<JSEngineValue>;
        pub type Promise = $crate::CorePromise<JSEngineValue>;
        pub type JSException = $crate::CoreJSException<JSEngineValue>;
        pub type JSArray = $crate::CoreJSArray<JSEngineValue>;
        pub type JSArrayBuffer = $crate::CoreJSArrayBuffer<JSEngineValue>;
        pub type JSBytes = $crate::CoreJSBytes<JSEngineValue>;
        pub type AnyJSTypedArray = $crate::CoreAnyJSTypedArray<JSEngineValue>;
        pub type JSTypedArray<T = u8> = $crate::CoreJSTypedArray<JSEngineValue, T>;
        pub type PropertyDescriptor = $crate::CorePropertyDescriptor<JSEngineValue>;
        pub type ThisMut<T> = rong_core::function::ThisMut<T, JSEngineValue>;
        pub type JSClassRef<T> = rong_core::function::JSClassRef<T, JSEngineValue>;
        pub type Rong = $crate::Rong<RongJS>;
        pub type PinnedRong<K, S> = $crate::PinnedRong<RongJS, K, S>;
    };
}

/// QuickJS, usable beside another engine in the same binary.
#[cfg(feature = "quickjs")]
pub mod quickjs {
    engine_api!(rong_quickjs::QuickJS);
}

/// JavaScriptCore, usable beside another engine in the same binary.
#[cfg(feature = "jscore")]
pub mod jscore {
    engine_api!(rong_jscore::JavaScriptCore);
}

/// ArkJS, usable beside another engine in the same binary.
#[cfg(feature = "arkjs")]
pub mod arkjs {
    engine_api!(rong_arkjs::HarmonyArkJS);
}

// The root aliases, `#[js_class]` and the module crates use one primary engine.
// Without a `primary-*` feature a platform engine outranks QuickJS, so a crate
// that adds QuickJS for its own isolates cannot, through feature unification,
// move the app's primary engine.
#[cfg(any(
    all(feature = "primary-quickjs", feature = "primary-jscore"),
    all(feature = "primary-quickjs", feature = "primary-arkjs"),
    all(feature = "primary-jscore", feature = "primary-arkjs"),
))]
compile_error!(
    "`rong` primary engine features are mutually exclusive: enable at most one of `primary-quickjs`, `primary-jscore`, or `primary-arkjs`."
);
#[cfg(any(
    all(feature = "primary-quickjs", not(feature = "quickjs")),
    all(feature = "primary-jscore", not(feature = "jscore")),
    all(feature = "primary-arkjs", not(feature = "arkjs")),
))]
compile_error!("a `rong` `primary-*` feature needs its engine feature enabled too.");

#[cfg(all(feature = "primary-arkjs", feature = "arkjs"))]
use arkjs as primary;
#[cfg(all(feature = "primary-jscore", feature = "jscore"))]
use jscore as primary;
#[cfg(all(feature = "primary-quickjs", feature = "quickjs"))]
use quickjs as primary;

#[cfg(not(any(
    feature = "primary-quickjs",
    feature = "primary-jscore",
    feature = "primary-arkjs"
)))]
mod default_primary {
    #[cfg(all(feature = "arkjs", not(feature = "jscore")))]
    pub use crate::arkjs::*;
    #[cfg(feature = "jscore")]
    pub use crate::jscore::*;
    #[cfg(all(feature = "quickjs", not(feature = "jscore"), not(feature = "arkjs")))]
    pub use crate::quickjs::*;
}
#[cfg(all(
    any(feature = "quickjs", feature = "jscore", feature = "arkjs"),
    not(any(
        feature = "primary-quickjs",
        feature = "primary-jscore",
        feature = "primary-arkjs"
    ))
))]
use default_primary as primary;

#[cfg(any(feature = "quickjs", feature = "jscore", feature = "arkjs"))]
pub use primary::{
    AnyJSTypedArray, Class, JSArray, JSArrayBuffer, JSBytes, JSContext, JSDate, JSEngineContext,
    JSEngineValue, JSException, JSFunc, JSObject, JSProxy, JSRuntime, JSSymbol, JSTypedArray,
    JSValue, Promise, PropertyDescriptor, RongJS,
};

// re-export macro public symbols to rong
pub use rong_macro::{
    FromJSObject, FromJSValue, IntoJSObject, js_api, js_class, js_method, js_numeric_enum, js_union,
};

/// A Trait for conversion from JavaScript values.
#[cfg(any(feature = "quickjs", feature = "jscore", feature = "arkjs"))]
pub trait TryFromJSValue: Sized {
    fn try_from_js(_value: JSValue) -> JSResult<Self>;
}
