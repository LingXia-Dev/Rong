use icu::locale::LocaleCanonicalizer;
use icu::locale::fallback::LocaleFallbacker;
use icu_provider::buf::{AsDeserializingBufferProvider, DeserializingBufferProvider};
use icu_provider::prelude::*;
use icu_provider_blob::BlobDataProvider;
use rong_core::{HostError, JSResult, RongJSError};
use std::rc::Rc;

/// An app's ICU4X locale data, loaded once from a data blob made by
/// `icu4x-datagen --format blob`. Cheap to clone: contexts of one app share it.
#[derive(Clone)]
pub struct IntlData(Rc<Inner>);

struct Inner {
    blob: BlobDataProvider,
    canonicalizer: LocaleCanonicalizer,
    fallbacker: LocaleFallbacker,
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

    fn new(blob: BlobDataProvider) -> JSResult<Self> {
        let canonicalizer =
            LocaleCanonicalizer::try_new_extended_unstable(&blob.as_deserializing())
                .map_err(invalid)?;
        let fallbacker =
            LocaleFallbacker::try_new_unstable(&blob.as_deserializing()).map_err(invalid)?;
        Ok(Self(Rc::new(Inner {
            blob,
            canonicalizer,
            fallbacker,
        })))
    }

    pub(crate) fn canonicalizer(&self) -> &LocaleCanonicalizer {
        &self.0.canonicalizer
    }

    /// The blob with locale fallback, for the ICU4X constructors.
    pub(crate) fn provider(&self) -> Provider<'_> {
        Provider {
            blob: &self.0.blob,
            fallbacker: &self.0.fallbacker,
        }
    }

    /// Whether `M` has data for `locale` itself or a locale it falls back to
    /// short of the root: the [[AvailableLocales]] test for a service.
    pub(crate) fn has_locale<M: DataMarker>(&self, locale: &icu::locale::Locale) -> bool
    where
        for<'a> DeserializingBufferProvider<'a, BlobDataProvider>: DataProvider<M>,
    {
        let locale = DataLocale::from(locale);
        let request = DataRequest {
            id: DataIdentifierBorrowed::for_locale(&locale),
            ..Default::default()
        };
        DataProvider::<M>::load(&self.provider(), request)
            .is_ok_and(|res| res.metadata.locale.is_some_and(|l| !l.is_unknown()))
    }
}

/// The blob behind locale fallback: a request for `en-GB` is answered from
/// `en` when the blob holds no `en-GB` of its own. The response's metadata
/// names the locale that answered.
pub(crate) struct Provider<'a> {
    blob: &'a BlobDataProvider,
    fallbacker: &'a LocaleFallbacker,
}

impl<M: DataMarker> DataProvider<M> for Provider<'_>
where
    for<'a> DeserializingBufferProvider<'a, BlobDataProvider>: DataProvider<M>,
{
    fn load(&self, req: DataRequest) -> Result<DataResponse<M>, DataError> {
        let blob = self.blob.as_deserializing();
        if M::INFO.is_singleton {
            return DataProvider::<M>::load(&blob, req);
        }
        let mut chain = self
            .fallbacker
            .for_config(M::INFO.fallback_config)
            .fallback_for(*req.id.locale);
        loop {
            let id = DataIdentifierBorrowed::for_marker_attributes_and_locale(
                req.id.marker_attributes,
                chain.get(),
            );
            let mut metadata = req.metadata;
            metadata.silent = true;
            match DataProvider::<M>::load(&blob, DataRequest { id, metadata }) {
                Ok(mut response) => {
                    response.metadata.locale = Some(*chain.get());
                    return Ok(response);
                }
                Err(e) if e.kind == DataErrorKind::IdentifierNotFound => {}
                Err(e) => return Err(e.with_req(M::INFO, req)),
            }
            if chain.get().is_unknown() {
                return Err(DataErrorKind::IdentifierNotFound.with_req(M::INFO, req));
            }
            chain.step();
        }
    }
}

fn invalid(e: impl std::fmt::Display) -> RongJSError {
    HostError::new(
        rong_core::error::E_INVALID_ARG,
        format!("Intl data blob cannot be used: {e}"),
    )
    .into()
}
