#![cfg(all(target_env = "ohos", feature = "intl"))]
//! HarmonyOS's JSVM has no `Intl` of its own; the `intl` feature gives it
//! rong's, with zones from the system ICU.

use rong::intl;
use rong_test::*;

const BLOB: &[u8] = include_bytes!("../intl/tests/data/intl-test.postcard");

#[test]
fn jsvm_gets_intl_once_configured() {
    run(|ctx| {
        let eval = |code: &str| -> JSResult<String> { ctx.eval(Source::from_bytes(code)) };
        // The engine's own: none, so nothing is shadowed.
        assert_eq!(eval("typeof globalThis.Intl")?, "undefined");

        let data = intl::IntlData::try_from_static_blob(BLOB).unwrap();
        intl::configure(ctx, data, intl::IntlSettings::default());
        rong::jsvm::intl::init(ctx)?;
        assert_eq!(eval("Intl.getCanonicalLocales('EN-us')[0]")?, "en-US");
        assert_eq!(
            eval("(1234567.891).toLocaleString('en-US')")?,
            "1,234,567.891"
        );
        // Named zones, through the system ICU: 2024-07-15T00:00Z.
        assert_eq!(
            eval(
                "new Intl.DateTimeFormat('en-US', { timeZone: 'Asia/Shanghai', \
                 hour: 'numeric', minute: '2-digit', hourCycle: 'h23' }).format(1721001600000)"
            )?,
            "08:00"
        );
        // New York in July: DST through ICU (Shanghai has none).
        assert_eq!(
            eval(
                "new Intl.DateTimeFormat('en-US', { timeZone: 'America/New_York', \
                 hour: 'numeric', hourCycle: 'h23' }).format(1721001600000)"
            )?,
            "20"
        );
        // The device's own zone, which the sandbox hides from the TZif reader.
        let zone = eval("Intl.DateTimeFormat().resolvedOptions().timeZone")?;
        assert!(zone.contains('/') || zone == "UTC", "{zone}");
        Ok(())
    });
}
