# rong_jsvm

rong's backend for OpenHarmony's JSVM-API (`libjsvm`, a V8).

## Overview

This crate binds the `OH_JSVM_*` API that HarmonyOS and OpenHarmony expose
to native code. It is a V8 built without ICU, not ArkTS's own runtime;
`rong::jsvm::intl::init` gives it an `Intl` (the `intl` feature).

## Platform Support

`rong_jsvm` is only supported on HarmonyOS/OpenHarmony targets such as:

- `aarch64-unknown-linux-ohos`

You must install the HarmonyOS/OpenHarmony native toolchain and set:

```bash
export OHOS_NDK_HOME=/path/to/ohos-sdk
```

Then build with:

```bash
cargo build --target aarch64-unknown-linux-ohos
```
