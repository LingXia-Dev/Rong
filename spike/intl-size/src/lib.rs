//! Measures what QuickJS's Intl adds to a binary: a QuickJS context that
//! evaluates one script, with and without `quickjs-intl`.

use rong::JSEngine;
use rong::quickjs::RongJS;

#[unsafe(no_mangle)]
pub extern "C" fn intl_probe(blob: *const u8, len: usize) -> i32 {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    #[cfg(feature = "intl")]
    {
        let blob = unsafe { std::slice::from_raw_parts(blob, len) }.to_vec();
        if let Ok(data) = rong::quickjs::intl::IntlData::try_from_blob(blob.into_boxed_slice()) {
            rong::quickjs::intl::configure(&ctx, data, Default::default());
            let _ = rong::quickjs::intl::init(&ctx);
        }
    }
    #[cfg(not(feature = "intl"))]
    let _ = (blob, len);
    ctx.eval::<i32>(rong::Source::from_bytes(
        "typeof Intl === 'object' ? new Intl.PluralRules('en').select(1).length : 0",
    ))
    .unwrap_or(-1)
}
