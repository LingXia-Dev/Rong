import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const runtimeUrl = new URL("../src/runtime.js", import.meta.url);
const runtimeSource = await readFile(runtimeUrl, "utf8");

function createRuntime(host) {
  const sandbox = host === undefined ? {} : { __RONG_TEST_HOST__: host };
  const context = vm.createContext(sandbox);
  new vm.Script(runtimeSource, { filename: "@rongjs/test/runtime.js" }).runInContext(context);
  return context;
}

function evaluate(context, source) {
  return new vm.Script(source, { filename: "fixture.test.js" }).runInContext(context);
}

function plain(value) {
  return JSON.parse(JSON.stringify(value));
}

test("source evaluation only registers sequential sync and async cases", async () => {
  const context = createRuntime();
  evaluate(
    context,
    `
      globalThis.executed = [];
      describe("classification", () => {
        test("sync pass", () => executed.push("sync pass"));
        test("sync fail", () => { executed.push("sync fail"); throw new Error("sync boom"); });
        test("async pass", async () => { await Promise.resolve(); executed.push("async pass"); });
        test("async fail", async () => { executed.push("async fail"); throw new TypeError("async boom"); });
        test.skip("skipped", () => executed.push("must not run"));
      });
    `,
  );

  assert.deepEqual(plain(context.executed), []);
  const report = plain(await context.__RONG_TEST__.run());
  assert.deepEqual(plain(context.executed), ["sync pass", "sync fail", "async pass", "async fail"]);
  assert.equal(report.total, 5);
  assert.equal(report.passed, 2);
  assert.equal(report.failed, 2);
  assert.equal(report.skipped, 1);
  assert.deepEqual(
    report.cases.map(({ full_name, status }) => ({ full_name, status })),
    [
      { full_name: "classification > sync pass", status: "passed" },
      { full_name: "classification > sync fail", status: "failed" },
      { full_name: "classification > async pass", status: "passed" },
      { full_name: "classification > async fail", status: "failed" },
      { full_name: "classification > skipped", status: "skipped" },
    ],
  );
  assert.match(report.cases[1].error.stack, /fixture\.test\.js/);
  assert.equal(report.cases[3].error.name, "TypeError");
});

test("nested hooks use deterministic order and teardown failures are retained", async () => {
  const context = createRuntime();
  evaluate(
    context,
    `
      globalThis.order = [];
      beforeEach(() => order.push("root before"));
      afterEach(() => order.push("root after"));
      describe("outer", () => {
        beforeEach(() => order.push("outer before"));
        afterEach(() => { order.push("outer after"); throw new Error("outer teardown"); });
        describe("inner", () => {
          beforeEach(() => { order.push("inner before"); throw new Error("setup"); });
          afterEach(() => { order.push("inner after"); throw new Error("inner teardown"); });
          test("case", () => order.push("body"));
        });
      });
    `,
  );

  const report = plain(await context.__RONG_TEST__.run());
  assert.deepEqual(plain(context.order), [
    "root before",
    "outer before",
    "inner before",
    "inner after",
    "outer after",
    "root after",
  ]);
  assert.equal(report.failed, 1);
  assert.equal(report.cases[0].error.message, "setup");
  assert.deepEqual(
    report.cases[0].error.causes.map((cause) => cause.message),
    ["inner teardown", "outer teardown"],
  );

  const teardownContext = createRuntime();
  evaluate(
    teardownContext,
    `
      afterEach(() => { throw new Error("teardown only"); });
      test("body passes", () => {});
    `,
  );
  const teardownReport = plain(await teardownContext.__RONG_TEST__.run());
  assert.equal(teardownReport.failed, 1);
  assert.equal(teardownReport.cases[0].error.message, "teardown only");
});

test("matchers support deep values, typed arrays, cycles, throws, and every not form", async () => {
  const context = createRuntime();
  evaluate(
    context,
    `
      test("positive matchers", () => {
        expect(NaN).toBe(NaN);
        expect({ a: [1, { b: true }] }).toEqual({ a: [1, { b: true }] });
        expect(new Uint16Array([1, 2])).toEqual(new Uint16Array([1, 2]));
        const left = { value: 1 }; left.self = left;
        const right = { value: 1 }; right.self = right;
        expect(left).toEqual(right);
        expect([1, 2]).toContain(2);
        expect("rong test").toContain("test");
        expect("rong test").toMatch(/^rong/);
        expect(true).toBeTruthy();
        expect(0).toBeFalsy();
        expect(null).toBeDefined();
        expect(undefined).toBeUndefined();
        expect(new TypeError()).toBeInstanceOf(TypeError);
        expect(() => { throw new TypeError("boom"); }).toThrow(TypeError);
        expect(() => { throw new Error("needle"); }).toThrow(/needle/);
        const reason = { reason: true };
        expect(() => { throw reason; }).toThrow(reason);
      });
      test("negated matchers", () => {
        expect(1).not.toBe(2);
        expect({ a: 1 }).not.toEqual({ a: 2 });
        expect([1]).not.toContain(2);
        expect("rong").not.toMatch(/test/);
        expect(0).not.toBeTruthy();
        expect(1).not.toBeFalsy();
        expect(undefined).not.toBeDefined();
        expect(null).not.toBeUndefined();
        expect({}).not.toBeInstanceOf(Array);
        expect(() => {}).not.toThrow();
      });
      test("assertion errors expose matcher values", () => {
        try {
          expect(1).toBe(2);
          throw new Error("matcher did not fail");
        } catch (error) {
          expect(error.name).toBe("AssertionError");
          expect(error.matcher).toBe("toBe");
          expect(error.actual).toBe(1);
          expect(error.expected).toBe(2);
        }
      });
      test("missing throw fails", () => expect(() => {}).toThrow());
    `,
  );

  const report = plain(await context.__RONG_TEST__.run());
  assert.deepEqual([report.passed, report.failed], [3, 1]);
  assert.equal(report.cases[3].error.name, "AssertionError");
  assert.equal(report.cases[3].error.message.includes("toThrow"), true);
});

test("host args, attachments, and awaited lifecycle events use the generic handshake", async () => {
  const events = [];
  const attachments = [];
  const host = {
    args: { locale: "en" },
    async attach(name, artifact) {
      attachments.push([name, artifact]);
    },
    async report(event) {
      await Promise.resolve();
      events.push(plain(event));
    },
  };
  const context = createRuntime(host);
  evaluate(
    context,
    `
      test("host", async () => {
        expect(test.args.locale).toBe("en");
        await test.attach("result", { ok: true });
      });
      test.skip("skip");
    `,
  );

  const report = plain(await context.__RONG_TEST__.run());
  assert.deepEqual(plain(attachments), [["result", { ok: true }]]);
  assert.deepEqual(events.map((event) => [event.type, event.status]), [
    ["case_started", undefined],
    ["case_finished", "passed"],
    ["case_started", undefined],
    ["case_finished", "skipped"],
  ]);
  assert.deepEqual([report.passed, report.skipped], [1, 1]);
});

test("registration is synchronous and each registry runs only once", async () => {
  const context = createRuntime();
  assert.throws(
    () => evaluate(context, `describe("async", async () => {});`),
    /must be synchronous/,
  );
  evaluate(context, `test("ok", () => {});`);
  await context.__RONG_TEST__.run();
  await assert.rejects(context.__RONG_TEST__.run(), /only run once/);
  assert.throws(() => evaluate(context, `test("late", () => {});`), /after the test run/);
});

test("runtime is timer-free, console-free, bounded, and idempotent", async () => {
  assert.ok(Buffer.byteLength(runtimeSource) <= 25 * 1024);
  const context = createRuntime();
  const controller = context.__RONG_TEST__;
  new vm.Script(runtimeSource).runInContext(context);
  assert.equal(context.__RONG_TEST__, controller);
  assert.equal(Object.keys(context).includes("__RONG_TEST__"), false);
  evaluate(
    context,
    `
      test("bounded", () => {
        const cyclic = {}; cyclic.self = cyclic;
        expect(cyclic).toEqual({ self: {} });
      });
    `,
  );
  const report = plain(await context.__RONG_TEST__.run());
  assert.equal(report.failed, 1);
  assert.ok(report.cases[0].error.message.length < 2500);
});

test("ESM entry exports the installed globals", async () => {
  const api = await import("../src/index.js");
  assert.equal(api.describe, globalThis.describe);
  assert.equal(api.test, globalThis.test);
  assert.equal(api.expect, globalThis.expect);
  assert.equal(typeof api.AssertionError, "function");
});
