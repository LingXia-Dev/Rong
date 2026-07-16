use rong_test::*;

#[test]
fn test_jsdate_creation() {
    run(|ctx| {
        // Test our JSDate creation
        let epoch_ms = 1640995200000.0; // 2022-01-01 00:00:00 UTC
        let date: JSDate = JSDate::new(ctx, epoch_ms);

        println!("Our Date is_date: {}", date.as_js_value().is_date());
        println!("Our Date is_object: {}", date.as_js_value().is_object());

        // Verify it's a Date
        assert!(date.as_js_value().is_date());

        // Try to get the time back
        let retrieved_time = date.get_time()?;
        assert_eq!(retrieved_time, epoch_ms);

        Ok(())
    });
}

#[test]
fn test_date_javascript_integration() {
    run(|ctx| {
        // Test creating Date using JavaScript
        let js_date: JSValue = ctx.eval(Source::from_bytes(b"new Date(1640995200000)"))?;
        assert!(js_date.is_date());

        // Convert to JSDate and test getting time
        let js_date_wrapper: JSDate = js_date.to_rust()?;
        let js_time = js_date_wrapper.get_time()?;
        assert_eq!(js_time, 1640995200000.0);

        // Test Date.now()
        let now_result: f64 = ctx.eval(Source::from_bytes(b"Date.now()"))?;
        assert!(now_result > 0.0);

        // Test Date methods
        let date_methods_test: bool = ctx.eval(Source::from_bytes(
            b"
            const date = new Date('2023-12-25T10:30:00.000Z');
            date instanceof Date &&
            date.getFullYear() === 2023 &&
            date.getMonth() === 11 &&
            date.getDate() === 25 &&
            date.toISOString() === '2023-12-25T10:30:00.000Z'
        ",
        ))?;
        assert!(date_methods_test);

        // Test invalid dates
        let invalid_date_test: bool = ctx.eval(Source::from_bytes(
            b"
            const invalidDate = new Date('invalid');
            invalidDate instanceof Date && isNaN(invalidDate.getTime())
        ",
        ))?;
        assert!(invalid_date_test);

        Ok(())
    });
}

#[test]
fn test_date_system_time_conversion() {
    run(|ctx| {
        // Test SystemTime conversion using JSDate
        let system_time = std::time::SystemTime::now();
        let date_from_system: JSDate = JSDate::from_system_time(ctx, system_time);
        assert!(date_from_system.as_js_value().is_date());

        // Convert back to SystemTime
        let converted_back = date_from_system.to_system_time()?;

        // Should be approximately the same (within 1 second)
        let diff = system_time
            .duration_since(converted_back)
            .or_else(|_| converted_back.duration_since(system_time))
            .unwrap();
        assert!(diff.as_secs() < 1);

        // Test JSValue to SystemTime conversion
        let date_value = date_from_system.into_js_value();
        let system_time2: std::time::SystemTime = date_value.to_rust()?;
        let diff2 = system_time
            .duration_since(system_time2)
            .or_else(|_| system_time2.duration_since(system_time))
            .unwrap();
        assert!(diff2.as_secs() < 1);

        Ok(())
    });
}

#[test]
fn system_time_before_epoch_round_trips() {
    run(|ctx| {
        let expected = std::time::UNIX_EPOCH - std::time::Duration::from_millis(1_500);
        let date: JSDate = JSDate::from_system_time(ctx, expected);
        assert_eq!(date.get_time()?, -1_500.0);
        assert_eq!(date.to_system_time()?, expected);
        Ok(())
    });
}

#[test]
fn invalid_js_date_cannot_convert_to_system_time() {
    run(|ctx| {
        let invalid: JSDate = ctx.eval(Source::from_bytes("new Date(NaN)"))?;
        assert!(invalid.to_system_time().is_err());
        Ok(())
    });
}

#[test]
fn test_date_comprehensive_types() {
    run(|ctx| {
        // Test various date creation methods
        let epoch_ms = 1640995200000.0; // 2022-01-01 00:00:00 UTC

        // Create from epoch milliseconds
        let date1: JSDate = JSDate::new(ctx, epoch_ms);
        assert!(date1.as_js_value().is_date());
        assert_eq!(date1.get_time()?, epoch_ms);

        // Create current time
        let date2: JSDate = JSDate::now(ctx);
        assert!(date2.as_js_value().is_date());
        assert!(date2.get_time()? > epoch_ms);

        // Test SystemTime creation
        let system_time = std::time::SystemTime::now();
        let date3: JSDate = JSDate::from_system_time(ctx, system_time);
        assert!(date3.as_js_value().is_date());

        // Test conversion to/from JSValue
        let date_value = date1.as_js_value().clone();
        assert!(date_value.is_date());

        let date_back: JSDate = date_value.to_rust()?;
        assert_eq!(date_back.get_time()?, epoch_ms);

        Ok(())
    });
}
