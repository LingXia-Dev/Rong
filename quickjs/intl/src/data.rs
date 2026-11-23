use icu::locale::LocaleCanonicalizer;
use icu_provider::buf::AsDeserializingBufferProvider;
use icu_provider_blob::BlobDataProvider;
use rong_core::{HostError, JSResult, RongJSError};
use std::rc::Rc;

/// An app's ICU4X locale data, loaded once from a data blob made by
/// `icu4x-datagen --format blob`. Cheap to clone: contexts of one app share it.
#[derive(Clone)]
pub struct IntlData(Rc<Inner>);

struct Inner {
    provider: BlobDataProvider,
    canonicalizer: LocaleCanonicalizer,
}

impl IntlData {
    /// Loads a blob owned by the caller.
    pub fn try_from_blob(blob: Box<[u8]>) -> JSResult<Self> {
        let provider = BlobDataProvider::try_new_from_blob(blob).map_err(invalid)?;
        Self::new(provider)
    }

    /// Loads a blob that lives for the whole program, such as one included
    /// with `include_bytes!`, without copying it.
    pub fn try_from_static_blob(blob: &'static [u8]) -> JSResult<Self> {
        let provider = BlobDataProvider::try_new_from_static_blob(blob).map_err(invalid)?;
        Self::new(provider)
    }

    fn new(provider: BlobDataProvider) -> JSResult<Self> {
        let canonicalizer =
            LocaleCanonicalizer::try_new_extended_unstable(&provider.as_deserializing())
                .map_err(invalid)?;
        Ok(Self(Rc::new(Inner {
            provider,
            canonicalizer,
        })))
    }

    pub(crate) fn canonicalizer(&self) -> &LocaleCanonicalizer {
        &self.0.canonicalizer
    }

    #[allow(dead_code)] // Read by the constructors from phase 1 on.
    pub(crate) fn provider(&self) -> &BlobDataProvider {
        &self.0.provider
    }
}

fn invalid(e: impl std::fmt::Display) -> RongJSError {
    HostError::new(
        rong_core::error::E_INVALID_ARG,
        format!("Intl data blob cannot be used: {e}"),
    )
    .into()
}
