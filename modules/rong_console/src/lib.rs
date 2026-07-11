use rong::{function::*, *};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, IsTerminal, Write};
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsoleTraceContext {
    pub namespace: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct StoredConsoleTraceContext(Option<ConsoleTraceContext>);

#[derive(Debug)]
pub enum LogLevel {
    Verbose,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Assert,
}

#[derive(Default)]
struct ConsoleRuntimeState {
    timers: RefCell<HashMap<String, Instant>>,
    counters: RefCell<HashMap<String, usize>>,
}

#[derive(Clone, Copy)]
struct InspectOptions {
    max_depth: usize,
    max_array_items: usize,
    max_object_keys: usize,
    quote_top_level_string: bool,
}

impl Default for InspectOptions {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_array_items: 100,
            max_object_keys: 100,
            quote_top_level_string: false,
        }
    }
}

pub fn set_trace_context(ctx: &JSContext, trace_context: ConsoleTraceContext) {
    ctx.set_state(StoredConsoleTraceContext(Some(trace_context)));
}

pub fn clear_trace_context(ctx: &JSContext) {
    ctx.set_state(StoredConsoleTraceContext(None));
}

pub fn trace_context(ctx: &JSContext) -> Option<&ConsoleTraceContext> {
    ctx.get_state::<StoredConsoleTraceContext>()
        .and_then(|trace| trace.0.as_ref())
}

fn emit_console_trace(level: tracing::Level, ctx: &JSContext, message: &str) {
    macro_rules! emit_with_level {
        ($level:expr) => {
            match trace_context(ctx) {
                Some(trace) => match (trace.namespace.as_deref(), trace.scope.as_deref()) {
                    (Some(namespace), Some(scope)) => tracing::event!(
                        target: "rong.js.console",
                        $level,
                        namespace,
                        scope,
                        message = message
                    ),
                    (Some(namespace), None) => tracing::event!(
                        target: "rong.js.console",
                        $level,
                        namespace,
                        message = message
                    ),
                    (None, Some(scope)) => tracing::event!(
                        target: "rong.js.console",
                        $level,
                        scope,
                        message = message
                    ),
                    (None, None) => tracing::event!(
                        target: "rong.js.console",
                        $level,
                        message = message
                    ),
                },
                None => tracing::event!(
                    target: "rong.js.console",
                    $level,
                    message = message
                ),
            }
        };
    }

    match level {
        tracing::Level::ERROR => emit_with_level!(tracing::Level::ERROR),
        tracing::Level::WARN => emit_with_level!(tracing::Level::WARN),
        tracing::Level::INFO => emit_with_level!(tracing::Level::INFO),
        tracing::Level::DEBUG => emit_with_level!(tracing::Level::DEBUG),
        tracing::Level::TRACE => emit_with_level!(tracing::Level::TRACE),
    }
}

fn write_console(ctx: &JSContext, level: LogLevel, message: String) {
    if tracing::dispatcher::has_been_set() {
        match level {
            LogLevel::Verbose | LogLevel::Info => {
                emit_console_trace(tracing::Level::INFO, ctx, &message);
            }
            LogLevel::Debug => {
                emit_console_trace(tracing::Level::DEBUG, ctx, &message);
            }
            LogLevel::Error => {
                emit_console_trace(tracing::Level::ERROR, ctx, &message);
            }
            LogLevel::Warn => {
                emit_console_trace(tracing::Level::WARN, ctx, &message);
            }
            LogLevel::Trace | LogLevel::Assert => {
                emit_console_trace(tracing::Level::ERROR, ctx, &message);
            }
        }
    } else {
        match level {
            LogLevel::Verbose | LogLevel::Info => {
                println!("{}", message);
            }
            LogLevel::Debug => {
                println!("DEBUG: {}", message);
            }
            LogLevel::Error => {
                eprintln!("ERROR: {}", message);
            }
            LogLevel::Warn => {
                eprintln!("WARN: {}", message);
            }
            LogLevel::Trace | LogLevel::Assert => {
                eprintln!("{}", message);
            }
        }
    }
}

fn console_writer_is_tty() -> bool {
    io::stdout().is_terminal()
}

/// Initialize the console module
pub fn init(ctx: &JSContext) -> JSResult<()> {
    if ctx.get_state::<ConsoleRuntimeState>().is_none() {
        ctx.set_state(ConsoleRuntimeState::default());
    }
    let console = JSObject::new(ctx);

    console.set("clear", JSFunc::new(ctx, clear)?)?;
    console.set("log", JSFunc::new(ctx, verbose)?)?;
    console.set("error", JSFunc::new(ctx, error)?)?;
    console.set("warn", JSFunc::new(ctx, warn)?)?;
    console.set("info", JSFunc::new(ctx, info)?)?;
    console.set("debug", JSFunc::new(ctx, debug)?)?;
    console.set("assert", JSFunc::new(ctx, console_assert)?)?;
    console.set("dir", JSFunc::new(ctx, dir)?)?;
    console.set("trace", JSFunc::new(ctx, trace)?)?;
    console.set("time", JSFunc::new(ctx, time)?)?;
    console.set("timeLog", JSFunc::new(ctx, time_log)?)?;
    console.set("timeEnd", JSFunc::new(ctx, time_end)?)?;
    console.set("count", JSFunc::new(ctx, count)?)?;
    console.set("countReset", JSFunc::new(ctx, count_reset)?)?;

    ctx.register_class::<Console>()?;
    ctx.global().set("console", console)?;
    Ok(())
}

fn log_message(ctx: &JSContext, level: LogLevel, message: String) {
    write_console(ctx, level, message);
}

fn clear() {
    if console_writer_is_tty() {
        // ANSI clear screen sequence
        print!("\x1B[2J\x1B[1;1H");
        // Ensure immediate output flush
        let _ = io::stdout().flush();
    } else {
        // In non-terminal environment, print a newline
        println!();
    }
}

fn verbose(ctx: JSContext, args: Rest<JSValue>) {
    let message = format_args(&ctx, args);
    log_message(&ctx, LogLevel::Info, message);
}

fn error(ctx: JSContext, args: Rest<JSValue>) {
    let message = format_args(&ctx, args);
    log_message(&ctx, LogLevel::Error, message);
}

fn warn(ctx: JSContext, args: Rest<JSValue>) {
    let message = format_args(&ctx, args);
    log_message(&ctx, LogLevel::Warn, message);
}

fn info(ctx: JSContext, args: Rest<JSValue>) {
    let message = format_args(&ctx, args);
    log_message(&ctx, LogLevel::Info, message);
}

fn debug(ctx: JSContext, args: Rest<JSValue>) {
    let message = format_args(&ctx, args);
    log_message(&ctx, LogLevel::Debug, message);
}

fn console_assert(ctx: JSContext, args: Rest<JSValue>) {
    let mut values = args.0;
    let condition = values
        .first()
        .cloned()
        .unwrap_or_else(|| JSValue::undefined(&ctx));
    if js_value_is_truthy(&condition) {
        return;
    }

    let extras = if values.is_empty() {
        Vec::new()
    } else {
        values.drain(1..).collect::<Vec<_>>()
    };
    let message = if extras.is_empty() {
        "Assertion failed".to_string()
    } else {
        format!("Assertion failed: {}", format_values(&ctx, extras))
    };
    log_message(&ctx, LogLevel::Assert, message);
}

fn dir(ctx: JSContext, args: Rest<JSValue>) {
    let mut values = args.0;
    let value = values
        .drain(..1)
        .next()
        .unwrap_or_else(|| JSValue::undefined(&ctx));
    let mut options = InspectOptions {
        quote_top_level_string: true,
        ..InspectOptions::default()
    };
    if let Some(obj) = values
        .first()
        .cloned()
        .and_then(|value| JSObject::from_js_value(&ctx, value).ok())
    {
        options = inspect_options_from_object(obj, options);
    }
    log_message(
        &ctx,
        LogLevel::Info,
        inspect_value_with_options(value, options),
    );
}

fn trace(ctx: JSContext, args: Rest<JSValue>) {
    let message = if args.is_empty() {
        "Trace".to_string()
    } else {
        format!("Trace: {}", format_args(&ctx, args))
    };

    let stack = ctx
        .eval::<String>(Source::from_bytes("new Error().stack"))
        .ok()
        .filter(|value| !value.is_empty());
    let rendered = if let Some(stack) = stack {
        let mut lines = stack.lines();
        let _ = lines.next();
        let rest = lines.collect::<Vec<_>>().join("\n");
        if rest.is_empty() {
            message
        } else {
            format!("{message}\n{rest}")
        }
    } else {
        message
    };
    log_message(&ctx, LogLevel::Trace, rendered);
}

fn time(ctx: JSContext, label: Optional<String>) {
    let label = normalize_console_label(label.0, "default");
    console_state(&ctx)
        .timers
        .borrow_mut()
        .insert(label, Instant::now());
}

fn time_log(ctx: JSContext, label: Optional<String>, args: Rest<JSValue>) {
    let label = normalize_console_label(label.0, "default");
    let Some(started_at) = console_state(&ctx).timers.borrow().get(&label).copied() else {
        log_message(
            &ctx,
            LogLevel::Warn,
            format!("Timer '{label}' does not exist"),
        );
        return;
    };

    let mut message = format!("{label}: {}", format_elapsed_ms(started_at.elapsed()));
    if !args.is_empty() {
        message.push(' ');
        message.push_str(&format_args(&ctx, args));
    }
    log_message(&ctx, LogLevel::Info, message);
}

fn time_end(ctx: JSContext, label: Optional<String>) {
    let label = normalize_console_label(label.0, "default");
    let Some(started_at) = console_state(&ctx).timers.borrow_mut().remove(&label) else {
        log_message(
            &ctx,
            LogLevel::Warn,
            format!("Timer '{label}' does not exist"),
        );
        return;
    };

    log_message(
        &ctx,
        LogLevel::Info,
        format!("{label}: {}", format_elapsed_ms(started_at.elapsed())),
    );
}

fn count(ctx: JSContext, label: Optional<String>) {
    let label = normalize_console_label(label.0, "default");
    let next = {
        let state = console_state(&ctx);
        let mut counters = state.counters.borrow_mut();
        let count = counters.entry(label.clone()).or_insert(0);
        *count += 1;
        *count
    };
    log_message(&ctx, LogLevel::Info, format!("{label}: {next}"));
}

fn count_reset(ctx: JSContext, label: Optional<String>) {
    let label = normalize_console_label(label.0, "default");
    let removed = console_state(&ctx)
        .counters
        .borrow_mut()
        .remove(&label)
        .is_some();
    if !removed {
        log_message(
            &ctx,
            LogLevel::Warn,
            format!("Count for '{label}' does not exist"),
        );
    }
}

fn format_args(_ctx: &JSContext, args: Rest<JSValue>) -> String {
    format_values(_ctx, args.0)
}

fn format_values(_ctx: &JSContext, args: Vec<JSValue>) -> String {
    let mut result = String::new();
    format_values_internal(&mut result, args);
    result
}

pub fn inspect_value(value: JSValue) -> String {
    inspect_value_with_options(value, InspectOptions::default())
}

fn inspect_value_with_options(value: JSValue, options: InspectOptions) -> String {
    let mut result = String::new();
    format_raw_inner(&mut result, value, &mut HashSet::default(), 0, options);
    result
}

fn format_values_internal(result: &mut String, args: Vec<JSValue>) {
    let size = args.len();
    let mut iter = args.into_iter().enumerate().peekable();

    while let Some((index, arg)) = iter.next() {
        // Handle formatted strings
        if index == 0
            && size > 1
            && let Ok(format_str) = arg.clone().to_rust::<String>()
        {
            let mut chars = format_str.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '%' {
                    match chars.next() {
                        Some('s') => {
                            if let Some((_, next_arg)) = iter.next() {
                                if let Ok(str) = next_arg.clone().to_rust::<String>() {
                                    result.push_str(&str);
                                } else {
                                    format_raw_inner(
                                        result,
                                        next_arg,
                                        &mut HashSet::default(),
                                        0,
                                        InspectOptions::default(),
                                    );
                                }
                            } else {
                                result.push_str("%s");
                            }
                            continue;
                        }
                        Some('d') | Some('i') => {
                            if let Some((_, next_arg)) = iter.next() {
                                if let Ok(num) = next_arg.clone().to_rust::<f64>() {
                                    result.push_str(&num.trunc().to_string());
                                } else {
                                    format_raw_inner(
                                        result,
                                        next_arg,
                                        &mut HashSet::default(),
                                        0,
                                        InspectOptions::default(),
                                    );
                                }
                            } else {
                                result.push_str("%d");
                            }
                            continue;
                        }
                        Some('f') => {
                            if let Some((_, next_arg)) = iter.next() {
                                if let Ok(num) = next_arg.clone().to_rust::<f64>() {
                                    result.push_str(&num.to_string());
                                } else {
                                    format_raw_inner(
                                        result,
                                        next_arg,
                                        &mut HashSet::default(),
                                        0,
                                        InspectOptions::default(),
                                    );
                                }
                            } else {
                                result.push_str("%f");
                            }
                            continue;
                        }
                        Some('o') | Some('O') => {
                            if let Some((_, next_arg)) = iter.next() {
                                format_raw_inner(
                                    result,
                                    next_arg,
                                    &mut HashSet::default(),
                                    0,
                                    InspectOptions::default(),
                                );
                            } else {
                                result.push_str("%o");
                            }
                            continue;
                        }
                        Some('%') => {
                            result.push('%');
                            continue;
                        }
                        Some(other) => {
                            result.push('%');
                            result.push(other);
                            continue;
                        }
                        None => {
                            result.push('%');
                            continue;
                        }
                    }
                }
                result.push(c);
            }

            for (_, extra) in iter.by_ref() {
                result.push(' ');
                format_raw_inner(
                    result,
                    extra,
                    &mut HashSet::default(),
                    0,
                    InspectOptions::default(),
                );
            }
            continue;
        }

        // Non-formatted string regular argument
        if index != 0 {
            result.push(' ');
        }

        // handle next arg
        format_raw_inner(
            result,
            arg,
            &mut HashSet::default(),
            0,
            InspectOptions::default(),
        );
    }
}

fn format_raw_inner(
    result: &mut String,
    value: JSValue,
    visited: &mut HashSet<usize>,
    depth: usize,
    options: InspectOptions,
) {
    if depth > options.max_depth {
        result.push_str("[Maximum recursion depth exceeded]");
        return;
    }

    match value.type_of() {
        JSValueType::Undefined => result.push_str("undefined"),
        JSValueType::Null => result.push_str("null"),

        JSValueType::Boolean => {
            if let Ok(b) = value.to_rust::<bool>() {
                result.push_str(if b { "true" } else { "false" });
            }
        }

        JSValueType::Number => {
            if let Ok(n) = value.to_rust::<f64>() {
                result.push_str(&n.to_string());
            }
        }

        JSValueType::BigInt => {
            if let Ok(s) = value.to_rust::<String>() {
                result.push_str(&ensure_bigint_suffix(&s));
            }
        }

        JSValueType::String => {
            if let Ok(s) = value.to_rust::<String>() {
                if depth > 0 || options.quote_top_level_string {
                    result.push('"');
                    result.push_str(&escape_string(&s));
                    result.push('"');
                } else {
                    result.push_str(&s);
                }
            }
        }

        JSValueType::Date => {
            if let Ok(s) = value.to_rust::<String>() {
                result.push_str(&s);
            }
        }

        JSValueType::Object | JSValueType::Array => {
            let obj: JSObject = value.clone().into();
            let hash = default_hash(&value);
            if visited.contains(&hash) {
                result.push_str("[Circular]");
                return;
            }
            visited.insert(hash);

            if let Some(typed_array) = AnyJSTypedArray::from_object(obj.clone()) {
                format_typed_array(result, obj, typed_array, visited, depth, options);
            } else if let Some(array) = JSArray::from_object(obj.clone()) {
                format_array(result, array, visited, depth, options);
            } else if let Some(name) = object_display_name(&obj)
                && name == "RegExp"
                && let Ok(source) = value.clone().to_rust::<String>()
            {
                result.push_str(&source);
            } else {
                format_object(result, obj, visited, depth, options);
            }
            visited.remove(&hash);
        }

        JSValueType::Function => {
            let obj: JSObject = value.into();
            if let Ok(name) = obj.get::<_, String>("name") {
                if !name.is_empty() {
                    result.push_str("[Function: ");
                    result.push_str(&name);
                    result.push(']');
                } else {
                    result.push_str("[Function]");
                }
            } else {
                result.push_str("[Function]");
            }
        }

        JSValueType::Symbol => {
            let obj: JSObject = value.into();
            if let Some(symbol) = JSSymbol::from_object(obj) {
                if let Ok(description) = symbol.descripiton() {
                    if !description.is_empty() {
                        result.push_str(&format!("Symbol({})", description));
                    } else {
                        result.push_str("Symbol()");
                    }
                } else {
                    result.push_str("Symbol()");
                }
            } else {
                result.push_str("Symbol()");
            }
        }

        JSValueType::Promise => {
            let obj: JSObject = value.into();
            if let Ok(state) = obj.get::<_, String>("state") {
                result.push_str(&format!("Promise <{}>", state));
            }
        }

        JSValueType::Constructor => {
            let obj: JSObject = value.into();
            if let Ok(name) = obj.get::<_, String>("name")
                && !name.is_empty()
            {
                result.push_str(&format!("[class {}]", name));
                return;
            }

            if let Ok(prototype) = obj.get::<_, JSObject>("prototype")
                && let Ok(constructor_name) = prototype.get::<_, String>("constructor")
                && !constructor_name.is_empty()
            {
                result.push_str(&format!("[class {}]", constructor_name));
            }
        }

        JSValueType::Error | JSValueType::Exception => {
            let obj: JSObject = value.into();
            format_error(result, obj, depth);
        }

        JSValueType::ArrayBuffer => {
            let obj: JSObject = value.into();
            format_array_buffer(result, obj);
        }

        JSValueType::Unknown => {
            result.push_str("[Unknown]");
        }
    }
}

fn format_array(
    result: &mut String,
    array: JSArray,
    visited: &mut HashSet<usize>,
    depth: usize,
    options: InspectOptions,
) {
    let total = array.len().unwrap_or(0) as usize;
    let mut written = 0usize;
    result.push_str("[ ");
    if let Ok(iter) = array.iter_values() {
        for item in iter.flatten() {
            if written >= options.max_array_items {
                break;
            }
            if written > 0 {
                result.push_str(", ");
            }
            format_raw_inner(result, item, visited, depth + 1, options);
            written += 1;
        }
    }
    if total > written {
        result.push_str(", ... ");
        result.push_str(&(total - written).to_string());
        result.push_str(" more");
    }
    result.push_str(" ]");
}

fn format_object(
    result: &mut String,
    obj: JSObject,
    visited: &mut HashSet<usize>,
    depth: usize,
    options: InspectOptions,
) {
    if let Some(name) = object_display_name(&obj)
        && name != "Object"
    {
        result.push_str(&name);
        result.push(' ');
    }
    result.push('{');
    let mut first = true;

    if let Ok(entries) = obj.entries() {
        let total = entries.len();
        for (idx, entry) in entries.into_iter().enumerate() {
            if idx >= options.max_object_keys {
                break;
            }
            if !first {
                result.push_str(", ");
            }
            first = false;

            if let Ok(key_str) = entry.key().clone().to_rust::<String>() {
                if needs_quotes(&key_str) {
                    result.push('"');
                    result.push_str(&escape_string(&key_str));
                    result.push('"');
                } else {
                    result.push_str(&key_str);
                }
                result.push_str(": ");

                format_raw_inner(result, entry.value().clone(), visited, depth + 1, options);
            }
        }
        if total > options.max_object_keys {
            if !first {
                result.push_str(", ");
            }
            result.push_str("... ");
            result.push_str(&(total - options.max_object_keys).to_string());
            result.push_str(" more");
        }
    }

    result.push('}');
}

fn format_array_buffer(result: &mut String, obj: JSObject) {
    if let Some(buffer) = JSArrayBuffer::from_object(obj.clone()) {
        let len = buffer.len();

        // For small ArrayBuffer, display its content
        if len <= 50 && len > 0 {
            result.push_str("ArrayBuffer { ");
            result.push_str(&format!("byteLength: {}", len));

            // Try to get and display the byte content
            let bytes = buffer.as_bytes();
            result.push_str(", bytes: [");

            for (i, byte) in bytes.iter().enumerate() {
                if i > 0 {
                    result.push_str(", ");
                }
                result.push_str(&format!("0x{:02x}", byte));
            }

            result.push(']');

            result.push_str(" }");
        } else {
            // For large ArrayBuffer, only display the length
            result.push_str(&format!("ArrayBuffer {{ byteLength: {} }}", len));
        }
    }
}

fn typed_array_kind_name(kind: JSTypedArrayKind) -> &'static str {
    match kind {
        JSTypedArrayKind::Int8 => "Int8Array",
        JSTypedArrayKind::Uint8 => "Uint8Array",
        JSTypedArrayKind::Uint8Clamped => "Uint8ClampedArray",
        JSTypedArrayKind::Int16 => "Int16Array",
        JSTypedArrayKind::Uint16 => "Uint16Array",
        JSTypedArrayKind::Int32 => "Int32Array",
        JSTypedArrayKind::Uint32 => "Uint32Array",
        JSTypedArrayKind::BigInt64 => "BigInt64Array",
        JSTypedArrayKind::BigUint64 => "BigUint64Array",
        JSTypedArrayKind::Float32 => "Float32Array",
        JSTypedArrayKind::Float64 => "Float64Array",
    }
}

fn format_typed_array(
    result: &mut String,
    obj: JSObject,
    typed_array: AnyJSTypedArray,
    visited: &mut HashSet<usize>,
    depth: usize,
    options: InspectOptions,
) {
    let len = typed_array.len();
    let preview_len = len.min(options.max_array_items);
    result.push_str(typed_array_kind_name(typed_array.kind()));
    result.push('(');
    result.push_str(&len.to_string());
    result.push_str(") [ ");

    for index in 0..preview_len {
        if index > 0 {
            result.push_str(", ");
        }
        match obj.get::<_, JSValue>(index as u32) {
            Ok(item) => format_raw_inner(result, item, visited, depth + 1, options),
            Err(_) => result.push_str("<unavailable>"),
        }
    }

    if len > preview_len {
        if preview_len > 0 {
            result.push_str(", ");
        }
        result.push_str("... ");
        result.push_str(&(len - preview_len).to_string());
        result.push_str(" more");
    }

    result.push_str(" ]");
}

fn ensure_bigint_suffix(value: &str) -> String {
    if value.ends_with('n') {
        value.to_string()
    } else {
        format!("{value}n")
    }
}

fn object_display_name(obj: &JSObject) -> Option<String> {
    let ctor = obj.get::<_, JSObject>("constructor").ok()?;
    let name = ctor.get::<_, String>("name").ok()?;
    if name.is_empty() { None } else { Some(name) }
}

fn format_error(result: &mut String, obj: JSObject, depth: usize) {
    let name = obj
        .get::<_, String>("name")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Error".to_string());
    let message = obj
        .get::<_, String>("message")
        .ok()
        .filter(|value| !value.is_empty());
    let headline = match message {
        Some(message) => format!("{name}: {message}"),
        None => name,
    };

    if depth == 0
        && let Ok(stack) = obj.get::<_, String>("stack")
        && !stack.is_empty()
    {
        if stack == headline || stack.starts_with(&(headline.clone() + "\n")) {
            result.push_str(&stack);
        } else {
            result.push_str(&headline);
            result.push('\n');
            result.push_str(&stack);
        }
        return;
    }

    result.push_str(&headline);
}

fn console_state(ctx: &JSContext) -> &ConsoleRuntimeState {
    if ctx.get_state::<ConsoleRuntimeState>().is_none() {
        ctx.set_state(ConsoleRuntimeState::default());
    }
    ctx.get_state::<ConsoleRuntimeState>()
        .expect("console runtime state should be installed")
}

fn normalize_console_label(label: Option<String>, default: &str) -> String {
    label
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn format_elapsed_ms(duration: std::time::Duration) -> String {
    format!("{:.3}ms", duration.as_secs_f64() * 1000.0)
}

fn js_value_is_truthy(value: &JSValue) -> bool {
    match value.type_of() {
        JSValueType::Undefined | JSValueType::Null => false,
        JSValueType::Boolean => value.clone().to_rust::<bool>().unwrap_or(false),
        JSValueType::Number => value
            .clone()
            .to_rust::<f64>()
            .map(|num| num != 0.0 && !num.is_nan())
            .unwrap_or(false),
        JSValueType::BigInt => value
            .clone()
            .to_rust::<String>()
            .map(|num| num != "0" && num != "0n")
            .unwrap_or(true),
        JSValueType::String => value
            .clone()
            .to_rust::<String>()
            .map(|text| !text.is_empty())
            .unwrap_or(false),
        JSValueType::Symbol
        | JSValueType::Object
        | JSValueType::Array
        | JSValueType::Date
        | JSValueType::Function
        | JSValueType::Promise
        | JSValueType::Constructor
        | JSValueType::Error
        | JSValueType::Exception
        | JSValueType::ArrayBuffer => true,
        JSValueType::Unknown => false,
    }
}

fn inspect_options_from_object(obj: JSObject, mut options: InspectOptions) -> InspectOptions {
    if let Ok(depth) = obj.get::<_, f64>("depth")
        && depth.is_finite()
        && depth >= 0.0
    {
        options.max_depth = depth as usize;
    }
    if let Ok(max_array_items) = obj.get::<_, f64>("maxArrayLength")
        && max_array_items.is_finite()
        && max_array_items >= 0.0
    {
        options.max_array_items = max_array_items as usize;
    }
    if let Ok(max_object_keys) = obj.get::<_, f64>("maxObjectKeys")
        && max_object_keys.is_finite()
        && max_object_keys >= 0.0
    {
        options.max_object_keys = max_object_keys as usize;
    }
    options
}

fn needs_quotes(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }

    let first_char = s.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() && first_char != '_' && first_char != '$' {
        return true;
    }

    !s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\x08' => result.push_str("\\b"),
            '\x0c' => result.push_str("\\f"),
            c if c.is_ascii_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

#[js_class]
pub struct Console {}

#[js_class]
impl Console {
    #[js_method(constructor)]
    fn new() -> Self {
        Self {}
    }

    #[js_method]
    fn log(&self, ctx: JSContext, args: Rest<JSValue>) {
        verbose(ctx, args);
    }

    #[js_method]
    fn error(&self, ctx: JSContext, args: Rest<JSValue>) {
        error(ctx, args);
    }

    #[js_method]
    fn warn(&self, ctx: JSContext, args: Rest<JSValue>) {
        warn(ctx, args);
    }

    #[js_method]
    fn info(&self, ctx: JSContext, args: Rest<JSValue>) {
        info(ctx, args);
    }

    #[js_method]
    fn debug(&self, ctx: JSContext, args: Rest<JSValue>) {
        debug(ctx, args);
    }

    #[js_method(rename = "assert")]
    fn console_assert(&self, ctx: JSContext, args: Rest<JSValue>) {
        console_assert(ctx, args);
    }

    #[js_method]
    fn dir(&self, ctx: JSContext, args: Rest<JSValue>) {
        dir(ctx, args);
    }

    #[js_method]
    fn trace(&self, ctx: JSContext, args: Rest<JSValue>) {
        trace(ctx, args);
    }

    #[js_method]
    fn time(&self, ctx: JSContext, label: Optional<String>) {
        time(ctx, label);
    }

    #[js_method(rename = "timeLog")]
    fn time_log(&self, ctx: JSContext, label: Optional<String>, args: Rest<JSValue>) {
        time_log(ctx, label, args);
    }

    #[js_method(rename = "timeEnd")]
    fn time_end(&self, ctx: JSContext, label: Optional<String>) {
        time_end(ctx, label);
    }

    #[js_method]
    fn count(&self, ctx: JSContext, label: Optional<String>) {
        count(ctx, label);
    }

    #[js_method(rename = "countReset")]
    fn count_reset(&self, ctx: JSContext, label: Optional<String>) {
        count_reset(ctx, label);
    }

    #[js_method]
    fn clear() {
        clear();
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

#[inline]
pub fn default_hash<T: Hash + ?Sized>(v: &T) -> usize {
    let mut state = DefaultHasher::default();
    v.hash(&mut state);
    state.finish() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::run;
    use std::sync::{Arc, Mutex};
    use tracing::{Event, Subscriber, field::Field, field::Visit};
    use tracing_subscriber::{
        Registry,
        layer::{Context, Layer, SubscriberExt},
    };

    #[derive(Default)]
    struct EventVisitor {
        message: Option<String>,
        namespace: Option<String>,
        scope: Option<String>,
    }

    impl Visit for EventVisitor {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "message" => self.message = Some(value.to_string()),
                "namespace" => self.namespace = Some(value.to_string()),
                "scope" => self.scope = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" && self.message.is_none() {
                self.message = Some(format!("{value:?}"));
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CapturedConsoleEvent {
        message: String,
        namespace: Option<String>,
        scope: Option<String>,
    }

    #[derive(Clone)]
    struct CaptureConsoleLayer {
        events: Arc<Mutex<Vec<CapturedConsoleEvent>>>,
    }

    impl<S> Layer<S> for CaptureConsoleLayer
    where
        S: Subscriber,
    {
        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            if event.metadata().target() != "rong.js.console" {
                return;
            }

            let mut visitor = EventVisitor::default();
            event.record(&mut visitor);
            if let Some(message) = visitor.message {
                self.events.lock().unwrap().push(CapturedConsoleEvent {
                    message,
                    namespace: visitor.namespace,
                    scope: visitor.scope,
                });
            }
        }
    }

    fn capture_console_events<F>(f: F) -> JSResult<Vec<CapturedConsoleEvent>>
    where
        F: FnOnce() -> JSResult<()>,
    {
        let events = Arc::new(Mutex::new(Vec::new()));
        let subscriber = Registry::default().with(CaptureConsoleLayer {
            events: events.clone(),
        });
        tracing::subscriber::with_default(subscriber, f)?;
        Ok(events.lock().unwrap().clone())
    }

    #[test]
    fn test_console_log_formatted_string() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"console.log("Name: %s, Age: %d", "Alice", 30);"#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert_eq!(
                output, "Name: Alice, Age: 30",
                "Output should match formatted string"
            );
            Ok(())
        });
    }

    #[test]
    fn test_console_log_unknown_formatter_keeps_literal_and_appends_args() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(r#"console.log("Hello %x", 42);"#))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert_eq!(output, "Hello %x 42");
            Ok(())
        });
    }

    #[test]
    fn test_console_log_formatter_fallback_on_type_mismatch() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(r#"console.log("Value=%d", { a: 1 });"#))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(output.starts_with("Value="));
            assert!(output.contains("{"));
            Ok(())
        });
    }

    #[test]
    fn test_console_log_circular_reference() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"
                    const obj = { name: "Circular Object" };
                    obj.self = obj;
                    console.log("Circular object:", obj);
                "#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(
                output.contains("[Circular]"),
                "Output '{}' should contain circular reference marker",
                output
            );
            Ok(())
        });
    }

    #[test]
    fn test_console_log_max_depth() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"
                    function createDeepObject(depth) {
                        if (depth <= 0) return {};
                        return { child: createDeepObject(depth - 1) };
                    }
                    const deepObj = createDeepObject(15);
                    console.log("Deep object:", deepObj);
                "#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(
                output.contains("[Maximum recursion depth exceeded]"),
                "Output '{}' should contain recursion depth warning",
                output
            );
            Ok(())
        });
    }

    #[test]
    fn test_console_can_forward_to_tracing() {
        run(|ctx| {
            init(ctx)?;
            let events = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(r#"console.info("hello tracing")"#))
            })?;
            assert_eq!(
                events,
                vec![CapturedConsoleEvent {
                    message: "hello tracing".to_string(),
                    namespace: None,
                    scope: None,
                }]
            );
            Ok(())
        });
    }

    #[test]
    fn test_console_can_emit_trace_context_fields() {
        run(|ctx| {
            init(ctx)?;
            set_trace_context(
                ctx,
                ConsoleTraceContext {
                    namespace: Some("miniapp".to_string()),
                    scope: Some("page".to_string()),
                },
            );
            let events = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(r#"console.info("hello context")"#))
            })?;
            assert_eq!(
                events,
                vec![CapturedConsoleEvent {
                    message: "hello context".to_string(),
                    namespace: Some("miniapp".to_string()),
                    scope: Some("page".to_string()),
                }]
            );
            clear_trace_context(ctx);
            Ok(())
        });
    }

    #[test]
    fn test_console_log_typed_array() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"console.log({ stdout: new Uint8Array([116, 111]), ok: true });"#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(output.contains("Uint8Array(2) [ 116, 111 ]"));
            assert!(output.contains("ok: true"));
            Ok(())
        });
    }

    #[test]
    fn test_console_log_class_instance_prefixes_constructor_name() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"
                    class User {
                      constructor() {
                        this.id = 1;
                      }
                    }
                    console.log(new User());
                "#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(output.contains("User {id: 1}"));
            Ok(())
        });
    }

    #[test]
    fn test_console_log_error_does_not_duplicate_headline() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(r#"console.log(new Error("boom"));"#))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert_eq!(output.matches("Error: boom").count(), 1);
            Ok(())
        });
    }

    #[test]
    fn test_console_assert_only_logs_on_failure() {
        run(|ctx| {
            init(ctx)?;
            let events = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"
                    console.assert(true, "ok");
                    console.assert(false, "boom", { code: 123 });
                "#,
                ))
            })?;
            assert_eq!(events.len(), 1);
            assert!(
                events[0]
                    .message
                    .contains("Assertion failed: boom {code: 123}")
            );
            Ok(())
        });
    }

    #[test]
    fn test_console_dir_supports_depth_option() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"console.dir({ nested: { value: 1 } }, { depth: 0 });"#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(output.contains("nested: [Maximum recursion depth exceeded]"));
            Ok(())
        });
    }

    #[test]
    fn test_console_trace_includes_trace_header() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"
                    function demoTrace() {
                      console.trace("hello");
                    }
                    demoTrace();
                "#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(output.contains("Trace: hello"));
            Ok(())
        });
    }

    #[test]
    fn test_console_time_and_count_lifecycle() {
        run(|ctx| {
            init(ctx)?;
            let output = capture_console_events(|| {
                ctx.eval::<()>(Source::from_bytes(
                    r#"
                    console.count("jobs");
                    console.count("jobs");
                    console.countReset("jobs");
                    console.count("jobs");
                    console.time("work");
                    console.timeLog("work", "phase-1");
                    console.timeEnd("work");
                "#,
                ))
            })?
            .into_iter()
            .map(|event| event.message)
            .collect::<Vec<_>>()
            .join("\n");
            assert!(output.contains("jobs: 1"));
            assert!(output.contains("jobs: 2"));
            assert!(output.contains("work: "));
            assert!(output.contains("phase-1"));
            Ok(())
        });
    }
}
