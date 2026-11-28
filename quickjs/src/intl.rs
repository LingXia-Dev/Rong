//! Installs rong's ECMA-402 `Intl` (`rong_intl`) on QuickJS, which has none.
//! Turned on by the `intl` feature (`rong`'s `intl`); reached as
//! `rong::quickjs::intl`.

use crate::{QJSContext, QuickJS};
use rong_core::{JSContext, JSResult};

/// Installs `Intl` on `ctx` once `rong::intl::configure` gave it data.
pub fn init(ctx: &JSContext<QJSContext>) -> JSResult<()> {
    rong_intl::init::<QuickJS>(ctx)
}
