# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_stream-v0.1.1) - 2026-01-29

### Added

- *(rong_stream)* add JSWritableStream wrapper for clearer semantics
- *(rong_stream)* add fast path for channel-to-channel stream piping
- *(rogn_stream)* implement tee for ReadableStream
- *(rong_stream)* ReadableStream implements the async iterable protocol and pipeTo
- *(rong_stream)* export readable_stream_take_receiver
- *(rong_stream)* implement web API ReadableStream and WritableStream

### Fixed

- cache JS ReadableStream instances in Response.body getter

### Other

- prepare workspace for crates.io publishing
- *(rong_stream)* optimize ReadableStream with zero-copy internal reads and unbounded controller
- *(modules)* convert async methods from &mut self to &self with interior mutability
- update README
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- *(rong_stream)* don't use eval to get this object
- update Readme
- 1st commit
