//! Native child-process helpers used by `rong_command`.

use rong::{
    HostError, JSArray, JSContext, JSContextService, JSObject, JSResult, JSValue, Promise,
    function::{Optional, Rest, This},
    js_class, js_method,
};
use rong_event::{Emitter, EmitterExt, EventEmitter};
use rong_stream::{JSReadableStream, JSWritableStream};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::sync::Notify;

#[cfg(windows)]
use tokio::sync::mpsc;

#[cfg(windows)]
enum ChildCommand {
    Kill,
}

#[derive(Clone, Default)]
struct ChildProcessTaskRegistry {
    handles: Rc<RefCell<Vec<tokio::task::JoinHandle<()>>>>,
}

impl ChildProcessTaskRegistry {
    fn ensure(ctx: &JSContext) -> Self {
        if let Some(registry) = ctx.get_service::<Self>() {
            return registry.clone();
        }

        let registry = Self::default();
        ctx.set_service(registry.clone());
        registry
    }

    fn track(&self, handle: tokio::task::JoinHandle<()>) {
        let mut handles = self.handles.borrow_mut();
        handles.retain(|task| !task.is_finished());
        handles.push(handle);
    }
}

impl JSContextService for ChildProcessTaskRegistry {
    fn on_shutdown(&self) {
        for handle in self.handles.borrow_mut().drain(..) {
            handle.abort();
        }
    }
}

fn type_error(message: impl Into<String>) -> HostError {
    HostError::new(rong::error::E_TYPE, message).with_name("TypeError")
}

/// Options for spawn/exec.
#[derive(Default, Clone)]
pub struct SpawnOptions {
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub shell: Option<bool>,
    pub timeout: Option<u64>, // timeout in milliseconds
}

impl SpawnOptions {
    fn from_js_object(_ctx: &JSContext, obj: &JSObject) -> JSResult<Self> {
        let mut opts = SpawnOptions::default();
        if obj.has_property("cwd")? {
            let cwd = obj
                .get::<_, String>("cwd")
                .map_err(|_| type_error("options.cwd must be a string"))?;
            opts.cwd = Some(cwd);
        }
        if obj.has_property("shell")? {
            let shell = obj
                .get::<_, bool>("shell")
                .map_err(|_| type_error("options.shell must be a boolean"))?;
            opts.shell = Some(shell);
        }
        if obj.has_property("env")? {
            let env_obj = obj
                .get::<_, JSObject>("env")
                .map_err(|_| type_error("options.env must be an object of string values"))?;
            let entries = env_obj
                .entries_as::<String, String>()
                .map_err(|_| type_error("options.env must be an object of string values"))?;
            let mut env_map = HashMap::with_capacity(entries.len());
            for (k, v) in entries {
                env_map.insert(k, v);
            }
            opts.env = Some(env_map);
        }
        if obj.has_property("timeout")? {
            let timeout = obj
                .get::<_, f64>("timeout")
                .map_err(|_| type_error("options.timeout must be a non-negative number"))?;
            if !timeout.is_finite() || timeout < 0.0 {
                return Err(type_error("options.timeout must be a non-negative number").into());
            }
            opts.timeout = Some(timeout as u64);
        }
        Ok(opts)
    }
}

/// ChildProcess class representing a spawned child process
#[js_class]
pub struct ChildProcess {
    events: EventEmitter,
    pid: Option<u32>,
    exit_code: Arc<Mutex<Option<i32>>>,
    exit_notify: Arc<Notify>,
    exited: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
    #[cfg(windows)]
    kill_tx: Option<mpsc::Sender<ChildCommand>>,
}

impl ChildProcess {
    pub fn new() -> Self {
        Self {
            events: EventEmitter::new(),
            pid: None,
            exit_code: Arc::new(Mutex::new(None)),
            exit_notify: Arc::new(Notify::new()),
            exited: Arc::new(AtomicBool::new(false)),
            killed: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            kill_tx: None,
        }
    }
}

#[js_class]
impl ChildProcess {
    #[js_method(constructor, private)]
    fn constructor() -> JSResult<Self> {
        rong::illegal_constructor("ChildProcess cannot be constructed directly. Use Rong.spawn().")
    }

    #[js_method(getter)]
    fn pid(&self) -> Option<u32> {
        self.pid
    }

    #[js_method(getter, rename = "exitCode")]
    fn exit_code(&self) -> Option<i32> {
        self.exit_code.lock().ok().and_then(|g| *g)
    }

    #[js_method(getter)]
    fn killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    #[js_method(getter, rename = "signalCode")]
    fn signal_code(&self) -> Option<i32> {
        None
    }

    #[js_method(getter)]
    fn success(&self) -> bool {
        self.exit_code() == Some(0)
    }

    #[js_method(getter)]
    fn exited(&self, ctx: JSContext) -> JSResult<Promise> {
        let this = self.clone();
        Promise::from_future(&ctx, None, async move { this.wait().await })
    }

    /// Kill the child process with optional signal.
    /// Supported signals: SIGTERM (default), SIGKILL, SIGINT, SIGHUP, SIGUSR1, SIGUSR2
    /// Returns true if the signal was sent successfully.
    #[js_method]
    pub(crate) fn kill(&self, signal: Optional<String>) -> bool {
        let Some(pid) = self.pid else {
            return false;
        };

        #[cfg(unix)]
        {
            let sig = match signal.0.as_deref() {
                None | Some("SIGTERM") => libc::SIGTERM,
                Some("SIGKILL") => libc::SIGKILL,
                Some("SIGINT") => libc::SIGINT,
                Some("SIGHUP") => libc::SIGHUP,
                Some("SIGUSR1") => libc::SIGUSR1,
                Some("SIGUSR2") => libc::SIGUSR2,
                Some("SIGQUIT") => libc::SIGQUIT,
                Some("SIGSTOP") => libc::SIGSTOP,
                Some("SIGCONT") => libc::SIGCONT,
                // Also support numeric signals or without SIG prefix
                Some(s) => {
                    if let Some(stripped) = s.strip_prefix("SIG") {
                        match stripped {
                            "TERM" => libc::SIGTERM,
                            "KILL" => libc::SIGKILL,
                            "INT" => libc::SIGINT,
                            "HUP" => libc::SIGHUP,
                            "USR1" => libc::SIGUSR1,
                            "USR2" => libc::SIGUSR2,
                            "QUIT" => libc::SIGQUIT,
                            "STOP" => libc::SIGSTOP,
                            "CONT" => libc::SIGCONT,
                            _ => return false,
                        }
                    } else if let Ok(num) = s.parse::<i32>() {
                        num
                    } else {
                        return false;
                    }
                }
            };
            // SAFETY: pid is valid process id obtained from spawn
            let ok = unsafe { libc::kill(pid as i32, sig) == 0 };
            if ok {
                self.killed.store(true, Ordering::SeqCst);
            }
            ok
        }

        #[cfg(windows)]
        {
            let _ = pid;
            let _ = signal;
            if let Some(tx) = &self.kill_tx {
                let ok = tx.try_send(ChildCommand::Kill).is_ok();
                if ok {
                    self.killed.store(true, Ordering::SeqCst);
                }
                return ok;
            }
            false
        }
    }

    /// Wait for the process to exit and return the exit code.
    #[js_method]
    pub(crate) async fn wait(&self) -> JSResult<Option<i32>> {
        loop {
            let notified = self.exit_notify.notified();
            if self.exited.load(Ordering::SeqCst) {
                break;
            }
            notified.await;
        }
        Ok(self.exit_code.lock().ok().and_then(|g| *g))
    }

    #[js_method]
    fn unref(&self) {}

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
        self.events.gc_mark_with(mark_fn);
    }
}

impl Default for ChildProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl Emitter for ChildProcess {
    fn get_event_emitter(&self) -> EventEmitter {
        self.events.clone()
    }
}

/// Result of exec command
#[js_class]
pub struct ExecResult {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

impl ExecResult {
    pub fn new() -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
            code: None,
        }
    }
}

#[js_class]
impl ExecResult {
    #[js_method(constructor, private)]
    fn constructor() -> JSResult<Self> {
        rong::illegal_constructor(
            "ExecResult cannot be constructed directly. Use Rong.$() or Rong.spawn().",
        )
    }

    #[js_method(getter)]
    fn stdout(&self) -> String {
        self.stdout.clone()
    }

    #[js_method(getter)]
    fn stderr(&self) -> String {
        self.stderr.clone()
    }

    #[js_method(getter)]
    fn code(&self) -> Option<i32> {
        self.code
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

impl Default for ExecResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape a string for safe use in shell commands (Unix)
#[cfg(not(target_os = "windows"))]
fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Read Rong.env from the host namespace and convert to a Rust map.
/// This allows JS modifications to propagate to child processes.
fn get_process_env(ctx: &JSContext) -> Option<HashMap<String, String>> {
    let rong = ctx.host_namespace();
    let env_obj: JSObject = rong.get("env").ok()?;

    let mut env_map = HashMap::new();
    if let Ok(entries) = env_obj.entries_as::<String, String>() {
        for (k, v) in entries {
            env_map.insert(k, v);
        }
    }
    Some(env_map)
}

/// Build a Command from the given parameters
fn build_command(
    command: &str,
    args: &[String],
    options: &SpawnOptions,
    use_shell: bool,
) -> Command {
    let mut cmd = if use_shell {
        #[cfg(target_os = "windows")]
        {
            let mut c = Command::new("cmd");
            c.arg("/C");
            let full_cmd = if args.is_empty() {
                command.to_string()
            } else {
                let escaped_args: Vec<String> = args
                    .iter()
                    .map(|a| {
                        if a.contains(|c: char| c.is_whitespace() || "\"&|<>^".contains(c)) {
                            format!("\"{}\"", a.replace('"', "\"\""))
                        } else {
                            a.clone()
                        }
                    })
                    .collect();
                format!("{} {}", command, escaped_args.join(" "))
            };
            c.arg(full_cmd);
            c
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut c = Command::new("sh");
            c.arg("-c");
            let full_cmd = if args.is_empty() {
                command.to_string()
            } else {
                let escaped_args: Vec<String> = args.iter().map(|a| shell_escape(a)).collect();
                format!("{} {}", command, escaped_args.join(" "))
            };
            c.arg(full_cmd);
            c
        }
    } else {
        let mut c = Command::new(command);
        c.args(args);
        c
    };

    if let Some(ref cwd) = options.cwd {
        cmd.current_dir(cwd);
    }

    if let Some(ref env) = options.env {
        cmd.env_clear();
        for (key, value) in env {
            cmd.env(key, value);
        }
    }

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);

    cmd
}

/// Helper to parse args from Optional<JSValue>
fn parse_args(args: &Optional<JSValue>) -> JSResult<Vec<String>> {
    if let Some(ref args_val) = args.0 {
        if args_val.is_null() || args_val.is_undefined() {
            return Ok(vec![]);
        }
        if let Some(arr) = args_val
            .clone()
            .into_object()
            .and_then(JSArray::from_object)
        {
            let mut result = Vec::new();
            for i in 0..arr.len()? {
                let val = arr
                    .get_opt::<String>(i)?
                    .ok_or_else(|| type_error("args must be an array of strings"))?;
                result.push(val);
            }
            return Ok(result);
        }
        return Err(type_error("args must be an array of strings").into());
    }
    Ok(vec![])
}

const STREAM_CHUNK_SIZE: usize = 8192;

#[cfg(unix)]
fn configure_timeout_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn configure_timeout_process_group(_cmd: &mut Command) {}

#[cfg(unix)]
fn kill_child_process_group(pid: u32) {
    // Negative pid targets the process group created via `process_group(0)`.
    unsafe {
        let _ = libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(windows)]
async fn terminate_child_process(child: &mut Child) {
    // `cmd /C` can outlive its direct child on timeout unless we terminate the tree.
    let terminated_tree = if let Some(pid) = child.id() {
        Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|status| status.success())
            .unwrap_or(false)
    } else {
        false
    };

    if !terminated_tree {
        let _ = child.start_kill();
    }
}

#[cfg(all(not(unix), not(windows)))]
fn kill_child_process_group(_pid: u32) {}

async fn read_all(
    mut stdout: Option<impl tokio::io::AsyncRead + Unpin>,
) -> Result<Vec<u8>, std::io::Error> {
    use tokio::io::AsyncReadExt;

    let mut buf = Vec::new();
    if let Some(ref mut out) = stdout {
        out.read_to_end(&mut buf).await?;
    }
    Ok(buf)
}

async fn run_command_with_output(
    mut child: Child,
    timeout: Option<u64>,
) -> JSResult<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    #[cfg(not(windows))]
    let child_pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_task = tokio::task::spawn_local(async move { read_all(stdout).await });
    let stderr_task = tokio::task::spawn_local(async move { read_all(stderr).await });

    let status = if let Some(timeout_ms) = timeout {
        match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait()).await {
            Ok(res) => res.map_err(|e| HostError::new(rong::error::E_IO, e.to_string()))?,
            Err(_) => {
                #[cfg(windows)]
                terminate_child_process(&mut child).await;

                #[cfg(not(windows))]
                {
                    let _ = child.start_kill();
                    if let Some(pid) = child_pid {
                        kill_child_process_group(pid);
                    }
                }
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(HostError::new(rong::error::E_TIMEOUT, "Command timed out").into());
            }
        }
    } else {
        child
            .wait()
            .await
            .map_err(|e| HostError::new(rong::error::E_IO, e.to_string()))?
    };

    let stdout_bytes = stdout_task
        .await
        .map_err(|e| HostError::new(rong::error::E_INTERNAL, e.to_string()))?
        .map_err(|e| HostError::new(rong::error::E_IO, e.to_string()))?;
    let stderr_bytes = stderr_task
        .await
        .map_err(|e| HostError::new(rong::error::E_INTERNAL, e.to_string()))?
        .map_err(|e| HostError::new(rong::error::E_IO, e.to_string()))?;

    Ok((status, stdout_bytes, stderr_bytes))
}

/// spawn(command, args?, options?) - Spawn a new process and return ChildProcess
/// This is now truly async - returns immediately with streaming stdout/stderr
pub(crate) fn spawn_native(
    ctx: JSContext,
    command: String,
    args: Optional<JSValue>,
    options: Optional<JSObject>,
) -> JSResult<JSObject> {
    let args_vec = parse_args(&args)?;

    let mut opts = if let Some(ref opts_obj) = options.0 {
        SpawnOptions::from_js_object(&ctx, opts_obj)?
    } else {
        SpawnOptions::default()
    };

    // If no explicit env option, use Rong.env so JS modifications propagate.
    if opts.env.is_none() {
        opts.env = get_process_env(&ctx);
    }

    let use_shell = opts.shell.unwrap_or(false);
    let mut cmd = build_command(&command, &args_vec, &opts, use_shell);

    let mut child = cmd
        .spawn()
        .map_err(|e| HostError::new(rong::error::E_IO, e.to_string()))?;

    let pid = child.id();

    // Take stdin/stdout/stderr for streaming
    let stdin_writer = child.stdin.take();
    let stdout_reader = child.stdout.take();
    let stderr_reader = child.stderr.take();
    let task_registry = ChildProcessTaskRegistry::ensure(&ctx);

    // Create ChildProcess instance
    let mut child_process = ChildProcess::new();
    child_process.pid = pid;
    let exit_events = child_process.events.clone();

    // Clones for the background wait task.
    let exit_code = child_process.exit_code.clone();
    let exit_notify = child_process.exit_notify.clone();
    let exited = child_process.exited.clone();
    let killed = child_process.killed.clone();
    let timeout = opts.timeout;

    #[cfg(windows)]
    let mut kill_rx = {
        let (tx, rx) = mpsc::channel::<ChildCommand>(4);
        child_process.kill_tx = Some(tx);
        rx
    };

    // Create the JS object
    let child_obj = JSValue::from_rust(&ctx, child_process);
    let child_obj: JSObject = child_obj.to_rust()?;

    // Create WritableStream for stdin
    if let Some(stdin) = stdin_writer {
        let stdin_stream = JSWritableStream::from_async_writer(&ctx, stdin)?;
        child_obj.set("stdin", stdin_stream.into_object())?;
    } else {
        child_obj.set("stdin", JSValue::null(&ctx))?;
    }

    // Create ReadableStream for stdout
    if let Some(stdout) = stdout_reader {
        let stdout_stream = JSReadableStream::from_async_reader(&ctx, stdout, STREAM_CHUNK_SIZE)?;
        child_obj.set("stdout", stdout_stream.into_object())?;
    } else {
        child_obj.set("stdout", JSValue::null(&ctx))?;
    }

    // Create ReadableStream for stderr
    if let Some(stderr) = stderr_reader {
        let stderr_stream = JSReadableStream::from_async_reader(&ctx, stderr, STREAM_CHUNK_SIZE)?;
        child_obj.set("stderr", stderr_stream.into_object())?;
    } else {
        child_obj.set("stderr", JSValue::null(&ctx))?;
    }

    // Start background wait task to emit 'exit' and resolve waiters
    let ctx_for_exit = ctx.clone();
    let child_obj_for_exit = child_obj.clone();

    let wait_task = rong::spawn_local(async move {
        let emit_exit = |code: Option<i32>| {
            if let Ok(mut ec) = exit_code.lock() {
                *ec = code;
            }
            exited.store(true, Ordering::SeqCst);
            exit_notify.notify_waiters();

            let exit_key = rong_event::EventKey::from("exit");
            if !exit_events.has_listeners(&exit_key) {
                return;
            }

            let code_val = match code {
                Some(code) => JSValue::from_rust(&ctx_for_exit, code),
                None => JSValue::null(&ctx_for_exit),
            };
            let _ = ChildProcess::do_emit(
                This(child_obj_for_exit.clone()),
                exit_key,
                Rest(vec![code_val]),
            );
        };

        #[cfg(windows)]
        {
            if let Some(timeout_ms) = timeout {
                let sleep = tokio::time::sleep(Duration::from_millis(timeout_ms));
                tokio::pin!(sleep);

                loop {
                    tokio::select! {
                        status = child.wait() => {
                            let code = status.ok().and_then(|s| s.code());
                            emit_exit(code);
                            break;
                        }
                        _ = &mut sleep => {
                            killed.store(true, Ordering::SeqCst);
                            terminate_child_process(&mut child).await;
                            let status = child.wait().await;
                            let code = status.ok().and_then(|s| s.code());
                            emit_exit(code);
                            break;
                        }
                        cmd = kill_rx.recv() => {
                            match cmd {
                                Some(ChildCommand::Kill) => {
                                    terminate_child_process(&mut child).await;
                                }
                                None => {
                                    break;
                                }
                            }
                        }
                    }
                }
            } else {
                loop {
                    tokio::select! {
                        status = child.wait() => {
                            let code = status.ok().and_then(|s| s.code());
                            emit_exit(code);
                            break;
                        }
                        cmd = kill_rx.recv() => {
                            match cmd {
                                Some(ChildCommand::Kill) => {
                                    terminate_child_process(&mut child).await;
                                }
                                None => {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if !exited.load(Ordering::SeqCst) {
                let status = child.wait().await;
                let code = status.ok().and_then(|s| s.code());
                emit_exit(code);
            }
        }

        #[cfg(not(windows))]
        {
            if let Some(timeout_ms) = timeout {
                let sleep = tokio::time::sleep(Duration::from_millis(timeout_ms));
                tokio::pin!(sleep);

                tokio::select! {
                    status = child.wait() => {
                        let code = status.ok().and_then(|s| s.code());
                        emit_exit(code);
                    }
                    _ = &mut sleep => {
                        killed.store(true, Ordering::SeqCst);
                        let _ = child.start_kill();
                        if let Some(pid) = pid {
                            kill_child_process_group(pid);
                        }
                        let status = child.wait().await;
                        let code = status.ok().and_then(|s| s.code());
                        emit_exit(code);
                    }
                }
            } else {
                let status = child.wait().await;
                let code = status.ok().and_then(|s| s.code());
                emit_exit(code);
            }
        }
    });
    task_registry.track(wait_task);

    Ok(child_obj)
}

/// exec(command, options?) - Execute a shell command and return a promise
pub(crate) fn exec_native(
    ctx: JSContext,
    command: String,
    options: Optional<JSObject>,
) -> JSResult<Promise> {
    let mut opts = if let Some(ref opts_obj) = options.0 {
        SpawnOptions::from_js_object(&ctx, opts_obj)?
    } else {
        SpawnOptions::default()
    };

    // If no explicit env option, use Rong.env so JS modifications propagate.
    if opts.env.is_none() {
        opts.env = get_process_env(&ctx);
    }

    let cwd = opts.cwd.clone();
    let env = opts.env.clone();
    let timeout = opts.timeout;

    Promise::from_future(&ctx, None, async move {
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(&command);
            c
        };

        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.arg("-c").arg(&command);
            c
        };

        if let Some(ref cwd) = cwd {
            cmd.current_dir(cwd);
        }

        if let Some(ref env) = env {
            cmd.env_clear();
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        if timeout.is_some() {
            configure_timeout_process_group(&mut cmd);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| HostError::new(rong::error::E_IO, e.to_string()))?;
        let (status, stdout, stderr) = run_command_with_output(child, timeout).await?;

        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            code: status.code(),
        })
    })
}

/// Initialize native child-process helpers for the Rong namespace.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    let _ = ChildProcessTaskRegistry::ensure(ctx);

    ctx.register_hidden_class::<ChildProcess>()?;
    ctx.register_hidden_class::<ExecResult>()?;

    ChildProcess::add_node_event_target_prototype(ctx)?;

    Ok(())
}
