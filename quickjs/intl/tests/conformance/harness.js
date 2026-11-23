// The conformance suite's harness: each case names the ECMA-402 clause it
// checks (its spec id) and records a result; conformance.rs reads
// `__results` after the file runs. Self-contained, so the same files run on
// QuickJS with rong_quickjs_intl and on JavaScriptCore's own Intl.
//
// `ENGINE` is "rong" when rong_quickjs_intl answers and "reference" when the
// engine's own Intl does. A case that only holds for one says so with
// `only`, and its reason is logged.
var __results = [];

function test(section, name, fn) {
  try {
    fn();
    __results.push([section, name, '']);
  } catch (e) {
    let message = e && e.message !== undefined ? String(e.message) : String(e);
    if (e && e.stack) message += '\n' + String(e.stack).split('\n').slice(0, 3).join('\n');
    __results.push([section, name, message || 'failed']);
  }
}

// A case that holds on one engine only, with the reason it does not on the
// other (a newer CLDR, a feature the reference lacks).
function only(engine, reason, section, name, fn) {
  if (ENGINE === engine) test(section, name, fn);
  else __results.push([section, name + ' [skipped: ' + reason + ']', 'skip']);
}

function show(v) {
  if (typeof v === 'string') return JSON.stringify(v);
  if (typeof v === 'bigint') return String(v) + 'n';
  if (typeof v === 'symbol') return v.toString();
  if (Object.is(v, -0)) return '-0';
  if (Array.isArray(v)) return '[' + v.map(show).join(', ') + ']';
  if (v && typeof v === 'object') {
    try { return JSON.stringify(v); } catch (e) { return '[object]'; }
  }
  return String(v);
}

function fail(message) {
  throw new Error(message);
}

const assert = {
  sameValue(actual, expected, message) {
    if (!Object.is(actual, expected)) {
      fail((message ? message + ': ' : '') + 'got ' + show(actual) + ', want ' + show(expected));
    }
  },
  notSameValue(actual, unexpected, message) {
    if (Object.is(actual, unexpected)) {
      fail((message ? message + ': ' : '') + 'got ' + show(actual) + ', which it must not be');
    }
  },
  ok(value, message) {
    if (!value) fail(message || 'expected a true value');
  },
  // Deep equality of arrays and plain objects, keys in order.
  deepEqual(actual, expected, message) {
    const same = (a, b) => {
      if (Object.is(a, b)) return true;
      if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
      if (Array.isArray(a) !== Array.isArray(b)) return false;
      const ka = Object.keys(a);
      const kb = Object.keys(b);
      if (ka.length !== kb.length) return false;
      for (let i = 0; i < ka.length; i++) {
        if (ka[i] !== kb[i] || !same(a[ka[i]], b[kb[i]])) return false;
      }
      return true;
    };
    if (!same(actual, expected)) {
      fail((message ? message + ': ' : '') + 'got ' + show(actual) + ', want ' + show(expected));
    }
  },
  throws(Type, fn, message) {
    try {
      fn();
    } catch (e) {
      if (!e || e.constructor !== Type) {
        fail((message ? message + ': ' : '') + 'threw ' + (e && e.constructor && e.constructor.name) +
          ' (' + (e && e.message) + '), want ' + Type.name);
      }
      return e;
    }
    fail((message ? message + ': ' : '') + 'did not throw, want ' + Type.name);
  },
};

// A property's descriptor: value (when given) and attributes.
function verifyProperty(object, key, want) {
  const desc = Object.getOwnPropertyDescriptor(object, key);
  if (!desc) fail(String(key) + ' is not an own property');
  for (const attr of ['writable', 'enumerable', 'configurable']) {
    if (attr in want) assert.sameValue(desc[attr], want[attr], String(key) + ' ' + attr);
  }
  if ('value' in want) assert.sameValue(desc.value, want.value, String(key) + ' value');
  return desc;
}

function isConstructor(f) {
  try {
    Reflect.construct(String, [], f);
    return true;
  } catch (e) {
    return false;
  }
}

// A built-in function: its name, length and attributes, and whether it is
// a constructor.
function verifyFunction(f, name, length, constructor) {
  assert.sameValue(typeof f, 'function', name + ' is a function');
  verifyProperty(f, 'name', { value: name, writable: false, enumerable: false, configurable: true });
  verifyProperty(f, 'length', { value: length, writable: false, enumerable: false, configurable: true });
  assert.sameValue(isConstructor(f), !!constructor, name + ' is ' + (constructor ? '' : 'not ') + 'a constructor');
  if (!constructor) assert.sameValue(Object.prototype.hasOwnProperty.call(f, 'prototype'), false, name + ' has no prototype');
}

// A method on a prototype or namespace: writable, not enumerable,
// configurable.
function verifyMethod(object, key, length) {
  verifyProperty(object, key, { writable: true, enumerable: false, configurable: true });
  verifyFunction(object[key], key, length, false);
}

// An accessor with only a getter, `get <key>`.
function verifyGetter(object, key) {
  const desc = Object.getOwnPropertyDescriptor(object, key);
  if (!desc) fail(key + ' is not an own property');
  assert.sameValue(desc.set, undefined, key + ' has no setter');
  assert.sameValue(desc.enumerable, false, key + ' enumerable');
  assert.sameValue(desc.configurable, true, key + ' configurable');
  verifyFunction(desc.get, 'get ' + key, 0, false);
}

function verifyToStringTag(object, tag) {
  verifyProperty(object, Symbol.toStringTag, { value: tag, writable: false, enumerable: false, configurable: true });
}

// An options object whose getters log each read and answer `values`.
function loggingOptions(log, values) {
  const options = {};
  for (const key of Object.keys(values)) {
    Object.defineProperty(options, key, {
      get() {
        log.push(key);
        return values[key];
      },
      enumerable: true,
    });
  }
  return options;
}

// An options object that logs the read of any property, in order.
function readLog(log) {
  return new Proxy({}, {
    get(target, key) {
      if (typeof key === 'string') log.push(key);
      return undefined;
    },
  });
}

// A value whose conversion to a string is logged.
function stringish(log, name, text) {
  return { toString() { log.push(name); return text; }, valueOf() { log.push(name + '.valueOf'); return text; } };
}

// The text of formatToParts' parts, which must equal format's result.
function joinParts(parts) {
  let out = '';
  for (const part of parts) out += part.value;
  return out;
}
