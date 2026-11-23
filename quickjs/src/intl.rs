//! ECMA-402 `Intl` for QuickJS, which has none of its own. Turned on by the
//! `intl` feature (`rong`'s `quickjs-intl`); reached as `rong::quickjs::intl`.
//!
//! Give a context its data and settings with [`configure`], then call
//! [`init`]. Without the feature a QuickJS context has no `Intl`.

use crate::{QJSContext, QuickJS};
use rong_core::{JSContext, JSResult};

pub use rong_quickjs_intl::{HourCycle, ICU4X_VERSION, IntlData, IntlSettings};

/// Gives `ctx` its locale data and settings. Call before [`init`].
pub fn configure(ctx: &JSContext<QJSContext>, data: IntlData, settings: IntlSettings) {
    rong_quickjs_intl::configure(ctx, data, settings);
}

/// Replaces the settings; formatters cached for the defaults are rebuilt on
/// next use. Returns `false` if [`configure`] was not called.
pub fn set_settings(ctx: &JSContext<QJSContext>, settings: IntlSettings) -> bool {
    rong_quickjs_intl::set_settings(ctx, settings)
}

/// The current settings, if [`configure`] was called.
pub fn settings(ctx: &JSContext<QJSContext>) -> Option<IntlSettings> {
    rong_quickjs_intl::settings(ctx)
}

/// Sets where problems with the app's data blob are reported, such as the
/// host's device log; the `log` crate otherwise. The first call wins;
/// returns whether this one did.
pub fn set_reporter(report: impl Fn(&str) + Send + Sync + 'static) -> bool {
    rong_quickjs_intl::set_reporter(report)
}

/// Installs `Intl` on `ctx` once [`configure`] gave it data.
pub fn init(ctx: &JSContext<QJSContext>) -> JSResult<()> {
    rong_quickjs_intl::init::<QuickJS>(ctx)
}
