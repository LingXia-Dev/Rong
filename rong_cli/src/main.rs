use rong::*;
use std::env;
use std::path::PathBuf;
use std::process;

mod completer;
mod logging;
mod repl;

#[derive(Debug)]
enum Command {
    Run { path: PathBuf },
    Compile { input: PathBuf, output: PathBuf },
    Repl,
    Help,
    Version,
}

fn usage() {
    println!(
        r#"
Usage:
  rong                      Start REPL (or run stdin when piped)
  rong <file.js|file.rong>
  rong compile <input.js> <output.rong>
  rong -v | --version
  rong -h | --help

Commands:
  compile     Compile JavaScript to bytecode

Options:
  -v, --version     Print version information
  -h, --help        Print this help message
"#
    );
}

fn init_cli_namespace(ctx: &JSContext) -> JSResult<()> {
    fn exit(status: u32) {
        process::exit(status as i32);
    }

    let rong = ctx.host_namespace();
    rong.set("exit", JSFunc::new(ctx, exit)?.name("exit")?)?;
    Ok(())
}

fn format_runtime_error(ctx: &JSContext, err: RongJSError) -> RongJSError {
    if let Some(thrown) = err.thrown_value(ctx)
        && let Some(obj) = thrown.into_object()
    {
        let name = obj
            .get::<_, String>("name")
            .unwrap_or_else(|_| "Error".to_string());
        if let Ok(message) = obj.get::<_, String>("message") {
            return HostError::new(rong::error::E_JS_THROWN, format!("{name}: {message}")).into();
        }
    }
    err
}

fn parse_args_from(args: Vec<String>) -> Result<Command, String> {
    if args.len() < 2 {
        return Ok(Command::Repl);
    }

    let index = 1;

    match args[index].as_str() {
        "-h" | "--help" => Ok(Command::Help),
        "-v" | "--version" => Ok(Command::Version),
        "compile" => {
            if args.len() < index + 3 {
                return Err("Missing input/output arguments for compile command".to_string());
            }
            Ok(Command::Compile {
                input: args[index + 1].clone().into(),
                output: args[index + 2].clone().into(),
            })
        }
        _ => {
            // If the argument looks like a file path (ends with .js or .rong), execute it.
            let arg = &args[index];
            if arg.ends_with(".js") || arg.ends_with(".rong") {
                Ok(Command::Run {
                    path: arg.clone().into(),
                })
            } else {
                Err(format!("Unknown command: {}", args[index]))
            }
        }
    }
}

fn parse_args() -> Result<Command, String> {
    parse_args_from(env::args().collect())
}

async fn run_file(ctx: &JSContext, path: PathBuf) -> Result<(), RongJSError> {
    let source = Source::from_path(ctx, &path).await?;

    // For JavaScript files, wrap top-level await in async IIFE
    match source.kind() {
        rong::SourceKind::JavaScript(code) => {
            let code_str = String::from_utf8_lossy(code);
            let wrapped = if code_str.contains("await ")
                && !code_str.contains("async function")
                && !code_str.contains("async (")
            {
                format!("(async () => {{ {} }})();", code_str)
            } else {
                code_str.to_string()
            };
            ctx.eval_async::<()>(Source::from_bytes(wrapped)).await
        }
        _ => ctx.eval_async::<()>(source).await,
    }
}

async fn compile_file(ctx: &JSContext, input: PathBuf, output: PathBuf) -> Result<(), RongJSError> {
    // 1. Load the source file
    let source = Source::from_path(ctx, &input).await?;

    // 2. Check source kind and get JavaScript code
    let js_code = match source.kind() {
        SourceKind::JavaScript(code) => code,
        SourceKind::ByteCode(_) => {
            return Err(HostError::new(
                rong::error::E_INVALID_ARG,
                "Cannot compile already compiled bytecode",
            )
            .with_name("TypeError")
            .into());
        }
    };

    // 3. Compile to bytecode
    let bytecode_source = ctx.compile_to_bytecode(js_code)?;

    // 4. Save the bytecode
    bytecode_source.save_bytecode(ctx, output).await
}

#[tokio::main]
async fn main() -> Result<(), RongJSError> {
    logging::init_tracing();

    let command = match parse_args() {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("Error: {}", err);
            usage();
            return Ok(());
        }
    };

    // Create a single Rong worker pool for commands that execute JS
    if matches!(command, Command::Help | Command::Version) {
        // No need to create a Rong worker pool for these simple commands
        match command {
            Command::Help => {
                usage();
            }
            Command::Version => {
                println!("rong v{}", env!("CARGO_PKG_VERSION"));
            }
            _ => unreachable!(),
        }
    } else {
        // Give workers the same module set as the main context
        rong_modules::worker::set_initializer(rong_modules::init_all);

        // For commands that need JS execution, use a single Rong worker pool
        Rong::<RongJS>::builder()
            .shared()
            .build()?
            .call(|runtime, _receiver| async move {
                let ctx = runtime.context();
                // Initialize all modules
                rong_modules::init_all(&ctx)?;
                init_cli_namespace(&ctx)?;

                // Process the command with the initialized context
                let result = match command {
                    Command::Repl => repl::run(&ctx).await,
                    Command::Run { path } => run_file(&ctx, path).await,
                    Command::Compile { input, output } => compile_file(&ctx, input, output).await,
                    _ => unreachable!(), // Help and Version already handled
                };
                result.map_err(|err| format_runtime_error(&ctx, err))
            })
            .await?
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_run_command() {
        let command =
            parse_args_from(vec!["rong".to_string(), "app.js".to_string()]).expect("parse args");

        match command {
            Command::Run { path } => assert_eq!(path, PathBuf::from("app.js")),
            other => panic!("unexpected command: {other:?}"),
        }
    }
}
