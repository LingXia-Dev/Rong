//! Uses exactly the locale services phase 0 needs, so datagen's
//! `--markers-for-bin` reads their markers from this binary.
use icu::locale::{LocaleCanonicalizer, LocaleExpander, fallback::LocaleFallbacker};
use icu_provider::buf::AsDeserializingBufferProvider;
use icu_provider_blob::BlobDataProvider;

fn main() {
    let bytes = std::fs::read(std::env::args().nth(1).expect("blob path")).unwrap();
    let p = BlobDataProvider::try_new_from_blob(bytes.into_boxed_slice()).unwrap();
    let c = LocaleCanonicalizer::try_new_extended_unstable(&p.as_deserializing()).unwrap();
    let e = LocaleExpander::try_new_extended_unstable(&p.as_deserializing()).unwrap();
    let f = LocaleFallbacker::try_new_unstable(&p.as_deserializing()).unwrap();
    for tag in [
        "iw",
        "en-u-kn-true",
        "sgn-GR",
        "art-lojban",
        "zh-hans-cn",
        "und-Latn-RS",
        "cmn-TW",
    ] {
        let mut l: icu::locale::Locale = tag.parse().unwrap();
        c.canonicalize(&mut l);
        let mut m = l.clone();
        e.maximize(&mut m.id);
        let chain: Vec<String> = {
            let mut it = f
                .for_config(Default::default())
                .fallback_for(l.clone().into());
            let mut v = Vec::new();
            for _ in 0..6 {
                v.push(it.get().to_string());
                if it.get().is_unknown() {
                    break;
                }
                it.step();
            }
            v
        };
        println!("LOCALE {tag} -> {l} max {m} chain {chain:?}");
    }
}
