const isWindows = Rong.env.OS === "Windows_NT" || !!Rong.env.ComSpec;
const tempDir = isWindows
  ? (Rong.env.TEMP || Rong.env.TMP || "C:\\Windows\\Temp")
  : "/tmp";

describe("Rong Shell", () => {
  test("Rong.$ supports text() and escaped interpolation", async () => {
    const message = "shell hello world";
    const text = await Rong.$`echo ${message}`.text();
    assert.ok(text.includes(message));
  });

  test("Rong.$ supports json()", async () => {
    const command = isWindows ? "echo [true,2]" : "echo [true,2]";
    const value = await Rong.$(command).json();
    assert.equal(value[0], true);
    assert.equal(value[1], 2);
  });

  test("Rong.$ supports nothrow()", async () => {
    const result = await Rong.$(
      isWindows ? "cmd /C exit 7" : "sh -c 'exit 7'",
    ).nothrow();
    assert.equal(result.exitCode, 7);
    assert.equal(result.success, false);
  });

  test("Rong.$ supports cwd()", async () => {
    const text = await Rong.$(isWindows ? "cd" : "pwd").cwd(tempDir).text();
    assert.ok(text.toLowerCase().includes(tempDir.toLowerCase()));
  });

  test("Rong.$ supports blob()", async () => {
    const message = "blob-shell-output";
    const blob = await Rong.$`echo ${message}`.blob();
    const text = await blob.text();
    assert.ok(text.includes(message));
  });

  test("Rong.$ throws ShellError with command metadata", async () => {
    let error = null;
    try {
      await Rong.$(isWindows ? "cmd /C exit 9" : "sh -c 'exit 9'");
    } catch (thrown) {
      error = thrown;
    }

    assert.ok(error);
    assert.equal(error.name, "ShellError");
    assert.equal(error.exitCode, 9);
    assert.equal(error.command, isWindows ? "cmd /C exit 9" : "sh -c 'exit 9'");
    assert.ok(error.stderr instanceof Uint8Array);
    assert.ok(error.stdout instanceof Uint8Array);
  });
});
