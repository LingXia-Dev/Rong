use rong_test::*;

#[test]
fn test_from_js() {
    run(|ctx| {
        let symbol: JSSymbol = ctx.eval(Source::from_bytes(b"Symbol('a b c')"))?;
        assert_eq!(symbol.descripiton()?, "a b c");
        Ok(())
    });
}

#[test]
fn test_symbol() {
    run(|ctx| {
        let symbol = JSSymbol::new(ctx, "a b c")?;
        assert!(symbol.is_symbol());
        assert_eq!(symbol.descripiton()?, "a b c");

        let symbol = JSSymbol::new(ctx, "test")?;
        let obj = JSObject::new(ctx);
        assert!(obj.set(symbol.clone(), 5).is_ok());
        assert_eq!(obj.get::<_, u32>(symbol).unwrap(), 5);

        let symbol = JSSymbol::new(ctx, "a\0b")?;
        assert_eq!(symbol.descripiton()?, "a\0b");

        Ok(())
    });
}
