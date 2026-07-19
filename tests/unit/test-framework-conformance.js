globalThis.__testFrameworkOrder = [];

describe("framework conformance", () => {
  beforeEach(() => __testFrameworkOrder.push("outer before"));
  afterEach(() => __testFrameworkOrder.push("outer after"));

  describe("nested hooks", () => {
    beforeEach(() => __testFrameworkOrder.push("inner before"));
    afterEach(() => __testFrameworkOrder.push("inner after"));

    test("run outer-to-inner and await the body", async () => {
      expect(__testFrameworkOrder).toEqual(["outer before", "inner before"]);
      await Promise.resolve();
      __testFrameworkOrder.push("body");
    });
  });
});

test("teardown ran inner-to-outer", () => {
  expect(__testFrameworkOrder).toEqual([
    "outer before",
    "inner before",
    "body",
    "inner after",
    "outer after",
  ]);
});

test("matchers are engine neutral", () => {
  expect(NaN).toBe(NaN);
  expect({ nested: [1, { ok: true }] }).toEqual({
    nested: [1, { ok: true }],
  });
  expect(new Uint8Array([1, 2, 3])).toEqual(new Uint8Array([1, 2, 3]));
  expect("rong test").toContain("test");
  expect("rong test").toMatch(/^rong/);
  expect(null).toBeDefined();
  expect(undefined).toBeUndefined();
  expect(new TypeError("boom")).toBeInstanceOf(TypeError);
  expect(() => {
    throw new TypeError("boom");
  }).toThrow(TypeError);
  expect(() => {}).not.toThrow();
});

test.skip("skipped callbacks are not invoked", () => {
  throw new Error("skipped callback ran");
});
