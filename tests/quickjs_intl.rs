#![cfg(feature = "intl")]
//! The `intl` feature gives a QuickJS context the standard `Intl`: data from
//! `rong::intl`, installed by `rong::quickjs::intl::init`.

use rong::JSEngine;
use rong::Source;
use rong::intl;
use rong::quickjs::RongJS;

const BLOB: &[u8] = include_bytes!("../intl/tests/data/intl-test.postcard");

#[test]
fn quickjs_gets_intl_only_once_configured() {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let has_intl = |ctx: &rong::quickjs::JSContext| -> bool {
        ctx.eval(Source::from_bytes("typeof Intl === 'object'"))
            .unwrap()
    };
    assert!(!has_intl(&ctx));
    rong::quickjs::intl::init(&ctx).unwrap();
    assert!(!has_intl(&ctx), "init without data installs nothing");

    let data = intl::IntlData::try_from_static_blob(BLOB).unwrap();
    intl::configure(&ctx, data, intl::IntlSettings::default());
    rong::quickjs::intl::init(&ctx).unwrap();
    let canonical: String = ctx
        .eval(Source::from_bytes("Intl.getCanonicalLocales('EN-us')[0]"))
        .unwrap();
    assert_eq!(canonical, "en-US");
}
