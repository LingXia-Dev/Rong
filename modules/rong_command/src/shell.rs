use crate::{child_process, io, sync_process};
use rong::function::{Optional, Rest};
use rong::*;
use rong_abort::AbortSignal;
use rong_buffer::Blob;
use rong_stream::{ReadableStream, WritableStream, WritableStreamDefaultWriter};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

fn type_error(message: impl Into<String>) -> HostError {
    HostError::new(rong::error::E_TYPE, message).with_name("TypeError")
}

#[derive(Clone, Default)]
struct ShellDefaults {
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    throws: bool,
    quiet: bool,
}

impl JSContextService for ShellDefaults {}

impl ShellDefaults {
    fn ensure(ctx: &JSContext) -> Self {
        if let Some(state) = ctx.get_service::<Self>() {
            return state.clone();
        }

        let state = Self {
            throws: true,
            ..Default::default()
        };
        ctx.set_service(state.clone());
        state
    }
}

#[derive(Clone)]
enum SpawnStdinMode {
    AutoClose,
    Pipe,
    Payload(Vec<u8>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProcessStreamMode {
    Pipe,
    Ignore,
    Inherit,
}

#[derive(Clone)]
struct NormalizedSpawn {
    cmd: Vec<String>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    shell: Option<bool>,
    timeout: Option<u64>,
    stdin: SpawnStdinMode,
    stdout: ProcessStreamMode,
    stderr: ProcessStreamMode,
    kill_signal: Option<String>,
    signal: Option<AbortSignal>,
    on_exit: Option<JSFunc>,
}

#[derive(Clone)]
struct ShellResultData {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
}

#[js_class(clone)]
struct ShellCommand {
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    throws: bool,
    quiet: bool,
}

#[js_class(clone)]
struct ShellError {
    message: String,
    command: String,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn js_array_to_strings(array: &JSArray, label: &str) -> JSResult<Vec<String>> {
    let mut values = Vec::with_capacity(array.len()? as usize);
    for index in 0..array.len()? {
        let value = array
            .get_opt::<String>(index)?
            .ok_or_else(|| type_error(format!("{label} must be an array of strings")))?;
        values.push(value);
    }
    Ok(values)
}

fn js_value_to_command_vec(value: JSValue, label: &str) -> JSResult<Vec<String>> {
    let Some(obj) = value.into_object() else {
        return Err(type_error(format!("{label} must be an array of strings")).into());
    };
    let Some(array) = JSArray::from_object(obj) else {
        return Err(type_error(format!("{label} must be an array of strings")).into());
    };
    js_array_to_strings(&array, label)
}

fn js_value_to_bytes(value: &JSValue, label: &str) -> JSResult<Option<Vec<u8>>> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }

    if let Some(obj) = value.clone().into_object() {
        if let Some(typed_array) = AnyJSTypedArray::from_object(obj.clone()) {
            let bytes = typed_array
                .byte_view()
                .ok_or_else(|| type_error(format!("{label} contains an invalid TypedArray")))?;
            return Ok(Some(bytes.to_vec()));
        }
        if let Some(array_buffer) = JSArrayBuffer::from_object(obj) {
            return Ok(Some(array_buffer.to_vec()));
        }
    }

    let text = value.clone().to_rust::<String>().map_err(|_| {
        type_error(format!(
            "{label} must be a string, ArrayBuffer, or TypedArray"
        ))
    })?;
    Ok(Some(text.into_bytes()))
}

fn normalize_env_map(env_obj: &JSObject) -> JSResult<HashMap<String, String>> {
    let mut next = HashMap::new();
    for (key, value) in env_obj.entries_as::<String, JSValue>()? {
        if value.is_null() || value.is_undefined() {
            continue;
        }
        next.insert(key, value.to_string());
    }
    Ok(next)
}

fn parse_stream_mode(obj: &JSObject, key: &str) -> JSResult<ProcessStreamMode> {
    if !obj.has_property(key)? {
        return Ok(ProcessStreamMode::Pipe);
    }

    let value: JSValue = obj.get(key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(ProcessStreamMode::Pipe);
    }

    let mode = value.to_rust::<String>().map_err(|_| {
        type_error(format!(
            "options.{key} must be \"pipe\", \"ignore\", or \"inherit\""
        ))
    })?;
    match mode.as_str() {
        "pipe" => Ok(ProcessStreamMode::Pipe),
        "ignore" => Ok(ProcessStreamMode::Ignore),
        "inherit" => Ok(ProcessStreamMode::Inherit),
        _ => Err(type_error(format!(
            "options.{key} must be \"pipe\", \"ignore\", or \"inherit\""
        ))
        .into()),
    }
}

fn parse_spawn_options(
    cmd_or_options: JSValue,
    options: Optional<JSObject>,
) -> JSResult<NormalizedSpawn> {
    let (cmd, opts_obj) = if let Some(obj) = cmd_or_options.clone().into_object() {
        if let Some(array) = JSArray::from_object(obj.clone()) {
            (js_array_to_strings(&array, "cmd")?, options.0)
        } else {
            let cmd_value: JSValue = obj.get("cmd")?;
            (
                js_value_to_command_vec(cmd_value, "options.cmd")?,
                Some(obj),
            )
        }
    } else {
        return Err(type_error(
            "Rong.spawn() expects a command array or an options object with cmd",
        )
        .into());
    };

    if cmd.is_empty() {
        return Err(type_error("Rong.spawn() requires a non-empty command array").into());
    }

    let mut normalized = NormalizedSpawn {
        cmd,
        cwd: None,
        env: None,
        shell: None,
        timeout: None,
        stdin: SpawnStdinMode::AutoClose,
        stdout: ProcessStreamMode::Pipe,
        stderr: ProcessStreamMode::Pipe,
        kill_signal: None,
        signal: None,
        on_exit: None,
    };

    if let Some(obj) = opts_obj {
        if obj.has_property("cwd")? {
            normalized.cwd = Some(
                obj.get::<_, String>("cwd")
                    .map_err(|_| type_error("options.cwd must be a string"))?,
            );
        }
        if obj.has_property("env")? {
            let env_obj = obj
                .get::<_, JSObject>("env")
                .map_err(|_| type_error("options.env must be an object"))?;
            normalized.env = Some(normalize_env_map(&env_obj)?);
        }
        if obj.has_property("shell")? {
            normalized.shell = Some(
                obj.get::<_, bool>("shell")
                    .map_err(|_| type_error("options.shell must be a boolean"))?,
            );
        }
        if obj.has_property("timeout")? {
            let timeout = obj
                .get::<_, f64>("timeout")
                .map_err(|_| type_error("options.timeout must be a non-negative number"))?;
            if !timeout.is_finite() || timeout < 0.0 {
                return Err(type_error("options.timeout must be a non-negative number").into());
            }
            normalized.timeout = Some(timeout as u64);
        }
        if obj.has_property("stdin")? {
            let stdin_value: JSValue = obj.get("stdin")?;
            if let Ok(mode) = stdin_value.clone().to_rust::<String>() {
                match mode.as_str() {
                    "pipe" => normalized.stdin = SpawnStdinMode::Pipe,
                    "inherit" => {
                        return Err(type_error(
                            "Rong.spawn() does not support stdin: \"inherit\" yet",
                        )
                        .into());
                    }
                    _ => {
                        normalized.stdin = SpawnStdinMode::Payload(mode.into_bytes());
                    }
                }
            } else if let Some(bytes) = js_value_to_bytes(&stdin_value, "options.stdin")? {
                normalized.stdin = SpawnStdinMode::Payload(bytes);
            }
        }

        normalized.stdout = parse_stream_mode(&obj, "stdout")?;
        normalized.stderr = parse_stream_mode(&obj, "stderr")?;

        if obj.has_property("killSignal")? {
            let signal_value: JSValue = obj.get("killSignal")?;
            normalized.kill_signal = Some(signal_value.to_string());
        }
        if obj.has_property("signal")? {
            normalized.signal = Some(
                obj.get::<_, AbortSignal>("signal")
                    .map_err(|_| type_error("options.signal must be an AbortSignal"))?,
            );
        }
        if obj.has_property("onExit")? {
            normalized.on_exit = Some(
                obj.get::<_, JSFunc>("onExit")
                    .map_err(|_| type_error("options.onExit must be a function"))?,
            );
        }
    }

    Ok(normalized)
}

fn build_native_spawn_options(
    ctx: &JSContext,
    options: &NormalizedSpawn,
) -> JSResult<Option<JSObject>> {
    let obj = JSObject::new(ctx);
    let mut has_any = false;

    if let Some(cwd) = &options.cwd {
        obj.set("cwd", cwd.clone())?;
        has_any = true;
    }
    if let Some(env) = &options.env {
        let env_obj = JSObject::new(ctx);
        for (key, value) in env {
            env_obj.set(key.as_str(), value.clone())?;
        }
        obj.set("env", env_obj)?;
        has_any = true;
    }
    if let Some(shell) = options.shell {
        obj.set("shell", shell)?;
        has_any = true;
    }
    if let Some(timeout) = options.timeout {
        obj.set("timeout", timeout as f64)?;
        has_any = true;
    }

    Ok(has_any.then_some(obj))
}

fn vec_to_js_array(ctx: &JSContext, values: &[String]) -> JSResult<JSArray> {
    let array = JSArray::new(ctx)?;
    for (index, value) in values.iter().enumerate() {
        array.set(index as u32, value.clone())?;
    }
    Ok(array)
}

fn bytes_to_uint8_array(ctx: &JSContext, bytes: Vec<u8>) -> JSResult<JSTypedArray<u8>> {
    let len = bytes.len();
    let buffer = JSArrayBuffer::from_bytes_owned(ctx, bytes)?;
    JSTypedArray::<u8>::from_array_buffer(ctx, buffer, 0, Some(len))
}

fn define_hidden_value(obj: &JSObject, key: &str, value: JSValue) -> JSResult<()> {
    obj.define_property(
        key,
        PropertyDescriptor::from_value(value)
            .hidden()
            .configurable()
            .writable(),
    )?;
    Ok(())
}

async fn collect_stream_bytes(stream_obj: JSObject) -> JSResult<Vec<u8>> {
    let ctx = stream_obj.context();
    let reader = {
        let stream = stream_obj.borrow::<ReadableStream>()?;
        stream.get_reader()?
    };

    let mut out = Vec::new();
    let result = async {
        loop {
            let chunk: JSObject = reader.read(ctx.clone()).await?;
            let done = chunk.get::<_, bool>("done")?;
            if done {
                break;
            }
            let value = chunk.get::<_, JSValue>("value")?;
            if let Some(bytes) = js_value_to_bytes(&value, "stream chunk")? {
                out.extend_from_slice(&bytes);
            }
        }
        Ok::<(), RongJSError>(())
    }
    .await;

    let _ = reader.release_lock();
    result?;
    Ok(out)
}

fn make_lines_iterator(ctx: &JSContext, text: String) -> JSResult<JSObject> {
    let normalized = text.replace('\r', "");
    let mut lines: VecDeque<String> = normalized
        .split('\n')
        .map(|line| line.to_string())
        .collect();
    if matches!(lines.back(), Some(last) if last.is_empty()) {
        lines.pop_back();
    }

    let queue = Rc::new(RefCell::new(lines));
    let iter = JSObject::new(ctx);
    let queue_for_next = queue.clone();
    iter.set(
        "next",
        JSFunc::new(ctx, move |ctx: JSContext| {
            let queue = queue_for_next.clone();
            async move {
                let result = JSObject::new(&ctx);
                if let Some(line) = queue.borrow_mut().pop_front() {
                    result.set("done", false)?;
                    result.set("value", line)?;
                } else {
                    result.set("done", true)?;
                    result.set("value", JSValue::undefined(&ctx))?;
                }
                Ok(result)
            }
        })?,
    )?;
    rong::install_async_iterator_symbol(ctx, &iter)?;
    Ok(iter)
}

pub(crate) fn decorate_readable(stream_obj: &JSObject) -> JSResult<()> {
    if stream_obj.has_property("__rongProcessReadable")? {
        return Ok(());
    }
    let ctx = stream_obj.context();
    define_hidden_value(
        stream_obj,
        "__rongProcessReadable",
        JSValue::from_rust(&ctx, true),
    )?;

    let bytes_stream = stream_obj.clone();
    define_hidden_value(
        stream_obj,
        "bytes",
        JSFunc::new(&ctx, move |ctx: JSContext| {
            let bytes_stream = bytes_stream.clone();
            async move { bytes_to_uint8_array(&ctx, collect_stream_bytes(bytes_stream).await?) }
        })?
        .name("bytes")?
        .into_js_value(&ctx),
    )?;

    let text_stream = stream_obj.clone();
    define_hidden_value(
        stream_obj,
        "text",
        JSFunc::new(&ctx, move || {
            let text_stream = text_stream.clone();
            async move {
                let bytes = collect_stream_bytes(text_stream).await?;
                Ok(String::from_utf8_lossy(&bytes).to_string())
            }
        })?
        .name("text")?
        .into_js_value(&ctx),
    )?;

    let json_stream = stream_obj.clone();
    define_hidden_value(
        stream_obj,
        "json",
        JSFunc::new(&ctx, move |ctx: JSContext| {
            let json_stream = json_stream.clone();
            async move {
                let bytes = collect_stream_bytes(json_stream).await?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                let json = ctx.global().get::<_, JSObject>("JSON")?;
                let parse = json.get::<_, JSFunc>("parse")?;
                parse.call::<_, JSValue>(Some(json), (text,))
            }
        })?
        .name("json")?
        .into_js_value(&ctx),
    )?;

    let blob_stream = stream_obj.clone();
    define_hidden_value(
        stream_obj,
        "blob",
        JSFunc::new(&ctx, move || {
            let blob_stream = blob_stream.clone();
            async move {
                let bytes = collect_stream_bytes(blob_stream).await?;
                Ok(Blob::from_parts(String::new(), bytes))
            }
        })?
        .name("blob")?
        .into_js_value(&ctx),
    )?;

    let lines_stream = stream_obj.clone();
    define_hidden_value(
        stream_obj,
        "lines",
        JSFunc::new(&ctx, move |ctx: JSContext| {
            let lines_stream = lines_stream.clone();
            async move {
                let bytes = collect_stream_bytes(lines_stream).await?;
                let text = String::from_utf8_lossy(&bytes).to_string();
                make_lines_iterator(&ctx, text)
            }
        })?
        .name("lines")?
        .into_js_value(&ctx),
    )?;

    Ok(())
}

fn decorate_writable(stream_obj: &JSObject) -> JSResult<()> {
    if stream_obj.has_property("__rongProcessWritable")? {
        return Ok(());
    }
    let ctx = stream_obj.context();
    define_hidden_value(
        stream_obj,
        "__rongProcessWritable",
        JSValue::from_rust(&ctx, true),
    )?;

    let writer_slot: Rc<RefCell<Option<Rc<WritableStreamDefaultWriter>>>> =
        Rc::new(RefCell::new(None));

    let write_stream = stream_obj.clone();
    let write_writer_slot = writer_slot.clone();
    define_hidden_value(
        stream_obj,
        "write",
        JSFunc::new(&ctx, move |value: JSValue| {
            let write_stream = write_stream.clone();
            let write_writer_slot = write_writer_slot.clone();
            async move {
                if write_writer_slot.borrow().is_none() {
                    let writer = {
                        let stream = write_stream.borrow::<WritableStream>()?;
                        stream.get_writer()?
                    };
                    *write_writer_slot.borrow_mut() = Some(Rc::new(writer));
                }
                let payload = js_value_to_bytes(&value, "stdin")?.ok_or_else(|| {
                    type_error("stdin must be a string, ArrayBuffer, or TypedArray")
                })?;
                let typed = bytes_to_uint8_array(&write_stream.context(), payload)?;
                let writer = write_writer_slot.borrow().as_ref().cloned();
                if let Some(writer) = writer {
                    writer
                        .write(typed.into_any().into_js_value(&write_stream.context()))
                        .await?;
                }
                Ok(write_stream.clone())
            }
        })?
        .name("write")?
        .into_js_value(&ctx),
    )?;

    define_hidden_value(
        stream_obj,
        "flush",
        JSFunc::new(&ctx, || async move { Ok::<(), RongJSError>(()) })?
            .name("flush")?
            .into_js_value(&ctx),
    )?;

    let end_stream = stream_obj.clone();
    let end_writer_slot = writer_slot;
    define_hidden_value(
        stream_obj,
        "end",
        JSFunc::new(&ctx, move || {
            let end_stream = end_stream.clone();
            let end_writer_slot = end_writer_slot.clone();
            async move {
                if end_writer_slot.borrow().is_none() {
                    let writer = {
                        let stream = end_stream.borrow::<WritableStream>()?;
                        stream.get_writer()?
                    };
                    *end_writer_slot.borrow_mut() = Some(Rc::new(writer));
                }
                let writer = end_writer_slot.borrow().as_ref().cloned();
                if let Some(writer) = writer {
                    writer.close().await?;
                }
                Ok(())
            }
        })?
        .name("end")?
        .into_js_value(&ctx),
    )?;

    Ok(())
}

async fn drain_stream(stream_obj: JSObject, target: ProcessStreamMode) -> JSResult<()> {
    let bytes = collect_stream_bytes(stream_obj).await?;
    if target == ProcessStreamMode::Inherit && !bytes.is_empty() {
        io::write_stdout_bytes_native(&bytes);
    }
    Ok(())
}

async fn drain_stream_to_stderr(stream_obj: JSObject, target: ProcessStreamMode) -> JSResult<()> {
    let bytes = collect_stream_bytes(stream_obj).await?;
    if target == ProcessStreamMode::Inherit && !bytes.is_empty() {
        io::write_stderr_bytes_native(&bytes);
    }
    Ok(())
}

fn setup_abort(child_obj: &JSObject, signal: Option<AbortSignal>, kill_signal: Option<String>) {
    let Some(signal) = signal else {
        return;
    };
    let child_obj = child_obj.clone();
    let signal_name = kill_signal.unwrap_or_else(|| "SIGTERM".to_string());

    if signal.aborted() {
        if let Ok(child) = child_obj.borrow::<child_process::ChildProcess>() {
            let _ = child.kill(Optional(Some(signal_name)));
        }
        return;
    }

    let mut abort_rx = signal.subscribe();
    rong::spawn_local(async move {
        let _ = abort_rx.recv().await;
        if let Ok(child) = child_obj.borrow::<child_process::ChildProcess>() {
            let _ = child.kill(Optional(Some(signal_name)));
        }
    });
}

fn spawn(
    ctx: JSContext,
    cmd_or_options: JSValue,
    options: Optional<JSObject>,
) -> JSResult<JSObject> {
    let normalized = parse_spawn_options(cmd_or_options, options)?;
    let native_options = build_native_spawn_options(&ctx, &normalized)?;
    let args = vec_to_js_array(&ctx, &normalized.cmd[1..])?;
    let child = child_process::spawn_native(
        ctx.clone(),
        normalized.cmd[0].clone(),
        Optional(Some(args.into_js_value(&ctx))),
        Optional(native_options),
    )?;

    if let Ok(stdin) = child.get::<_, JSObject>("stdin") {
        match normalized.stdin.clone() {
            SpawnStdinMode::Pipe => {
                decorate_writable(&stdin)?;
            }
            SpawnStdinMode::AutoClose => {
                let stdin_obj = stdin.clone();
                rong::spawn_local(async move {
                    let _ = decorate_writable(&stdin_obj);
                    let end = stdin_obj.get::<_, JSFunc>("end");
                    if let Ok(end) = end {
                        let _ = end.call_async::<_, ()>(Some(stdin_obj.clone()), ()).await;
                    }
                });
                child.set("stdin", JSValue::null(&ctx))?;
            }
            SpawnStdinMode::Payload(payload) => {
                let stdin_obj = stdin.clone();
                let ctx_for_payload = ctx.clone();
                rong::spawn_local(async move {
                    let _ = decorate_writable(&stdin_obj);
                    if let Ok(write) = stdin_obj.get::<_, JSFunc>("write") {
                        let typed = match bytes_to_uint8_array(&ctx_for_payload, payload) {
                            Ok(typed) => typed,
                            Err(_) => return,
                        };
                        let _ = write
                            .call_async::<_, JSObject>(Some(stdin_obj.clone()), (typed,))
                            .await;
                    }
                    if let Ok(end) = stdin_obj.get::<_, JSFunc>("end") {
                        let _ = end.call_async::<_, ()>(Some(stdin_obj.clone()), ()).await;
                    }
                });
                child.set("stdin", JSValue::null(&ctx))?;
            }
        }
    } else {
        child.set("stdin", JSValue::null(&ctx))?;
    }

    if let Ok(stdout) = child.get::<_, JSObject>("stdout") {
        match normalized.stdout {
            ProcessStreamMode::Pipe => decorate_readable(&stdout)?,
            ProcessStreamMode::Ignore | ProcessStreamMode::Inherit => {
                let stdout_obj = stdout.clone();
                let mode = normalized.stdout;
                rong::spawn_local(async move {
                    let _ = drain_stream(stdout_obj, mode).await;
                });
                child.set("stdout", JSValue::null(&ctx))?;
            }
        }
    }

    if let Ok(stderr) = child.get::<_, JSObject>("stderr") {
        match normalized.stderr {
            ProcessStreamMode::Pipe => decorate_readable(&stderr)?,
            ProcessStreamMode::Ignore | ProcessStreamMode::Inherit => {
                let stderr_obj = stderr.clone();
                let mode = normalized.stderr;
                rong::spawn_local(async move {
                    let _ = drain_stream_to_stderr(stderr_obj, mode).await;
                });
                child.set("stderr", JSValue::null(&ctx))?;
            }
        }
    }

    if let Some(on_exit) = normalized.on_exit {
        let child_obj = child.clone();
        rong::spawn_local(async move {
            let proc = if let Ok(proc) = child_obj.borrow::<child_process::ChildProcess>() {
                proc.clone()
            } else {
                let error: RongJSError =
                    HostError::new(rong::error::E_INTERNAL, "ChildProcess missing").into();
                let _ = on_exit
                    .call_async::<_, JSValue>(
                        None,
                        (
                            child_obj.clone(),
                            JSValue::null(&child_obj.context()),
                            JSValue::null(&child_obj.context()),
                            error.into_catch_value(&child_obj.context()),
                        ),
                    )
                    .await;
                return;
            };
            let wait_result = proc.wait().await;
            match wait_result {
                Ok(code) => {
                    let _ = on_exit
                        .call_async::<_, JSValue>(
                            None,
                            (
                                child_obj.clone(),
                                code,
                                JSValue::null(&child_obj.context()),
                                JSValue::undefined(&child_obj.context()),
                            ),
                        )
                        .await;
                }
                Err(error) => {
                    let _ = on_exit
                        .call_async::<_, JSValue>(
                            None,
                            (
                                child_obj.clone(),
                                JSValue::null(&child_obj.context()),
                                JSValue::null(&child_obj.context()),
                                error.into_catch_value(&child_obj.context()),
                            ),
                        )
                        .await;
                }
            }
        });
    }

    setup_abort(&child, normalized.signal, normalized.kill_signal);
    Ok(child)
}

fn spawn_sync(
    ctx: JSContext,
    cmd_or_options: JSValue,
    options: Optional<JSObject>,
) -> JSResult<JSObject> {
    let normalized = parse_spawn_options(cmd_or_options, options)?;
    let native = JSObject::new(&ctx);
    native.set("cmd", vec_to_js_array(&ctx, &normalized.cmd)?)?;
    if let Some(cwd) = normalized.cwd {
        native.set("cwd", cwd)?;
    }
    if let Some(env) = normalized.env {
        let env_obj = JSObject::new(&ctx);
        for (key, value) in env {
            env_obj.set(key.as_str(), value)?;
        }
        native.set("env", env_obj)?;
    }
    if let Some(shell) = normalized.shell {
        native.set("shell", shell)?;
    }
    if let Some(timeout) = normalized.timeout {
        native.set("timeout", timeout as f64)?;
    }
    match normalized.stdin {
        SpawnStdinMode::Pipe | SpawnStdinMode::AutoClose => {
            native.set("stdin", JSValue::null(&ctx))?;
        }
        SpawnStdinMode::Payload(payload) => {
            native.set("stdin", bytes_to_uint8_array(&ctx, payload)?)?;
        }
    }
    let stdout_mode = match normalized.stdout {
        ProcessStreamMode::Pipe => "pipe",
        ProcessStreamMode::Ignore => "ignore",
        ProcessStreamMode::Inherit => "inherit",
    };
    let stderr_mode = match normalized.stderr {
        ProcessStreamMode::Pipe => "pipe",
        ProcessStreamMode::Ignore => "ignore",
        ProcessStreamMode::Inherit => "inherit",
    };
    native.set("stdout", stdout_mode)?;
    native.set("stderr", stderr_mode)?;

    let result = sync_process::spawn_sync_native(ctx.clone(), native)?;
    let stdout = result.get::<_, JSArrayBuffer>("stdout")?.to_vec();
    let stderr = result.get::<_, JSArrayBuffer>("stderr")?.to_vec();
    result.set("stdout", bytes_to_uint8_array(&ctx, stdout)?)?;
    result.set("stderr", bytes_to_uint8_array(&ctx, stderr)?)?;
    Ok(result)
}

#[cfg(target_os = "windows")]
fn escape_cmd_literal(text: &str) -> String {
    if text.is_empty() {
        return "\"\"".to_string();
    }

    let needs_wrapping =
        text.contains(char::is_whitespace) || text.contains(|c: char| "^&|<>()%".contains(c));

    if !needs_wrapping {
        return text.to_string();
    }

    let escaped = text.replace('%', "%%").replace('"', "\"\"");
    format!("\"{escaped}\"")
}

#[cfg(not(target_os = "windows"))]
fn escape_cmd_literal(text: &str) -> String {
    if text.is_empty() {
        return "''".to_string();
    }

    format!("'{}'", text.replace('\'', "'\\''"))
}

fn shell_escape_value(value: JSValue) -> JSResult<String> {
    if let Some(obj) = value.clone().into_object() {
        if let Some(array) = JSArray::from_object(obj.clone()) {
            let mut parts = Vec::with_capacity(array.len()? as usize);
            for item in array.iter_values()? {
                parts.push(shell_escape_value(item?)?);
            }
            return Ok(parts.join(" "));
        }
        if obj.has_property("raw")? {
            let raw: JSValue = obj.get("raw")?;
            return Ok(raw.to_string());
        }
    }

    let text = value.to_string();
    Ok(escape_cmd_literal(&text))
}

fn compose_shell_command(strings: JSArray, values: Rest<JSValue>) -> JSResult<String> {
    let raw_candidate: JSValue = strings.get("raw")?;
    let raw_strings = if let Some(obj) = raw_candidate.into_object() {
        JSArray::from_object(obj).unwrap_or(strings.clone())
    } else {
        strings.clone()
    };

    let mut command = String::new();
    let cooked = js_array_to_strings(&raw_strings, "template strings")?;
    for (index, chunk) in cooked.iter().enumerate() {
        command.push_str(chunk);
        if let Some(value) = values.0.get(index) {
            command.push_str(&shell_escape_value(value.clone())?);
        }
    }
    Ok(command)
}

fn shell_defaults_snapshot(ctx: &JSContext) -> ShellDefaults {
    ShellDefaults::ensure(ctx)
}

fn shell_default_cwd(ctx: JSContext, path: Optional<String>) -> JSResult<JSValue> {
    let mut state = ShellDefaults::ensure(&ctx);
    if let Some(path) = path.0 {
        state.cwd = Some(path);
        ctx.set_service(state);
        return ctx.host_namespace().get::<_, JSValue>("$");
    }
    Ok(match state.cwd {
        Some(path) => JSValue::from_rust(&ctx, path),
        None => JSValue::undefined(&ctx),
    })
}

fn shell_default_env(ctx: JSContext, env: Optional<JSObject>) -> JSResult<JSValue> {
    let mut state = ShellDefaults::ensure(&ctx);
    if let Some(env_obj) = env.0 {
        state.env = Some(normalize_env_map(&env_obj)?);
        ctx.set_service(state);
        return ctx.host_namespace().get::<_, JSValue>("$");
    }

    if let Some(env_map) = state.env {
        let obj = JSObject::new(&ctx);
        for (key, value) in env_map {
            obj.set(key.as_str(), value)?;
        }
        return Ok(obj.into_js_value());
    }
    Ok(JSValue::undefined(&ctx))
}

fn shell_default_throws(ctx: JSContext, value: Optional<bool>) -> JSResult<JSObject> {
    let mut state = ShellDefaults::ensure(&ctx);
    state.throws = value.0.unwrap_or(true);
    ctx.set_service(state);
    ctx.host_namespace().get("$")
}

fn shell_default_nothrow(ctx: JSContext) -> JSResult<JSObject> {
    let mut state = ShellDefaults::ensure(&ctx);
    state.throws = false;
    ctx.set_service(state);
    ctx.host_namespace().get("$")
}

fn shell_default_quiet(ctx: JSContext) -> JSResult<JSObject> {
    let mut state = ShellDefaults::ensure(&ctx);
    state.quiet = true;
    ctx.set_service(state);
    ctx.host_namespace().get("$")
}

fn shell_escape(value: JSValue) -> JSResult<String> {
    shell_escape_value(value)
}

fn shell_tag(ctx: JSContext, first: JSValue, rest: Rest<JSValue>) -> JSResult<ShellCommand> {
    let command = if let Some(obj) = first.clone().into_object() {
        if let Some(array) = JSArray::from_object(obj) {
            compose_shell_command(array, rest)?
        } else {
            first.to_string()
        }
    } else {
        first.to_string()
    };
    let state = shell_defaults_snapshot(&ctx);
    Ok(ShellCommand {
        command,
        cwd: state.cwd,
        env: state.env,
        throws: state.throws,
        quiet: state.quiet,
    })
}

fn shell_result_to_object(ctx: &JSContext, data: ShellResultData) -> JSResult<JSObject> {
    let result = JSObject::new(ctx);
    result.set("stdout", bytes_to_uint8_array(ctx, data.stdout)?)?;
    result.set("stderr", bytes_to_uint8_array(ctx, data.stderr)?)?;
    result.set("exitCode", data.exit_code)?;
    result.set("success", data.exit_code == Some(0))?;
    Ok(result)
}

impl ShellCommand {
    async fn run_internal(&self, ctx: JSContext) -> JSResult<ShellResultData> {
        let options = JSObject::new(&ctx);
        let mut has_options = false;
        if let Some(cwd) = &self.cwd {
            options.set("cwd", cwd.clone())?;
            has_options = true;
        }
        if let Some(env) = &self.env {
            let env_obj = JSObject::new(&ctx);
            for (key, value) in env {
                env_obj.set(key.as_str(), value.clone())?;
            }
            options.set("env", env_obj)?;
            has_options = true;
        }

        let exec = child_process::exec_native(
            ctx.clone(),
            self.command.clone(),
            Optional(has_options.then_some(options)),
        )?;
        let result = exec.into_future::<JSObject>().await?;
        let stdout = result.get::<_, String>("stdout")?.into_bytes();
        let stderr = result.get::<_, String>("stderr")?.into_bytes();
        let exit_code = result.get::<_, Option<i32>>("code")?;

        let payload = ShellResultData {
            stdout,
            stderr,
            exit_code,
        };

        if payload.exit_code != Some(0) && self.throws {
            return Err(ShellError::new_internal(
                &ctx,
                self.command.clone(),
                payload.exit_code,
                payload.stdout.clone(),
                payload.stderr.clone(),
            )?);
        }

        if !self.quiet && !payload.stderr.is_empty() {
            io::write_stderr_bytes_native(&payload.stderr);
        }

        Ok(payload)
    }

    fn with_overrides(
        &self,
        cwd: Option<Option<String>>,
        env: Option<HashMap<String, String>>,
        throws: Option<bool>,
        quiet: Option<bool>,
    ) -> Self {
        Self {
            command: self.command.clone(),
            cwd: cwd.unwrap_or_else(|| self.cwd.clone()),
            env: env.or_else(|| self.env.clone()),
            throws: throws.unwrap_or(self.throws),
            quiet: quiet.unwrap_or(self.quiet),
        }
    }
}

impl ShellError {
    fn new_internal(
        ctx: &JSContext,
        command: String,
        exit_code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    ) -> JSResult<RongJSError> {
        let message = format!(
            "Shell command failed with exit code {}: {}",
            exit_code
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string()),
            command
        );
        let obj = Class::lookup::<ShellError>(ctx)?.instance(ShellError {
            message,
            command,
            exit_code,
            stdout,
            stderr,
        });
        Ok(RongJSError::from_thrown_value(obj.into_js_value()))
    }
}

#[js_class]
impl ShellError {
    #[js_method(constructor)]
    fn constructor(message: String) -> JSResult<Self> {
        Ok(Self {
            message,
            command: String::new(),
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }

    #[js_method(getter)]
    fn name(&self) -> String {
        "ShellError".to_string()
    }

    #[js_method(getter)]
    fn message(&self) -> String {
        self.message.clone()
    }

    #[js_method(getter)]
    fn command(&self) -> String {
        self.command.clone()
    }

    #[js_method(getter, rename = "exitCode")]
    fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    #[js_method(getter)]
    fn stdout(&self, ctx: JSContext) -> JSResult<JSTypedArray<u8>> {
        bytes_to_uint8_array(&ctx, self.stdout.clone())
    }

    #[js_method(getter)]
    fn stderr(&self, ctx: JSContext) -> JSResult<JSTypedArray<u8>> {
        bytes_to_uint8_array(&ctx, self.stderr.clone())
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

#[js_class]
impl ShellCommand {
    #[js_method(constructor, private)]
    fn constructor() -> JSResult<Self> {
        rong::illegal_constructor("ShellCommand cannot be constructed directly. Use Rong.$.")
    }

    #[js_method]
    fn cwd(&self, path: String) -> Self {
        self.with_overrides(Some(Some(path)), None, None, None)
    }

    #[js_method]
    fn env(&self, values: JSObject) -> JSResult<Self> {
        let mut next = self.env.clone().unwrap_or_default();
        next.extend(normalize_env_map(&values)?);
        Ok(self.with_overrides(None, Some(next), None, None))
    }

    #[js_method]
    fn quiet(&self) -> Self {
        self.with_overrides(None, None, None, Some(true))
    }

    #[js_method]
    fn nothrow(&self) -> Self {
        self.with_overrides(None, None, Some(false), None)
    }

    #[js_method]
    fn throws(&self, value: Optional<bool>) -> Self {
        self.with_overrides(None, None, Some(value.0.unwrap_or(true)), None)
    }

    #[js_method]
    async fn run(&self, ctx: JSContext) -> JSResult<JSObject> {
        shell_result_to_object(&ctx, self.run_internal(ctx.clone()).await?)
    }

    #[js_method]
    async fn text(&self, ctx: JSContext) -> JSResult<String> {
        let result = self.run_internal(ctx).await?;
        Ok(String::from_utf8_lossy(&result.stdout).to_string())
    }

    #[js_method]
    async fn json(&self, ctx: JSContext) -> JSResult<JSValue> {
        let text = self.text(ctx.clone()).await?;
        let json = ctx.global().get::<_, JSObject>("JSON")?;
        let parse = json.get::<_, JSFunc>("parse")?;
        parse.call::<_, JSValue>(Some(json), (text,))
    }

    #[js_method]
    async fn blob(&self, ctx: JSContext) -> JSResult<Blob> {
        let bytes = self.run_internal(ctx).await?.stdout;
        Ok(Blob::from_parts(String::new(), bytes))
    }

    #[js_method]
    async fn lines(&self, ctx: JSContext) -> JSResult<JSObject> {
        make_lines_iterator(&ctx, self.text(ctx.clone()).await?)
    }

    #[js_method]
    fn then(
        &self,
        ctx: JSContext,
        on_fulfilled: Optional<JSFunc>,
        on_rejected: Optional<JSFunc>,
    ) -> JSResult<Promise> {
        let this = self.clone();
        let promise_ctx = ctx.clone();
        Promise::from_future(&ctx, None, async move {
            match this.run_internal(promise_ctx.clone()).await {
                Ok(data) => {
                    let result = shell_result_to_object(&promise_ctx, data)?;
                    if let Some(handler) = on_fulfilled.0 {
                        handler.call_async::<_, JSValue>(None, (result,)).await
                    } else {
                        Ok(result.into_js_value())
                    }
                }
                Err(error) => {
                    if let Some(handler) = on_rejected.0 {
                        handler
                            .call_async::<_, JSValue>(None, (error.into_catch_value(&promise_ctx),))
                            .await
                    } else {
                        Err(error)
                    }
                }
            }
        })
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    let _ = ShellDefaults::ensure(ctx);
    ctx.register_hidden_class::<ShellCommand>()?;
    ctx.register_class::<ShellError>()?;

    let rong = ctx.host_namespace();
    rong.set("spawn", JSFunc::new(ctx, spawn)?.name("spawn")?)?;
    rong.set(
        "spawnSync",
        JSFunc::new(ctx, spawn_sync)?.name("spawnSync")?,
    )?;

    let shell = JSFunc::new(ctx, shell_tag)?.name("$")?;
    shell.set("cwd", JSFunc::new(ctx, shell_default_cwd)?.name("cwd")?)?;
    shell.set("env", JSFunc::new(ctx, shell_default_env)?.name("env")?)?;
    shell.set(
        "throws",
        JSFunc::new(ctx, shell_default_throws)?.name("throws")?,
    )?;
    shell.set(
        "nothrow",
        JSFunc::new(ctx, shell_default_nothrow)?.name("nothrow")?,
    )?;
    shell.set(
        "quiet",
        JSFunc::new(ctx, shell_default_quiet)?.name("quiet")?,
    )?;
    shell.set("escape", JSFunc::new(ctx, shell_escape)?.name("escape")?)?;

    rong.set("$", shell)?;
    let shell_error_ctor: JSObject = ctx.global().get("ShellError")?;
    rong.set("ShellError", shell_error_ctor)?;
    Ok(())
}
