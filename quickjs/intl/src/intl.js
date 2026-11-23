// ECMA-402 abstract operations whose coercions are observable, and the Intl
// namespace. Evaluates to a function that installs Intl from the natives;
// intrinsics are captured here so later changes to globals cannot reach them.
(function install(natives) {
  'use strict';
  const {
    canonicalizeTag, defaultLocale, settingsGeneration, resolveLocale, supportedLocales,
    pluralResolve, pluralResolveRange, pluralCategories,
  } = natives;
  const $Object = Object;
  const $String = String;
  const $Number = Number;
  const $create = Object.create;
  const $apply = Reflect.apply;
  const $toPrimitive = Symbol.toPrimitive;
  const $toStringTag = Symbol.toStringTag;
  const $WeakMap = WeakMap;
  const $weakGet = WeakMap.prototype.get;
  const $weakSet = WeakMap.prototype.set;
  const $TypeError = TypeError;
  const $RangeError = RangeError;
  const $isFinite = Number.isFinite;
  const $floor = Math.floor;
  const $min = Math.min;
  const $max = Math.max;
  const $defineProperty = Object.defineProperty;
  const $push = Array.prototype.push;
  const $includes = Array.prototype.includes;
  // No spreading or iteration anywhere here: a script may have replaced
  // the array iterator.
  const call = (f, self, ...args) => $apply(f, self, args);

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
    else {
      // ToString, which String() is not: a Symbol is a TypeError.
      if (typeof value === 'symbol') throw new $TypeError('Cannot convert a Symbol to a string');
      value = $String(value);
    }
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

  // A data property as CreateDataPropertyOrThrow or a builtin defines it; the
  // descriptor has no prototype, so inherited `get`/`set` cannot reach it.
  function define(o, key, value, enumerable, writable) {
    $defineProperty(o, key, {
      __proto__: null, value, writable, enumerable, configurable: true,
    });
  }

  // ToPrimitive with hint number.
  function ToPrimitiveNumber(input) {
    if (!isObject(input)) return input;
    const exotic = input[$toPrimitive];
    if (exotic !== undefined && exotic !== null) {
      if (typeof exotic !== 'function') throw new $TypeError('Symbol.toPrimitive is not callable');
      const result = call(exotic, input, 'number');
      if (isObject(result)) throw new $TypeError('Cannot convert object to primitive value');
      return result;
    }
    const valueOf = input.valueOf;
    if (typeof valueOf === 'function') {
      const result = call(valueOf, input);
      if (!isObject(result)) return result;
    }
    const toString = input.toString;
    if (typeof toString === 'function') {
      const result = call(toString, input);
      if (!isObject(result)) return result;
    }
    throw new $TypeError('Cannot convert object to primitive value');
  }

  // ToIntlMathematicalValue up to the string the natives parse exactly;
  // -0 travels as "-0".
  function ToIntlMathematicalValue(value) {
    const prim = ToPrimitiveNumber(value);
    if (typeof prim === 'bigint') return $String(prim);
    if (typeof prim === 'string') return prim;
    const x = $Number(prim);
    if (x === 0 && 1 / x < 0) return '-0';
    return $String(x);
  }

  // ResolveOptions for a service without relevant extension keys.
  function ResolveOptions(service, locales, options, coerce) {
    const requested = CanonicalizeLocaleList(locales);
    options = coerce ? CoerceOptionsToObject(options) : GetOptionsObject(options);
    GetOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit');
    return { __proto__: null, options, locale: resolveLocale(service, requested) };
  }

  // SupportedLocales, behind each service's supportedLocalesOf.
  function SupportedLocales(service, locales, options) {
    const requested = CanonicalizeLocaleList(locales);
    options = CoerceOptionsToObject(options);
    GetOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit');
    return supportedLocales(service, requested);
  }

  const INCREMENTS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 2500, 5000];
  const ROUNDING_MODES = ['ceil', 'floor', 'expand', 'trunc', 'halfCeil', 'halfFloor',
    'halfExpand', 'halfTrunc', 'halfEven'];

  // SetNumberFormatDigitOptions, into the slot record `obj`.
  function SetNumberFormatDigitOptions(obj, options, minFDDefault, maxFDDefault, notation) {
    const minIntegerDigits = GetNumberOption(options, 'minimumIntegerDigits', 1, 21, 1);
    const minFD = options.minimumFractionDigits;
    const maxFD = options.maximumFractionDigits;
    const minSD = options.minimumSignificantDigits;
    const maxSD = options.maximumSignificantDigits;
    const roundingIncrement = GetNumberOption(options, 'roundingIncrement', 1, 5000, 1);
    if (!call($includes, INCREMENTS, roundingIncrement)) {
      throw new $RangeError('roundingIncrement must be one of 1, 2, 5, 10, 20, 25, 50, ... 5000');
    }
    const roundingMode = GetOption(options, 'roundingMode', 'string', ROUNDING_MODES, 'halfExpand');
    const roundingPriority = GetOption(options, 'roundingPriority', 'string',
      ['auto', 'morePrecision', 'lessPrecision'], 'auto');
    const trailingZeroDisplay = GetOption(options, 'trailingZeroDisplay', 'string',
      ['auto', 'stripIfInteger'], 'auto');
    if (roundingIncrement !== 1) maxFDDefault = minFDDefault;
    obj.minimumIntegerDigits = minIntegerDigits;
    obj.roundingIncrement = roundingIncrement;
    obj.roundingMode = roundingMode;
    obj.trailingZeroDisplay = trailingZeroDisplay;
    const hasSD = minSD !== undefined || maxSD !== undefined;
    const hasFD = minFD !== undefined || maxFD !== undefined;
    let needSD = true;
    let needFD = true;
    if (roundingPriority === 'auto') {
      needSD = hasSD;
      if (needSD || (!hasFD && notation === 'compact')) needFD = false;
    }
    if (needSD) {
      if (hasSD) {
        const min = DefaultNumberOption(minSD, 1, 21, 1);
        obj.minimumSignificantDigits = min;
        obj.maximumSignificantDigits = DefaultNumberOption(maxSD, min, 21, 21);
      } else {
        obj.minimumSignificantDigits = 1;
        obj.maximumSignificantDigits = 21;
      }
    }
    if (needFD) {
      if (hasFD) {
        let min = DefaultNumberOption(minFD, 0, 100, undefined);
        let max = DefaultNumberOption(maxFD, 0, 100, undefined);
        if (min === undefined) min = $min(minFDDefault, max);
        else if (max === undefined) max = $max(maxFDDefault, min);
        else if (min > max) throw new $RangeError('minimumFractionDigits is greater than maximumFractionDigits');
        obj.minimumFractionDigits = min;
        obj.maximumFractionDigits = max;
      } else {
        obj.minimumFractionDigits = minFDDefault;
        obj.maximumFractionDigits = maxFDDefault;
      }
    }
    if (!needSD && !needFD) {
      obj.minimumFractionDigits = 0;
      obj.maximumFractionDigits = 0;
      obj.minimumSignificantDigits = 1;
      obj.maximumSignificantDigits = 2;
      obj.roundingType = 'morePrecision';
      obj.roundingPriority = 'morePrecision';
    } else if (roundingPriority === 'morePrecision' || roundingPriority === 'lessPrecision') {
      obj.roundingType = roundingPriority;
      obj.roundingPriority = roundingPriority;
    } else if (hasSD) {
      obj.roundingType = 'significantDigits';
      obj.roundingPriority = 'auto';
    } else {
      obj.roundingType = 'fractionDigits';
      obj.roundingPriority = 'auto';
    }
    if (roundingIncrement !== 1) {
      if (obj.roundingType !== 'fractionDigits') {
        throw new $TypeError('roundingIncrement needs fraction digits rounding');
      }
      if (obj.maximumFractionDigits !== obj.minimumFractionDigits) {
        throw new $RangeError('roundingIncrement needs equal minimum and maximum fraction digits');
      }
    }
  }

  // Intl.PluralRules
  const pluralSlots = new $WeakMap();
  function pluralRulesOf(pr, method) {
    const slots = isObject(pr) ? call($weakGet, pluralSlots, pr) : undefined;
    if (slots === undefined) {
      throw new $TypeError('Intl.PluralRules.prototype.' + method + ' called on an incompatible receiver');
    }
    return slots;
  }

  class PluralRules {
    constructor() {
      const r = ResolveOptions('PluralRules', arguments[0], arguments[1], true);
      const options = r.options;
      const slots = $create(null);
      slots.locale = r.locale;
      slots.type = GetOption(options, 'type', 'string', ['cardinal', 'ordinal'], 'cardinal');
      const notation = GetOption(options, 'notation', 'string',
        ['standard', 'scientific', 'engineering', 'compact'], 'standard');
      slots.notation = notation;
      const compactDisplay = GetOption(options, 'compactDisplay', 'string', ['short', 'long'], 'short');
      if (notation === 'compact') slots.compactDisplay = compactDisplay;
      SetNumberFormatDigitOptions(slots, options, 0, 3, notation);
      call($weakSet, pluralSlots, this, slots);
    }

    static supportedLocalesOf(locales) {
      return SupportedLocales('PluralRules', locales, arguments[1]);
    }

    select(value) {
      const slots = pluralRulesOf(this, 'select');
      return pluralResolve(slots, ToIntlMathematicalValue(value))[0];
    }

    selectRange(start, end) {
      const slots = pluralRulesOf(this, 'selectRange');
      if (start === undefined || end === undefined) {
        throw new $TypeError('selectRange needs a start and an end');
      }
      const x = ToIntlMathematicalValue(start);
      const y = ToIntlMathematicalValue(end);
      const category = pluralResolveRange(slots, x, y);
      if (category === null || category === undefined) {
        throw new $RangeError('selectRange cannot take NaN');
      }
      return category;
    }

    resolvedOptions() {
      const s = pluralRulesOf(this, 'resolvedOptions');
      const options = {};
      const put = (key, value) => { if (value !== undefined) define(options, key, value, true, true); };
      put('locale', s.locale);
      put('type', s.type);
      put('notation', s.notation);
      put('compactDisplay', s.compactDisplay);
      put('minimumIntegerDigits', s.minimumIntegerDigits);
      put('minimumFractionDigits', s.minimumFractionDigits);
      put('maximumFractionDigits', s.maximumFractionDigits);
      put('minimumSignificantDigits', s.minimumSignificantDigits);
      put('maximumSignificantDigits', s.maximumSignificantDigits);
      put('pluralCategories', pluralCategories(s.locale, s.type));
      put('roundingIncrement', s.roundingIncrement);
      put('roundingMode', s.roundingMode);
      put('roundingPriority', s.roundingPriority);
      put('trailingZeroDisplay', s.trailingZeroDisplay);
      return options;
    }
  }
  $defineProperty(PluralRules.prototype, $toStringTag, {
    __proto__: null, value: 'Intl.PluralRules', writable: false, enumerable: false, configurable: true,
  });

  const Intl = {};
  $defineProperty(Intl, Symbol.toStringTag, {
    value: 'Intl', writable: false, enumerable: false, configurable: true,
  });
  const methods = {
    getCanonicalLocales(locales) {
      return CanonicalizeLocaleList(locales);
    },
  };
  define(Intl, 'getCanonicalLocales', methods.getCanonicalLocales, false, true);
  define(Intl, 'PluralRules', PluralRules, false, true);
  $defineProperty(globalThis, 'Intl', {
    value: Intl, writable: true, enumerable: false, configurable: true,
  });

  // The helpers the constructors build on, kept out of reach of scripts.
  return {
    CanonicalizeLocaleList, GetOptionsObject, CoerceOptionsToObject, GetOption,
    GetNumberOption, DefaultNumberOption, REQUIRED, defaultLocale, settingsGeneration,
  };
})
