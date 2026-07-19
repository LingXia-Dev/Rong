describe("Rong.sleep", () => {
  test("exposes Bun-aligned sleep APIs on Rong", () => {
    assert.ok(typeof Rong.sleep === "function");
    assert.ok(typeof Rong.sleepSync === "function");
    assert.equal(globalThis.timers, undefined);
  });

  test("resolves after the requested delay", async () => {
    const start = Date.now();
    await Rong.sleep(80);
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 75 && elapsed <= 180,
      `Rong.sleep should wait about 80ms, got ${elapsed}ms`,
    );
  });

  test("resolves immediately for zero and negative delays", async () => {
    let start = Date.now();
    await Rong.sleep(0);
    let elapsed = Date.now() - start;
    assert.ok(elapsed >= 0 && elapsed < 50, "zero delay should resolve quickly");

    start = Date.now();
    await Rong.sleep(-25);
    elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 0 && elapsed < 50,
      "negative delay should be treated as immediate",
    );
  });

  test("accepts a Date target for async sleep", async () => {
    const start = Date.now();
    await Rong.sleep(new Date(start + 70));
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 60 && elapsed <= 180,
      `Date-based Rong.sleep should wait about 70ms, got ${elapsed}ms`,
    );
  });

  test("resolves immediately for past Date targets", async () => {
    const start = Date.now();
    await Rong.sleep(new Date(start - 1000));
    const elapsed = Date.now() - start;
    assert.ok(elapsed >= 0 && elapsed < 50, "past Date should resolve quickly");
  });

  test("throws for invalid async sleep input", async () => {
    try {
      await Rong.sleep("100");
      assert.fail("Rong.sleep should reject non-number, non-Date input");
    } catch (e) {
      assert.ok(/number of milliseconds or a Date/.test(e.message));
    }
  });

  test("throws for invalid async sleep Date input", async () => {
    try {
      await Rong.sleep(new Date("invalid"));
      assert.fail("Rong.sleep should reject invalid Date input");
    } catch (e) {
      assert.ok(/target Date must be valid/.test(e.message));
    }
  });

  test("blocks synchronously for at least the requested delay", () => {
    const start = Date.now();
    Rong.sleepSync(40);
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 35 && elapsed <= 300,
      `Rong.sleepSync should block for about 40ms without oversleeping excessively, got ${elapsed}ms`,
    );
  });

  test("treats negative sleepSync delay as immediate", () => {
    const start = Date.now();
    Rong.sleepSync(-10);
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 0 && elapsed < 50,
      "negative sleepSync delay should be treated as immediate",
    );
  });

  test("throws for invalid sleepSync input", () => {
    try {
      Rong.sleepSync(new Date());
      assert.fail("Rong.sleepSync should reject Date input");
    } catch (e) {
      assert.ok(/number of milliseconds/.test(e.message));
    }
  });
});
