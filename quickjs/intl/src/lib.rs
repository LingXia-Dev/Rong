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

mod cldr_dates;
mod cldr_numbers;
mod cldr_units;
mod data;
mod date_time_format;
mod list_format;
mod locale;
mod number;
mod number_format;
mod plural;
mod relative_time;
mod settings;
mod value_aliases;

use rong_core::{
    HostError, JSContext, JSContextImpl, JSContextService, JSEngine, JSFunc, JSObject, JSResult,
    Source,
};
use std::rc::Rc;

pub use data::IntlData;

/// The ICU4X release whose data format and CLDR version the blobs follow.
pub const ICU4X_VERSION: &str = "2.4.0";
/// The CLDR release of ICU4X's data and of the number symbols this crate
/// takes from CLDR directly.
pub const CLDR_VERSION: &str = cldr_numbers::CLDR_VERSION;
pub use settings::{HourCycle, IntlSettings};

type Reporter = Box<dyn Fn(&str) + Send + Sync>;
static REPORTER: std::sync::OnceLock<Reporter> = std::sync::OnceLock::new();

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
    numbers: number_format::Cache,
    date_times: date_time_format::Cache,
    relative_times: relative_time::Cache,
    lists: list_format::Cache,
    /// ICU4X's case mapper, loaded on the first tailored case mapping.
    case_mapper: std::cell::OnceCell<Option<icu::casemap::CaseMapper>>,
    /// Services whose default already reported a host locale missing from
    /// the blob.
    reported_defaults: std::cell::RefCell<std::collections::HashSet<String>>,
}

impl IntlState {
    /// DefaultLocale for `service`: a locale the blob supports for it. The
    /// host's locale goes through the same lookup as a requested one; when
    /// nothing matches, the blob's declared fallback, then the first locale
    /// the blob holds for the service. `und` only for a blob with no data
    /// for the service at all, whose answers do come from the root.
    fn default_locale(&self, service: &str) -> String {
        let available = |tag: &str| available(&self.data, service, tag);
        let host = self.settings.default_locale(self.data.canonicalizer());
        if let Some(found) = locale::best_available_locale(&available, &host) {
            return found;
        }
        let chosen = self
            .data
            .fallback_locale()
            .and_then(|tag| locale::best_available_locale(&available, tag))
            .or_else(|| {
                service_locales(&self.data, service)
                    .into_iter()
                    .find(|tag| available(tag))
            })
            .unwrap_or_else(|| String::from("und"));
        if self
            .reported_defaults
            .borrow_mut()
            .insert(service.to_string())
        {
            report(&format!(
                "rong_quickjs_intl: the data blob has no {service} data for the host locale \
                 {host}; {service} defaults to {chosen}"
            ));
        }
        chosen
    }
}

impl JSContextService for IntlState {}

/// Gives `ctx` its locale data and settings. Call before [`init`]; settings
/// can change later with [`set_settings`].
pub fn configure<C: JSContextImpl>(ctx: &JSContext<C>, data: IntlData, settings: IntlSettings) {
    ctx.set_service(IntlState {
        data,
        settings: settings::SettingsRecord::new(settings),
        plurals: plural::Cache::default(),
        numbers: number_format::Cache::default(),
        date_times: Default::default(),
        relative_times: Default::default(),
        lists: Default::default(),
        case_mapper: Default::default(),
        reported_defaults: Default::default(),
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
        JSFunc::<E::Value>::new(ctx, move |service: String| -> JSResult<String> {
            Ok(state(&own)?.default_locale(&service))
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
                let default = state.default_locale(&service);
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
    let own = ctx.clone();
    natives.set(
        "resolveNumberFormat",
        JSFunc::<E::Value>::new(
            ctx,
            move |requested: Vec<String>,
                  nu: Option<String>,
                  service: Option<String>|
                  -> JSResult<Vec<String>> {
                let state = state(&own)?;
                // NumberFormat, or a service that formats with one.
                let service = service.unwrap_or_else(|| String::from("NumberFormat"));
                let available = |tag: &str| available(&state.data, &service, tag);
                let default = state.default_locale(&service);
                let resolved = locale::resolve_locale(
                    state.data.canonicalizer(),
                    &available,
                    &requested,
                    &[("nu", nu)],
                    &["nu"],
                    &NumberingSystems(&state.data),
                    &default,
                );
                let nu = resolved
                    .values
                    .into_iter()
                    .find_map(|(k, v)| (k == "nu").then_some(v).flatten())
                    .unwrap_or_else(|| String::from("latn"));
                Ok(vec![resolved.locale, resolved.data_locale, nu])
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "numberFormatParts",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>, value: String| -> JSResult<Vec<Vec<String>>> {
                let state = state(&own)?;
                let options = number_format_options::<E>(&slots)?;
                let (format, compact, measure) = number_format_data(state, &options)?;
                let parts = number_format::format_to_parts(
                    &format,
                    &options,
                    compact.as_deref(),
                    measure.as_deref(),
                    &number::to_intl_mv(&value),
                );
                Ok(parts
                    .into_iter()
                    .map(|(kind, value)| vec![kind.to_string(), value])
                    .collect())
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "relativeTimeParts",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>,
                  number_slots: JSObject<E::Value>,
                  unit: String,
                  value: String|
                  -> JSResult<Vec<Vec<String>>> {
                let state = state(&own)?;
                let text = |key: &str| -> JSResult<String> { slots.get::<_, String>(key) };
                let options = number_format_options::<E>(&number_slots)?;
                let (format, compact, _) = number_format_data(state, &options)?;
                let locale = text("dataLocale")?;
                let rules = state.plurals.rules(&state.data, &options.locale, false);
                let parts = relative_time::format_to_parts(
                    &state.relative_times,
                    &state.data,
                    rules.as_ref().map(|r| r.rules()),
                    &locale,
                    &text("style")?,
                    text("numeric")? == "auto",
                    &unit,
                    &number::to_intl_mv(&value),
                    |x| {
                        number_format::format_with_operands(
                            &format,
                            &options,
                            compact.as_deref(),
                            x,
                        )
                    },
                )
                .map_err(|e| {
                    report(&format!("rong_quickjs_intl: {e}"));
                    bad(&e)
                })?;
                Ok(parts
                    .into_iter()
                    .map(|(kind, value, unit)| {
                        vec![
                            kind.to_string(),
                            value,
                            unit.unwrap_or_default().to_string(),
                        ]
                    })
                    .collect())
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "listParts",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>, list: Vec<String>| -> JSResult<Vec<Vec<String>>> {
                let state = state(&own)?;
                let text = |key: &str| -> JSResult<String> { slots.get::<_, String>(key) };
                let parts = list_format::format_to_parts(
                    &state.lists,
                    &state.data,
                    &text("locale")?,
                    &text("type")?,
                    &text("style")?,
                    &list,
                )
                .map_err(|e| {
                    report(&format!("rong_quickjs_intl: {e}"));
                    bad(&e)
                })?;
                Ok(parts
                    .into_iter()
                    .map(|(kind, value)| vec![kind.to_string(), value])
                    .collect())
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "numberFormatRangeParts",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>,
                  start: String,
                  end: String|
                  -> JSResult<Option<Vec<Vec<String>>>> {
                let state = state(&own)?;
                let (x, y) = (number::to_intl_mv(&start), number::to_intl_mv(&end));
                // A NaN end is a RangeError, thrown by the caller.
                if x == number::IntlMV::NotANumber || y == number::IntlMV::NotANumber {
                    return Ok(None);
                }
                let options = number_format_options::<E>(&slots)?;
                let (format, compact, measure) = number_format_data(state, &options)?;
                let parts = number_format::format_range_to_parts(
                    &format,
                    &options,
                    compact.as_deref(),
                    measure.as_deref(),
                    &x,
                    &y,
                );
                Ok(Some(
                    parts
                        .into_iter()
                        .map(|(kind, value, source)| {
                            vec![kind.to_string(), value, source.to_string()]
                        })
                        .collect(),
                ))
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "currencyDigits",
        JSFunc::<E::Value>::new(ctx, move |code: String| -> JSResult<f64> {
            Ok(f64::from(currency_digits(&state(&own)?.data, &code)))
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "resolveDateTimeFormat",
        JSFunc::<E::Value>::new(
            ctx,
            move |requested: Vec<String>,
                  ca: Option<String>,
                  nu: Option<String>,
                  hc: Option<String>|
                  -> JSResult<Vec<String>> {
                let state = state(&own)?;
                let available = |tag: &str| available(&state.data, "DateTimeFormat", tag);
                let default = state.default_locale("DateTimeFormat");
                let resolved = locale::resolve_locale(
                    state.data.canonicalizer(),
                    &available,
                    &requested,
                    &[
                        ("ca", ca.map(|v| canonical_u_value("ca", v))),
                        ("nu", nu),
                        ("hc", hc),
                    ],
                    &["ca", "hc", "nu"],
                    &DateTimeLocaleData(state),
                    &default,
                );
                let value = |key: &str| {
                    resolved
                        .values
                        .iter()
                        .find_map(|(k, v)| (k == key).then(|| v.clone()).flatten())
                };
                let [mut hour_cycle, hour_cycle_12, hour_cycle_24] = state
                    .date_times
                    .hour_cycles(&state.data, &resolved.data_locale);
                if let Some(hc) = state.settings.get().hour_cycle {
                    hour_cycle = match hc {
                        HourCycle::H11 => "h11",
                        HourCycle::H12 => "h12",
                        HourCycle::H23 => "h23",
                        HourCycle::H24 => "h24",
                    }
                    .to_string();
                }
                Ok(vec![
                    resolved.locale.clone(),
                    resolved.data_locale.clone(),
                    value("ca").unwrap_or_else(|| String::from("gregory")),
                    value("nu").unwrap_or_else(|| String::from("latn")),
                    value("hc").unwrap_or_else(|| NULL_HC.to_string()),
                    hour_cycle,
                    hour_cycle_12,
                    hour_cycle_24,
                ])
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "primaryTimeZone",
        JSFunc::<E::Value>::new(ctx, move |name: String| -> JSResult<Option<String>> {
            let state = state(&own)?;
            no_database(&name)?;
            Ok(date_time_format::primary_zone(&state.data, &name).ok())
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "systemTimeZone",
        JSFunc::<E::Value>::new(ctx, move || -> JSResult<String> {
            let state = state(&own)?;
            let name = state
                .settings
                .get()
                .time_zone
                .or_else(rong_quickjs_tz::system_zone);
            // UTC stands in for a host zone that cannot be read, never for
            // one that cannot be answered: that would give wrong offsets.
            let Some(name) = name else {
                if !rong_quickjs_tz::has_database() {
                    return Err(no_database_error("the host's time zone"));
                }
                return Ok(String::from("UTC"));
            };
            no_database(&name)?;
            Ok(date_time_format::primary_zone(&state.data, &name)
                .unwrap_or_else(|_| String::from("UTC")))
        })?,
    )?;
    let own = ctx.clone();
    natives.set(
        "dateTimeFormatFields",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>| -> JSResult<Vec<Vec<String>>> {
                let state = state(&own)?;
                let request = date_time_request::<E>(&slots)?;
                let format = state.date_times.get(&state.data, &request).map_err(|e| {
                    report(&format!("rong_quickjs_intl: {e}"));
                    bad(&e)
                })?;
                Ok(format
                    .fields
                    .iter()
                    .map(|(k, v)| vec![k.to_string(), v.clone()])
                    .collect())
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "dateTimeFormatParts",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>, x: f64| -> JSResult<Vec<Vec<String>>> {
                let state = state(&own)?;
                let request = date_time_request::<E>(&slots)?;
                let zone: String = slots.get("timeZone")?;
                let format = state.date_times.get(&state.data, &request).map_err(|e| {
                    report(&format!("rong_quickjs_intl: {e}"));
                    bad(&e)
                })?;
                let parts = date_time_format::format_to_parts(&state.data, &format, &zone, x)
                    .map_err(|e| bad(&e))?;
                Ok(parts
                    .into_iter()
                    .map(|(kind, value, _)| vec![kind.to_string(), value])
                    .collect())
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "dateTimeFormatRangeParts",
        JSFunc::<E::Value>::new(
            ctx,
            move |slots: JSObject<E::Value>, x: f64, y: f64| -> JSResult<Vec<Vec<String>>> {
                let state = state(&own)?;
                let request = date_time_request::<E>(&slots)?;
                let zone: String = slots.get("timeZone")?;
                let format = state.date_times.get(&state.data, &request).map_err(|e| {
                    report(&format!("rong_quickjs_intl: {e}"));
                    bad(&e)
                })?;
                let parts =
                    date_time_format::format_range_to_parts(&state.data, &format, &zone, x, y)
                        .map_err(|e| {
                            report(&format!("rong_quickjs_intl: {e}"));
                            bad(&e)
                        })?;
                Ok(parts
                    .into_iter()
                    .map(|(kind, value, source)| vec![kind.to_string(), value, source.to_string()])
                    .collect())
            },
        )?,
    )?;
    let own = ctx.clone();
    natives.set(
        "transformCase",
        JSFunc::<E::Value>::new(
            ctx,
            move |text: String,
                  requested: Option<String>,
                  upper: bool|
                  -> JSResult<Option<String>> {
                let state = state(&own)?;
                let tag = requested
                    .unwrap_or_else(|| state.settings.default_locale(state.data.canonicalizer()));
                Ok(transform_case(state, &text, &tag, upper))
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

/// A named zone with no tz database to answer it: an error naming the
/// cause, not `Invalid time zone`.
fn no_database(name: &str) -> JSResult<()> {
    match rong_quickjs_tz::canonical_zone(name) {
        Err(rong_quickjs_tz::TzError::NoDatabase(_)) => Err(no_database_error(name)),
        _ => Ok(()),
    }
}

fn no_database_error(what: &str) -> rong_core::RongJSError {
    bad(&rong_quickjs_tz::TzError::NoDatabase(what.to_string()).to_string())
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
        "NumberFormat" => data.has_locale::<icu::decimal::provider::DecimalSymbolsV1>(&locale),
        "RelativeTimeFormat" => {
            data.has_locale::<icu_experimental::relativetime::provider::DatetimeRelativeSecondLongV1>(
                &locale,
            )
        }
        "ListFormat" => {
            data.falls_back_into(&locale, &data.locales_any::<icu::list::provider::ListAndV1>())
        }
        "DateTimeFormat" => data.falls_back_into(&locale, &date_time_locales(data)),
        _ => false,
    }
}

/// The languages whose case mappings ICU4X tailors (SpecialCasing's lt, tr
/// and az, and CLDR's el and hy); every other locale maps as `und`.
const CASE_LOCALES: &[&str] = &["az", "el", "hy", "lt", "tr"];

/// TransformCase for a tailored language: `None` when `tag`
/// (LookupMatchingLocaleByPrefix over [`CASE_LOCALES`]) falls to `und`,
/// whose mapping is the engine's own default case conversion.
fn transform_case(state: &IntlState, text: &str, tag: &str, upper: bool) -> Option<String> {
    let language = tag.split('-').next().unwrap_or_default();
    if CASE_LOCALES.binary_search(&language).is_err() {
        return None;
    }
    let mapper = state.case_mapper.get_or_init(|| {
        icu::casemap::CaseMapper::try_new_unstable(&state.data.provider())
            .inspect_err(|e| {
                report(&format!(
                    "rong_quickjs_intl: the data blob has no case mapping data \
                     (CaseMapV1); {tag} cases as und: {e}"
                ))
            })
            .ok()
    });
    let mapper = mapper.as_ref()?.as_borrowed();
    let id: icu::locale::LanguageIdentifier = language.parse().ok()?;
    Some(if upper {
        mapper.uppercase_to_string(text, &id).into_owned()
    } else {
        mapper.lowercase_to_string(text, &id).into_owned()
    })
}

/// The locales with Gregorian date patterns, the base every calendar's
/// formatting needs.
fn date_time_locales(data: &IntlData) -> Vec<String> {
    data.locales_any::<icu::datetime::provider::semantic_skeletons::DatetimePatternsDateGregorianV1>()
}

/// [[LocaleData]] for `ca`, `nu` and `hc`.
struct DateTimeLocaleData<'a>(&'a IntlState);

/// The internal value of a null `hc`; matches the bootstrap's.
const NULL_HC: &str = "<null>";

impl locale::LocaleData for DateTimeLocaleData<'_> {
    fn values(&self, locale: &str, key: &str) -> Vec<String> {
        match key {
            "ca" => {
                let default = default_calendar(&self.0.data, locale);
                let mut list = vec![default.clone()];
                list.extend(
                    date_time_format::CALENDARS
                        .iter()
                        .filter(|c| **c != default)
                        .map(|c| c.to_string()),
                );
                list
            }
            "nu" => number_format::numbering_systems(&self.0.data, locale),
            "hc" => [NULL_HC, "h11", "h12", "h23", "h24"]
                .iter()
                .map(|v| v.to_string())
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// The locale's preferred calendar as ECMA-402 names it; gregory without
/// the blob's calendar preferences.
fn default_calendar(data: &IntlData, locale: &str) -> String {
    use icu::calendar::AnyCalendarKind;
    use icu::calendar::preferences::CalendarPreferences;
    let Ok(parsed) = locale.parse::<icu::locale::Locale>() else {
        return String::from("gregory");
    };
    let kind =
        AnyCalendarKind::try_new_unstable(&data.provider(), CalendarPreferences::from(&parsed))
            .unwrap_or(AnyCalendarKind::Gregorian);
    match kind {
        AnyCalendarKind::Buddhist => "buddhist",
        AnyCalendarKind::Chinese => "chinese",
        AnyCalendarKind::Coptic => "coptic",
        AnyCalendarKind::Dangi => "dangi",
        AnyCalendarKind::EthiopianAmeteAlem => "ethioaa",
        AnyCalendarKind::Ethiopian => "ethiopic",
        AnyCalendarKind::Hebrew => "hebrew",
        AnyCalendarKind::Indian => "indian",
        AnyCalendarKind::HijriTabularTypeIIFriday => "islamic-civil",
        AnyCalendarKind::HijriTabularTypeIIThursday => "islamic-tbla",
        AnyCalendarKind::HijriUmmAlQura => "islamic-umalqura",
        AnyCalendarKind::Iso => "iso8601",
        AnyCalendarKind::Japanese => "japanese",
        AnyCalendarKind::Persian => "persian",
        AnyCalendarKind::Roc => "roc",
        _ => "gregory",
    }
    .to_string()
}

/// CanonicalizeUValue for a key's option value.
fn canonical_u_value(key: &str, value: String) -> String {
    value_aliases::VALUE_ALIASES
        .iter()
        .find(|(k, alias, _)| *k == key && *alias == value)
        .map_or(value, |(_, _, canonical)| canonical.to_string())
}

/// The date-time request in a DateTimeFormat's slots.
fn date_time_request<E: JSEngine>(
    slots: &JSObject<E::Value>,
) -> JSResult<date_time_format::Request> {
    // A slot or option may be present and undefined.
    let text = |key: &str| -> JSResult<Option<String>> {
        Ok(slots.get_opt::<_, Option<String>>(key)?.flatten())
    };
    let options: JSObject<E::Value> = slots.get("formatOptions")?;
    let mut components = Vec::new();
    for key in [
        "weekday",
        "era",
        "year",
        "month",
        "day",
        "dayPeriod",
        "hour",
        "minute",
        "second",
        "fractionalSecondDigits",
        "timeZoneName",
    ] {
        if key == "fractionalSecondDigits" {
            if let Some(n) = options.get_opt::<_, Option<f64>>(key)?.flatten() {
                components.push((key.to_string(), (n as u8).to_string()));
            }
        } else if let Some(v) = options.get_opt::<_, Option<String>>(key)?.flatten() {
            components.push((key.to_string(), v));
        }
    }
    Ok(date_time_format::Request {
        data_locale: text("dataLocale")?.unwrap_or_default(),
        calendar: text("calendar")?.unwrap_or_else(|| String::from("gregory")),
        numbering_system: text("numberingSystem")?.unwrap_or_else(|| String::from("latn")),
        hour_cycle: text("hourCycle")?,
        date_style: text("dateStyle")?,
        time_style: text("timeStyle")?,
        components,
    })
}

/// CurrencyDigits: the currency's minor unit digits as CLDR records them
/// (ISO 4217's, as ICU's), 2 for a code it does not list.
fn currency_digits(data: &IntlData, code: &str) -> u8 {
    use icu_experimental::dimension::provider::currency::fractions::CurrencyFractionsV1;
    let response: Result<icu_provider::DataResponse<CurrencyFractionsV1>, _> =
        icu_provider::DataProvider::load(&data.provider(), Default::default());
    let Ok(response) = response else {
        static REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            report(
                "rong_quickjs_intl: the data blob has no currency fractions; currencies use 2 digits",
            );
        }
        return 2;
    };
    let fractions = response.payload.get();
    tinystr::TinyAsciiStr::<3>::try_from_str(code)
        .ok()
        .and_then(|c| fractions.fractions.get_copied(&c.to_unvalidated()))
        .unwrap_or(fractions.default)
        .digits
}

/// [[LocaleData]] for `nu`: the locale's own numbering system, then every
/// system with a simple digit mapping.
struct NumberingSystems<'a>(&'a IntlData);

impl locale::LocaleData for NumberingSystems<'_> {
    fn values(&self, locale: &str, key: &str) -> Vec<String> {
        match key {
            "nu" => number_format::numbering_systems(self.0, locale),
            _ => Vec::new(),
        }
    }
}

/// The locales the blob holds for a service's data, sorted.
fn service_locales(data: &IntlData, service: &str) -> Vec<String> {
    match service {
        "PluralRules" => data.locales::<icu::plurals::provider::PluralsCardinalV1>(),
        "NumberFormat" => data.locales::<icu::decimal::provider::DecimalSymbolsV1>(),
        "RelativeTimeFormat" => {
            data.locales::<icu_experimental::relativetime::provider::DatetimeRelativeSecondLongV1>()
        }
        "ListFormat" => data.locales_any::<icu::list::provider::ListAndV1>(),
        "DateTimeFormat" => date_time_locales(data),
        _ => Vec::new(),
    }
}

/// The data a NumberFormat's options need: the locale format, compact
/// patterns and the currency's or unit's affixes.
type NumberFormatData = (
    Rc<number_format::LocaleFormat>,
    Option<Rc<number_format::Compact>>,
    Option<Rc<number_format::Affixes>>,
);

fn number_format_data(
    state: &IntlState,
    options: &number_format::Options,
) -> JSResult<NumberFormatData> {
    let failed = |e: String| {
        report(&format!("rong_quickjs_intl: {e}"));
        bad(&e)
    };
    let format = state
        .numbers
        .get(
            &state.data,
            &options.locale,
            &options.numbering_system,
            options.style,
        )
        .map_err(failed)?;
    let compact = match options.notation {
        plural::Notation::CompactShort | plural::Notation::CompactLong => Some(
            state
                .numbers
                .compact(
                    &state.data,
                    &options.locale,
                    options.notation == plural::Notation::CompactLong,
                )
                .map_err(failed)?,
        ),
        _ => None,
    };
    let measure = match &options.measure {
        Some(measure) => Some(
            state
                .numbers
                .measure(&state.data, &format, options, measure)
                .map_err(failed)?,
        ),
        None => None,
    };
    Ok((format, compact, measure))
}

/// The slots SetNumberFormatDigitOptions fills, as the bootstrap keeps them.
fn digit_options<E: JSEngine>(slots: &JSObject<E::Value>) -> JSResult<number::DigitOptions> {
    let text = |key: &str| -> JSResult<String> { slots.get::<_, String>(key) };
    // Digit slots SetNumberFormatDigitOptions leaves unset are absent, not undefined.
    let count = |key: &str| -> JSResult<u8> {
        let n: Option<f64> = slots.get_opt(key)?;
        Ok(n.unwrap_or(0.0) as u8)
    };
    Ok(number::DigitOptions {
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
    })
}

/// An Intl.PluralRules' slots, as the bootstrap keeps them.
fn plural_options<E: JSEngine>(slots: &JSObject<E::Value>) -> JSResult<plural::Options> {
    let text = |key: &str| -> JSResult<String> { slots.get::<_, String>(key) };
    let kind = text("type")?;
    let notation = text("notation")?;
    let compact_display: Option<String> = slots.get_opt("compactDisplay")?;
    Ok(plural::Options {
        locale: text("locale")?,
        ordinal: kind == "ordinal",
        notation: plural::Notation::parse(&notation, compact_display.as_deref().unwrap_or("short"))
            .ok_or_else(|| bad("unknown notation"))?,
        digits: digit_options::<E>(slots)?,
    })
}

/// An Intl.NumberFormat's slots, as the bootstrap keeps them.
fn number_format_options<E: JSEngine>(
    slots: &JSObject<E::Value>,
) -> JSResult<number_format::Options> {
    use number_format::{CurrencyDisplay, Grouping, Measure, SignDisplay, Style, UnitDisplay};
    let text = |key: &str| -> JSResult<String> { slots.get::<_, String>(key) };
    let (style, measure) = match text("style")?.as_str() {
        "percent" => (Style::Percent, None),
        "currency" => (
            Style::Decimal,
            Some(Measure::Currency {
                code: text("currency")?,
                display: match text("currencyDisplay")?.as_str() {
                    "code" => CurrencyDisplay::Code,
                    "narrowSymbol" => CurrencyDisplay::NarrowSymbol,
                    "name" => CurrencyDisplay::Name,
                    _ => CurrencyDisplay::Symbol,
                },
                accounting: text("currencySign")? == "accounting",
            }),
        ),
        "unit" => (
            Style::Decimal,
            Some(Measure::Unit {
                unit: text("unit")?,
                display: match text("unitDisplay")?.as_str() {
                    "long" => UnitDisplay::Long,
                    "narrow" => UnitDisplay::Narrow,
                    _ => UnitDisplay::Short,
                },
            }),
        ),
        _ => (Style::Decimal, None),
    };
    let compact_display: Option<String> = slots
        .get_opt::<_, Option<String>>("compactDisplay")?
        .flatten();
    let notation = plural::Notation::parse(
        &text("notation")?,
        compact_display.as_deref().unwrap_or("short"),
    )
    .ok_or_else(|| bad("unknown notation"))?;
    // [[UseGrouping]] is a string or false.
    let grouping: Option<String> = slots.get_opt::<_, String>("useGrouping").ok().flatten();
    let grouping = match grouping.as_deref() {
        Some("always") => Grouping::Always,
        Some("auto") => Grouping::Auto,
        Some("min2") => Grouping::Min2,
        _ => Grouping::Off,
    };
    let sign = match text("signDisplay")?.as_str() {
        "auto" => SignDisplay::Auto,
        "never" => SignDisplay::Never,
        "always" => SignDisplay::Always,
        "exceptZero" => SignDisplay::ExceptZero,
        "negative" => SignDisplay::Negative,
        _ => return Err(bad("unknown signDisplay")),
    };
    Ok(number_format::Options {
        locale: text("dataLocale")?,
        numbering_system: text("numberingSystem")?,
        style,
        measure,
        notation,
        digits: digit_options::<E>(slots)?,
        grouping,
        sign,
    })
}
