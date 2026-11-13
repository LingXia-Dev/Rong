// The generated tests keep the `#[cfg(feature = ...)]` gates they have in the
// root crate, whose engine features this crate does not declare.
#![allow(unexpected_cfgs)]

#[cfg(target_env = "ohos")]
pub mod prelude;

/// Generated test modules — each corresponds to a `tests/*.rs` file.
/// Created by build.rs at compile time.
#[cfg(target_env = "ohos")]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/tests/mod.rs"));
}

/// Generated test registry.
#[cfg(target_env = "ohos")]
mod registry {
    include!(concat!(env!("OUT_DIR"), "/registry.rs"));
}

pub use rong_test_harness::TestEntry;

/// Returns all test entries discovered from `tests/*.rs` by build.rs.
#[cfg(target_env = "ohos")]
pub fn all_tests() -> Vec<TestEntry> {
    registry::all_tests()
}

/// Host builds only need the crate to compile so the Harmony shell can be checked.
#[cfg(not(target_env = "ohos"))]
pub fn all_tests() -> Vec<TestEntry> {
    Vec::new()
}
