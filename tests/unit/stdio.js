describe("Rong stdio", () => {
  test("exposes runtime stdio on Rong without leaking globals", async () => {
    assert.equal(typeof globalThis.stdin, "undefined");
    assert.equal(typeof globalThis.stdout, "undefined");
    assert.equal(typeof globalThis.stderr, "undefined");

    assert.equal(typeof Rong.stdin.text, "function");
    assert.equal(typeof Rong.stdin.bytes, "function");
    assert.equal(typeof Rong.stdout.write, "function");
    assert.equal(typeof Rong.stdout.flush, "function");
    assert.equal(typeof Rong.stderr.write, "function");
    assert.equal(typeof Rong.stderr.flush, "function");

    assert.equal(Rong.stdout.write("hello"), undefined);
    Rong.stdout.write(Uint8Array.of(45, 111, 117, 116).buffer);
    assert.equal(Rong.stdout.flush(), undefined);

    assert.equal(Rong.stderr.write("warn"), undefined);
    Rong.stderr.write(Uint8Array.of(45, 101, 114, 114));
    assert.equal(Rong.stderr.flush(), undefined);

    if (globalThis.__stdioMode === "text") {
      assert.equal(await Rong.stdin.text(), "stdin-payload");
    } else if (globalThis.__stdioMode === "bytes") {
      assert.equal(
        new TextDecoder().decode(await Rong.stdin.bytes()),
        "stdin-payload",
      );
    } else {
      throw new Error(`Unexpected __stdioMode: ${globalThis.__stdioMode}`);
    }
  });
});
