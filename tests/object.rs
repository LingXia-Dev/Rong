use rong_test::*;

#[test]
fn basic() {
    run(|ctx| {
        let v = 3;
        let key = "key";
        let obj = JSObject::new(ctx);
        assert!(obj.is_object());

        assert!(obj.set(key, v).is_ok());
        assert!(obj.has_property(key).unwrap());

        obj.delete(key).unwrap();
        assert!(!obj.has_property(key).unwrap());

        let value = JSValue::from_rust(ctx, v);

        // JSValue as Property Value
        assert!(obj.set(key, value.clone()).is_ok());
        assert_eq!(obj.get::<&str, i32>(key).unwrap(), v);
        assert!(obj.set(9, value.clone()).is_ok());
        assert_eq!(obj.get::<i32, i32>(9).unwrap(), v);

        let objv = JSObject::new(ctx);
        assert!(obj.set("obj", objv).is_ok());
        assert!(obj.has_property("obj").unwrap());
        Ok(())
    });
}

#[test]
fn from_javascript() {
    run(|ctx| {
        let obj: JSObject = ctx
            .eval(Source::from_bytes(
                br#"
                let a3 = [];
                a3[1] = "foo";
                ({
                    a1: [0,,2,,5],
                    a2: [0,"michael",{},undefined,5],
                    a3: a3,
                    func1: () => 1,
                    func2: function(){ return "bar"},
                    obj1: {
                        a: 1,
                        b: "foo",
                    },
                })
                "#,
            ))
            .unwrap();
        assert!(obj.get::<_, JSObject>("a1").unwrap().is_array());
        assert!(obj.get::<_, JSObject>("a2").unwrap().is_array());
        assert!(obj.get::<_, JSObject>("a3").unwrap().is_array());
        assert!(obj.get::<_, JSObject>("func1").unwrap().is_function());
        assert!(obj.get::<_, JSObject>("func2").unwrap().is_function());
        assert!(!obj.get::<_, JSObject>("obj1").unwrap().is_function());
        assert!(!obj.get::<_, JSObject>("obj1").unwrap().is_array());
        assert!(obj.get::<_, JSObject>("obj1").unwrap().is_object());
        assert_eq!(
            obj.get::<_, JSObject>("obj1")
                .unwrap()
                .get::<_, i32>("a")
                .unwrap(),
            1
        );

        let result: Result<String, RongJSError> = obj.get("None");
        assert!(result.is_err());
        match result {
            Err(e) if e.is_property_not_found() => (),
            _ => panic!("Expected PropertyNotFound error"),
        }
        Ok(())
    })
}

#[test]
fn test_object_display() {
    run(|ctx| {
        // Test object with properties
        let code = "({foo: 'bar', num: 42})";
        let obj: JSObject = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", obj), "object");
        assert_eq!(format!("{:?}", obj), "JSObject(object)");

        // Test array object
        let code = "[1, 2, 3]";
        let obj: JSObject = ctx.eval(Source::from_bytes(code)).unwrap();
        assert_eq!(format!("{}", obj), "array");
        assert_eq!(format!("{:?}", obj), "JSObject(array)");
        Ok(())
    });
}

#[test]
fn test_object_properties() {
    run(|ctx| {
        // Create a test object
        let source = Source::from_bytes(
            r#"
            let obj = {
                name: "test",
                age: 42,
                greet: function() { return "Hello"; }
            };
            obj;
            "#,
        );

        let obj: JSObject = ctx.eval(source).unwrap();

        // Test basic property operations
        assert_eq!(obj.get::<_, String>("name").unwrap(), "test");
        assert_eq!(obj.get::<_, i32>("age").unwrap(), 42);
        assert!(obj.has_property("greet").unwrap());

        // Test entries
        let entries = obj.entries().unwrap();
        assert_eq!(entries.len(), 3);

        // Test typed entries with flexible value types
        let entries: Vec<(String, JSValue)> = obj.entries_as().unwrap();
        assert!(entries.iter().any(|(k, _)| k == "name"));

        // Test keys
        let keys: Vec<String> = obj.keys_as().unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"name".to_string()));
        assert!(keys.contains(&"age".to_string()));
        assert!(keys.contains(&"greet".to_string()));

        // Test values
        let values = obj.values().unwrap();
        assert_eq!(values.count(), 3);

        // Test property modification
        assert!(obj.set("name", "updated").is_ok());
        assert_eq!(obj.get::<_, String>("name").unwrap(), "updated");

        // Test property deletion
        assert!(obj.delete("age").unwrap());
        assert!(!obj.has_property("age").unwrap());

        // Test non-existent property
        assert!(obj.get::<_, String>("nonexistent").is_err());
        Ok(())
    });
}

#[test]
fn test_object_property_attributes() {
    run(|ctx| {
        // Test property attributes
        let source = Source::from_bytes(
            r#"
            let obj = {};
            Object.defineProperty(obj, 'readOnly', {
                value: 'constant',
                writable: false,
                enumerable: true
            });
            Object.defineProperty(obj, 'hidden', {
                value: 'secret',
                enumerable: false
            });
            obj;
            "#,
        );

        let obj: JSObject = ctx.eval(source).unwrap();

        // Test read-only property
        assert_eq!(obj.get::<_, String>("readOnly").unwrap(), "constant");

        // Test non-enumerable property
        let keys: Vec<String> = obj.keys_as().unwrap();
        assert!(keys.contains(&"readOnly".to_string()));
        assert!(!keys.contains(&"hidden".to_string()));

        // Test property existence
        assert!(obj.has_property("hidden").unwrap());
        assert!(obj.has_property("readOnly").unwrap());
        Ok(())
    });
}

#[test]
fn object_set_surfaces_engine_write_failures() {
    run(|ctx| {
        let obj: JSObject = ctx.eval(Source::from_bytes("Object.freeze({})"))?;
        assert!(obj.set("value", 1).is_err());
        Ok(())
    });
}

#[test]
fn test_object_prototype() {
    run(|ctx| {
        // Test prototype chain
        let source = Source::from_bytes(
            r#"
            function Animal(name) {
                this.name = name;
            }
            Animal.prototype.speak = function() {
                return this.name + " makes a sound";
            };

            let dog = new Animal("Dog");
            dog;
            "#,
        );

        let obj: JSObject = ctx.eval(source).unwrap();

        // Test instance property
        assert_eq!(obj.get::<_, String>("name").unwrap(), "Dog");

        // Test prototype method
        let result: String = ctx
            .eval(Source::from_bytes(
                r#"
                dog.speak();
                "#,
            ))
            .unwrap();
        assert_eq!(result, "Dog makes a sound");

        // Test own properties
        let own_keys: Vec<String> = obj.keys_as().unwrap();
        assert!(own_keys.contains(&"name".to_string()));
        assert!(!own_keys.contains(&"speak".to_string())); // speak is on the prototype
        Ok(())
    });
}

#[test]
fn rejects_circular_prototype_chain() {
    run(|ctx| {
        let obj = JSObject::new(ctx);
        assert!(!obj.prototype(obj.clone()));
        assert_eq!(ctx.eval::<i32>(Source::from_bytes("1 + 1"))?, 2);
        Ok(())
    });
}

#[test]
fn test_object_undefined_vs_missing_property() {
    run(|ctx| {
        let obj: JSObject = ctx.eval(Source::from_bytes(
            r#"
            ({
                presentUndefined: undefined,
                presentNull: null,
            })
            "#,
        ))?;

        let undefined_value: JSValue = obj.get("presentUndefined")?;
        assert!(undefined_value.is_undefined());

        let optional_undefined: Option<String> = obj.get("presentUndefined")?;
        assert_eq!(optional_undefined, None);

        let optional_null: Option<String> = obj.get("presentNull")?;
        assert_eq!(optional_null, None);

        let missing_value: Result<JSValue, RongJSError> = obj.get("missing");
        assert!(matches!(missing_value, Err(err) if err.is_property_not_found()));

        let get_opt_undefined: JSValue = obj.get_opt("presentUndefined")?.unwrap();
        assert!(get_opt_undefined.is_undefined());

        let get_opt_missing: Option<JSValue> = obj.get_opt("missing")?;
        assert!(get_opt_missing.is_none());

        let tri_state_undefined: Option<Option<String>> = obj.get_opt("presentUndefined")?;
        assert_eq!(tri_state_undefined, Some(None));

        let tri_state_missing: Option<Option<String>> = obj.get_opt("missing")?;
        assert_eq!(tri_state_missing, None);

        Ok(())
    });
}

#[test]
fn test_property_descriptor_false_flags_on_existing_property() {
    run(|ctx| {
        let obj = JSObject::new(ctx);
        obj.set("fixed", 1)?;

        obj.define_property(
            "fixed",
            PropertyDescriptor::from_rust(ctx, 2)
                .readonly()
                .hidden()
                .non_configurable(),
        )?;

        ctx.global().set("__fixed_obj", obj.clone())?;

        let desc: JSObject = ctx.eval(Source::from_bytes(
            "Object.getOwnPropertyDescriptor(__fixed_obj, 'fixed')",
        ))?;
        assert_eq!(desc.get::<_, i32>("value")?, 2);
        assert!(!desc.get::<_, bool>("writable")?);
        assert!(!desc.get::<_, bool>("enumerable")?);
        assert!(!desc.get::<_, bool>("configurable")?);

        let keys: Vec<String> = obj.keys_as()?;
        assert!(!keys.iter().any(|key| key == "fixed"));

        assert!(!obj.delete("fixed")?);
        assert_eq!(obj.get::<_, i32>("fixed")?, 2);

        Ok(())
    });
}

#[test]
fn test_from_json_string_invalid_json_throws_syntax_error() {
    run(|ctx| {
        let err = JSObject::from_json_string(ctx, r#"{foo:1}"#).unwrap_err();
        let thrown = thrown_object(ctx, &err)?;
        let name: String = thrown.get("name")?;
        let message: String = thrown.get("message")?;

        assert_eq!(name, "SyntaxError");
        assert!(
            !message.is_empty(),
            "Expected non-empty SyntaxError message"
        );
        assert!(
            !message.contains("Unexpected end of JSON input"),
            "Property-name syntax errors should not be reported as EOF: {}",
            message
        );

        let nul_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            JSObject::from_json_string(ctx, "{\0}")
        }));
        assert!(nul_result.is_ok(), "invalid JSON must not panic");
        assert!(nul_result.unwrap().is_err());
        Ok(())
    });
}
