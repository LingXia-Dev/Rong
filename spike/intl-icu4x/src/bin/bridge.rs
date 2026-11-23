//! What an author pays per `toLocaleString` call when it reaches a cached
//! ICU4X formatter through rong on QuickJS, against the parts of that cost.

use fixed_decimal::{Decimal, FloatPrecision, SignedRoundingMode, UnsignedRoundingMode};
use icu::decimal::DecimalFormatter;
use icu::locale::Locale;
use rong::*;
use std::time::Instant;

const CALLS: u32 = 200_000;

fn timed(ctx: &JSContext, name: &str, body: &str) {
    let source = format!(
        "(() => {{ let last; for (let i = 0; i < {CALLS}; i++) {{ last = {body}; }} return String(last); }})()"
    );
    // Warm up once, then measure.
    let _ = ctx.eval::<String>(Source::from_bytes(source.clone()));
    let start = Instant::now();
    let last = ctx.eval::<String>(Source::from_bytes(source)).unwrap();
    let ns = start.elapsed().as_nanos() / CALLS as u128;
    println!("BRIDGE {name}: {ns} ns per call (last {last:?})");
}

fn main() {
    let rt = RongJS::runtime();
    let ctx = rt.context();
    let loc: Locale = "en-US".parse().unwrap();
    let formatter = DecimalFormatter::try_new((&loc).into(), Default::default()).unwrap();

    ctx.global()
        .set(
            "__noop",
            JSFunc::new(&ctx, |_x: f64| -> String { String::from("x") }).unwrap(),
        )
        .unwrap();
    ctx.global()
        .set(
            "__format",
            JSFunc::new(&ctx, move |x: f64| -> String {
                let Ok(mut d) = Decimal::try_from_f64(x, FloatPrecision::RoundTrip) else {
                    return String::from("NaN");
                };
                d.round_with_mode(
                    -3,
                    SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand),
                );
                d.absolute.trim_end();
                formatter.format_to_string(&d)
            })
            .unwrap(),
        )
        .unwrap();
    ctx.eval::<()>(Source::from_bytes(
        "Number.prototype.toLocaleString = function () { return __format(Number(this)); };",
    ))
    .unwrap();

    timed(&ctx, "JS only: String(i + 0.5)", "String(i + 0.5)");
    timed(
        &ctx,
        "native call returning a constant string",
        "__noop(i + 0.5)",
    );
    timed(
        &ctx,
        "native call formatting with a cached ICU4X formatter",
        "__format(i + 1234.5)",
    );
    timed(
        &ctx,
        "(i + 1234.5).toLocaleString() routed to it",
        "(i + 1234.5).toLocaleString()",
    );
}
