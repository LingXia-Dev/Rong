# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_core-v0.1.1) - 2026-01-29

### Added

- add network and timeout error constants
- *(core)* add context-scoped state storage API
- *(core)* make stream coalesce target configurable
- add service container to JSContext with lifecycle management
- *(core)* Add JS invocation scheduler with priority and coalescing
- *(core)* implement fmt::Display for JSValueType
- *(core)* introduce tokio net runtime
- *(core)* RustFunc.prototype inherits Function.prototype
- *(core)* add global net runtime + builder integration
- *(core)* accept Vec<T> as function parameter and improve Promise resolution
- *(core)* introduce JSDate type
- *(core)* support Date type t JSTypeOf
- *(core)* implement JSParameterType for Option<T>
- *(core)* support save user data to JSCotnext
- *(core)* add call_async method to JSFunc for Promise support
- *(core)* API to creates a JSObject from a JSON string
- *(core)* add gc_mark method to trait JSClass
- *(core)* introduce WorkerMessage
- introduce new API set to use threadpool to run JSRuntime
- *(core)* JSObject has new method json_stringify
- trait JSClass has new method call_without_new
- *(core)* save engine name to JSRuntime
- *(core)* API with_nanme to Source
- *(core)* add global object Danity to Context
- *(core)* new trait IntoJSIterator
- *(core)* support IntoJSAsyncIterator to avoid clone
- *(core)* RustyJSError can hold JSValue as error

### Fixed

- *(core)* improve error handling and eliminate unsafe lifetime transmutes
- *(core)* replace message dropping with backpressure in task messaging
- *(core)* Fix potential panics in get_user_data methods
- *(core)* define JSFunc::name via descriptor to match spec
- *(core)* Keep `this` alive for the future's lifetime
- *(core)* fix compiler warning
- *(core)* shutdown services when Rc strong count is 1
- *(core)* try_map should pass error directly
- *(core)* ensure proper JSRuntime/Service shutdown order
- *(core)* Resolve block_on downcast failure using callback mechanism
- *(core)* message can be received in user async function
- *(core)* fix post_message always return Ok
- Pass JSRuntime by value to eliminate lifetime issues in async contexts
- TypeError etc works on Promise(rust async function)

### Other

- prepare workspace for crates.io publishing
- *(core)* fix rustdoc warnings for type annotations and URLs
- *(core)* remove Send bounds from async iterator and add mutex poison recovery
- *(core)* replace service_executor with bg runtime and user_agent module
- update README
- *(core)* extract ThrownValueStore into separate module
- *(core)* change FromJSValue and IntoJSValue to work with JSValue wrapper
- *(core)* restructure error handling with HostError and separate error creation from throwing
- *(core)* replace unsafe exception pointers with generational handles
- delete v8
- *(core)* delete user_data from JSContextInner since it has ContextServiceContainer
- *(core:net)* optimize stream processing with coalescing and larger channels
- *(core)* restructure service executor into modular components with new API exports
- *(core)* rename with_net_threads to with_service_threads
- rename net module to service_executor
- gate spawn helper behind rong module
- *(core:net)* enhance download functionality with BodySink trait
- *(core)* introduce enum HttpBody
- cargo fmt
- update Readme
- rong offer wrapper func spawn
- *(core:iterator)* simplify API
- improve Cargo workspace for better dependency management
- *(core)* remove Arc support from JSContext user data to simplify API
- *(core)* simplify promise and result handling
- *(core)* refactor RongJSError::JSValue
- change identifier of JS bytecode to RONG
- gc_mark use callback mode to make lifecyle easy
- *(core)* delete shutdown_signal from JSRuntime
- *(core)* Replace std::sync::Mutex with tokio::sync::Mutex and update async API
- *(rong)* improve comment
- clean up worker state and lifecycle handling
- *(core)* JSRuntime's get_shutdown_signal has its own notifier
- drop solution of scheduler
- run_pending_jobs can tell it's necessary to run for JSRuntime
- move method context from JSEngine to JSRuntime
- stop using ctx.spawn_local
- upgrade rust to 2024
- standardize package names to use underscore instead of hyphens
- re-org crate graph
- rename
- rename project name to Rong
- Source can save and load bytecode
- *(core)* TypeError if it's not instance of class on borrowing
- *(core)* imporve error message
- plan to be a standalone product
- 1st commit
