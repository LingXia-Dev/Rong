# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_console-v0.1.1) - 2026-01-29

### Added

- *(console)* print Date as String
- integrate arkjs into core and module crates

### Fixed

- *(rong_console)* improve console.log format string handling
- *(tests)* make console tests deterministic with thread-local output buffer
- *(console)* Set a custom console writer for the current thread.
- make JS engine selection explicit

### Other

- prepare workspace for crates.io publishing
- *(console)* remove unwrap calls and improve error handling
- update README
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- update Readme
- *(module:console)* use OnceLock in CONSOLE_WRITER instead of Mutex
- improve Cargo workspace for better dependency management
- console test function use test helper API
- rename modules with rong_ prefix
- 1st commit
