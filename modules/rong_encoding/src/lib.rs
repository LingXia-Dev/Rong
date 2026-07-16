use rong::*;

mod base64;
mod text_decoder;
mod text_encoder;

pub use base64::{atob, btoa};
pub use text_decoder::TextDecoder;
pub use text_encoder::TextEncoder;

pub fn init(ctx: &JSContext) -> JSResult<()> {
    text_encoder::init(ctx)?;
    text_decoder::init(ctx)?;

    let atob = JSFunc::new(ctx, atob)?;
    let btoa = JSFunc::new(ctx, btoa)?;
    ctx.global().set("atob", atob)?;
    ctx.global().set("btoa", btoa)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    #[test]
    fn test_encoding() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            rong_console::init(&ctx)?;

            let passed = UnitJSRunner::load_script(&ctx, "encoding.js")
                .await?
                .run()
                .await?;
            assert!(passed);
            Ok(())
        });
    }

    #[test]
    fn encode_into_respects_typed_array_view_offset() {
        run(|ctx| {
            init(ctx)?;
            let result: Vec<i32> = ctx.eval(Source::from_bytes(
                r#"
                const backing = new Uint8Array([9, 9, 9, 9, 9]);
                const view = new Uint8Array(backing.buffer, 2, 2);
                const counts = new TextEncoder().encodeInto("AB", view);
                [...backing, counts.read, counts.written]
                "#,
            ))?;
            assert_eq!(result, vec![9, 9, 65, 66, 9, 2, 2]);
            Ok(())
        });
    }

    #[test]
    fn encode_into_never_splits_utf8_and_reports_utf16_units_read() {
        run(|ctx| {
            init(ctx)?;
            let result: Vec<i32> = ctx.eval(Source::from_bytes(
                r#"
                const encoder = new TextEncoder();
                const tooSmall = new Uint8Array(1);
                const first = encoder.encodeInto("éA", tooSmall);
                const exact = new Uint8Array(2);
                const second = encoder.encodeInto("éA", exact);
                const astral = new Uint8Array(4);
                const third = encoder.encodeInto("💩A", astral);
                [
                  first.read, first.written, tooSmall[0],
                  second.read, second.written, ...exact,
                  third.read, third.written, ...astral,
                ]
                "#,
            ))?;
            assert_eq!(
                result,
                vec![0, 0, 0, 1, 2, 0xc3, 0xa9, 2, 4, 0xf0, 0x9f, 0x92, 0xa9]
            );
            Ok(())
        });
    }
}
