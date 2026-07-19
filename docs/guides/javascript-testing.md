# JavaScript testing with `@rongjs/test`

`@rongjs/test` is Rong's small, engine-neutral JavaScript test framework. It
works in QuickJS, JavaScriptCore, ArkJS, and other hosts that can evaluate
JavaScript and await promises. The package has no runtime dependencies and
does not require Node.js APIs, timers, a filesystem, or Rong modules.

The API for defining tests consists of `describe` and `test`. The complete
author-facing API also includes `test.skip`, `beforeEach`, `afterEach`, and
`expect`.

## Install and write a test

Install the package when tests are bundled as ECMAScript modules:

```bash
npm install --save-dev @rongjs/test
```

Import only the APIs used by the test:

```js
import { describe, expect, test } from "@rongjs/test";

describe("URL", () => {
  test("parses the host", () => {
    const url = new URL("https://example.com/path");
    expect(url.host).toBe("example.com");
  });
});
```

`describe` groups cases and contributes to their full names. Its callback must
be synchronous because it only registers tests. Put asynchronous work in a
hook or test callback instead.

Tests run sequentially in registration order. Return a promise, or use an
`async` function, when a test performs asynchronous work:

```js
test("loads a record", async () => {
  const record = await loadRecord("42");
  expect(record.id).toBe("42");
});
```

There is no callback-style `done` API. A synchronous throw or rejected promise
fails the current case; later cases still run.

## Suites, hooks, and skipped tests

Suites may be nested. `beforeEach` hooks run from the outer suite inward, and
`afterEach` hooks run from the inner suite outward. Hooks are awaited and a
hook failure fails the affected case.

```js
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  test,
} from "@rongjs/test";

describe("storage", () => {
  let storage;

  beforeEach(async () => {
    storage = await openStorage();
  });

  afterEach(async () => {
    await storage.close();
  });

  test("stores a value", async () => {
    await storage.set("answer", 42);
    expect(await storage.get("answer")).toBe(42);
  });

  test.skip("deletes expired values");
});
```

Teardown hooks still run after setup or test-body failures. If teardown also
fails, its error is retained as a cause of the primary failure.

## Assertions

Call `expect(actual)` and select a matcher:

| Matcher | Behavior |
| --- | --- |
| `toBe(expected)` | Compares with `Object.is` |
| `toEqual(expected)` | Recursively compares primitives, arrays, plain objects, and typed arrays |
| `toContain(expected)` | Checks a string substring or an array item |
| `toMatch(expected)` | Matches a string against a substring or regular expression |
| `toBeTruthy()` / `toBeFalsy()` | Checks JavaScript truthiness |
| `toBeDefined()` / `toBeUndefined()` | Checks whether the value is `undefined` |
| `toBeInstanceOf(Type)` | Applies `instanceof` |
| `toThrow(expected?)` | Checks a synchronous function for a thrown value, message, pattern, or error type |

Every matcher supports `.not`:

```js
expect(result).not.toBeUndefined();
expect(() => parse("bad input")).toThrow(/invalid/i);
```

Matcher failures throw `AssertionError`. The error includes `matcher`,
`actual`, and `expected` fields in addition to the normal error name, message,
and stack.

## Running registered tests

Loading a test file registers cases; it does not execute them. The embedding
host must call the controller after source evaluation completes:

```js
const report = await globalThis.__RONG_TEST__.run();
```

`run()` resolves to a JSON-compatible report even when cases fail:

```ts
interface TestReport {
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  duration_ms: number;
  cases: Array<{
    name: string;
    full_name: string;
    status: "passed" | "failed" | "skipped";
    duration_ms: number;
    error?: {
      name: string;
      message: string;
      stack: string;
      causes?: unknown[];
    };
  }>;
}
```

The host decides how `report.failed` affects its process exit code or protocol
status. A registry can run only once, so each independent run should use a
fresh JavaScript context.

## Embedding the framework

ESM test bundles import from `@rongjs/test`; the package installs the framework
when the module is evaluated. A host without an ESM loader can instead resolve
`@rongjs/test/runtime` and evaluate that dependency-free file as a classic
script before evaluating test source. Classic-script loading installs
`describe`, `test`, `beforeEach`, `afterEach`, and `expect` as globals. It does
not install `it`.

The execution order is always:

1. Optionally set host extensions.
2. Load the test runtime.
3. Evaluate test source to register cases.
4. Await `globalThis.__RONG_TEST__.run()`.
5. Consume the returned report and tear down the context.

An embedder may provide arguments, artifact handling, and live case events
before loading the runtime:

```js
globalThis.__RONG_TEST_HOST__ = {
  args: { locale: "en" },
  attach: async (name, artifact) => {
    // Store an artifact in the host.
  },
  report: async (event) => {
    // Receive case_started and case_finished events.
  },
};
```

The runtime exposes `args` and `attach` as `test.args` and `test.attach`.
Their meaning is defined by the host, not by Rong.

The host remains responsible for discovering files, transforming TypeScript,
loading modules, enforcing deadlines, interrupting execution, capturing
console output, and selecting a process or protocol status. The framework does
not provide parallel cases, per-case timeouts, snapshots, mocks, spies, fake
timers, coverage, or browser emulation.

## Testing Rong modules in this repository

Rong's JavaScript suites under `tests/unit` use the same globals and runtime.
The Rust `UnitJSRunner` creates the context, loads the shared runtime, evaluates
the selected suite, and runs its registered cases. Use the Cargo and engine
commands in the [repository testing guide](../internals/testing.md) to execute
those suites.
