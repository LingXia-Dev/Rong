# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_timer-v0.1.1) - 2026-01-29

### Added

- integrate arkjs into core and module crates

### Fixed

- *(clippy)* resolve all clippy warnings across codebase
- *(rong_timer)* fix shutdown deadlock and interval timing behavior
- make JS engine selection explicit
- *(rong_timer)* if not a repeating timer, always clean up and return
- *(rong_timer)* clear interval/timeout ignore null etc

### Other

- *(timer)* run timers on bg runtime, dispatch callbacks on JS thread
- prepare workspace for crates.io publishing
- update README
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- *(rong_timer)* attach async timer API to global object timers
- update Readme
- rong offer wrapper func spawn
- *(core:iterator)* simplify API
- improve Cargo workspace for better dependency management
- *(module:timer)* timer cleanup mechanism to use JSRuntimeService.on_shutdown
- Merge branch 'multiple_runtime'
- *(module:timer)* rong_timer example use new runtime API
- move method context from JSEngine to JSRuntime
- stop using ctx.spawn_local
- rename modules with rong_ prefix
- 1st commit
