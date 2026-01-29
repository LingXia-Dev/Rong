# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_jscore-v0.1.1) - 2026-01-29

### Added

- *(jscore)* add platform-specific cfg attributes for BigInt
- *(jscore)* implement BigInt support with macOS version compatibility
- *(jscore)* implement create_date
- jscore and quickjs implement is_date

### Fixed

- *(jscore)* fix memory management, value semantics, and function call behavior
- *(jsc)* use borrowed protection for enumerated property names
- *(jscore)* protect deferred Promise callbacks to prevent JSC crash
- convert i64/u64 to JS type smartly
- fix wrong crate name javascriptcore
- TypeError etc works on Promise(rust async function)

### Other

- prepare workspace for crates.io publishing
- format code
- *(javascriptcore)* implement JSErrorFactory and JSExceptionThrower traits
- cargo fmt
- upgrade bindgen to 0.72
- run_pending_jobs can tell it's necessary to run for JSRuntime
- upgrade rust to 2024
- standardize package names to use underscore instead of hyphens
- re-org crate graph
- rename
- skip IsString check for string conversion
- *(jsc)* delete constructor.c since it's misleading
- plan to be a standalone product
