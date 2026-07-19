describe("Rong Environment", () => {
  test("only exposes runtime env through Rong", () => {
    assert.equal(typeof globalThis.process, "undefined");
    assert.equal(typeof globalThis.child_process, "undefined");
    assert.ok(typeof Rong.env === "object");
    assert.ok(Rong.env !== null);
    assert.ok(typeof Rong.version === "string");
    assert.ok(Rong.version.length > 0);
    assert.ok(typeof Rong.revision === "string");
    assert.ok(Rong.revision.length > 0);
  });

  test("exposes runtime version and git revision", () => {
    assert.ok(/^\d+\.\d+\.\d+/.test(Rong.version));
    assert.ok(/^[0-9a-f]{7,40}$|^unknown$/.test(Rong.revision));
  });

  test("Rong.env is a stable mutable object", () => {
    const original = Rong.env;
    Rong.env.RONG_ENV_ALIAS_CHECK = "ok";
    assert.equal(Rong.env.RONG_ENV_ALIAS_CHECK, "ok");
    assert.ok(Rong.env === original);
    delete Rong.env.RONG_ENV_ALIAS_CHECK;
  });

  test("Rong.argv and Rong.args are available", () => {
    const argv = Rong.argv;
    const args = Rong.args;
    assert.ok(Array.isArray(argv));
    assert.ok(Array.isArray(args));
    assert.ok(argv.length >= args.length);
    assert.equal(argv.slice(2).length, args.length);
    for (let index = 0; index < args.length; index += 1) {
      assert.equal(argv[index + 2], args[index]);
    }
  });
});
