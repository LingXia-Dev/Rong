use crate::{authorize_process, process_authority};
use rong::{
    AnyJSTypedArray, HostError, JSArray, JSArrayBuffer, JSContext, JSObject, JSResult, JSValue,
};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum StreamMode {
    #[default]
    Pipe,
    Ignore,
    Inherit,
}

impl StreamMode {
    fn from_js_object(obj: &JSObject, key: &str, default: StreamMode) -> JSResult<Self> {
        if !obj.has_property(key)? {
            return Ok(default);
        }

        let value: JSValue = obj.get(key)?;
        if value.is_null() || value.is_undefined() {
            return Ok(StreamMode::Ignore);
        }

        let mode: String = value.to_rust().map_err(|_| {
            HostError::new(
                rong::error::E_TYPE,
                format!("options.{key} must be \"pipe\", \"ignore\", or \"inherit\""),
            )
            .with_name("TypeError")
        })?;

        match mode.as_str() {
            "pipe" => Ok(StreamMode::Pipe),
            "ignore" => Ok(StreamMode::Ignore),
            "inherit" => Ok(StreamMode::Inherit),
            _ => Err(HostError::new(
                rong::error::E_TYPE,
                format!("options.{key} must be \"pipe\", \"ignore\", or \"inherit\""),
            )
            .with_name("TypeError")
            .into()),
        }
    }
}

#[derive(Default)]
struct SpawnSyncOptions {
    cmd: Vec<String>,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    shell: bool,
    stdin: Option<Vec<u8>>,
    stdout: StreamMode,
    stderr: StreamMode,
    timeout: Option<u64>,
}

impl SpawnSyncOptions {
    fn from_js_object(obj: &JSObject) -> JSResult<Self> {
        let mut options = Self {
            stdout: StreamMode::Pipe,
            stderr: StreamMode::Pipe,
            ..Default::default()
        };

        let cmd_value: JSValue = obj.get("cmd")?;
        options.cmd = parse_string_array(&cmd_value, "options.cmd")?;
        if options.cmd.is_empty() {
            return Err(
                HostError::new(rong::error::E_INVALID_ARG, "options.cmd cannot be empty")
                    .with_name("TypeError")
                    .into(),
            );
        }

        if obj.has_property("cwd")? {
            options.cwd = Some(obj.get::<_, String>("cwd").map_err(|_| {
                HostError::new(rong::error::E_TYPE, "options.cwd must be a string")
                    .with_name("TypeError")
            })?);
        }

        if obj.has_property("env")? {
            let env_obj = obj.get::<_, JSObject>("env").map_err(|_| {
                HostError::new(rong::error::E_TYPE, "options.env must be an object")
                    .with_name("TypeError")
            })?;
            let entries = env_obj.entries_as::<String, String>().map_err(|_| {
                HostError::new(
                    rong::error::E_TYPE,
                    "options.env must contain string values",
                )
                .with_name("TypeError")
            })?;
            options.env = Some(entries.into_iter().collect());
        }

        if obj.has_property("shell")? {
            options.shell = obj.get::<_, bool>("shell").map_err(|_| {
                HostError::new(rong::error::E_TYPE, "options.shell must be a boolean")
                    .with_name("TypeError")
            })?;
        }

        if obj.has_property("stdin")? {
            let stdin_value: JSValue = obj.get("stdin")?;
            options.stdin = js_value_to_bytes(&stdin_value, "options.stdin")?;
        }

        options.stdout = StreamMode::from_js_object(obj, "stdout", StreamMode::Pipe)?;
        options.stderr = StreamMode::from_js_object(obj, "stderr", StreamMode::Pipe)?;

        if obj.has_property("timeout")? {
            let timeout = obj.get::<_, f64>("timeout").map_err(|_| {
                HostError::new(
                    rong::error::E_TYPE,
                    "options.timeout must be a non-negative number",
                )
                .with_name("TypeError")
            })?;
            if !timeout.is_finite() || timeout < 0.0 {
                return Err(HostError::new(
                    rong::error::E_TYPE,
                    "options.timeout must be a non-negative number",
                )
                .with_name("TypeError")
                .into());
            }
            options.timeout = Some(timeout as u64);
        }

        Ok(options)
    }
}

fn parse_string_array(value: &JSValue, label: &str) -> JSResult<Vec<String>> {
    let Some(obj) = value.clone().into_object() else {
        return Err(HostError::new(
            rong::error::E_TYPE,
            format!("{label} must be an array of strings"),
        )
        .with_name("TypeError")
        .into());
    };

    let Some(array) = JSArray::from_object(obj) else {
        return Err(HostError::new(
            rong::error::E_TYPE,
            format!("{label} must be an array of strings"),
        )
        .with_name("TypeError")
        .into());
    };

    let mut values = Vec::with_capacity(array.len()? as usize);
    for index in 0..array.len()? {
        let item = array.get_opt::<String>(index)?.ok_or_else(|| {
            HostError::new(
                rong::error::E_TYPE,
                format!("{label} must be an array of strings"),
            )
            .with_name("TypeError")
        })?;
        values.push(item);
    }

    Ok(values)
}

fn js_value_to_bytes(value: &JSValue, label: &str) -> JSResult<Option<Vec<u8>>> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }

    if !value.is_object() {
        let text: String = value.clone().to_rust().map_err(|_| {
            HostError::new(
                rong::error::E_TYPE,
                format!("{label} must be a string, ArrayBuffer, or TypedArray"),
            )
            .with_name("TypeError")
        })?;
        return Ok(Some(text.into_bytes()));
    }

    let obj = value.clone().into_object().ok_or_else(|| {
        HostError::new(
            rong::error::E_TYPE,
            format!("{label} must be a string, ArrayBuffer, or TypedArray"),
        )
        .with_name("TypeError")
    })?;

    if let Some(typed_array) = AnyJSTypedArray::from_object(obj.clone()) {
        let bytes = typed_array.byte_view().ok_or_else(|| {
            HostError::new(
                rong::error::E_TYPE,
                format!("{label} contains an invalid TypedArray"),
            )
            .with_name("TypeError")
        })?;
        return Ok(Some(bytes.to_vec()));
    }

    if let Some(array_buffer) = JSArrayBuffer::from_object(obj) {
        return Ok(Some(array_buffer.to_vec()));
    }

    Err(HostError::new(
        rong::error::E_TYPE,
        format!("{label} must be a string, ArrayBuffer, or TypedArray"),
    )
    .with_name("TypeError")
    .into())
}

#[cfg(not(target_os = "windows"))]
fn shell_escape(input: &str) -> String {
    if input.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", input.replace('\'', "'\\''"))
}

fn build_sync_command(options: &SpawnSyncOptions) -> Command {
    let mut command = if options.shell {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C");
            cmd.arg(options.cmd.join(" "));
            cmd
        }

        #[cfg(not(target_os = "windows"))]
        {
            let mut cmd = Command::new("sh");
            cmd.arg("-c");
            let shell_command = options
                .cmd
                .iter()
                .map(|part| shell_escape(part))
                .collect::<Vec<_>>()
                .join(" ");
            cmd.arg(shell_command);
            cmd
        }
    } else {
        let mut cmd = Command::new(&options.cmd[0]);
        cmd.args(&options.cmd[1..]);
        cmd
    };

    if let Some(cwd) = &options.cwd {
        command.current_dir(cwd);
    }

    if let Some(env) = &options.env {
        command.env_clear();
        for (key, value) in env {
            command.env(key, value);
        }
    }

    command.stdin(match options.stdin {
        Some(_) => Stdio::piped(),
        None => Stdio::null(),
    });
    command.stdout(match options.stdout {
        StreamMode::Pipe => Stdio::piped(),
        StreamMode::Ignore => Stdio::null(),
        StreamMode::Inherit => Stdio::inherit(),
    });
    command.stderr(match options.stderr {
        StreamMode::Pipe => Stdio::piped(),
        StreamMode::Ignore => Stdio::null(),
        StreamMode::Inherit => Stdio::inherit(),
    });

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command
}

fn kill_sync_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        let _ = libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn wait_for_sync_child(
    child: &mut std::process::Child,
    timeout: Option<Duration>,
    authority: Option<&dyn crate::ProcessAuthority>,
) -> JSResult<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(authority) = authority
            && let Err(message) = authority.authorize()
        {
            kill_sync_process_tree(child);
            let _ = child.wait();
            return Err(HostError::new(rong::error::E_PERMISSION_DENIED, message).into());
        }
        match child
            .try_wait()
            .map_err(|err| HostError::new(rong::error::E_IO, err.to_string()))?
        {
            Some(status) => return Ok(status),
            None => {
                if timeout.is_some_and(|timeout| start.elapsed() >= timeout) {
                    kill_sync_process_tree(child);
                    return child
                        .wait()
                        .map_err(|err| HostError::new(rong::error::E_IO, err.to_string()).into());
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn read_pipe_to_end<T>(pipe: Option<T>) -> thread::JoinHandle<Vec<u8>>
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

fn build_spawn_sync_result(
    ctx: &JSContext,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> JSResult<JSObject> {
    let result = JSObject::new(ctx);
    result.set("exitCode", exit_code)?;
    result.set("success", exit_code == Some(0))?;
    result.set("signalCode", JSValue::null(ctx))?;
    result.set("stdout", JSArrayBuffer::from_bytes_owned(ctx, stdout)?)?;
    result.set("stderr", JSArrayBuffer::from_bytes_owned(ctx, stderr)?)?;
    Ok(result)
}

pub(crate) fn spawn_sync_native(ctx: JSContext, options: JSObject) -> JSResult<JSObject> {
    authorize_process(&ctx)?;
    let authority = process_authority(&ctx);
    let options = SpawnSyncOptions::from_js_object(&options)?;
    let mut command = build_sync_command(&options);
    let mut child = command
        .spawn()
        .map_err(|err| HostError::new(rong::error::E_IO, err.to_string()))?;

    if let Some(stdin_bytes) = &options.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin
            .write_all(stdin_bytes)
            .map_err(|err| HostError::new(rong::error::E_IO, err.to_string()))?;
    }

    let stdout_task = read_pipe_to_end(child.stdout.take());
    let stderr_task = read_pipe_to_end(child.stderr.take());

    let status = wait_for_sync_child(
        &mut child,
        options.timeout.map(Duration::from_millis),
        authority.as_deref(),
    );

    let stdout = stdout_task.join().unwrap_or_default();
    let stderr = stderr_task.join().unwrap_or_default();
    let status = status?;

    build_spawn_sync_result(&ctx, status.code(), stdout, stderr)
}

pub fn init(ctx: &JSContext) -> JSResult<()> {
    let _ = ctx;
    Ok(())
}

#[cfg(all(test, unix))]
mod authority_tests {
    use super::*;
    use crate::ProcessAuthority;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    struct RevocableAuthority(AtomicBool);

    impl ProcessAuthority for RevocableAuthority {
        fn authorize(&self) -> Result<(), String> {
            self.0
                .load(Ordering::SeqCst)
                .then_some(())
                .ok_or_else(|| "process session revoked".to_string())
        }
    }

    #[test]
    fn revocation_terminates_a_synchronous_child() {
        let authority = Arc::new(RevocableAuthority(AtomicBool::new(true)));
        let mut options = SpawnSyncOptions::default();
        options.cmd = vec!["sh".to_string(), "-c".to_string(), "sleep 30".to_string()];
        let mut child = build_sync_command(&options)
            .spawn()
            .expect("spawn test child");
        let authority_for_revoke = authority.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            authority_for_revoke.0.store(false, Ordering::SeqCst);
        });

        let result = wait_for_sync_child(&mut child, None, Some(authority.as_ref()));
        assert!(result.is_err());
    }
}
