use rong_test::*;

#[test]
fn test_array_basic_operations() {
    run(|ctx| {
        // Create new array
        let array = ctx
            .eval::<JSArray>(Source::from_bytes("let arr = [1, 2, 3]; arr"))
            .unwrap();

        // Test length
        assert_eq!(array.len().unwrap(), 3);

        // Test get
        assert_eq!(array.get_opt::<i32>(0).unwrap(), Some(1));
        assert_eq!(array.get_opt::<i32>(10).unwrap(), None); // Out of bounds

        // Test set
        array.set(0, 10).unwrap();
        assert_eq!(array.get_opt::<i32>(0).unwrap(), Some(10));

        // Test is_empty
        assert!(!array.is_empty().unwrap());
        Ok(())
    });
}

#[test]
fn test_array_iteration() {
    run(|ctx| {
        // Create array from JavaScript
        let array = ctx
            .eval::<JSArray>(Source::from_bytes("[10, 20, 30]"))
            .unwrap();

        // Test iterator
        let mut sum = 0;
        for item in array.iter_present::<i32>().unwrap() {
            sum += item.unwrap();
        }
        assert_eq!(sum, 60);

        let iter = array.iter_values().unwrap();
        assert_eq!(iter.len(), 3);

        // Test empty iterator
        let empty_array = JSArray::new(ctx).unwrap();
        let mut empty_iter = empty_array.iter_values().unwrap();
        assert_eq!(empty_iter.len(), 0);
        assert!(empty_iter.next().is_none());
        Ok(())
    });
}

#[test]
fn test_array_creation() {
    run(|ctx| {
        // Create empty array from Rust
        let array = JSArray::new(ctx).unwrap();
        assert!(array.is_empty().unwrap());

        // Push elements
        array.push(1).unwrap();
        array.push(2).unwrap();
        array.push(3).unwrap();

        // Verify contents
        assert_eq!(array.len().unwrap(), 3);
        assert_eq!(array.get_opt::<i32>(0).unwrap(), Some(1));
        assert_eq!(array.get_opt::<i32>(1).unwrap(), Some(2));
        assert_eq!(array.get_opt::<i32>(2).unwrap(), Some(3));
        Ok(())
    });
}

#[test]
fn push_rejects_maximum_array_length() {
    run(|ctx| {
        let array: JSArray = ctx.eval(Source::from_bytes(
            "const array = []; array.length = 4294967295; array",
        ))?;
        assert!(array.push(1).is_err());
        Ok(())
    });
}

#[test]
fn test_array_pop() {
    run(|ctx| {
        // Create array
        let array = ctx
            .eval::<JSArray>(Source::from_bytes("[1, 2, 3]"))
            .unwrap();

        // Test pop
        assert_eq!(array.pop_opt::<i32>().unwrap(), Some(3));
        assert_eq!(array.len().unwrap(), 2);

        // Pop remaining elements
        assert_eq!(array.pop_opt::<i32>().unwrap(), Some(2));
        assert_eq!(array.pop_opt::<i32>().unwrap(), Some(1));

        // Test pop on empty array
        assert!(array.pop_opt::<i32>().unwrap().is_none());
        Ok(())
    });
}

#[test]
fn test_array_from_rust() {
    run(|ctx| {
        // Create array from Rust
        let array = JSArray::new(ctx).unwrap();
        array.push("hello").unwrap();
        array.push(42).unwrap();
        array.push(true).unwrap();

        ctx.global().set("__rust_array", array)?;

        // Verify contents in JavaScript
        let result: String = ctx
            .eval(Source::from_bytes(
                r#"
            let arr = __rust_array;
            arr[0] + ' ' + arr[1] + ' ' + arr[2]
            "#,
            ))
            .unwrap();
        assert_eq!(result, "hello 42 true");
        Ok(())
    });
}

#[test]
fn test_array_iterator_edge_cases() {
    run(|ctx| {
        // Test empty array
        let empty_array = JSArray::new(ctx).unwrap();
        let mut empty_iter = empty_array.iter_values().unwrap();
        assert_eq!(empty_iter.len(), 0);
        assert!(empty_iter.next().is_none());

        // Test array with mixed types
        let mixed_array = ctx
            .eval::<JSArray>(Source::from_bytes("[1, 'two', true]"))
            .unwrap();

        // Test iteration with concrete types
        assert_eq!(mixed_array.get_opt::<i32>(0).unwrap(), Some(1));
        assert_eq!(
            mixed_array.get_opt::<String>(1).unwrap(),
            Some("two".to_string())
        );
        assert_eq!(mixed_array.get_opt::<bool>(2).unwrap(), Some(true));
        Ok(())
    });
}

#[test]
fn test_array_get_opt_distinguishes_hole_from_undefined() {
    run(|ctx| {
        let array = ctx
            .eval::<JSArray>(Source::from_bytes("[1, , undefined, null]"))
            .unwrap();

        assert_eq!(array.get_opt::<i32>(0).unwrap(), Some(1));
        assert_eq!(array.get_opt::<i32>(1).unwrap(), None);

        let explicit_undefined: JSValue = array.get_opt(2)?.unwrap();
        assert!(explicit_undefined.is_undefined());

        assert_eq!(array.get_opt::<Option<String>>(2).unwrap(), Some(None));
        assert_eq!(array.get_opt::<Option<String>>(3).unwrap(), Some(None));
        Ok(())
    });
}

#[test]
fn test_vec_to_js_array() {
    run(|ctx| {
        let vec = vec![1, 2, 3];
        let js_array = JSArray::from_js_value(ctx, vec.into_js_value(ctx))?;
        assert!(js_array.is_array());
        assert_eq!(js_array.len()?, 3);
        Ok(())
    });
}

#[test]
fn test_js_array_to_vec() {
    run(|ctx| {
        let vec = ctx
            .eval::<Vec<i32>>(Source::from_bytes("[1, 2, 3]"))
            .unwrap();
        assert_eq!(vec, vec![1, 2, 3]);
        Ok(())
    });
}

#[test]
fn test_empty_js_array_to_vec() {
    run(|ctx| {
        let vec = ctx.eval::<Vec<i32>>(Source::from_bytes("[]")).unwrap();
        assert!(vec.is_empty());
        Ok(())
    });
}
