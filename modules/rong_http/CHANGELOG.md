# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_http-v0.1.1) - 2026-01-29

### Added

- *(http)* implement FormData parsing for Request and Response
- implement HTTP redirect support in fetch API
- *(rong_http)* implement gc_mark for Resposne
- *(rong_http)* body return ReabableStream implementing async iterator
- *(rong_http)* fetch supprots ReadableStream
- *(rong_http)* switch to core net runtime
- *(modules:http)* not allow multiple body reads
- *(rong_http)* support check whether domain is allowed
- integrate arkjs into core and module crates

### Fixed

- *(clippy)* resolve all clippy warnings across codebase
- *(http)* ensure Response clones share body consumption state
- cache JS ReadableStream instances in Response.body getter
- make JS engine selection explicit
- *(rong_http)* gc_mark http request

### Other

- prepare workspace for crates.io publishing
- format code
- *(modules)* convert async methods from &mut self to &self with interior mutability
- *(modules)* migrate HTTP client to rong_http module
- *(core)* replace service_executor with bg runtime and user_agent module
- update README
- *(core)* change FromJSValue and IntoJSValue to work with JSValue wrapper
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- *(rong_http)* optimize fetch and response handling
- rename net module to service_executor
- *(rong_http)* use HttpBody and remove BodyKind::Hyper
- *(rong_http)* use Bytes for buffered body and align fetch/respons
- *(rong_http)* increase delay for abort-on-read
- *(rong_http)* use rong core user agent API
- *(rong_http)* test upload and downlaod using ReadableStream
- cargo fmt
- update Readme
- *(core:iterator)* simplify API
- rename modules with rong_ prefix
- rename fetch to rong-http
- 1st commit
