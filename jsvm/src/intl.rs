//! Installs rong's ECMA-402 `Intl` (`rong_intl`) on JSVM. HarmonyOS's JSVM
//! is a V8 built without ICU, so it has no `Intl` and its `toLocale*` methods
//! ignore the locale. Turned on by the `intl` feature (`rong`'s `intl`);
//! reached as `rong::jsvm::intl`.

use crate::{JSVM, JSVMContext};
use rong_core::{JSContext, JSResult};

/// Installs `Intl` on `ctx` once `rong::intl::configure` gave it data. A JSVM
/// that has its own `Intl` keeps it.
pub fn init(ctx: &JSContext<JSVMContext>) -> JSResult<()> {
    if ctx.global().has_property("Intl")? {
        return Ok(());
    }
    rong_intl::init::<JSVM>(ctx)
}
