//! Intl.ListFormat's CreatePartsFromList over ICU4X's `ListFormatter`.

use crate::data::IntlData;
use icu::list::options::{ListFormatterOptions, ListLength};
use icu::list::{ListFormatter, parts};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use writeable::{PartsWrite, Writeable};

/// ICU4X formatters per context, built on first use.
#[derive(Default)]
pub(crate) struct Cache {
    formatters: RefCell<HashMap<(String, String, String), Rc<ListFormatter>>>,
}

impl Cache {
    fn get(
        &self,
        data: &IntlData,
        locale: &str,
        kind: &str,
        style: &str,
    ) -> Result<Rc<ListFormatter>, String> {
        let key = (locale.to_string(), kind.to_string(), style.to_string());
        if let Some(found) = self.formatters.borrow().get(&key) {
            return Ok(found.clone());
        }
        let parsed = locale
            .parse::<icu::locale::Locale>()
            .map_err(|_| format!("{locale} is not a locale"))?;
        let options = ListFormatterOptions::default().with_length(match style {
            "short" => ListLength::Short,
            "narrow" => ListLength::Narrow,
            _ => ListLength::Wide,
        });
        let provider = data.provider();
        let prefs = (&parsed).into();
        let built = match kind {
            "disjunction" => ListFormatter::try_new_or_unstable(&provider, prefs, options),
            "unit" => ListFormatter::try_new_unit_unstable(&provider, prefs, options),
            _ => ListFormatter::try_new_and_unstable(&provider, prefs, options),
        }
        .map_err(|_| {
            format!("the data blob has no {kind} list patterns for {locale} or its fallbacks")
        })?;
        let built = Rc::new(built);
        self.formatters.borrow_mut().insert(key, built.clone());
        Ok(built)
    }
}

/// Collects the formatter's output with the spans of its elements.
#[derive(Default)]
struct Collector {
    text: String,
    elements: Vec<(usize, usize)>,
}

impl fmt::Write for Collector {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.text.push_str(s);
        Ok(())
    }
}

impl PartsWrite for Collector {
    type SubPartsWrite = Self;
    fn with_part(
        &mut self,
        part: writeable::Part,
        mut f: impl FnMut(&mut Self::SubPartsWrite) -> fmt::Result,
    ) -> fmt::Result {
        let start = self.text.len();
        let result = f(self);
        if part == parts::ELEMENT {
            self.elements.push((start, self.text.len()));
        }
        result
    }
}

/// CreatePartsFromList: `element` parts for the strings, `literal` parts
/// for the pattern text between them.
pub(crate) fn format_to_parts(
    cache: &Cache,
    data: &IntlData,
    locale: &str,
    kind: &str,
    style: &str,
    list: &[String],
) -> Result<Vec<(&'static str, String)>, String> {
    let formatter = cache.get(data, locale, kind, style)?;
    let mut sink = Collector::default();
    formatter
        .format(list.iter().map(String::as_str))
        .write_to_parts(&mut sink)
        .map_err(|_| String::from("list formatting failed"))?;
    let mut out = Vec::new();
    let mut at = 0;
    for (start, end) in sink.elements {
        if start > at {
            out.push(("literal", sink.text[at..start].to_string()));
        }
        out.push(("element", sink.text[start..end].to_string()));
        at = end;
    }
    if at < sink.text.len() {
        out.push(("literal", sink.text[at..].to_string()));
    }
    Ok(out)
}
