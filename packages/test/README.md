# `@rongjs/test`

A small, zero-dependency JavaScript test framework for embedded runtimes. It
provides sequential async tests, nested suites, hooks, matchers, structured
reports, and optional lifecycle events without depending on Node.js APIs,
timers, a filesystem, or Rong runtime modules.

```js
import { describe, test, expect } from "@rongjs/test";

describe("URL", () => {
  test("parses the host", () => {
    expect(new URL("https://example.com/path").host).toBe("example.com");
  });
});

const report = await globalThis.__RONG_TEST__.run();
```

Embedders must evaluate test source first and call `run()` afterward. Source
evaluation only registers cases. The returned report is JSON-compatible, and
ordinary case failures resolve in `report.cases` instead of rejecting `run()`.

## Classic-script loading

`src/runtime.js` has no imports or exports, so an embedder may evaluate it as a
classic script before evaluating test source. It installs `describe`, `test`,
`beforeEach`, `afterEach`, and `expect` as globals. It does not install `it`.

## Host extensions

A host may define this object before loading the runtime:

```js
globalThis.__RONG_TEST_HOST__ = {
  args: { locale: "en" },
  attach: async (name, artifact) => {},
  report: async (event) => {},
};
```

`args` and `attach` become `test.args` and `test.attach`. The optional reporter
receives `case_started` and `case_finished` events. The framework assigns no
meaning to host arguments or artifacts.

The host remains responsible for source loading, overall deadlines,
interruption/cancellation, console capture, and converting failed cases into a
process or protocol status.
