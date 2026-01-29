# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_quickjs_sys-v0.1.1) - 2026-01-29

### Added

- upgrade quickjs to v0.11.0
- *(quickjs)* success to build for harmony os
- *(quickjs)* export C API for gc_mark
- *(quickjs)* upgrade to 0.9.0

### Fixed

- *(quickjs)* fix memory leaks, exception handling, and value identity semantics
- update build for quickjs-ng v0.11.0 compatibility
- *(quickjs)* use stdbool to fix compiler issue for android

### Other

- prepare workspace for crates.io publishing
- *(quickjs)* improve build configuration and cache invalidation
- update README
- *(quickjs)* update Android NDK configuration
- delete v8
- update Readme
- upgrade bindgen to 0.72
- *(quickjs)* use stdbool to replace JS_BOOL
- upgrade rust to 2024
- standardize package names to use underscore instead of hyphens
- re-org crate graph
- rename
- plan to be a standalone product
- 1st commit
