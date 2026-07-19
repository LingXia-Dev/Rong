const isWindows = Rong.env.OS === "Windows_NT" || !!Rong.env.ComSpec;
const tempDir = isWindows
  ? (Rong.env.TEMP || Rong.env.TMP || "C:\\Windows\\Temp")
  : "/tmp";

function echoCmd(text) {
  if (isWindows) {
    return ["cmd", "/C", "echo", text];
  }
  return ["/bin/echo", text];
}

function stdinEchoCmd() {
  if (isWindows) {
    return [
      "powershell",
      "-NoProfile",
      "-Command",
      "[Console]::Out.Write([Console]::In.ReadToEnd())",
    ];
  }
  return ["/bin/cat"];
}

function pwdCmd() {
  if (isWindows) {
    return ["cmd", "/C", "cd"];
  }
  return ["/bin/pwd"];
}

describe("Rong Spawn", () => {
  test("does not expose child_process global", () => {
    assert.equal(typeof globalThis.child_process, "undefined");
  });

  test("Rong.spawn() exposes exited promise and decorated stdout", async () => {
    const proc = Rong.spawn(echoCmd("hello-from-rong"));
    const text = await proc.stdout.text();
    const code = await proc.exited;

    assert.ok(text.includes("hello-from-rong"));
    assert.equal(code, 0);
    assert.equal(proc.success, true);
  });

  test("Rong.spawn() supports piped stdin writes", async () => {
    const proc = Rong.spawn(stdinEchoCmd(), { stdin: "pipe" });

    await proc.stdin.write("hello through stdin");
    await proc.stdin.end();

    const text = await proc.stdout.text();
    assert.equal(text, "hello through stdin");
  });

  test("Rong.spawn() supports cwd and onExit", async () => {
    let observed = null;
    const proc = Rong.spawn(pwdCmd(), {
      cwd: tempDir,
      onExit(_proc, exitCode) {
        observed = exitCode;
      },
    });

    const text = await proc.stdout.text();
    await proc.exited;

    assert.ok(text.toLowerCase().includes(tempDir.toLowerCase()));
    assert.equal(observed, 0);
  });

  test("Rong.spawn() supports one-shot stdin payloads", async () => {
    const proc = Rong.spawn(stdinEchoCmd(), { stdin: "payload-once" });
    const text = await proc.stdout.text();
    assert.equal(text, "payload-once");
    assert.equal(await proc.exited, 0);
  });

  test("Rong.env changes propagate into child processes", async () => {
    Rong.env.RONG_ENV_CHILD = "child-visible";
    const proc = Rong.spawn(
      isWindows
        ? ["cmd", "/C", "echo", "%RONG_ENV_CHILD%"]
        : ["sh", "-c", "echo $RONG_ENV_CHILD"],
    );
    const text = await proc.stdout.text();
    assert.ok(text.includes("child-visible"));
    delete Rong.env.RONG_ENV_CHILD;
  });

  test("Rong.spawnSync() returns captured stdout/stderr buffers", () => {
    const result = Rong.spawnSync(echoCmd("sync-rong"));
    const text = new TextDecoder().decode(result.stdout);

    assert.ok(text.includes("sync-rong"));
    assert.equal(result.success, true);
    assert.equal(result.exitCode, 0);
  });

  test("Rong.spawn() accepts object-form options", async () => {
    const proc = Rong.spawn({
      cmd: echoCmd("object-form"),
    });
    assert.ok((await proc.stdout.text()).includes("object-form"));
    assert.equal(await proc.exited, 0);
  });

  test("Rong.spawn() supports stdout: ignore", async () => {
    const proc = Rong.spawn(echoCmd("ignored-output"), { stdout: "ignore" });
    assert.equal(proc.stdout, null);
    assert.equal(await proc.exited, 0);
  });

  test("Rong.spawn() can be killed", async () => {
    const proc = Rong.spawn(
      isWindows
        ? ["powershell", "-NoProfile", "-Command", "Start-Sleep -Seconds 5"]
        : ["sleep", "5"],
    );
    assert.equal(proc.killed, false);
    assert.equal(proc.kill("SIGTERM"), true);
    const code = await proc.exited;
    assert.equal(proc.killed, true);
    assert.ok(code !== 0);
  });

  test("Rong.spawn() supports timeout", async () => {
    const started = Date.now();
    const proc = Rong.spawn(
      isWindows
        ? ["powershell", "-NoProfile", "-Command", "Start-Sleep -Seconds 5"]
        : ["sleep", "5"],
      {
        timeout: 50,
      },
    );
    const code = await proc.exited;
    const elapsed = Date.now() - started;
    assert.ok(elapsed < 2000);
    assert.ok(proc.killed || code !== 0);
  });

  test("Rong.spawnSync() supports cwd and stdin payload", () => {
    const result = Rong.spawnSync({
      cmd: stdinEchoCmd(),
      cwd: tempDir,
      stdin: "sync-stdin",
    });
    assert.equal(new TextDecoder().decode(result.stdout), "sync-stdin");
    assert.equal(result.exitCode, 0);
  });

  test("Rong.spawnSync() supports ignored output", () => {
    const result = Rong.spawnSync(echoCmd("sync-ignore"), { stdout: "ignore" });
    assert.equal(result.stdout.byteLength, 0);
    assert.equal(result.exitCode, 0);
  });
});
