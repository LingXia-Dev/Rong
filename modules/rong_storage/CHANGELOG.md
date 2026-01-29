# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_storage-v0.1.1) - 2026-01-29

### Added

- *(rong_storage)* add close method to Storage for proper database lifecycle management
- *(rong_storage)* implement lazy database initialization
- convert storage module to async API
- *(rong_storage)* storage info contains key count
- *(rong_storage)* support JSDate type
- add new module local storage

### Fixed

- *(rong_storage)* Create the storage table if it doesn't exist
- *(rong_storage)* check over size on setting

### Other

- prepare workspace for crates.io publishing
- update README
- *(core)* change FromJSValue and IntoJSValue to work with JSValue wrapper
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- convert rong_storage to class-based API with per-instance storage
- *(rong_storage)* upgrade redb to 3.1.0
- cargo fmt
- *(module:storage)* stop creating default storage database
- update Readme
- 1st commit
