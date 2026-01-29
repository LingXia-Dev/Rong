# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_quickjs-v0.1.1) - 2026-01-29

### Added

- upgrade quickjs to v0.11.0
- *(quickjs)* implement create_date
- jscore and quickjs implement is_date
- *(quickjs)* success to build for harmony os
- *(quickjs)* implement gc_mark
- *(quickjs)* export C API for gc_mark
- *(quickjs)* upgrade to 0.9.0

### Fixed

- *(quickjs)* implement runtime lifetime guards with safe job draining
- *(quickjs)* fix memory leaks, exception handling, and value identity semantics
- *(quickjs)* improve hash implementation for cross-platform compatibility
- update build for quickjs-ng v0.11.0 compatibility
- convert i64/u64 to JS type smartly
- *(quickjs)* use stdbool to fix compiler issue for android
- *(quickjs)* Promise object leak issue
- *(quickjs)* fix throw JSValue twice
- TypeError etc works on Promise(rust async function)

### Other

- prepare workspace for crates.io publishing
- format code
- *(quickjs)* improve build configuration and cache invalidation
- *(quickjs)* implement JSErrorFactory and JSExceptionThrower traits
- *(quickjs)* update Android NDK configuration
- upgrade bindgen to 0.72
- improve Cargo workspace for better dependency management
- gc_mark use callback mode to make lifecyle easy
- *(quickjs)* use stdbool to replace JS_BOOL
- run_pending_jobs can tell it's necessary to run for JSRuntime
- upgrade rust to 2024
- standardize package names to use underscore instead of hyphens
- re-org crate graph
- rename
- skip IsString check for string conversion
- plan to be a standalone product
