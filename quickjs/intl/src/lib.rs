//! ECMA-402 `Intl` for QuickJS, which has none.
//!
//! Internal to `rong_quickjs`: embedders turn it on with `rong`'s
//! `quickjs-intl` feature and use `rong::quickjs::intl`, not this crate.
//! JavaScriptCore and ArkJS have their own `Intl` and never link this one.
//! The bar is ECMA-402 itself, with JavaScriptCore and ArkJS as references.
//!
//! Locale data comes from an ICU4X data blob loaded per app ([`IntlData`]), so
//! an app carries only the locales it ships. The default locale, time zone and
//! hour cycle come from the embedder's [`IntlSettings`].
//!
//! The code is written against rong's engine-generic API and instantiated for
//! QuickJS by `rong_quickjs`, so this crate does not depend on the engine.

mod data;
mod locale;
mod number;
mod plural;
mod settings;
mod value_aliases;

use rong_core::{
    HostError, JSContext, JSContextImpl, JSContextService, JSEngine, JSFunc, JSObject, JSResult,
    Source,
};

pub use data::IntlData;

/// The ICU4X release whose data format and CLDR version the blobs follow.
pub const ICU4X_VERSION: &str = "2.4.0";
pub use settings::{HourCycle, IntlSettings};

static REPORTER: std::sync::OnceLock<Box<dyn Fn(&str) + Send + Sync>> = std::sync::OnceLock::new();

/// Sets where problems with the app's data blob are reported, such as the
/// host's device log; the `log` crate otherwise. The first call wins;
/// returns whether this one did.
pub fn set_reporter(report: impl Fn(&str) + Send + Sync + 'static) -> bool {
    REPORTER.set(Box::new(report)).is_ok()
}

pub(crate) fn report(line: &str) {
    match REPORTER.get() {
        Some(report) => report(line),
        None => log::warn!("{line}"),
    }
}

/// The ECMA-402 abstract operations whose coercion order is observable, and
/// the `Intl` namespace. Evaluates to a function that takes the natives.
const BOOTSTRAP: &str = include_str!("intl.js");

/// Per-context `Intl` state: the app's data and the settings record.
struct IntlState {
    data: IntlData,
    settings: settings::SettingsRecord,
    plurals: plural::Cache,
}

impl JSContextService for IntlState {}

/// Gives `ctx` its locale data and settings. Call before [`init`]; settings
/// can change later with [`set_settings`].
pub fn configure<C: JSContextImpl>(ctx: &JSContext<C>, data: IntlData, settings: IntlSettings) {
    ctx.set_service(IntlState {
        data,
        settings: settings::SettingsRecord::new(settings),
        plurals: plural::Cache::default(),
    });
}

/// Replaces the settings record. Its generation advances, so formatters
/// cached for the default locale, zone or hour cycle are rebuilt on next use.
pub fn set_settings<C: JSContextImpl>(ctx: &JSContext<C>, settings: IntlSettings) -> bool {
    match ctx.get_service::<IntlState>() {
        Some(state) => {
            state.settings.replace(settings);
            true
        }
        None => false,
    }
}

/// The current settings, if [`configure`] was called.
pub fn settings<C: JSContextImpl>(ctx: &JSContext<C>) -> Option<IntlSettings> {
    ctx.get_service::<IntlState>()
        .map(|state| state.settings.get())
}

/// Installs `Intl` on `ctx` once [`configure`] gave it data; without data it
/// does nothing. QuickJS only: `rong_quickjs` instantiates it for QuickJS.
pub fn init<E: JSEngine>(ctx: &JSContext<E::Context>) -> JSResult<()>
where
    E::Context: 'static,
{
    if ctx.get_service::<IntlState>().is_none() {
        return Ok(());
    }
    let natives = JSObject::<E::Value>::new(ctx);
    let own = ctx.clone();
    natives.set(
        "canonicalizeTag",
        JSFunc::<E::Value>::new(ctx, move |tag: String| -> Option<String> {
            let state = own.get_service::<IntlState>()?;
            locale::canonicalize(state.data.canonicalizer(), &tag)
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "defaultLocale",
        JSFunc::<E::Value>::new(ctx, move || -> String {
            own.get_service::<IntlState>()
                .map(|state| state.settings.default_locale(state.data.canonicalizer()))
                .unwrap_or_else(|| String::from(settings::FALLBACK_LOCALE))
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "settingsGeneration",
        JSFunc::<E::Value>::new(ctx, move || -> f64 {
            own.get_service::<IntlState>()
                .map_or(0.0, |state| state.settings.generation() as f64)
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "resolveLocale",
        JSFunc::<E::Value>::new(
            ctx,
            move |service: String, requested: Vec<String>| -> JSResult<String> {
                let state = state(&own)?;
                let available = |tag: &str| available(&state.data, &service, tag);
                let default = state.settings.default_locale(state.data.canonicalizer());
                let resolved = locale::resolve_locale(
                    state.data.canonicalizer(),
                    &available,
                    &requested,
                    &[],
                    &[],
                    &locale::NoKeys,
                    &default,
                );
                Ok(resolved.locale)
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "supportedLocales",
        JSFunc::<E::Value>::new(
            ctx,
            move |service: String, requested: Vec<String>| -> JSResult<Vec<String>> {
                let state = state(&own)?;
                let available = |tag: &str| available(&state.data, &service, tag);
                Ok(locale::supported_locales(&available, &requested))
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "pluralResolve",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>, value: String| -> JSResult<Vec<String>> {
                let state = state(&own)?;
                let options = plural_options::<E>(&slots)?;
                let (category, string) = plural::resolve(
                    &state.plurals,
                    &state.data,
                    &options,
                    &number::to_intl_mv(&value),
                );
                Ok(vec![plural::name(category).to_string(), string])
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "pluralResolveRange",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>,
                  start: String,
                  end: String|
                  -> JSResult<Option<String>> {
                let state = state(&own)?;
                let options = plural_options::<E>(&slots)?;
                let (x, y) = (number::to_intl_mv(&start), number::to_intl_mv(&end));
                // ResolvePluralRange: a NaN end is a RangeError, thrown by the caller.
                if x == number::IntlMV::NotANumber || y == number::IntlMV::NotANumber {
                    return Ok(None);
                }
                let (x, y) = (
                    plural::resolve(&state.plurals, &state.data, &options, &x),
                    plural::resolve(&state.plurals, &state.data, &options, &y),
                );
                let category = if x.1 == y.1 {
                    x.0
                } else {
                    plural::select_range(
                        &state.plurals,
                        &state.data,
                        &options.locale,
                        options.ordinal,
                        x.0,
                        y.0,
                    )
                };
                Ok(Some(plural::name(category).to_string()))
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "pluralCategories",
        JSFunc::<E::Value>::new(
            ctx,
            move |locale: String, kind: String| -> JSResult<Vec<String>> {
                let state = state(&own)?;
                let categories =
                    plural::categories(&state.plurals, &state.data, &locale, kind == "ordinal");
                Ok(categories.into_iter().map(String::from).collect())
            },
        )?,
    )?;
    let install: JSFunc<E::Value> = ctx.eval(Source::from_bytes(BOOTSTRAP))?;
    // The returned helpers serve the constructors from phase 1 on.
    let _helpers: JSObject<E::Value> = install.call(None, (natives,))?;
    Ok(())
}

fn state<C: JSContextImpl>(ctx: &JSContext<C>) -> JSResult<&IntlState> {
    ctx.get_service::<IntlState>()
        .ok_or_else(|| bad("Intl is not configured for this context"))
}

fn bad(message: &str) -> rong_core::RongJSError {
    HostError::new(rong_core::error::E_INVALID_ARG, message.to_string()).into()
}

/// A service's [[AvailableLocales]]: the blob holds its data for the locale
/// or a locale it falls back to, short of the root.
fn available(data: &IntlData, service: &str, tag: &str) -> bool {
    let Ok(locale) = tag.parse::<icu::locale::Locale>() else {
        return false;
    };
    match service {
        "PluralRules" => data.has_locale::<icu::plurals::provider::PluralsCardinalV1>(&locale),
        _ => false,
    }
}

/// An Intl.PluralRules' slots, as the bootstrap keeps them.
fn plural_options<E: JSEngine>(slots: &JSObject<E::Value>) -> JSResult<plural::Options> {
    let text = |key: &str| -> JSResult<String> { slots.get::<_, String>(key) };
    // Digit slots SetNumberFormatDigitOptions leaves unset are absent, not undefined.
    let count = |key: &str| -> JSResult<u8> {
        let n: Option<f64> = slots.get_opt(key)?;
        Ok(n.unwrap_or(0.0) as u8)
    };
    let kind = text("type")?;
    let notation = text("notation")?;
    let compact_display: Option<String> = slots.get_opt("compactDisplay")?;
    let digits = number::DigitOptions {
        min_integer: count("minimumIntegerDigits")?,
        min_fraction: count("minimumFractionDigits")?,
        max_fraction: count("maximumFractionDigits")?,
        min_significant: count("minimumSignificantDigits")?,
        max_significant: count("maximumSignificantDigits")?,
        increment: slots.get::<_, f64>("roundingIncrement")? as u16,
        rounding_type: number::RoundingType::parse(&text("roundingType")?)
            .ok_or_else(|| bad("unknown rounding type"))?,
        rounding_mode: number::RoundingMode::parse(&text("roundingMode")?)
            .ok_or_else(|| bad("unknown rounding mode"))?,
        strip_if_integer: text("trailingZeroDisplay")? == "stripIfInteger",
    };
    Ok(plural::Options {
        locale: text("locale")?,
        ordinal: kind == "ordinal",
        notation: plural::Notation::parse(&notation, compact_display.as_deref().unwrap_or("short"))
            .ok_or_else(|| bad("unknown notation"))?,
        digits,
    })
}
