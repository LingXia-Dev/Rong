# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/LingXia-Dev/Rong/releases/tag/rong_buffer-v0.1.1) - 2026-01-29

### Added

- *(rong_buffer)* add File::from_parts constructor for programmatic creation
- integrate arkjs into core and module crates

### Fixed

- make JS engine selection explicit

### Other

- prepare workspace for crates.io publishing
- format code
- *(modules)* convert async methods from &mut self to &self with interior mutability
- update README
- *(modules)* update modules to use HostError API and let-chains
- delete v8
- update Readme
- rename modules with rong_ prefix
- 1st commit
