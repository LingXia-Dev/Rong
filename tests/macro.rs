use rong_macro::{FromJSObject, js_class, js_method};
use rong_test::*;
use std::sync::{Mutex, OnceLock};
use tokio::time::Duration;

#[js_class(clone)]
#[derive(Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

static ORIGIN: OnceLock<Mutex<Point>> = OnceLock::new();

#[js_class(rename = "PointX")]
impl Point {
    #[js_method(constructor)]
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[js_method(getter, rename = "ORIGIN")]
    fn get_origin() -> Point {
        ORIGIN
            .get_or_init(|| Mutex::new(Point { x: 0x5a, y: 0xa5 }))
            .lock()
            .unwrap()
            .clone()
    }

    #[js_method(setter, rename = "ORIGIN")]
    fn set_origin(point: Point) {
        if let Some(origin) = ORIGIN.get() {
            *origin.lock().unwrap() = point;
        }
    }

    #[js_method(getter, enumerable, rename = "x")]
    fn getx(&self) -> i32 {
        self.x
    }

    #[js_method(setter, rename = "x")]
    fn setx(&mut self, x: i32) {
        self.x = x;
    }

    #[js_method(getter, rename = "y")]
    fn gety(&self) -> i32 {
        self.y
    }

    #[js_method(setter, rename = "y")]
    fn sety(&mut self, y: i32) {
        self.y = y;
    }

    #[js_method(rename = "Add")]
    fn add(&self, other: Point) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    #[js_method(rename = "create")]
    fn create(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[js_method(rename = "moveBy")]
    fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    #[js_method(rename = "moveByAsync")]
    async fn move_by_async(&self, dx: i32, dy: i32) -> Self {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    #[js_method(rename = "createAsync")]
    async fn create_async(x: i32, y: i32) -> Self {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self { x, y }
    }
}

#[derive(FromJSObject)]
struct Person {
    #[js_name = "firstName"]
    first_name: String,
    #[js_name = "lastName"]
    last_name: String,
    age: i32,
    nickname: Option<String>,
    #[js_default = "active"]
    status: String,
    #[js_default]
    score: i32,
}

#[derive(FromJSObject, Debug)]
struct Config {
    name: String,
    // Optional field
    description: Option<String>,
    // Required field
    version: String,
    // Field with default
    #[js_default = "production"]
    environment: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constructor() {
        run(|ctx| {
            ctx.register_class::<Point>()?;
            let point: Point = ctx.eval(Source::from_bytes("new PointX(2, 3)"))?;
            assert_eq!(point, Point { x: 2, y: 3 });
            Ok(())
        });
    }

    #[test]
    fn test_from_js_obj_with_defaults() {
        run(|ctx| {
            // Test complete object
            let person: Person = ctx.eval(Source::from_bytes(
                r#"
                ({
                    firstName: "John",
                    lastName: "Doe",
                    age: 30,
                    nickname: "Johnny",
                    status: "premium",
                    score: 100
                })
            "#,
            ))?;

            assert_eq!(person.first_name, "John");
            assert_eq!(person.last_name, "Doe");
            assert_eq!(person.age, 30);
            assert_eq!(person.nickname, Some("Johnny".to_string()));
            assert_eq!(person.status, "premium");
            assert_eq!(person.score, 100);

            // Test object with defaults
            let person_minimal: Person = ctx.eval(Source::from_bytes(
                r#"
                ({
                    firstName: "Jane",
                    lastName: "Smith",
                    age: 25
                })
            "#,
            ))?;

            assert_eq!(person_minimal.first_name, "Jane");
            assert_eq!(person_minimal.last_name, "Smith");
            assert_eq!(person_minimal.age, 25);
            assert_eq!(person_minimal.nickname, None);
            assert_eq!(person_minimal.status, "active"); // default value
            assert_eq!(person_minimal.score, 0); // Default::default()

            Ok(())
        });
    }

    #[test]
    fn optional_derived_field_accepts_null_and_undefined() {
        run(|ctx| {
            let null_nickname: Person = ctx.eval(Source::from_bytes(
                r#"({
                    firstName: "Null",
                    lastName: "Value",
                    age: 30,
                    nickname: null
                })"#,
            ))?;
            assert_eq!(null_nickname.nickname, None);

            let undefined_nickname: Person = ctx.eval(Source::from_bytes(
                r#"({
                    firstName: "Undefined",
                    lastName: "Value",
                    age: 31,
                    nickname: undefined
                })"#,
            ))?;
            assert_eq!(undefined_nickname.nickname, None);
            Ok(())
        });
    }

    #[test]
    fn test_from_js_obj_error_handling() {
        run(|ctx| {
            // Test missing required field
            let result: Result<Config, _> = ctx.eval(Source::from_bytes(
                r#"
                ({
                    name: "MyApp"
                    // Missing required 'version' field
                })
            "#,
            ));

            assert!(result.is_err());
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("Required field 'version' is missing"));

            // Test successful parsing with optional and default fields
            let config: Config = ctx.eval(Source::from_bytes(
                r#"
                ({
                    name: "MyApp",
                    version: "1.0.0",
                    description: "A test application"
                })
            "#,
            ))?;

            assert_eq!(config.name, "MyApp");
            assert_eq!(config.version, "1.0.0");
            assert_eq!(config.description, Some("A test application".to_string()));
            assert_eq!(config.environment, "production"); // default value

            Ok(())
        });
    }

    #[test]
    fn test_instance_property() {
        run(|ctx| {
            ctx.register_class::<Point>()?;

            // Test value behavior
            let result: i32 = ctx.eval(Source::from_bytes(
                r#"
                    let p = new PointX(1, 2);
                    p.x = 10;
                    p.y = 20;
                    p.x + p.y
                    "#,
            ))?;
            assert_eq!(result, 30);

            // Test property attributes
            let desc: bool = ctx.eval(Source::from_bytes(
                r#"
                    p = new PointX(1, 2);
                    let desc = Object.getOwnPropertyDescriptor(p.constructor.prototype, 'x');
                    desc.configurable === true &&
                    desc.enumerable === true &&
                    typeof desc.get === 'function' &&
                    typeof desc.set === 'function'
                    "#,
            ))?;
            assert!(
                desc,
                "x should be configurable and enumerable with accessors"
            );

            // Test non-enumerable property
            let desc: bool = ctx.eval(Source::from_bytes(
                r#"
                    desc = Object.getOwnPropertyDescriptor(p.constructor.prototype, 'y');
                    desc.configurable === true &&  // 添加 configurable 测试
                    desc.enumerable === false &&
                    typeof desc.get === 'function' &&
                    typeof desc.set === 'function'
                    "#,
            ))?;
            assert!(
                desc,
                "y should be configurable but not enumerable with accessors"
            );
            Ok(())
        });
    }

    #[test]
    fn test_instance_method() {
        run(|ctx| {
            ctx.register_class::<Point>()?;
            let result: Point = ctx.eval(Source::from_bytes(
                r#"
                let p1 = new PointX(1, 2);
                let p2 = new PointX(3, 4);
                p1.Add(p2)
            "#,
            ))?;
            assert_eq!(result, Point { x: 4, y: 6 });
            Ok(())
        });
    }

    #[test]
    fn test_static_method() {
        run(|ctx| {
            ctx.register_class::<Point>()?;
            let result: Point = ctx.eval(Source::from_bytes("PointX.create(5, 6)"))?;
            assert_eq!(result, Point { x: 5, y: 6 });
            Ok(())
        });
    }

    #[test]
    fn test_mutable_instance_method() {
        run(|ctx| {
            ctx.register_class::<Point>()?;
            let result: Point = ctx.eval(Source::from_bytes(
                r#"
                    let p = new PointX(1, 2);
                    p.moveBy(10, 20);
                    p
                "#,
            ))?;
            assert_eq!(result, Point { x: 11, y: 22 });
            Ok(())
        });
    }

    #[test]
    fn test_static_property() {
        run(|ctx| {
            ctx.register_class::<Point>()?;

            // Test value behavior
            let result: Point = ctx.eval(Source::from_bytes("PointX.ORIGIN"))?;
            assert_eq!(result, Point { x: 0x5a, y: 0xa5 });

            // Test setting new value
            let result: Point = ctx.eval(Source::from_bytes(
                r#"
                    PointX.ORIGIN = new PointX(1, 2);
                    PointX.ORIGIN
                "#,
            ))?;
            assert_eq!(result, Point { x: 1, y: 2 });

            // Test property attributes
            let desc: bool = ctx.eval(Source::from_bytes(
                r#"
                    let desc = Object.getOwnPropertyDescriptor(PointX, 'ORIGIN');
                    desc.configurable === true &&
                    desc.enumerable === false &&
                    typeof desc.get === 'function' &&
                    typeof desc.set === 'function'
                    "#,
            ))?;
            assert!(
                desc,
                "ORIGIN should be configurable but not enumerable, with getter and setter"
            );
            Ok(())
        });
    }

    #[test]
    fn test_async_instance_method() {
        async_run!(|ctx: JSContext| async move {
            ctx.register_class::<Point>()?;

            let result: Point = ctx
                .eval_async(Source::from_bytes(
                    r#"
                    (async function() {
                        let p = new PointX(1, 2);
                        return await p.moveByAsync(10, 20);
                    })();
                "#,
                ))
                .await?;
            assert_eq!(result, Point { x: 11, y: 22 });
            Ok(())
        });
    }

    #[test]
    fn test_async_static_method() {
        async_run!(|ctx: JSContext| async move {
            ctx.register_class::<Point>()?;

            let result: Point = ctx
                .eval_async(Source::from_bytes(
                    r#"
                    (async function() {
                        return await PointX.createAsync(5, 6);
                    })();
                "#,
                ))
                .await?;
            assert_eq!(result, Point { x: 5, y: 6 });
            Ok(())
        });
    }

    #[test]
    fn test_rename_attribute() {
        run(|ctx| {
            // Test deserialization with renamed fields
            let person: Person = ctx.eval(Source::from_bytes(
                r#"
                ({
                    firstName: "John",
                    lastName: "Doe",
                    age: 30,
                    required_field: "test"
                })
            "#,
            ))?;
            assert_eq!(person.first_name, "John");
            assert_eq!(person.last_name, "Doe");
            assert_eq!(person.age, 30);
            assert_eq!(person.nickname, None);
            assert_eq!(person.status, "active"); // default value
            Ok(())
        });
    }

    #[test]
    fn test_missing_required_field() {
        run(|ctx| {
            // Test deserialization with missing required field (using Config which has required fields)
            let result = ctx.eval::<Config>(Source::from_bytes(
                r#"
                ({
                    name: "MyApp"
                    // Missing required 'version' field
                })
            "#,
            ));
            assert!(result.is_err());
            Ok(())
        });
    }

    #[test]
    fn test_get_method_syntax() {
        run(|ctx| {
            let result: Person = ctx.eval(Source::from_bytes(
                r#"
                ({
                    firstName: "John",
                    lastName: "Doe",
                    age: 30,
                    required_field: "test",
                    nickname: "Johnny"
                })
            "#,
            ))?;

            assert_eq!(result.first_name, "John");
            assert_eq!(result.last_name, "Doe");
            assert_eq!(result.age, 30);
            assert_eq!(result.status, "active");
            assert_eq!(result.nickname, Some("Johnny".to_string()));
            Ok(())
        });
    }
}
