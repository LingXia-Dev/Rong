(function installRongTestRuntime(global) {
  "use strict";

  var hasOwn = Object.prototype.hasOwnProperty;
  var objectTag = Object.prototype.toString;
  var existing = global.__RONG_TEST__;
  if (existing && existing.__rong_test_runtime__ === 1) return;
  if (hasOwn.call(global, "__RONG_TEST__")) {
    throw new Error("globalThis.__RONG_TEST__ is already defined");
  }

  var state = "registering";
  var cases = [];
  var rootSuite = createSuite("", null);
  var currentSuite = rootSuite;
  var host = global.__RONG_TEST_HOST__;
  var reporter = host && typeof host.report === "function" ? host.report : null;

  function createSuite(name, parent) {
    return { name: name, parent: parent, before_each: [], after_each: [] };
  }

  function defineGlobal(name, value) {
    if (hasOwn.call(global, name)) {
      throw new Error("globalThis." + name + " is already defined");
    }
    Object.defineProperty(global, name, {
      value: value,
      enumerable: false,
      configurable: false,
      writable: false,
    });
  }

  function assertRegistering(api) {
    if (state !== "registering") {
      throw new Error(api + "() cannot be called after the test run has started");
    }
  }

  function assertName(name, api) {
    if (typeof name !== "string" || name.length === 0) {
      throw new TypeError(api + "() requires a non-empty string name");
    }
  }

  function assertCallback(callback, api) {
    if (typeof callback !== "function") {
      throw new TypeError(api + "() requires a function");
    }
  }

  function describe(name, define) {
    assertRegistering("describe");
    assertName(name, "describe");
    assertCallback(define, "describe");
    var parent = currentSuite;
    var suite = createSuite(name, parent);
    currentSuite = suite;
    try {
      var result = define();
      if (result && typeof result.then === "function") {
        throw new TypeError("describe() callbacks must be synchronous");
      }
    } finally {
      currentSuite = parent;
    }
  }

  function registerTest(name, callback, skipped) {
    assertRegistering("test");
    assertName(name, "test");
    if (!skipped || callback !== undefined) assertCallback(callback, "test");
    cases.push({ name: name, run: callback, suite: currentSuite, skipped: skipped });
  }

  function test(name, callback) {
    registerTest(name, callback, false);
  }

  test.skip = function skip(name, callback) {
    registerTest(name, callback, true);
  };
  test.args = host ? host.args : undefined;
  if (host && typeof host.attach === "function") test.attach = host.attach;

  function beforeEach(callback) {
    assertRegistering("beforeEach");
    assertCallback(callback, "beforeEach");
    currentSuite.before_each.push(callback);
  }

  function afterEach(callback) {
    assertRegistering("afterEach");
    assertCallback(callback, "afterEach");
    currentSuite.after_each.push(callback);
  }

  function AssertionError(matcher, actual, expected, message) {
    var error = Error.call(this, message);
    this.name = "AssertionError";
    this.message = message;
    this.matcher = matcher;
    this.actual = actual;
    this.expected = expected;
    if (error.stack) this.stack = error.stack;
  }
  AssertionError.prototype = Object.create(Error.prototype);
  AssertionError.prototype.constructor = AssertionError;

  function safeString(value) {
    try {
      return String(value);
    } catch (_) {
      return "[Unstringifiable]";
    }
  }

  function truncate(value, limit) {
    var text = safeString(value);
    return text.length <= limit ? text : text.slice(0, limit) + "…";
  }

  function isTypedArray(value) {
    return (
      typeof ArrayBuffer !== "undefined" &&
      ArrayBuffer.isView(value) &&
      objectTag.call(value) !== "[object DataView]"
    );
  }

  function isPlainObject(value) {
    if (objectTag.call(value) !== "[object Object]") return false;
    var prototype = Object.getPrototypeOf(value);
    return prototype === Object.prototype || prototype === null;
  }

  function formatValue(value) {
    var seen = [];
    function format(current, depth) {
      if (current === null) return "null";
      var type = typeof current;
      if (type === "undefined") return "undefined";
      if (type === "string") return JSON.stringify(truncate(current, 160));
      if (type === "number" || type === "boolean" || type === "bigint") {
        return String(current) + (type === "bigint" ? "n" : "");
      }
      if (type === "symbol") return truncate(String(current), 160);
      if (type === "function") return "[Function " + (current.name || "anonymous") + "]";
      if (depth >= 4) return "[…]";
      if (seen.indexOf(current) !== -1) return "[Circular]";
      seen.push(current);
      try {
        if (Array.isArray(current)) {
          var arrayParts = [];
          var arrayLimit = Math.min(current.length, 12);
          for (var i = 0; i < arrayLimit; i += 1) {
            arrayParts.push(hasOwn.call(current, i) ? format(current[i], depth + 1) : "<empty>");
          }
          if (current.length > arrayLimit) arrayParts.push("…");
          return "[" + arrayParts.join(", ") + "]";
        }
        if (isTypedArray(current)) {
          var typedParts = [];
          var typedLimit = Math.min(current.length, 12);
          for (var j = 0; j < typedLimit; j += 1) typedParts.push(String(current[j]));
          if (current.length > typedLimit) typedParts.push("…");
          return objectTag.call(current).slice(8, -1) + "[" + typedParts.join(", ") + "]";
        }
        if (current instanceof Error) {
          return current.name + "(" + JSON.stringify(truncate(current.message, 160)) + ")";
        }
        if (objectTag.call(current) === "[object RegExp]") return String(current);
        if (isPlainObject(current)) {
          var keys = Object.keys(current);
          var objectParts = [];
          var objectLimit = Math.min(keys.length, 12);
          for (var k = 0; k < objectLimit; k += 1) {
            var key = keys[k];
            var formatted;
            try {
              formatted = format(current[key], depth + 1);
            } catch (_) {
              formatted = "[Unformattable]";
            }
            objectParts.push(JSON.stringify(truncate(key, 80)) + ": " + formatted);
          }
          if (keys.length > objectLimit) objectParts.push("…");
          return "{" + objectParts.join(", ") + "}";
        }
        return objectTag.call(current);
      } finally {
        seen.pop();
      }
    }
    try {
      return truncate(format(value, 0), 2000);
    } catch (_) {
      return "[Unformattable]";
    }
  }

  function deepEqual(left, right, leftSeen, rightSeen) {
    if (Object.is(left, right)) return true;
    if (left === null || right === null || typeof left !== "object" || typeof right !== "object") {
      return false;
    }

    var seenIndex = leftSeen.indexOf(left);
    if (seenIndex !== -1) return rightSeen[seenIndex] === right;
    if (rightSeen.indexOf(right) !== -1) return false;
    leftSeen.push(left);
    rightSeen.push(right);

    if (Array.isArray(left) || Array.isArray(right)) {
      if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) return false;
      for (var i = 0; i < left.length; i += 1) {
        if (hasOwn.call(left, i) !== hasOwn.call(right, i)) return false;
        if (hasOwn.call(left, i) && !deepEqual(left[i], right[i], leftSeen, rightSeen)) return false;
      }
      return true;
    }

    if (isTypedArray(left) || isTypedArray(right)) {
      if (!isTypedArray(left) || !isTypedArray(right)) return false;
      if (objectTag.call(left) !== objectTag.call(right) || left.length !== right.length) return false;
      for (var j = 0; j < left.length; j += 1) {
        if (!Object.is(left[j], right[j])) return false;
      }
      return true;
    }

    if (!isPlainObject(left) || !isPlainObject(right)) return false;
    var leftKeys = Object.keys(left);
    var rightKeys = Object.keys(right);
    if (leftKeys.length !== rightKeys.length) return false;
    for (var k = 0; k < leftKeys.length; k += 1) {
      var key = leftKeys[k];
      if (!hasOwn.call(right, key) || !deepEqual(left[key], right[key], leftSeen, rightSeen)) return false;
    }
    return true;
  }

  function contains(actual, expected) {
    if (typeof actual === "string" && typeof expected === "string") {
      return actual.indexOf(expected) !== -1;
    }
    if (Array.isArray(actual)) {
      for (var i = 0; i < actual.length; i += 1) {
        if (Object.is(actual[i], expected) || (actual[i] !== actual[i] && expected !== expected)) return true;
      }
    }
    return false;
  }

  function regexMatches(regex, actual) {
    var lastIndex = regex.lastIndex;
    try {
      return regex.test(actual);
    } finally {
      regex.lastIndex = lastIndex;
    }
  }

  function thrownMatches(thrown, expected) {
    if (expected === undefined) return true;
    if (typeof expected === "string") {
      var message = thrown && typeof thrown.message === "string" ? thrown.message : safeString(thrown);
      return message.indexOf(expected) !== -1;
    }
    if (objectTag.call(expected) === "[object RegExp]") {
      var thrownMessage = thrown && typeof thrown.message === "string" ? thrown.message : safeString(thrown);
      return regexMatches(expected, thrownMessage);
    }
    if (typeof expected === "function") {
      try {
        return thrown instanceof expected;
      } catch (_) {
        return false;
      }
    }
    return Object.is(thrown, expected);
  }

  function createMatchers(actual, negated) {
    function check(matcher, pass, expected) {
      var accepted = negated ? !pass : pass;
      if (!accepted) {
        var qualified = negated ? "not." + matcher : matcher;
        var message =
          "Expected " +
          formatValue(actual) +
          (negated ? " not" : "") +
          " " +
          matcher +
          (arguments.length >= 3 ? " " + formatValue(expected) : "");
        throw new AssertionError(qualified, actual, expected, message);
      }
    }

    var matchers = {
      toBe: function toBe(expected) {
        check("toBe", Object.is(actual, expected), expected);
      },
      toEqual: function toEqual(expected) {
        check("toEqual", deepEqual(actual, expected, [], []), expected);
      },
      toContain: function toContain(expected) {
        check("toContain", contains(actual, expected), expected);
      },
      toMatch: function toMatch(expected) {
        var pass = false;
        if (typeof actual === "string" && typeof expected === "string") pass = actual.indexOf(expected) !== -1;
        else if (typeof actual === "string" && objectTag.call(expected) === "[object RegExp]") {
          pass = regexMatches(expected, actual);
        }
        check("toMatch", pass, expected);
      },
      toBeTruthy: function toBeTruthy() {
        check("toBeTruthy", Boolean(actual));
      },
      toBeFalsy: function toBeFalsy() {
        check("toBeFalsy", !actual);
      },
      toBeDefined: function toBeDefined() {
        check("toBeDefined", actual !== undefined);
      },
      toBeUndefined: function toBeUndefined() {
        check("toBeUndefined", actual === undefined);
      },
      toBeInstanceOf: function toBeInstanceOf(expected) {
        var pass = false;
        if (typeof expected === "function") {
          try {
            pass = actual instanceof expected;
          } catch (_) {}
        }
        check("toBeInstanceOf", pass, expected);
      },
      toThrow: function toThrow(expected) {
        var threw = false;
        var thrown;
        if (typeof actual === "function") {
          try {
            actual();
          } catch (error) {
            threw = true;
            thrown = error;
          }
        }
        check("toThrow", threw && thrownMatches(thrown, expected), expected);
      },
    };
    Object.defineProperty(matchers, "not", {
      enumerable: true,
      get: function getNot() {
        return createMatchers(actual, !negated);
      },
    });
    return matchers;
  }

  function expect(actual) {
    return createMatchers(actual, false);
  }

  function safeProperty(value, key) {
    try {
      return value != null ? value[key] : undefined;
    } catch (_) {
      return undefined;
    }
  }

  function normalizeError(error, depth, seen) {
    depth = depth || 0;
    seen = seen || [];
    var objectLike = error !== null && (typeof error === "object" || typeof error === "function");
    var name = objectLike ? safeProperty(error, "name") : undefined;
    var message = objectLike ? safeProperty(error, "message") : undefined;
    var stack = objectLike ? safeProperty(error, "stack") : undefined;
    var normalized = {
      name: typeof name === "string" && name ? truncate(name, 200) : "Error",
      message: typeof message === "string" ? truncate(message, 4000) : truncate(safeString(error), 4000),
      stack: typeof stack === "string" ? truncate(stack, 16000) : "",
    };
    if (objectLike && depth < 3 && seen.indexOf(error) === -1) {
      seen.push(error);
      var cause = safeProperty(error, "cause");
      if (cause !== undefined) normalized.causes = [normalizeError(cause, depth + 1, seen)];
      seen.pop();
    }
    return normalized;
  }

  function suiteChain(suite) {
    var chain = [];
    for (var current = suite; current; current = current.parent) chain.unshift(current);
    return chain;
  }

  function fullName(testCase) {
    var names = [];
    for (var current = testCase.suite; current; current = current.parent) {
      if (current.name) names.unshift(current.name);
    }
    names.push(testCase.name);
    return names.join(" > ");
  }

  async function emit(event) {
    if (reporter) await reporter.call(host, event);
  }

  async function executeCase(testCase) {
    var name = testCase.name;
    var full_name = fullName(testCase);
    await emit({ type: "case_started", name: name, full_name: full_name });
    var startedAt = Date.now();
    var result;
    if (testCase.skipped) {
      result = { name: name, full_name: full_name, status: "skipped", duration_ms: 0 };
    } else {
      var chain = suiteChain(testCase.suite);
      var primary;
      var hasPrimary = false;
      var later = [];
      try {
        for (var i = 0; i < chain.length; i += 1) {
          var beforeHooks = chain[i].before_each;
          for (var j = 0; j < beforeHooks.length; j += 1) await beforeHooks[j]();
        }
        await testCase.run();
      } catch (error) {
        primary = error;
        hasPrimary = true;
      }
      for (var k = chain.length - 1; k >= 0; k -= 1) {
        var afterHooks = chain[k].after_each;
        for (var m = 0; m < afterHooks.length; m += 1) {
          try {
            await afterHooks[m]();
          } catch (error) {
            if (!hasPrimary) {
              primary = error;
              hasPrimary = true;
            } else {
              later.push(error);
            }
          }
        }
      }
      var duration = Math.max(0, Date.now() - startedAt);
      if (hasPrimary) {
        var normalized = normalizeError(primary);
        if (later.length) {
          var causes = normalized.causes || [];
          for (var n = 0; n < later.length && causes.length < 8; n += 1) {
            causes.push(normalizeError(later[n]));
          }
          normalized.causes = causes;
        }
        result = {
          name: name,
          full_name: full_name,
          status: "failed",
          duration_ms: duration,
          error: normalized,
        };
      } else {
        result = { name: name, full_name: full_name, status: "passed", duration_ms: duration };
      }
    }
    var finished = {
      type: "case_finished",
      name: result.name,
      full_name: result.full_name,
      status: result.status,
      duration_ms: result.duration_ms,
    };
    if (result.error) finished.error = result.error;
    await emit(finished);
    return result;
  }

  async function run() {
    if (state !== "registering") throw new Error("The test registry can only run once");
    state = "running";
    var startedAt = Date.now();
    var results = [];
    try {
      for (var i = 0; i < cases.length; i += 1) results.push(await executeCase(cases[i]));
      var report = {
        total: results.length,
        passed: 0,
        failed: 0,
        skipped: 0,
        duration_ms: Math.max(0, Date.now() - startedAt),
        cases: results,
      };
      for (var j = 0; j < results.length; j += 1) report[results[j].status] += 1;
      return report;
    } finally {
      state = "finished";
    }
  }

  var controller = { run: run };
  Object.defineProperty(controller, "AssertionError", { value: AssertionError });
  Object.defineProperty(controller, "__rong_test_runtime__", { value: 1 });
  defineGlobal("describe", describe);
  defineGlobal("test", test);
  defineGlobal("beforeEach", beforeEach);
  defineGlobal("afterEach", afterEach);
  defineGlobal("expect", expect);
  Object.defineProperty(global, "__RONG_TEST__", {
    value: controller,
    enumerable: false,
    configurable: false,
    writable: false,
  });
})(globalThis);
