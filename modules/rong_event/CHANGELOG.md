# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_event-v0.1.1) - 2026-01-29

### Added

- integrate arkjs into core and module crates

### Fixed

- *(rong_event)* propagate listener errors and add mutex poison recovery
- *(rong_event)* Allow removing a listener during emit
- make JS engine selection explicit
- *(rong_event)* use gc_mark to fix leak on quickjs
- *(rong_event)* prevent JS function reference leaks in EventEmitter

### Other

- prepare workspace for crates.io publishing
- update README
- *(core)* change FromJSValue and IntoJSValue to work with JSValue wrapper
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- update Readme
- *(rong_event)* enjoy FromJSObj
- use gc_mark_with as method name
- *(rong_event)* rename gc_mark_callback to gc_mark
- *(rong_event)* add/remove listener does not need to return JSObject
- *(rong_event)* Emitter's get_inner_emitter return itself
- *(rong_event)* simplify Drop for EventEmitter
- rename modules with rong_ prefix
- 1st commit
