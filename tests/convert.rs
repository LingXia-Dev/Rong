use rong_test::*;

use std::string::String;

#[test]
fn test_convert() {
    run(|ctx| {
        let jsvalue = JSValue::from_rust(ctx, false);
        assert!(jsvalue.is_boolean());
        assert!(!jsvalue.to_rust::<bool>().unwrap());

        let jsvalue = JSValue::from_rust(ctx, i32::MIN);
        assert!(jsvalue.is_number());
        assert_eq!(i32::MIN, jsvalue.to_rust::<i32>().unwrap());

        let jsvalue = JSValue::from_rust(ctx, u32::MAX);
        assert_eq!(u32::MAX, jsvalue.to_rust::<u32>().unwrap());

        // Test i64: small values should be regular numbers, large values should be BigInt
        let small_i64: i64 = 42;
        let jsvalue = JSValue::from_rust(ctx, small_i64);
        assert!(jsvalue.is_number(), "Small i64 should be a regular number");
        assert_eq!(small_i64, jsvalue.to_rust::<i64>().unwrap());

        let jsvalue = JSValue::from_rust(ctx, i64::MIN);
        assert!(jsvalue.is_bigint(), "i64::MIN should be a BigInt");
        assert_eq!(i64::MIN, jsvalue.to_rust::<i64>().unwrap());

        // Test u64: small values should be regular numbers, large values should be BigInt
        let small_u64: u64 = 42;
        let jsvalue = JSValue::from_rust(ctx, small_u64);
        assert!(jsvalue.is_number(), "Small u64 should be a regular number");
        assert_eq!(small_u64, jsvalue.to_rust::<u64>().unwrap());

        let jsvalue = JSValue::from_rust(ctx, u64::MAX);
        assert!(jsvalue.is_bigint(), "u64::MAX should be a BigInt");
        assert_eq!(u64::MAX, jsvalue.to_rust::<u64>().unwrap());

        // Test JavaScript safe integer boundary (2^53 - 1)
        let safe_max: i64 = (1i64 << 53) - 1;
        let jsvalue = JSValue::from_rust(ctx, safe_max);
        assert!(
            jsvalue.is_number(),
            "JS safe max should be a regular number"
        );
        assert_eq!(safe_max, jsvalue.to_rust::<i64>().unwrap());

        let unsafe_max: i64 = 1i64 << 53;
        let jsvalue = JSValue::from_rust(ctx, unsafe_max);
        assert!(jsvalue.is_bigint(), "Beyond JS safe max should be a BigInt");
        assert_eq!(unsafe_max, jsvalue.to_rust::<i64>().unwrap());

        // Test conversion from JS number and BigInt
        let js_num: JSValue = ctx.eval(Source::from_bytes("42")).unwrap();
        assert_eq!(42i64, js_num.to_rust::<i64>().unwrap());

        let js_bigint: JSValue = ctx.eval(Source::from_bytes("9007199254740992n")).unwrap();
        assert_eq!(1i64 << 53, js_bigint.to_rust::<i64>().unwrap());

        let jsvalue = JSValue::from_rust(ctx, f64::MIN);
        assert_eq!(f64::MIN, jsvalue.to_rust::<f64>().unwrap());

        let hello = "Hello";
        let jsvalue = JSValue::from_rust(ctx, hello);
        assert!(jsvalue.is_string());
        let output: String = jsvalue.to_rust().unwrap();
        assert_eq!(String::from(hello), output);

        let jsvalue = JSValue::undefined(ctx);
        assert!(jsvalue.is_undefined());

        // Test usize conversion: small values should be regular numbers
        let test_usize: usize = 42;
        let jsvalue = JSValue::from_rust(ctx, test_usize);
        assert!(
            jsvalue.is_number(),
            "Small usize should be a regular number"
        );
        let output: usize = jsvalue.to_rust().unwrap();
        assert_eq!(test_usize, output);

        // Test large usize conversion: large values should be BigInt
        let large_usize: usize = usize::MAX;
        let jsvalue = JSValue::from_rust(ctx, large_usize);
        assert!(jsvalue.is_bigint(), "Large usize should be a BigInt");
        let output: usize = jsvalue.to_rust().unwrap();
        assert_eq!(large_usize, output);

        Ok(())
    });
}

#[test]
fn test_convert_from_js() {
    run(|ctx| {
        let result: u64 = ctx.eval(Source::from_bytes(b"16"))?;
        assert_eq!(16, result);

        let result: u32 = ctx.eval(Source::from_bytes(b"16"))?;
        assert_eq!(16, result);

        let result: i32 = ctx.eval(Source::from_bytes(b"-16"))?;
        assert_eq!(-16, result);

        let result: i64 = ctx.eval(Source::from_bytes(b"-16"))?;
        assert_eq!(-16, result);

        let result: f64 = ctx.eval(Source::from_bytes(b"-0.89"))?;
        assert_eq!(-0.89, result);

        let result = ctx.eval::<i32>(Source::from_bytes(b"null"));
        assert!(result.is_err());
        let result = ctx.eval::<i64>(Source::from_bytes(b"null"));
        assert!(result.is_err());
        let result = ctx.eval::<u32>(Source::from_bytes(b"string"));
        assert!(result.is_err());
        let result = ctx.eval::<u64>(Source::from_bytes(b"undefined"));
        assert!(result.is_err());
        let result = ctx.eval::<f64>(Source::from_bytes(b"undefined"));
        assert!(result.is_err());

        Ok(())
    });
}

#[test]
fn narrow_integer_conversions_reject_out_of_range_values() {
    run(|ctx| {
        assert!(ctx.eval::<u8>(Source::from_bytes("256")).is_err());
        assert!(ctx.eval::<i8>(Source::from_bytes("128")).is_err());
        assert!(ctx.eval::<u16>(Source::from_bytes("65536")).is_err());
        assert!(ctx.eval::<i16>(Source::from_bytes("32768")).is_err());
        Ok(())
    });
}

#[test]
fn test_option_conversion_from_js() {
    run(|ctx| {
        let undefined: Option<String> = ctx.eval(Source::from_bytes("undefined"))?;
        assert_eq!(undefined, None);

        let null_value: Option<String> = ctx.eval(Source::from_bytes("null"))?;
        assert_eq!(null_value, None);

        let text: Option<String> = ctx.eval(Source::from_bytes("'hello'"))?;
        assert_eq!(text, Some("hello".to_string()));

        Ok(())
    });
}

#[cfg(feature = "jscore")]
#[test]
fn test_jscore_i32_u32_conversion_semantics() {
    run(|ctx| {
        // JS ToInt32 semantics (wrap modulo 2^32, then signed).
        let i32_over_max: i32 = ctx.eval(Source::from_bytes(b"2147483648"))?;
        assert_eq!(i32::MIN, i32_over_max);

        let i32_mod_zero: i32 = ctx.eval(Source::from_bytes(b"4294967296"))?;
        assert_eq!(0, i32_mod_zero);

        let i32_nan: i32 = ctx.eval(Source::from_bytes(b"NaN"))?;
        assert_eq!(0, i32_nan);

        // JS ToUint32 semantics (wrap modulo 2^32).
        let u32_neg_one: u32 = ctx.eval(Source::from_bytes(b"-1"))?;
        assert_eq!(u32::MAX, u32_neg_one);

        let u32_over_mod: u32 = ctx.eval(Source::from_bytes(b"4294967297"))?;
        assert_eq!(1, u32_over_mod);

        let u32_nan: u32 = ctx.eval(Source::from_bytes(b"NaN"))?;
        assert_eq!(0, u32_nan);

        Ok(())
    });
}

#[test]
fn test_equal() {
    run(|ctx| {
        let boolean = JSValue::from_rust(ctx, false);
        let integer = JSValue::from_rust(ctx, i32::MAX);
        let integer2 = JSValue::from_rust(ctx, i32::MAX);
        assert!(boolean != integer);
        assert!(integer == integer.clone());
        assert!(integer == integer2);
        Ok(())
    });
}

#[test]
fn test_display() {
    run(|ctx| {
        // Test undefined
        let jsvalue = JSValue::undefined(ctx);
        assert_eq!(format!("{}", jsvalue), "undefined");

        // Test boolean
        let jsvalue = JSValue::from_rust(ctx, true);
        assert_eq!(format!("{}", jsvalue), "true");

        // Test number
        let jsvalue = JSValue::from_rust(ctx, 42);
        assert_eq!(format!("{}", jsvalue), "42");

        // Test string
        let jsvalue = JSValue::from_rust(ctx, "hello");
        assert_eq!(format!("{}", jsvalue), "hello");

        // Test object
        let code = "({foo: 'bar'})";
        let jsvalue: JSValue = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", jsvalue), "object");

        // Test array
        let code = "[1, 2, 3]";
        let jsvalue: JSValue = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", jsvalue), "array");

        // Test function
        let code = "(function name() {})";
        let jsvalue: JSValue = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", jsvalue), "function");

        // Test promise
        let code = "new Promise(() => {})";
        let jsvalue: JSValue = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", jsvalue), "promise");

        // Test null
        let code = "null";
        let jsvalue: JSValue = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", jsvalue), "null");
        Ok(())
    });
}

#[test]
fn test_string_with_null() {
    run(|ctx| {
        // Test string containing null character
        let result: String = ctx
            .eval(Source::from_bytes(
                br#"
                "before" + String.fromCharCode(0) + "after"
                "#,
            ))
            .unwrap();

        // In JavaScript, the string is: "before\0after"
        // "before" (6 chars) + "\0" (1 char) + "after" (5 chars) = 12 total
        assert_eq!(result.len(), 12, "String length should be 12 (6 + 1 + 5)");

        // Verify the string contains the null character
        assert!(
            result.contains('\0'),
            "String should contain null character"
        );

        // Verify content before and after null character
        let parts: Vec<&str> = result.split('\0').collect();
        assert_eq!(parts[0], "before", "Content before null character");
        assert_eq!(parts[1], "after", "Content after null character");

        // Test empty string with only null character
        let result: String = ctx
            .eval(Source::from_bytes(
                br#"
                String.fromCharCode(0)
                "#,
            ))
            .unwrap();
        assert_eq!(
            result.len(),
            1,
            "Single null character string length should be 1"
        );
        assert_eq!(result.as_bytes(), &[0], "Should be a single null byte");

        // Test string with multiple null characters
        let result: String = ctx
            .eval(Source::from_bytes(
                br#"
                "a" + String.fromCharCode(0) + "b" + String.fromCharCode(0) + "c"
                "#,
            ))
            .unwrap();
        assert_eq!(
            result.len(),
            5,
            "String length should be 5 (a + \0 + b + \0 + c)"
        );
        let parts: Vec<&str> = result.split('\0').collect();
        assert_eq!(parts, &["a", "b", "c"], "Should split into three parts");
        Ok(())
    });
}
