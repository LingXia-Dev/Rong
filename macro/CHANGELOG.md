# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_macro-v0.1.1) - 2026-01-29

### Added

- *(macro)* FromJSObj supports default value
- *(macro)* Derive macro for serialization from Rust struct to JavaScript object
- *(macro)* support gc_mark
- *(macro)* support derive FromJSValue and IntoJSValue for enum type
- *(macro)* new macro FromJSValue

### Fixed

- *(macro)* use fully-qualified Clone::clone to avoid inherent method conflicts
- *(macro)* prevent unsafe async methods with &mut self and improve ThisMut handling

### Other

- prepare workspace for crates.io publishing
- update README
- *(core)* change FromJSValue and IntoJSValue to work with JSValue wrapper
- *(macro)* update generated code to use HostError API and let-chains
- delete v8
- cargo fmt
- update Readme
- forgot to track serialize.rs for IntoJSObj
- upgrade rong_macro to Rust 2024
- standardize package names to use underscore instead of hyphens
- re-org crate graph
- rename
- *(macro)* rename file name object to instance
- plan to be a standalone product
- 1st commit
