//! ECMA-402 locale operations on language tags that are already strings:
//! the JS side does the observable coercions and calls in here.

// Resolution and supported-locale lookup serve the constructors.
#![cfg_attr(not(test), allow(dead_code))]

use icu::locale::{Locale, LocaleCanonicalizer};
use std::borrow::Cow;

/// ECMA-402 IsStructurallyValidLanguageTag: a `unicode_locale_id` without the
/// backwards-compatible syntax (`_`, `root`, a leading script) and without a
/// repeated variant or singleton.
pub(crate) fn is_structurally_valid(tag: &str) -> bool {
    if tag.is_empty() || !tag.is_ascii() || tag.contains('_') {
        return false;
    }
    let subtags: Vec<&str> = tag.split('-').collect();
    let language = subtags[0];
    if !(matches!(language.len(), 2 | 3 | 5..=8)
        && language.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return false;
    }
    let mut variants: Vec<String> = Vec::new();
    let mut singletons: Vec<u8> = Vec::new();
    let mut in_extension = false;
    for subtag in &subtags[1..] {
        if subtag.len() == 1 {
            let singleton = subtag.as_bytes()[0].to_ascii_lowercase();
            if singletons.contains(&singleton) {
                return false;
            }
            singletons.push(singleton);
            in_extension = true;
            if singleton == b'x' {
                break;
            }
            continue;
        }
        // Variants come before any extension: 5-8 alphanumerics, or a digit
        // and three alphanumerics.
        let is_variant = (subtag.len() >= 5 && subtag.len() <= 8)
            || (subtag.len() == 4 && subtag.as_bytes()[0].is_ascii_digit());
        if !in_extension && is_variant {
            let variant = subtag.to_ascii_lowercase();
            if variants.contains(&variant) {
                return false;
            }
            variants.push(variant);
        }
    }
    parse(tag).is_some()
}

/// Parses `tag`, holding a 5-8 letter language subtag (valid in BCP 47,
/// beyond what ICU4X represents) aside as `und`: alias rules for `und`
/// apply to any language, so canonicalizing that way keeps their meaning.
fn parse(tag: &str) -> Option<(Locale, Option<String>)> {
    let language = tag.split('-').next()?;
    if language.len() < 5 {
        return Some((Locale::try_from_str(tag).ok()?, None));
    }
    let rest = &tag[language.len()..];
    let locale = Locale::try_from_str(&format!("und{rest}")).ok()?;
    Some((locale, Some(language.to_ascii_lowercase())))
}

/// ECMA-402 CanonicalizeUnicodeLocaleId, or `None` when `tag` is not
/// structurally valid.
pub(crate) fn canonicalize(canonicalizer: &LocaleCanonicalizer, tag: &str) -> Option<String> {
    if !is_structurally_valid(tag) {
        return None;
    }
    let (mut locale, long_language) = parse(tag)?;
    canonicalizer.canonicalize(&mut locale);
    let mut canonical = locale.to_string();
    if let Some(language) = long_language
        && let Some(rest) = canonical.strip_prefix("und")
    {
        canonical = format!("{language}{rest}");
    }
    Some(replace_value_aliases(&canonical))
}

/// UTS 35's last canonicalization step, which ICU4X leaves out: replace
/// deprecated or aliased `-u-` keyword and `-t-` field values with their
/// preferred form, and drop a `-u-` value of `true`.
fn replace_value_aliases(tag: &str) -> String {
    let exts = extensions(tag);
    if !exts.iter().any(|(s, _, _)| *s == b'u' || *s == b't') {
        return tag.to_string();
    }
    let mut out = String::with_capacity(tag.len());
    let mut copied = 0;
    for (singleton, start, end) in exts {
        if singleton != b'u' && singleton != b't' {
            continue;
        }
        out.push_str(&tag[copied..start]);
        out.push('-');
        out.push(singleton as char);
        let is_key = |sub: &str| {
            let b = sub.as_bytes();
            b.len() == 2
                && if singleton == b'u' {
                    b[1].is_ascii_alphabetic()
                } else {
                    b[0].is_ascii_alphabetic() && b[1].is_ascii_digit()
                }
        };
        let mut key: Option<&str> = None;
        let mut value: Vec<&str> = Vec::new();
        let flush = |out: &mut String, key: Option<&str>, value: &mut Vec<&str>| {
            if let Some(key) = key {
                out.push('-');
                out.push_str(key);
                let joined = value.join("-");
                let mapped = crate::value_aliases::VALUE_ALIASES
                    .iter()
                    .find(|(k, from, _)| *k == key && *from == joined)
                    .map_or(joined.as_str(), |(_, _, to)| to);
                if !mapped.is_empty() && !(singleton == b'u' && mapped == "true") {
                    out.push('-');
                    out.push_str(mapped);
                }
            }
            value.clear();
        };
        for sub in tag[start..end].split('-').skip(2) {
            if is_key(sub) {
                flush(&mut out, key, &mut value);
                key = Some(sub);
            } else if key.is_some() {
                value.push(sub);
            } else {
                out.push('-');
                out.push_str(sub);
            }
        }
        flush(&mut out, key, &mut value);
        copied = end;
    }
    out.push_str(&tag[copied..]);
    out
}

/// The singleton subtags of a canonical tag, with the byte range each
/// extension covers (from its leading `-`). Stops at a private-use `x`.
fn extensions(tag: &str) -> Vec<(u8, usize, usize)> {
    let mut found: Vec<(u8, usize, usize)> = Vec::new();
    let mut start = 0;
    for (i, subtag) in tag.split('-').enumerate() {
        let at = start;
        start += subtag.len() + 1;
        if i == 0 || subtag.len() != 1 {
            continue;
        }
        if let Some(last) = found.last_mut() {
            last.2 = at.saturating_sub(1);
        }
        let singleton = subtag.as_bytes()[0].to_ascii_lowercase();
        found.push((singleton, at.saturating_sub(1), tag.len()));
        if singleton == b'x' {
            break;
        }
    }
    found
}

/// The tag without its Unicode (`-u-`) extension.
pub(crate) fn remove_unicode_extension(tag: &str) -> Cow<'_, str> {
    match extensions(tag).into_iter().find(|(s, _, _)| *s == b'u') {
        Some((_, start, end)) => Cow::Owned(format!("{}{}", &tag[..start], &tag[end..])),
        None => Cow::Borrowed(tag),
    }
}

/// The tag's Unicode extension, from its leading `-u-`.
pub(crate) fn unicode_extension(tag: &str) -> Option<&str> {
    extensions(tag)
        .into_iter()
        .find(|(s, _, _)| *s == b'u')
        .map(|(_, start, end)| &tag[start..end])
}

/// ECMA-402 BestAvailableLocale.
pub(crate) fn best_available_locale(
    available: &dyn Fn(&str) -> bool,
    locale: &str,
) -> Option<String> {
    let mut candidate = locale;
    loop {
        if available(candidate) {
            return Some(candidate.to_string());
        }
        let mut pos = candidate.rfind('-')?;
        if pos >= 2 && candidate.as_bytes()[pos - 2] == b'-' {
            pos -= 2;
        }
        candidate = &candidate[..pos];
    }
}

/// The result of a locale matcher: the available locale and the Unicode
/// extension of the request it matched, if any.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Matched {
    pub locale: String,
    pub extension: Option<String>,
}

/// ECMA-402 LookupMatcher. The best-fit matcher is this one as well: the
/// standard leaves best fit to the implementation and lookup is one.
pub(crate) fn lookup_matcher(
    available: &dyn Fn(&str) -> bool,
    requested: &[String],
    default_locale: &str,
) -> Matched {
    for locale in requested {
        let no_extension = remove_unicode_extension(locale);
        if let Some(found) = best_available_locale(available, &no_extension) {
            return Matched {
                locale: found,
                extension: unicode_extension(locale).map(str::to_string),
            };
        }
    }
    Matched {
        locale: default_locale.to_string(),
        extension: None,
    }
}

/// ECMA-402 LookupSupportedLocales.
pub(crate) fn supported_locales(
    available: &dyn Fn(&str) -> bool,
    requested: &[String],
) -> Vec<String> {
    requested
        .iter()
        .filter(|locale| {
            best_available_locale(available, &remove_unicode_extension(locale)).is_some()
        })
        .cloned()
        .collect()
}

/// A `-u-` keyword: key and value (`""` for a bare key, meaning `true`).
type Keyword = (String, String);

/// ECMA-402 UnicodeExtensionComponents: attributes and keywords of `-u-...`,
/// keeping the first of a repeated key.
fn unicode_extension_components(extension: &str) -> (Vec<String>, Vec<Keyword>) {
    let mut attributes = Vec::new();
    let mut keywords: Vec<Keyword> = Vec::new();
    let mut current: Option<Keyword> = None;
    for subtag in extension.split('-').skip(2) {
        if subtag.len() == 2 {
            if let Some(keyword) = current.take()
                && !keywords.iter().any(|(k, _)| *k == keyword.0)
            {
                keywords.push(keyword);
            }
            current = Some((subtag.to_string(), String::new()));
        } else if let Some((_, value)) = current.as_mut() {
            if !value.is_empty() {
                value.push('-');
            }
            value.push_str(subtag);
        } else if !attributes.iter().any(|a| a == subtag) {
            attributes.push(subtag.to_string());
        }
    }
    if let Some(keyword) = current
        && !keywords.iter().any(|(k, _)| *k == keyword.0)
    {
        keywords.push(keyword);
    }
    (attributes, keywords)
}

/// What a service supports for each relevant extension key: the values a
/// locale accepts, its default first.
pub(crate) trait LocaleData {
    fn values(&self, locale: &str, key: &str) -> Vec<String>;
}

/// The locale data of a service without relevant extension keys.
pub(crate) struct NoKeys;

impl LocaleData for NoKeys {
    fn values(&self, _locale: &str, _key: &str) -> Vec<String> {
        Vec::new()
    }
}

/// ECMA-402 ResolveLocale's result: the locale with the keywords that took
/// effect, and the value of each relevant key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub locale: String,
    pub data_locale: String,
    pub values: Vec<(String, Option<String>)>,
}

/// ECMA-402 ResolveLocale. `options` holds each relevant key's option value
/// as the constructor read it (already ASCII-lowercased and canonicalized
/// by the caller where the standard says so); a key without an entry or
/// with `None` was not given.
pub(crate) fn resolve_locale(
    canonicalizer: &LocaleCanonicalizer,
    available: &dyn Fn(&str) -> bool,
    requested: &[String],
    options: &[(&str, Option<String>)],
    relevant_keys: &[&str],
    data: &dyn LocaleData,
    default_locale: &str,
) -> Resolved {
    let matched = lookup_matcher(available, requested, default_locale);
    let found = matched.locale;
    let (attributes, keywords) = matched
        .extension
        .as_deref()
        .map(unicode_extension_components)
        .unwrap_or_default();
    let mut supported: Vec<Keyword> = Vec::new();
    let mut values = Vec::new();
    for &key in relevant_keys {
        let key_data = data.values(&found, key);
        let mut value = key_data.first().cloned();
        let mut supported_keyword: Option<Keyword> = None;
        if let Some((_, requested_value)) = keywords.iter().find(|(k, _)| k == key) {
            if !requested_value.is_empty() {
                if key_data.contains(requested_value) {
                    value = Some(requested_value.clone());
                    supported_keyword = Some((key.to_string(), requested_value.clone()));
                }
            } else if key_data.iter().any(|v| v == "true") {
                value = Some(String::from("true"));
                supported_keyword = Some((key.to_string(), String::new()));
            }
        }
        if let Some((_, Some(option))) = options.iter().find(|(k, _)| *k == key) {
            let option = if option.is_empty() {
                "true"
            } else {
                option.as_str()
            };
            if value.as_deref() != Some(option) && key_data.iter().any(|v| v == option) {
                value = Some(option.to_string());
                supported_keyword = None;
            }
        }
        values.push((key.to_string(), value));
        supported.extend(supported_keyword);
    }
    let locale = if supported.is_empty() && attributes.is_empty() {
        found.clone()
    } else {
        insert_unicode_extension(canonicalizer, &found, &attributes, &supported)
    };
    Resolved {
        locale,
        data_locale: found,
        values,
    }
}

/// ECMA-402 InsertUnicodeExtensionAndCanonicalize.
fn insert_unicode_extension(
    canonicalizer: &LocaleCanonicalizer,
    locale: &str,
    attributes: &[String],
    keywords: &[Keyword],
) -> String {
    let mut extension = String::from("-u");
    for attribute in attributes {
        extension.push('-');
        extension.push_str(attribute);
    }
    for (key, value) in keywords {
        extension.push('-');
        extension.push_str(key);
        if !value.is_empty() {
            extension.push('-');
            extension.push_str(value);
        }
    }
    if extension.len() == 2 {
        return locale.to_string();
    }
    // Before any private use, after the other extensions.
    let insert_at = extensions(locale)
        .into_iter()
        .find(|(s, _, _)| *s == b'x')
        .map_or(locale.len(), |(_, start, _)| start);
    let tag = format!(
        "{}{}{}",
        &locale[..insert_at],
        extension,
        &locale[insert_at..]
    );
    canonicalize(canonicalizer, &tag).unwrap_or(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(locales: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |l| locales.contains(&l)
    }

    #[test]
    fn structural_validity() {
        for tag in [
            "en",
            "en-US",
            "de-1996",
            "sl-rozaj-biske",
            "en-u-ca-gregory",
            "und-x-private",
        ] {
            assert!(is_structurally_valid(tag), "{tag}");
        }
        for tag in [
            "",
            "en_US",
            "root",
            "Latn-DE",
            "i-klingon",
            "x-private",
            "de-gregory-gregory",
            "de-u-ca-gregory-u-nu-latn",
            "en-a-bar-a-baz",
            "é",
        ] {
            assert!(!is_structurally_valid(tag), "{tag}");
        }
        // A repeated singleton inside private use is private use.
        assert!(is_structurally_valid("en-x-u-u"));
    }

    #[test]
    fn value_aliases_are_replaced() {
        assert_eq!(
            replace_value_aliases("und-u-ca-ethiopic-amete-alem"),
            "und-u-ca-ethioaa"
        );
        assert_eq!(replace_value_aliases("und-u-kb-yes"), "und-u-kb");
        assert_eq!(
            replace_value_aliases("und-u-ks-primary-ms-imperial"),
            "und-u-ks-level1-ms-uksystem"
        );
        assert_eq!(
            replace_value_aliases("und-Latn-t-und-hani-m0-names"),
            "und-Latn-t-und-hani-m0-prprname"
        );
        assert_eq!(
            replace_value_aliases("en-u-attr-ca-gregory-x-u-ks-primary"),
            "en-u-attr-ca-gregory-x-u-ks-primary"
        );
    }

    #[test]
    fn extensions_split_at_singletons() {
        assert_eq!(
            remove_unicode_extension("de-u-co-phonebk-x-private"),
            "de-x-private"
        );
        assert_eq!(
            remove_unicode_extension("en-a-foo-u-ca-gregory-z-bar"),
            "en-a-foo-z-bar"
        );
        assert_eq!(
            unicode_extension("en-a-foo-u-ca-gregory-z-bar"),
            Some("-u-ca-gregory")
        );
        assert_eq!(unicode_extension("en-x-u-ca"), None);
        assert_eq!(remove_unicode_extension("en-US"), "en-US");
    }

    #[test]
    fn best_available_drops_subtags_from_the_end() {
        let available = set(&["zh", "de-CH"]);
        assert_eq!(
            best_available_locale(&available, "zh-Hant-TW"),
            Some("zh".into())
        );
        assert_eq!(
            best_available_locale(&available, "de-CH-1996"),
            Some("de-CH".into())
        );
        assert_eq!(best_available_locale(&available, "fr-FR"), None);
        // A singleton is dropped with the subtag after it.
        let available = set(&["en"]);
        assert_eq!(
            best_available_locale(&available, "en-a-foo"),
            Some("en".into())
        );
    }

    #[test]
    fn lookup_keeps_the_extension_of_the_request_it_matched() {
        let available = set(&["en", "he"]);
        let requested = vec![
            "fr".to_string(),
            "he-IL-u-nu-hebr".to_string(),
            "en".to_string(),
        ];
        assert_eq!(
            lookup_matcher(&available, &requested, "en"),
            Matched {
                locale: "he".into(),
                extension: Some("-u-nu-hebr".into())
            }
        );
        assert_eq!(
            lookup_matcher(&available, &["fr".into()], "en").locale,
            "en"
        );
        assert_eq!(
            supported_locales(&available, &requested),
            vec!["he-IL-u-nu-hebr".to_string(), "en".to_string()]
        );
    }

    #[test]
    fn extension_components_keep_the_first_of_a_key() {
        let (attributes, keywords) =
            unicode_extension_components("-u-attr-ca-gregory-kn-ca-buddhist-co");
        assert_eq!(attributes, vec!["attr".to_string()]);
        assert_eq!(
            keywords,
            vec![
                ("ca".into(), "gregory".into()),
                ("kn".into(), String::new()),
                ("co".into(), String::new())
            ]
        );
    }

    struct Calendars;

    impl LocaleData for Calendars {
        fn values(&self, locale: &str, key: &str) -> Vec<String> {
            match (locale, key) {
                ("th", "ca") => vec!["buddhist".into(), "gregory".into()],
                (_, "ca") => vec!["gregory".into(), "buddhist".into()],
                (_, "kn") => vec!["false".into(), "true".into()],
                _ => Vec::new(),
            }
        }
    }

    #[test]
    fn resolve_locale_takes_supported_keywords_and_options() {
        let data = crate::IntlData::try_from_static_blob(include_bytes!(
            "../tests/data/intl-test.postcard"
        ))
        .unwrap();
        let c = data.canonicalizer();
        let available = set(&["en", "th", "he"]);
        let keys = ["ca", "kn"];
        let resolve = |requested: &[&str], options: &[(&str, Option<String>)]| {
            let requested: Vec<String> = requested.iter().map(|s| s.to_string()).collect();
            resolve_locale(c, &available, &requested, options, &keys, &Calendars, "en")
        };
        // A supported keyword stays in the locale; an unsupported one is dropped.
        let r = resolve(&["th-TH-u-ca-gregory-nu-thai"], &[]);
        assert_eq!(r.locale, "th-u-ca-gregory");
        assert_eq!(r.data_locale, "th");
        assert_eq!(r.values[0], ("ca".to_string(), Some("gregory".to_string())));
        // A bare key means true.
        let r = resolve(&["en-u-kn"], &[]);
        assert_eq!(r.locale, "en-u-kn");
        assert_eq!(r.values[1], ("kn".to_string(), Some("true".to_string())));
        // An option that differs overrides the keyword and drops it from the locale.
        let r = resolve(&["th-u-ca-gregory"], &[("ca", Some("buddhist".into()))]);
        assert_eq!(r.locale, "th");
        assert_eq!(r.values[0].1.as_deref(), Some("buddhist"));
        // Nothing available falls back to the default locale and its defaults.
        let r = resolve(&["fr"], &[]);
        assert_eq!(
            (r.locale.as_str(), r.values[0].1.as_deref()),
            ("en", Some("gregory"))
        );
    }
}
