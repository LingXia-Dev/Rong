# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_fs-v0.1.1) - 2026-01-29

### Added

- *(fs)* implement Unix file permission mode support
- *(rong_fs)* FsFile's readable return async iterator ReadableStream
- *(rong_fs)* FsFile support ReadableStream and WriteableStream
- *(rong_fs)* support Deno compatible FsFile
- *(rong_fs)* File access guard trait for controlling file access permissions
- integrate arkjs into core and module crates

### Fixed

- *(rong_fs)* Add explicit flush calls to file write operations
- make JS engine selection explicit
- *(rong_fs)* remove wrong dependency rong_quickjs

### Other

- prepare workspace for crates.io publishing
- format code
- *(rong_fs)* optimize file stream reading and improve stat safety
- *(modules)* convert async methods from &mut self to &self with interior mutability
- update README
- *(modules)* update modules to use HostError API and let-chains
- *(rong_fs)* use resolved paths from file access guard
- delete v8
- perf(rong)fs): increase file write channel buffer size
- cargo fmt
- update Readme
- *(rong_fs)* introduce misc.rs to contains some functions
- *(core:iterator)* simplify API
- *(rong_fs)* fix clippy warning
- standardize package names to use underscore instead of hyphens
- rename modules with rong_ prefix
- 1st commit
