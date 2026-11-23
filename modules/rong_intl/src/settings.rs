use icu::locale::LocaleCanonicalizer;
use std::cell::{Cell, RefCell};

/// The locale used when the settings name none or an invalid one.
pub(crate) const FALLBACK_LOCALE: &str = "en-US";

/// The embedder's defaults for `Intl` on engines without one of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntlSettings {
    /// The default locale as a BCP 47 tag, such as `"he-IL"`. `None`, or a tag
    /// that is not structurally valid, means `en-US`.
    pub locale: Option<String>,
    /// The default time zone as an IANA name. `None` means the system's zone.
    pub time_zone: Option<String>,
    /// The default hour cycle. `None` means the locale's own.
    pub hour_cycle: Option<HourCycle>,
}

/// A Unicode hour cycle (`hc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HourCycle {
    H11,
    H12,
    H23,
    H24,
}

/// The settings with a generation that advances on every change, so caches
/// keyed on it see the change without comparing the settings themselves.
pub(crate) struct SettingsRecord {
    settings: RefCell<IntlSettings>,
    generation: Cell<u64>,
}

impl SettingsRecord {
    pub(crate) fn new(settings: IntlSettings) -> Self {
        Self {
            settings: RefCell::new(settings),
            generation: Cell::new(1),
        }
    }

    pub(crate) fn get(&self) -> IntlSettings {
        self.settings.borrow().clone()
    }

    pub(crate) fn replace(&self, settings: IntlSettings) {
        *self.settings.borrow_mut() = settings;
        self.generation.set(self.generation.get() + 1);
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.get()
    }

    /// ECMA-402 DefaultLocale: the settings' locale, canonicalized, without
    /// its Unicode extension.
    pub(crate) fn default_locale(&self, canonicalizer: &LocaleCanonicalizer) -> String {
        self.settings
            .borrow()
            .locale
            .as_deref()
            .and_then(|tag| crate::locale::canonicalize(canonicalizer, tag))
            .map(|tag| crate::locale::remove_unicode_extension(&tag).into_owned())
            .unwrap_or_else(|| String::from(FALLBACK_LOCALE))
    }
}
