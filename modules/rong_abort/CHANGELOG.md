# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_abort-v0.1.1) - 2026-01-29

### Added

- *(rong_http)* implement gc_mark for Resposne
- integrate arkjs into core and module crates

### Fixed

- *(abort)* fix event ordering and add mutex poison recovery
- make JS engine selection explicit
- *(rong_abort)* refactor abort_controller to fix memory leak
- *(rong_abort)* fix leak using gc_mark on quickjs

### Other

- prepare workspace for crates.io publishing
- update README
- *(core)* change FromJSValue and IntoJSValue to work with JSValue wrapper
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- update Readme
- rong offer wrapper func spawn
- use gc_mark_with as method name
- *(rong_event)* rename gc_mark_callback to gc_mark
- *(rong_event)* Emitter's get_inner_emitter return itself
- stop using ctx.spawn_local
- rename modules with rong_ prefix
- 1st commit
