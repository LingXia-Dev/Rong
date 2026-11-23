// ECMA-402 abstract operations whose coercions are observable, and the Intl
// namespace. Evaluates to a function that installs Intl from the natives;
// intrinsics are captured here so later changes to globals cannot reach them.
(function install(natives) {
  'use strict';
  const { canonicalizeTag, defaultLocale, settingsGeneration } = natives;
  const $Object = Object;
  const $String = String;
  const $TypeError = TypeError;
  const $RangeError = RangeError;
  const $isFinite = Number.isFinite;
  const $floor = Math.floor;
  const $min = Math.min;
  const $defineProperty = Object.defineProperty;
  const $push = Array.prototype.push;
  const $includes = Array.prototype.includes;
  const $call = Function.prototype.call;
  const call = (f, self, ...args) => $call.apply(f, [self, ...args]);

  function isObject(v) {
    return (typeof v === 'object' && v !== null) || typeof v === 'function';
  }

  function ToObject(v) {
    if (v === null || v === undefined) throw new $TypeError('Cannot convert ' + v + ' to an object');
    return $Object(v);
  }

  function ToLength(v) {
    const n = +v; // ToNumber: throws for BigInt and Symbol
    if (n !== n || n <= 0) return 0;
    if (n === Infinity) return 2 ** 53 - 1;
    return $min($floor(n), 2 ** 53 - 1);
  }

  // CanonicalizeLocaleList
  function CanonicalizeLocaleList(locales) {
    if (locales === undefined) return [];
    const seen = [];
    const O = typeof locales === 'string' ? [locales] : ToObject(locales);
    const len = ToLength(O.length);
    for (let k = 0; k < len; k++) {
      const Pk = $String(k);
      if (!(Pk in O)) continue;
      const kValue = O[Pk];
      if (typeof kValue !== 'string' && !isObject(kValue)) {
        throw new $TypeError('Locale list entries must be strings or objects, got ' + typeof kValue);
      }
      const tag = $String(kValue);
      const canonical = canonicalizeTag(tag);
      if (canonical === null || canonical === undefined) {
        throw new $RangeError('Incorrect locale information provided: ' + tag);
      }
      if (!call($includes, seen, canonical)) call($push, seen, canonical);
    }
    return seen;
  }

  // GetOptionsObject
  function GetOptionsObject(options) {
    if (options === undefined) return $Object.create(null);
    if (isObject(options)) return options;
    throw new $TypeError('Options must be an object');
  }

  // CoerceOptionsToObject
  function CoerceOptionsToObject(options) {
    if (options === undefined) return $Object.create(null);
    return ToObject(options);
  }

  // GetOption: type is 'boolean' or 'string'; values is an array or undefined.
  function GetOption(options, property, type, values, fallback) {
    let value = options[property];
    if (value === undefined) {
      if (fallback === REQUIRED) throw new $RangeError(property + ' is required');
      return fallback;
    }
    if (type === 'boolean') value = !!value;
    else value = $String(value);
    if (values !== undefined && !call($includes, values, value)) {
      throw new $RangeError('Value ' + value + ' out of range for ' + property +
        ' (allowed: ' + values.join(', ') + ')');
    }
    return value;
  }
  const REQUIRED = { required: true };

  // DefaultNumberOption
  function DefaultNumberOption(value, minimum, maximum, fallback) {
    if (value === undefined) return fallback;
    value = +value;
    if (!$isFinite(value) || value < minimum || value > maximum) {
      throw new $RangeError(value + ' is outside the range ' + minimum + ' to ' + maximum);
    }
    return $floor(value);
  }

  // GetNumberOption
  function GetNumberOption(options, property, minimum, maximum, fallback) {
    return DefaultNumberOption(options[property], minimum, maximum, fallback);
  }

  const Intl = {};
  $defineProperty(Intl, Symbol.toStringTag, {
    value: 'Intl', writable: false, enumerable: false, configurable: true,
  });
  const methods = {
    getCanonicalLocales(locales) {
      return CanonicalizeLocaleList(locales);
    },
  };
  $defineProperty(Intl, 'getCanonicalLocales', {
    value: methods.getCanonicalLocales, writable: true, enumerable: false, configurable: true,
  });
  $defineProperty(globalThis, 'Intl', {
    value: Intl, writable: true, enumerable: false, configurable: true,
  });

  // The helpers the constructors build on, kept out of reach of scripts.
  return {
    CanonicalizeLocaleList, GetOptionsObject, CoerceOptionsToObject, GetOption,
    GetNumberOption, DefaultNumberOption, REQUIRED, defaultLocale, settingsGeneration,
  };
})
