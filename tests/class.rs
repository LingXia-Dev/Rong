use rong::JSEngineValue;
use rong_macro::js_class;
use rong_test::*;
use std::cell::Ref;

#[js_class]
struct Point {
    x: i32,
    y: i32,
    jsobj: Option<JSObject>,
}

impl Point {
    fn add(&self, p: &Point) -> Self {
        Self {
            x: self.x + p.x,
            y: self.y + p.y,
            jsobj: None,
        }
    }

    fn sadd(x: i32, y: i32) -> Self {
        Self {
            x: x + 1,
            y: y + 1,
            jsobj: None,
        }
    }

    fn set_jsobj(&mut self, obj: JSObject) {
        self.jsobj = Some(obj);
    }
}

fn borrow_point(obj: &JSObject) -> JSResult<Ref<'_, Point>> {
    obj.borrow::<Point>()
}

impl JSClass<JSEngineValue> for Point {
    const NAME: &'static str = "Point";

    fn data_constructor() -> Constructor<JSEngineValue> {
        Constructor::new(|x, y| Point { x, y, jsobj: None })
    }

    fn class_setup(class: &ClassSetup<JSEngineValue>) -> JSResult<()> {
        let x_getter =
            class.new_func(|this: This<function::JSClassRef<Point>>| Ok(this.borrow()?.x))?;
        let x_setter = class.new_func(|this: ThisMut<Point>, x: i32| -> JSResult<()> {
            let mut point = this.borrow_mut()?;
            point.x = x;
            Ok(())
        })?;
        class.property(
            "x",
            PropertyDescriptor::from_accessor(x_getter, x_setter).configurable(),
        )?;

        let y_getter =
            class.new_func(|this: This<function::JSClassRef<Point>>| Ok(this.borrow()?.y))?;
        let y_setter = class.new_func(|this: ThisMut<Point>, y: i32| -> JSResult<()> {
            let mut point = this.borrow_mut()?;
            point.y = y;
            Ok(())
        })?;
        class.property(
            "y",
            PropertyDescriptor::from_accessor(y_getter, y_setter).configurable(),
        )?;

        let origin_getter = class.new_func(|| Point {
            x: 0x5a,
            y: 0xa5,
            jsobj: None,
        })?;
        let origin_setter = class.new_func(|| {
            // Read-only property, setter does nothing
        })?;
        class.static_property(
            "origin",
            PropertyDescriptor::from_accessor(origin_getter, origin_setter).configurable(),
        )?;

        class.method(
            "add",
            |this: This<function::JSClassRef<Point>>,
             p: function::JSClassRef<Point>|
             -> JSResult<Point> {
                let this = this.borrow()?;
                let p = p.borrow()?;
                Ok(this.add(&p))
            },
        )?;

        class.method(
            "setJSObj",
            |this: ThisMut<Point>, callback: JSObject| -> JSResult<()> {
                let mut point = this.borrow_mut()?;
                point.set_jsobj(callback);
                Ok(())
            },
        )?;

        class.static_method("sadd", |x: i32, y: i32| Self::sadd(x, y))?;
        Ok(())
    }

    fn gc_mark_with<F>(&self, mut mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        if let Some(obj) = &self.jsobj {
            mark_fn(obj.as_js_value());
        }
    }
}

#[test]
fn constructor() {
    run(|ctx| {
        ctx.register_class::<Point>()?;
        let point = ctx.eval::<JSObject>(Source::from_bytes(b"let point=new Point(2,3); point"))?;
        let point = borrow_point(&point)?;
        assert_eq!(point.x, 2);
        assert_eq!(point.y, 3);

        // Test instance_of with Point
        let obj = ctx.eval::<JSObject>(Source::from_bytes(b"new Point(2,3)"))?;
        assert!(Class::instance_of::<Point>(&obj));

        // Test instance_of with non-Point object
        let obj = ctx.eval::<JSObject>(Source::from_bytes(b"let o = {}; o"))?;
        assert!(!Class::instance_of::<Point>(&obj));

        assert_eq!(
            ctx.eval::<String>(Source::from_bytes(b"typeof Point"))?,
            "function"
        );
        assert!(ctx.eval::<bool>(Source::from_bytes(b"Point.prototype.constructor==Point"))?);
        assert!(ctx.eval::<bool>(Source::from_bytes(b"Point.constructor === Function"))?);
        assert!(ctx.eval::<bool>(Source::from_bytes(b"Point instanceof Function"))?);

        assert!(ctx.eval::<bool>(Source::from_bytes(b"point instanceof Point"))?);
        Ok(())
    });
}

#[test]
fn hidden_constructor_on_rong_namespace() {
    run(|ctx| {
        ctx.register_hidden_class::<Point>()?;
        let ctor = Class::lookup::<Point>(ctx)?.clone();
        ctx.host_namespace().set("HiddenPoint", ctor)?;

        assert_eq!(
            ctx.eval::<String>(Source::from_bytes(b"typeof HiddenPoint"))?,
            "undefined"
        );
        assert!(ctx.eval::<bool>(Source::from_bytes(b"globalThis.HiddenPoint === undefined"))?);
        assert_eq!(
            ctx.eval::<String>(Source::from_bytes(b"typeof Rong.HiddenPoint"))?,
            "function"
        );
        assert!(ctx.eval::<bool>(Source::from_bytes(
            b"Rong.HiddenPoint.constructor === Function"
        ))?);
        assert!(ctx.eval::<bool>(Source::from_bytes(b"Rong.HiddenPoint instanceof Function"))?);
        assert!(ctx.eval::<bool>(Source::from_bytes(
            b"Rong.HiddenPoint.prototype.constructor === Rong.HiddenPoint"
        ))?);

        let point = ctx.eval::<JSObject>(Source::from_bytes(
            b"let point = new Rong.HiddenPoint(4, 5); point",
        ))?;
        let point = borrow_point(&point)?;
        assert_eq!(point.x, 4);
        assert_eq!(point.y, 5);

        assert!(ctx.eval::<bool>(Source::from_bytes(
            b"(new Rong.HiddenPoint(1, 2)) instanceof Rong.HiddenPoint"
        ))?);

        let result = ctx.eval::<i32>(Source::from_bytes(
            br#"
            class ColorPoint extends Rong.HiddenPoint {
                constructor(x, y, color) {
                    super(x, y);
                    this.color = color;
                }
                getColor() {
                    return this.color;
                }
            }

            let p = new ColorPoint(2, 3, 0x5fa5);
            if (!(ColorPoint.prototype.__proto__ === Rong.HiddenPoint.prototype)) {
                throw new Error("Prototype chain broken");
            }
            if (!(ColorPoint.__proto__ === Rong.HiddenPoint)) {
                throw new Error("Constructor chain broken");
            }

            let added = p.add(new Rong.HiddenPoint(1, 2));
            if (added.x !== 3 || added.y !== 5) {
                throw new Error("Inherited method failed");
            }

            p.getColor()
            "#,
        ))?;
        assert_eq!(result, 0x5fa5);

        Ok(())
    });
}

#[test]
fn rustfunc_class_hidden_from_global() {
    run(|ctx| {
        assert_eq!(
            ctx.eval::<String>(Source::from_bytes(b"typeof RustFunc"))?,
            "undefined"
        );
        assert!(ctx.eval::<bool>(Source::from_bytes(b"globalThis.RustFunc === undefined"))?);
        Ok(())
    });
}

#[test]
fn basic_add_fn() {
    run(|ctx| {
        let func = JSFunc::new(ctx, |a: i32, b: i32, c: i32| a + b + c)?.name("add")?;
        ctx.global().set("add", func)?;

        assert!(ctx.eval::<JSFunc>(Source::from_bytes(b"add")).is_ok());
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add(7, 9,1)")).unwrap(),
            17
        );
        assert_eq!(
            ctx.eval::<i32>(Source::from_bytes(b"add.length")).unwrap(),
            3
        );
        assert_eq!(
            ctx.eval::<String>(Source::from_bytes(b"add.name")).unwrap(),
            "add"
        );
        Ok(())
    });
}

#[test]
fn test_property_getter_setter() {
    run(|ctx| {
        ctx.register_class::<Point>()?;

        // Test getter
        let point = ctx.eval::<JSObject>(Source::from_bytes(b"let p = new Point(5, 10); p"))?;
        {
            let point = borrow_point(&point)?;
            assert_eq!(point.x, 5);
        }

        // Test setter
        let point = ctx
            .eval::<JSObject>(Source::from_bytes(b"p.x = 15; p"))
            .unwrap();
        let point = borrow_point(&point)?;
        assert_eq!(point.x, 15);

        // Test property descriptor
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"Object.getOwnPropertyDescriptor(Point.prototype, 'x').configurable"
            ))
            .unwrap()
        );
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"Object.getOwnPropertyDescriptor(Point.prototype, 'x').get !== undefined"
            ))
            .unwrap()
        );
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"Object.getOwnPropertyDescriptor(Point.prototype, 'x').set !== undefined"
            ))
            .unwrap()
        );
        Ok(())
    });
}

#[test]
fn test_instance_method() {
    run(|ctx| {
        ctx.register_class::<Point>()?;

        // Test method call
        let result = ctx
            .eval::<JSObject>(Source::from_bytes(
                b"let p1 = new Point(1, 2); let p2 = new Point(3, 4); p1.add(p2)",
            ))
            .unwrap();
        let result = borrow_point(&result)?;
        assert_eq!(result.x, 4); // 1 + 3
        assert_eq!(result.y, 6); // 2 + 4

        // Test method exists on prototype
        assert!(
            ctx.eval::<bool>(Source::from_bytes(b"'add' in Point.prototype"))
                .unwrap()
        );
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"typeof Point.prototype.add === 'function'"
            ))
            .unwrap()
        );
        Ok(())
    });
}

#[test]
fn test_static_method() {
    run(|ctx| {
        ctx.register_class::<Point>()?;

        // Test static method call
        let result = ctx
            .eval::<JSObject>(Source::from_bytes(b"Point.sadd(5, 7)"))
            .unwrap();
        let result = borrow_point(&result)?;
        assert_eq!(result.x, 6); // 5 + 1
        assert_eq!(result.y, 8); // 7 + 1

        // Test static method exists on constructor
        assert!(
            ctx.eval::<bool>(Source::from_bytes(b"'sadd' in Point"))
                .unwrap()
        );
        assert!(
            ctx.eval::<bool>(Source::from_bytes(b"typeof Point.sadd === 'function'"))
                .unwrap()
        );
        Ok(())
    });
}

#[test]
fn test_static_property() {
    run(|ctx| {
        ctx.register_class::<Point>()?;

        // Test static getter
        let origin = ctx
            .eval::<JSObject>(Source::from_bytes(b"Point.origin"))
            .unwrap();
        let origin = borrow_point(&origin)?;
        assert_eq!(origin.x, 0x5a);
        assert_eq!(origin.y, 0xa5);

        // Test property exists on constructor
        assert!(
            ctx.eval::<bool>(Source::from_bytes(b"'origin' in Point"))
                .unwrap()
        );

        // Test property descriptor
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"Object.getOwnPropertyDescriptor(Point, 'origin').configurable"
            ))
            .unwrap()
        );
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"Object.getOwnPropertyDescriptor(Point, 'origin').get !== undefined"
            ))
            .unwrap()
        );
        assert!(
            ctx.eval::<bool>(Source::from_bytes(
                b"Object.getOwnPropertyDescriptor(Point, 'origin').set !== undefined"
            ))
            .unwrap()
        );

        // Test property is not on prototype or instances
        assert!(
            !ctx.eval::<bool>(Source::from_bytes(b"'origin' in Point.prototype"))
                .unwrap()
        );
        assert!(
            !ctx.eval::<bool>(Source::from_bytes(b"'origin' in (new Point(0, 0))"))
                .unwrap()
        );
        Ok(())
    });
}

#[test]
fn test_extend_class() {
    run(|ctx| {
        ctx.register_class::<Point>()?;

        // Test class extension with method inheritance
        let result = ctx
            .eval::<i32>(Source::from_bytes(
                br#"

                class ColorPoint extends Point {
                    constructor(x, y, color) {
                        super(x, y);
                        this.color = color;
                    }
                    get_color() {
                        return this.color;
                    }
                }
                let p = new ColorPoint(2, 3, 0x5fa5);

                // Verify prototype chain
                if (!(ColorPoint.prototype.__proto__ === Point.prototype)) {
                    throw new Error('Prototype chain broken');
                }
                if (!(ColorPoint.__proto__ === Point)) {
                    throw new Error('Constructor chain broken');
                }

                // Verify inherited methods work
                let added = p.add(new Point(1, 2));
                if (added.x !== 3 || added.y !== 5) {
                     throw new Error('Inherited method failed');
                }

                // Verify new method works
                p.get_color()
                "#,
            ))
            .unwrap();
        assert_eq!(result, 0x5fa5);
        Ok(())
    });
}

#[test]
fn test_instance_hold_object_fromjs() {
    run(|ctx| {
        ctx.register_class::<Point>()?;

        // Add print function to the global object
        ctx.global()
            .set("print", JSFunc::new(ctx, |msg: String| println!("{}", msg)))?;

        // Create a Point and register a callback function
        ctx.eval::<()>(Source::from_bytes(
            br#"
            let point = new Point(10, 20);
            point.name="hello";
            point.setJSObj( { a:1, });
         "#,
        ))?;

        Ok(())
    });
}
