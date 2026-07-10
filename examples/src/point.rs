use rong::*;
use rong::{js_class, js_method};

// Define the Point struct with js_class macro
#[js_class]
#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

// Implement methods and expose them to JavaScript
#[js_class(rename = "Point2D")]
impl Point {
    // Constructor
    #[js_method(constructor)]
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    // Instance properties with getters and setters
    #[js_method(getter, enumerable)]
    fn x(&self) -> i32 {
        self.x
    }

    #[js_method(setter, rename = "x")]
    fn set_x(&mut self, x: i32) {
        self.x = x;
    }

    #[js_method(getter, enumerable)]
    fn y(&self) -> i32 {
        self.y
    }

    #[js_method(setter, rename = "y")]
    fn set_y(&mut self, y: i32) {
        self.y = y;
    }

    // Regular instance method
    #[js_method(rename = "add")]
    fn add(&self, other: Point) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }

    // Static method
    #[js_method]
    fn origin() -> Self {
        Self { x: 0, y: 0 }
    }

    // Mutable instance method
    #[js_method(rename = "moveBy")]
    fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

fn main() {
    // Create a JavaScript runtime and context
    let rt = RongJS::runtime();
    let ctx = rt.context();

    // Register our Point class with JavaScript
    let _ = ctx.register_class::<Point>();

    // Run some JavaScript code that uses our Point class
    let result = ctx
        .eval::<String>(Source::from_bytes(
            r#"
        // Create points using constructor
        let p1 = new Point2D(10, 20);
        let p2 = new Point2D(30, 40);

        // Test getters and setters
        p1.x = 15;
        p1.y = 25;

        // Test instance method
        let p3 = p1.add(p2);

        // Test static method
        let origin = Point2D.origin();

        // Test mutable method
        p1.moveBy(5, 5);

        // Return a string representation of our points
        `Points:
        p1(${p1.x}, ${p1.y})  // Should be (20, 30) after moveBy
        p2(${p2.x}, ${p2.y})  // Original (30, 40)
        p3(${p3.x}, ${p3.y})  // Sum of original p1 + p2 (45, 65)
        origin(${origin.x}, ${origin.y})`  // (0, 0)
        "#,
        ))
        .unwrap();

    println!("{}", result);

    // Demonstrate Rust-JavaScript interop
    let rust_point = Point::new(100, 200);
    let js_point = ctx
        .eval::<Point>(Source::from_bytes(
            r#"
        let p = new Point2D(1, 2);
        p.moveBy(10, 20);
        p
        "#,
        ))
        .unwrap();

    println!("\nMixing Rust and JavaScript:");
    println!("Rust point: {:?}", rust_point);
    println!("JS point: {:?}", js_point);
}
