//! ECMA-402 `Intl` for engines that have none.
//!
//! rong provides `Intl` only where the engine has none: QuickJS. JavaScriptCore
//! and ArkJS keep their own, and [`init`] leaves an existing `Intl` untouched.
//! The bar is ECMA-402 itself, with JavaScriptCore and ArkJS as references.
//!
//! Locale data comes from an ICU4X data blob loaded per app ([`IntlData`]), so
//! an app carries only the locales it ships. The default locale, time zone and
//! hour cycle come from the embedder's [`IntlSettings`]; on engines with their
//! own `Intl` the engine's (the system's) defaults apply instead.
//!
//! ```rust,no_run
//! use rong::*;
//!
//! fn setup(ctx: &JSContext, blob: Vec<u8>) -> JSResult<()> {
//!     let data = rong_intl::IntlData::try_from_blob(blob.into_boxed_slice())?;
//!     let settings = rong_intl::IntlSettings {
//!         locale: Some("he-IL".into()),
//!         ..Default::default()
//!     };
//!     rong_intl::configure(ctx, data, settings);
//!     rong_intl::init(ctx)
//! }
//! ```

mod data;
mod locale;
mod settings;
mod value_aliases;

use rong::*;

pub use data::IntlData;

/// The ICU4X release whose data format and CLDR version the blobs follow.
pub const ICU4X_VERSION: &str = "2.4.0";
pub use settings::{HourCycle, IntlSettings};

/// The ECMA-402 abstract operations whose coercion order is observable, and
/// the `Intl` namespace. Evaluates to a function that takes the natives.
const BOOTSTRAP: &str = include_str!("intl.js");

/// Per-context `Intl` state: the app's data and the settings record.
struct IntlState {
    data: IntlData,
    settings: settings::SettingsRecord,
}

impl JSContextService for IntlState {}

/// Gives `ctx` its locale data and settings. Call before [`init`]; settings
/// can change later with [`set_settings`].
pub fn configure(ctx: &JSContext, data: IntlData, settings: IntlSettings) {
    ctx.set_service(IntlState {
        data,
        settings: settings::SettingsRecord::new(settings),
    });
}

/// Replaces the settings record. Its generation advances, so formatters
/// cached for the default locale, zone or hour cycle are rebuilt on next use.
pub fn set_settings(ctx: &JSContext, settings: IntlSettings) -> bool {
    match ctx.get_service::<IntlState>() {
        Some(state) => {
            state.settings.replace(settings);
            true
        }
        None => false,
    }
}

/// The current settings, if [`configure`] was called.
pub fn settings(ctx: &JSContext) -> Option<IntlSettings> {
    ctx.get_service::<IntlState>()
        .map(|state| state.settings.get())
}

/// Installs `Intl` when the engine has none and [`configure`] gave it data.
/// An engine's own `Intl` is never replaced, extended or shadowed.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    let has_intl: bool = ctx.eval(Source::from_bytes("typeof globalThis.Intl !== 'undefined'"))?;
    if has_intl || ctx.get_service::<IntlState>().is_none() {
        return Ok(());
    }
    let natives = JSObject::new(ctx);
    let own = ctx.clone();
    natives.set(
        "canonicalizeTag",
        JSFunc::new(ctx, move |tag: String| -> Option<String> {
            let state = own.get_service::<IntlState>()?;
            locale::canonicalize(state.data.canonicalizer(), &tag)
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "defaultLocale",
        JSFunc::new(ctx, move || -> String {
            own.get_service::<IntlState>()
                .map(|state| state.settings.default_locale(state.data.canonicalizer()))
                .unwrap_or_else(|| String::from(settings::FALLBACK_LOCALE))
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "settingsGeneration",
        JSFunc::new(ctx, move || -> f64 {
            own.get_service::<IntlState>()
                .map_or(0.0, |state| state.settings.generation() as f64)
        })?,
    )?;
    let install: JSFunc = ctx.eval(Source::from_bytes(BOOTSTRAP))?;
    // The returned helpers serve the constructors from phase 1 on.
    let _helpers: JSObject = install.call(None, (natives,))?;
    Ok(())
}
