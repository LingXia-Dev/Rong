// ECMA-402 abstract operations whose coercions are observable, and the Intl
// namespace. Evaluates to a function that installs Intl from the natives;
// intrinsics are captured here so later changes to globals cannot reach them.
(function install(natives) {
  'use strict';
  const {
    canonicalizeTag, defaultLocale, settingsGeneration, resolveLocale, supportedLocales,
    pluralResolve, pluralResolveRange, pluralCategories,
    resolveNumberFormat, numberFormatParts, numberFormatRangeParts, currencyDigits,
    resolveDateTimeFormat, primaryTimeZone, systemTimeZone, dateTimeFormatFields,
    dateTimeFormatParts, dateTimeFormatRangeParts, transformCase,
    relativeTimeParts, listParts,
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
  const $getPrototypeOf = Object.getPrototypeOf;
  const $setPrototypeOf = Object.setPrototypeOf;
  const $FunctionPrototype = Function.prototype;
  const $charCodeAt = String.prototype.charCodeAt;
  const $indexOf = String.prototype.indexOf;
  const $slice = String.prototype.slice;
  const $fromCharCode = String.fromCharCode;
  const $Symbol = Symbol;
  const $dateNow = Date.now;
  const $abs = Math.abs;
  const $trunc = Math.trunc;
  const $reflectGet = Reflect.get;
  const $numberValueOf = Number.prototype.valueOf;
  const $bigIntValueOf = BigInt.prototype.valueOf;
  const $dateGetTime = Date.prototype.getTime;
  const $toUpperCase = String.prototype.toUpperCase;
  const $toLowerCase = String.prototype.toLowerCase;
  const TypedArrayPrototype = Object.getPrototypeOf(Int8Array.prototype);
  const $typedArrayEntries = TypedArrayPrototype.entries;
  const $typedArrayLength = Object.getOwnPropertyDescriptor(TypedArrayPrototype, 'length').get;
  const $iterator = Symbol.iterator;
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
  // -0 travels as "-0", a BigInt as "n" and its digits (exact, no
  // RoundMVResult). No StringNumericLiteral starts with "n".
  function ToIntlMathematicalValue(value) {
    const prim = ToPrimitiveNumber(value);
    if (typeof prim === 'bigint') return 'n' + $String(prim);
    if (typeof prim === 'string') return prim[0] === 'n' ? 'NaN' : prim;
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

  // A derived class whose constructor never calls super: no object is made
  // from NewTarget before ours (an ordinary function would read its
  // prototype twice), and the fallback is %Intl.PluralRules.prototype%, not
  // %Object.prototype%. The heritage is undone below.
  class PluralRules extends $Object {
    constructor() {
      let proto = new.target.prototype;
      if (!isObject(proto)) proto = PluralRulesPrototype;
      const pr = $create(proto);
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
      call($weakSet, pluralSlots, pr, slots);
      return pr;
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
  $setPrototypeOf(PluralRules, $FunctionPrototype);
  const PluralRulesPrototype = PluralRules.prototype;
  $defineProperty(PluralRulesPrototype, $toStringTag, {
    __proto__: null, value: 'Intl.PluralRules', writable: false, enumerable: false, configurable: true,
  });

  // Intl.NumberFormat
  const numberFormatSlots = new $WeakMap();
  // %Intl%.[[FallbackSymbol]], for the normative optional constructor mode.
  const FallbackSymbol = $Symbol('IntlLegacyConstructedSymbol');
  const SANCTIONED_UNITS = ['acre', 'bit', 'byte', 'celsius', 'centimeter', 'day', 'degree',
    'fahrenheit', 'fluid-ounce', 'foot', 'gallon', 'gigabit', 'gigabyte', 'gram', 'hectare',
    'hour', 'inch', 'kilobit', 'kilobyte', 'kilogram', 'kilometer', 'liter', 'megabit',
    'megabyte', 'meter', 'microsecond', 'mile', 'mile-scandinavian', 'milliliter',
    'millimeter', 'millisecond', 'minute', 'month', 'nanosecond', 'ounce', 'percent',
    'petabyte', 'pound', 'second', 'stone', 'terabit', 'terabyte', 'week', 'yard', 'year'];

  // ASCII case mapping, without String.prototype methods a script can replace.
  function asciiCase(s, upper) {
    let out = '';
    for (let i = 0; i < s.length; i++) {
      let c = call($charCodeAt, s, i);
      if (upper && c >= 97 && c <= 122) c -= 32;
      else if (!upper && c >= 65 && c <= 90) c += 32;
      out += $fromCharCode(c);
    }
    return out;
  }

  function isAlphanumeric(c) {
    return (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
  }

  // The `type` nonterminal of a Unicode locale identifier: alphanum{3,8}
  // subtags joined by hyphens.
  function isUnicodeType(s) {
    let run = 0;
    for (let i = 0; i <= s.length; i++) {
      const c = i < s.length ? call($charCodeAt, s, i) : 45;
      if (c === 45) {
        if (run < 3 || run > 8) return false;
        run = 0;
      } else if (isAlphanumeric(c)) {
        run++;
      } else {
        return false;
      }
    }
    return true;
  }

  // IsWellFormedCurrencyCode
  function IsWellFormedCurrencyCode(currency) {
    if (currency.length !== 3) return false;
    for (let i = 0; i < 3; i++) {
      const c = call($charCodeAt, currency, i);
      if (!((c >= 65 && c <= 90) || (c >= 97 && c <= 122))) return false;
    }
    return true;
  }

  // IsWellFormedUnitIdentifier
  function IsWellFormedUnitIdentifier(unit) {
    if (call($includes, SANCTIONED_UNITS, unit)) return true;
    const at = call($indexOf, unit, '-per-');
    if (at < 0) return false;
    const numerator = call($slice, unit, 0, at);
    const denominator = call($slice, unit, at + 5);
    if (call($indexOf, denominator, '-per-') >= 0) return false;
    return call($includes, SANCTIONED_UNITS, numerator) &&
      call($includes, SANCTIONED_UNITS, denominator);
  }

  // GetBooleanOrStringNumberFormatOption
  function GetBooleanOrStringNumberFormatOption(options, property, values, fallback) {
    let value = options[property];
    if (value === undefined) return fallback;
    if (value === true) return true;
    if (!value) return false;
    if (typeof value === 'symbol') throw new $TypeError('Cannot convert a Symbol to a string');
    value = $String(value);
    if (!call($includes, values, value)) {
      throw new $RangeError('Value ' + value + ' out of range for ' + property);
    }
    return value;
  }

  // SetNumberFormatUnitOptions
  function SetNumberFormatUnitOptions(slots, options) {
    const style = GetOption(options, 'style', 'string',
      ['decimal', 'percent', 'currency', 'unit'], 'decimal');
    slots.style = style;
    const currency = GetOption(options, 'currency', 'string', undefined, undefined);
    if (currency === undefined) {
      if (style === 'currency') throw new $TypeError('Currency code is required with currency style');
    } else if (!IsWellFormedCurrencyCode(currency)) {
      throw new $RangeError('Invalid currency code: ' + currency);
    }
    const currencyDisplay = GetOption(options, 'currencyDisplay', 'string',
      ['code', 'symbol', 'narrowSymbol', 'name'], 'symbol');
    const currencySign = GetOption(options, 'currencySign', 'string',
      ['standard', 'accounting'], 'standard');
    const unit = GetOption(options, 'unit', 'string', undefined, undefined);
    if (unit === undefined) {
      if (style === 'unit') throw new $TypeError('Unit is required with unit style');
    } else if (!IsWellFormedUnitIdentifier(unit)) {
      throw new $RangeError('Invalid unit: ' + unit);
    }
    const unitDisplay = GetOption(options, 'unitDisplay', 'string',
      ['short', 'narrow', 'long'], 'short');
    if (style === 'currency') {
      slots.currency = asciiCase(currency, true);
      slots.currencyDisplay = currencyDisplay;
      slots.currencySign = currencySign;
    }
    if (style === 'unit') {
      slots.unit = unit;
      slots.unitDisplay = unitDisplay;
    }
  }

  // OrdinaryHasInstance(C, value), for C whose prototype is `prototype`.
  function isInstanceOf(value, prototype) {
    if (!isObject(value)) return false;
    let proto = $getPrototypeOf(value);
    while (proto !== null) {
      if (proto === prototype) return true;
      proto = $getPrototypeOf(proto);
    }
    return false;
  }
  function isNumberFormatInstance(value) {
    return isInstanceOf(value, NumberFormatPrototype);
  }

  // UnwrapNumberFormat, then RequireInternalSlot.
  function numberFormatOf(nf, method) {
    if (!isObject(nf)) {
      throw new $TypeError('Intl.NumberFormat.prototype.' + method + ' called on a non-object');
    }
    let slots = call($weakGet, numberFormatSlots, nf);
    if (slots === undefined && isNumberFormatInstance(nf)) {
      const inner = nf[FallbackSymbol];
      slots = isObject(inner) ? call($weakGet, numberFormatSlots, inner) : undefined;
    }
    if (slots === undefined) {
      throw new $TypeError('Intl.NumberFormat.prototype.' + method + ' called on an incompatible receiver');
    }
    return slots;
  }

  // RequireInternalSlot only (formatToParts does not unwrap).
  function numberFormatSlotsOf(nf, method) {
    const slots = isObject(nf) ? call($weakGet, numberFormatSlots, nf) : undefined;
    if (slots === undefined) {
      throw new $TypeError('Intl.NumberFormat.prototype.' + method + ' called on an incompatible receiver');
    }
    return slots;
  }

  // PartitionNumberPattern, as [type, value] pairs.
  function partitionNumber(slots, value) {
    return numberFormatParts(slots, ToIntlMathematicalValue(value));
  }

  function NumberFormat() {
    const locales = arguments[0];
    const newTarget = new.target === undefined ? NumberFormat : new.target;
    let proto = newTarget.prototype;
    if (!isObject(proto)) proto = NumberFormatPrototype;
    const nf = $create(proto);
    // ResolveOptions with the `nu` resolution option.
    const requested = CanonicalizeLocaleList(locales);
    const options = CoerceOptionsToObject(arguments[1]);
    GetOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit');
    let nu = GetOption(options, 'numberingSystem', 'string', undefined, undefined);
    if (nu !== undefined) {
      if (!isUnicodeType(nu)) throw new $RangeError('Invalid numberingSystem: ' + nu);
      nu = asciiCase(nu, false);
    }
    const resolved = resolveNumberFormat(requested, nu);
    const slots = $create(null);
    slots.locale = resolved[0];
    slots.dataLocale = resolved[1];
    slots.numberingSystem = resolved[2];
    SetNumberFormatUnitOptions(slots, options);
    const style = slots.style;
    const notation = GetOption(options, 'notation', 'string',
      ['standard', 'scientific', 'engineering', 'compact'], 'standard');
    slots.notation = notation;
    let minFD = 0;
    let maxFD = 3;
    if (style === 'currency' && notation === 'standard') {
      minFD = maxFD = currencyDigits(slots.currency);
    } else if (style === 'percent') {
      minFD = maxFD = 0;
    }
    SetNumberFormatDigitOptions(slots, options, minFD, maxFD, notation);
    const compactDisplay = GetOption(options, 'compactDisplay', 'string', ['short', 'long'], 'short');
    let defaultUseGrouping = 'auto';
    if (notation === 'compact') {
      slots.compactDisplay = compactDisplay;
      defaultUseGrouping = 'min2';
    }
    let useGrouping = GetBooleanOrStringNumberFormatOption(options, 'useGrouping',
      ['min2', 'auto', 'always', 'true', 'false'], defaultUseGrouping);
    if (useGrouping === 'true' || useGrouping === 'false') useGrouping = defaultUseGrouping;
    if (useGrouping === true) useGrouping = 'always';
    slots.useGrouping = useGrouping;
    slots.signDisplay = GetOption(options, 'signDisplay', 'string',
      ['auto', 'never', 'always', 'exceptZero', 'negative'], 'auto');
    call($weakSet, numberFormatSlots, nf, slots);
    // ChainNumberFormat
    if (new.target === undefined && isNumberFormatInstance(this)) {
      $defineProperty(this, FallbackSymbol, {
        __proto__: null, value: nf, writable: false, enumerable: false, configurable: false,
      });
      return this;
    }
    return nf;
  }
  const NumberFormatPrototype = NumberFormat.prototype;
  $defineProperty(NumberFormat, 'prototype', { writable: false });

  const numberFormatMethods = {
    supportedLocalesOf(locales) {
      return SupportedLocales('NumberFormat', locales, arguments[1]);
    },
    get format() {
      const slots = numberFormatOf(this, 'format');
      if (slots.boundFormat === undefined) {
        // An anonymous built-in of length 1, not a constructor.
        const bound = { __proto__: null, f: (value) => FormatParts(slots, value, false) };
        const format = bound.f;
        $defineProperty(format, 'name', { value: '' });
        slots.boundFormat = format;
      }
      return slots.boundFormat;
    },
    formatToParts(value) {
      return FormatParts(numberFormatSlotsOf(this, 'formatToParts'), value, true);
    },
    formatRange(start, end) {
      return FormatRangeParts(numberFormatSlotsOf(this, 'formatRange'), start, end, false);
    },
    formatRangeToParts(start, end) {
      return FormatRangeParts(numberFormatSlotsOf(this, 'formatRangeToParts'), start, end, true);
    },
    resolvedOptions() {
      const s = numberFormatOf(this, 'resolvedOptions');
      const options = {};
      const put = (key, value) => { if (value !== undefined) define(options, key, value, true, true); };
      put('locale', s.locale);
      put('numberingSystem', s.numberingSystem);
      put('style', s.style);
      put('currency', s.currency);
      put('currencyDisplay', s.currencyDisplay);
      put('currencySign', s.currencySign);
      put('unit', s.unit);
      put('unitDisplay', s.unitDisplay);
      put('minimumIntegerDigits', s.minimumIntegerDigits);
      put('minimumFractionDigits', s.minimumFractionDigits);
      put('maximumFractionDigits', s.maximumFractionDigits);
      put('minimumSignificantDigits', s.minimumSignificantDigits);
      put('maximumSignificantDigits', s.maximumSignificantDigits);
      put('useGrouping', s.useGrouping);
      put('notation', s.notation);
      put('compactDisplay', s.compactDisplay);
      put('signDisplay', s.signDisplay);
      put('roundingIncrement', s.roundingIncrement);
      put('roundingMode', s.roundingMode);
      put('roundingPriority', s.roundingPriority);
      put('trailingZeroDisplay', s.trailingZeroDisplay);
      return options;
    },
  };

  // FormatNumeric (a string) or FormatNumericToParts (an array of parts).
  // FormatNumericRange / FormatNumericRangeToParts.
  function FormatRangeParts(slots, start, end, asParts) {
    if (start === undefined || end === undefined) {
      throw new $TypeError('formatRange needs a start and an end');
    }
    const x = ToIntlMathematicalValue(start);
    const y = ToIntlMathematicalValue(end);
    const parts = numberFormatRangeParts(slots, x, y);
    if (parts === null || parts === undefined) {
      throw new $RangeError('formatRange cannot take NaN');
    }
    if (!asParts) {
      let out = '';
      for (let i = 0; i < parts.length; i++) out += parts[i][1];
      return out;
    }
    const result = [];
    for (let i = 0; i < parts.length; i++) {
      const part = {};
      define(part, 'type', parts[i][0], true, true);
      define(part, 'value', parts[i][1], true, true);
      define(part, 'source', parts[i][2], true, true);
      define(result, $String(i), part, true, true);
    }
    return result;
  }

  function FormatParts(slots, value, asParts) {
    const parts = partitionNumber(slots, value);
    if (!asParts) {
      let out = '';
      for (let i = 0; i < parts.length; i++) out += parts[i][1];
      return out;
    }
    const result = [];
    for (let i = 0; i < parts.length; i++) {
      const part = {};
      define(part, 'type', parts[i][0], true, true);
      define(part, 'value', parts[i][1], true, true);
      define(result, $String(i), part, true, true);
    }
    return result;
  }

  define(NumberFormat, 'supportedLocalesOf', numberFormatMethods.supportedLocalesOf, false, true);
  $defineProperty(NumberFormatPrototype, 'format', {
    __proto__: null,
    get: $Object.getOwnPropertyDescriptor(numberFormatMethods, 'format').get,
    set: undefined, enumerable: false, configurable: true,
  });
  define(NumberFormatPrototype, 'formatToParts', numberFormatMethods.formatToParts, false, true);
  define(NumberFormatPrototype, 'formatRange', numberFormatMethods.formatRange, false, true);
  define(NumberFormatPrototype, 'formatRangeToParts', numberFormatMethods.formatRangeToParts, false, true);
  define(NumberFormatPrototype, 'resolvedOptions', numberFormatMethods.resolvedOptions, false, true);
  $defineProperty(NumberFormatPrototype, $toStringTag, {
    __proto__: null, value: 'Intl.NumberFormat', writable: false, enumerable: false, configurable: true,
  });

  // Intl.DateTimeFormat
  const dateTimeFormatSlots = new $WeakMap();
  // The component options, in the order of the table of components.
  const DATE_TIME_COMPONENTS = [
    ['weekday', ['narrow', 'short', 'long']],
    ['era', ['narrow', 'short', 'long']],
    ['year', ['2-digit', 'numeric']],
    ['month', ['2-digit', 'numeric', 'narrow', 'short', 'long']],
    ['day', ['2-digit', 'numeric']],
    ['dayPeriod', ['narrow', 'short', 'long']],
    ['hour', ['2-digit', 'numeric']],
    ['minute', ['2-digit', 'numeric']],
    ['second', ['2-digit', 'numeric']],
    ['fractionalSecondDigits', undefined],
    ['timeZoneName', ['short', 'long', 'shortOffset', 'longOffset', 'shortGeneric',
      'longGeneric']],
  ];
  const STYLES = ['full', 'long', 'medium', 'short'];
  // The internal value for a null [[hc]], which no locale tag can carry.
  const NULL_HC = '<null>';

  function isDigit(c) { return c >= 48 && c <= 57; }

  // IsTimeZoneOffsetString with ParseTimeZoneOffsetString: the offset in
  // minutes, undefined if `s` is not a UTCOffset, and a RangeError for one
  // with seconds (more than one MinuteSecond).
  function parseOffsetMinutes(s) {
    const n = s.length;
    if (n < 3) return undefined;
    const sign = call($charCodeAt, s, 0);
    if (sign !== 43 && sign !== 45) return undefined;
    const two = (at) => {
      if (at + 2 > n) return -1;
      const a = call($charCodeAt, s, at);
      const b = call($charCodeAt, s, at + 1);
      return isDigit(a) && isDigit(b) ? (a - 48) * 10 + (b - 48) : -1;
    };
    const hours = two(1);
    if (hours < 0 || hours > 23) return undefined;
    let at = 3;
    let minutes = 0;
    let groups = 0;
    let extended;
    while (at < n) {
      const c = call($charCodeAt, s, at);
      if (extended === undefined) extended = c === 58;
      if (extended) {
        if (c !== 58) break;
        at++;
      }
      const v = two(at);
      if (v < 0 || v > 59) return undefined;
      if (groups === 0) minutes = v;
      groups++;
      at += 2;
      if (groups === 2) break;
    }
    if (at < n) {
      // A fraction may follow the seconds only.
      const c = call($charCodeAt, s, at);
      if (groups !== 2 || (c !== 46 && c !== 44) || at + 1 >= n) return undefined;
      for (let i = at + 1; i < n; i++) {
        if (!isDigit(call($charCodeAt, s, i)) || i - at > 9) return undefined;
      }
    }
    if (groups > 1) throw new $RangeError('Time zone offset with seconds: ' + s);
    const total = hours * 60 + minutes;
    return sign === 45 ? -total : total;
  }

  // FormatOffsetTimeZoneIdentifier
  function FormatOffsetTimeZoneIdentifier(offsetMinutes) {
    const sign = offsetMinutes >= 0 ? '+' : '-';
    const absolute = $abs(offsetMinutes);
    const pad = (v) => (v < 10 ? '0' : '') + v;
    return sign + pad($floor(absolute / 60)) + ':' + pad(absolute % 60);
  }

  // CreateDateTimeFormat
  function CreateDateTimeFormat(newTarget, locales, options, required, defaults) {
    let proto = newTarget.prototype;
    if (!isObject(proto)) proto = DateTimeFormatPrototype;
    const dtf = $create(proto);
    // ResolveOptions, with ca, nu, hour12 and hc.
    const requested = CanonicalizeLocaleList(locales);
    options = CoerceOptionsToObject(options);
    GetOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit');
    const resolution = (property, type, values) => {
      let value = GetOption(options, property, type, values, undefined);
      if (value === undefined) return undefined;
      value = $String(value);
      if (!isUnicodeType(value)) throw new $RangeError('Invalid ' + property + ': ' + value);
      return value;
    };
    const ca = resolution('calendar', 'string', undefined);
    const nu = resolution('numberingSystem', 'string', undefined);
    const hour12 = resolution('hour12', 'boolean', undefined);
    let hcOption = resolution('hourCycle', 'string', ['h11', 'h12', 'h23', 'h24']);
    if (hour12 !== undefined) hcOption = NULL_HC;
    const resolved = resolveDateTimeFormat(requested,
      ca === undefined ? undefined : asciiCase(ca, false),
      nu === undefined ? undefined : asciiCase(nu, false), hcOption);
    const slots = $create(null);
    slots.locale = resolved[0];
    slots.dataLocale = resolved[1];
    slots.calendar = resolved[2];
    slots.numberingSystem = resolved[3];
    let hc = resolved[4] === NULL_HC ? null : resolved[4];
    if (hour12 === 'true') hc = resolved[6];
    else if (hour12 === 'false') hc = resolved[7];
    else if (hc === null) hc = resolved[5];
    let timeZone = GetOption(options, 'timeZone', 'string', undefined, undefined);
    if (timeZone === undefined) timeZone = systemTimeZone();
    const offset = parseOffsetMinutes(timeZone);
    if (offset !== undefined) {
      timeZone = FormatOffsetTimeZoneIdentifier(offset);
    } else {
      const primary = primaryTimeZone(timeZone);
      if (primary == null) throw new $RangeError('Invalid time zone: ' + timeZone);
      timeZone = primary;
    }
    slots.timeZone = timeZone;
    const formatOptions = $create(null);
    let hasExplicitFormatComponents = false;
    for (let i = 0; i < DATE_TIME_COMPONENTS.length; i++) {
      const key = DATE_TIME_COMPONENTS[i][0];
      const value = key === 'fractionalSecondDigits'
        ? GetNumberOption(options, key, 1, 3, undefined)
        : GetOption(options, key, 'string', DATE_TIME_COMPONENTS[i][1], undefined);
      formatOptions[key] = value;
      if (value !== undefined) hasExplicitFormatComponents = true;
    }
    GetOption(options, 'formatMatcher', 'string', ['basic', 'best fit'], 'best fit');
    const dateStyle = GetOption(options, 'dateStyle', 'string', STYLES, undefined);
    const timeStyle = GetOption(options, 'timeStyle', 'string', STYLES, undefined);
    slots.dateStyle = dateStyle;
    slots.timeStyle = timeStyle;
    if (dateStyle !== undefined || timeStyle !== undefined) {
      if (hasExplicitFormatComponents) {
        throw new $TypeError('dateStyle and timeStyle cannot be used with date or time components');
      }
      if (required === 'date' && timeStyle !== undefined) {
        throw new $TypeError('timeStyle cannot be used to format a date only');
      }
      if (required === 'time' && dateStyle !== undefined) {
        throw new $TypeError('dateStyle cannot be used to format a time only');
      }
    } else {
      let needDefaults = true;
      const keys = [];
      if (required === 'date' || required === 'any') {
        call($push, keys, 'weekday', 'year', 'month', 'day');
      }
      if (required === 'time' || required === 'any') {
        call($push, keys, 'dayPeriod', 'hour', 'minute', 'second', 'fractionalSecondDigits');
      }
      for (let i = 0; i < keys.length; i++) {
        if (formatOptions[keys[i]] !== undefined) needDefaults = false;
      }
      if (needDefaults) {
        if (defaults === 'date' || defaults === 'all') {
          formatOptions.year = formatOptions.month = formatOptions.day = 'numeric';
        }
        if (defaults === 'time' || defaults === 'all') {
          formatOptions.hour = formatOptions.minute = formatOptions.second = 'numeric';
        }
      }
    }
    slots.formatOptions = formatOptions;
    slots.hourCycle = hc;
    // [[DateTimeFormat]]: the pattern's fields; [[HourCycle]] only with an hour.
    const fields = dateTimeFormatFields(slots);
    const format = $create(null);
    let hasHour = false;
    for (let i = 0; i < fields.length; i++) {
      if (fields[i][0] === 'hour') hasHour = true;
      format[fields[i][0]] = fields[i][0] === 'fractionalSecondDigits'
        ? $Number(fields[i][1]) : fields[i][1];
    }
    slots.format = format;
    if (!hasHour) slots.hourCycle = undefined;
    call($weakSet, dateTimeFormatSlots, dtf, slots);
    return dtf;
  }

  function DateTimeFormat() {
    const newTarget = new.target === undefined ? DateTimeFormat : new.target;
    const dtf = CreateDateTimeFormat(newTarget, arguments[0], arguments[1], 'any', 'date');
    // ChainDateTimeFormat
    if (new.target === undefined && isInstanceOf(this, DateTimeFormatPrototype)) {
      $defineProperty(this, FallbackSymbol, {
        __proto__: null, value: dtf, writable: false, enumerable: false, configurable: false,
      });
      return this;
    }
    return dtf;
  }
  const DateTimeFormatPrototype = DateTimeFormat.prototype;
  $defineProperty(DateTimeFormat, 'prototype', { writable: false });

  // UnwrapDateTimeFormat, then RequireInternalSlot.
  function dateTimeFormatOf(dtf, method) {
    if (!isObject(dtf)) {
      throw new $TypeError('Intl.DateTimeFormat.prototype.' + method + ' called on a non-object');
    }
    let slots = call($weakGet, dateTimeFormatSlots, dtf);
    if (slots === undefined && isInstanceOf(dtf, DateTimeFormatPrototype)) {
      const inner = dtf[FallbackSymbol];
      slots = isObject(inner) ? call($weakGet, dateTimeFormatSlots, inner) : undefined;
    }
    if (slots === undefined) {
      throw new $TypeError('Intl.DateTimeFormat.prototype.' + method +
        ' called on an incompatible receiver');
    }
    return slots;
  }

  // FormatDateTime (a string) or FormatDateTimeToParts (an array of parts),
  // after ToNumber of the date as the callers do it.
  function FormatDateTimeParts(slots, x, asParts) {
    const parts = dateTimeFormatParts(slots, timeClip(x));
    if (!asParts) {
      let out = '';
      for (let i = 0; i < parts.length; i++) out += parts[i][1];
      return out;
    }
    const result = [];
    for (let i = 0; i < parts.length; i++) {
      const part = {};
      define(part, 'type', parts[i][0], true, true);
      define(part, 'value', parts[i][1], true, true);
      define(result, $String(i), part, true, true);
    }
    return result;
  }

  // TimeClip, with the RangeError FormatDateTime and the range forms throw.
  function timeClip(x) {
    if (!$isFinite(x) || $abs(x) > 8.64e15) throw new $RangeError('Invalid time value');
    return $trunc(x) + 0;
  }

  // FormatDateTimeRange (a string) or FormatDateTimeRangeToParts.
  function FormatDateTimeRangeParts(slots, x, y, asParts) {
    const parts = dateTimeFormatRangeParts(slots, timeClip(x), timeClip(y));
    if (!asParts) {
      let out = '';
      for (let i = 0; i < parts.length; i++) out += parts[i][1];
      return out;
    }
    const result = [];
    for (let i = 0; i < parts.length; i++) {
      const part = {};
      define(part, 'type', parts[i][0], true, true);
      define(part, 'value', parts[i][1], true, true);
      define(part, 'source', parts[i][2], true, true);
      define(result, $String(i), part, true, true);
    }
    return result;
  }

  function rangeOf(dtf, method, startDate, endDate, asParts) {
    const slots = isObject(dtf) ? call($weakGet, dateTimeFormatSlots, dtf) : undefined;
    if (slots === undefined) {
      throw new $TypeError('Intl.DateTimeFormat.prototype.' + method +
        ' called on an incompatible receiver');
    }
    if (startDate === undefined || endDate === undefined) {
      throw new $TypeError(method + ' needs a start and an end date');
    }
    const x = +startDate;
    const y = +endDate;
    return FormatDateTimeRangeParts(slots, x, y, asParts);
  }

  function dateValue(date) {
    return date === undefined ? $dateNow() : +date;
  }

  const dateTimeFormatMethods = {
    supportedLocalesOf(locales) {
      return SupportedLocales('DateTimeFormat', locales, arguments[1]);
    },
    get format() {
      const slots = dateTimeFormatOf(this, 'format');
      if (slots.boundFormat === undefined) {
        const bound = { __proto__: null, f: (date) => FormatDateTimeParts(slots, dateValue(date), false) };
        const format = bound.f;
        $defineProperty(format, 'name', { value: '' });
        slots.boundFormat = format;
      }
      return slots.boundFormat;
    },
    formatToParts(date) {
      const slots = isObject(this) ? call($weakGet, dateTimeFormatSlots, this) : undefined;
      if (slots === undefined) {
        throw new $TypeError('Intl.DateTimeFormat.prototype.formatToParts called on an incompatible receiver');
      }
      return FormatDateTimeParts(slots, dateValue(date), true);
    },
    formatRange(startDate, endDate) {
      return rangeOf(this, 'formatRange', startDate, endDate, false);
    },
    formatRangeToParts(startDate, endDate) {
      return rangeOf(this, 'formatRangeToParts', startDate, endDate, true);
    },
    resolvedOptions() {
      const s = dateTimeFormatOf(this, 'resolvedOptions');
      const options = {};
      const put = (key, value) => { if (value !== undefined) define(options, key, value, true, true); };
      put('locale', s.locale);
      put('calendar', s.calendar);
      put('numberingSystem', s.numberingSystem);
      put('timeZone', s.timeZone);
      if (s.hourCycle !== undefined) {
        put('hourCycle', s.hourCycle);
        put('hour12', s.hourCycle === 'h11' || s.hourCycle === 'h12');
      }
      const styled = s.dateStyle !== undefined || s.timeStyle !== undefined;
      for (let i = 0; i < DATE_TIME_COMPONENTS.length; i++) {
        const key = DATE_TIME_COMPONENTS[i][0];
        if (!styled) put(key, s.format[key]);
      }
      put('dateStyle', s.dateStyle);
      put('timeStyle', s.timeStyle);
      return options;
    },
  };
  define(DateTimeFormat, 'supportedLocalesOf', dateTimeFormatMethods.supportedLocalesOf, false, true);
  $defineProperty(DateTimeFormatPrototype, 'format', {
    __proto__: null,
    get: $Object.getOwnPropertyDescriptor(dateTimeFormatMethods, 'format').get,
    set: undefined, enumerable: false, configurable: true,
  });
  define(DateTimeFormatPrototype, 'formatToParts', dateTimeFormatMethods.formatToParts, false, true);
  define(DateTimeFormatPrototype, 'formatRange', dateTimeFormatMethods.formatRange, false, true);
  define(DateTimeFormatPrototype, 'formatRangeToParts', dateTimeFormatMethods.formatRangeToParts,
    false, true);
  define(DateTimeFormatPrototype, 'resolvedOptions', dateTimeFormatMethods.resolvedOptions, false, true);
  $defineProperty(DateTimeFormatPrototype, $toStringTag, {
    __proto__: null, value: 'Intl.DateTimeFormat', writable: false, enumerable: false, configurable: true,
  });

  // ECMA-402's locale-sensitive functions of ECMA-262 (Locale Sensitive
  // Functions of the ECMAScript Language Specification).
  // ToString: a template literal throws for a Symbol, as String() does not.
  function ToString(v) {
    return `${v}`;
  }

  function RequireObjectCoercible(v) {
    if (v === undefined || v === null) throw new $TypeError('Cannot convert undefined or null to object');
    return v;
  }

  // TransformCase: tailored languages through ICU4X, the rest by the
  // engine's Unicode default case conversion.
  function TransformCase(string, locales, upper) {
    const requested = CanonicalizeLocaleList(locales);
    const mapped = transformCase(string, requested.length > 0 ? requested[0] : undefined, upper);
    if (mapped !== undefined && mapped !== null) return mapped;
    return call(upper ? $toUpperCase : $toLowerCase, string);
  }

  // JoinIndexedProperties with toLocaleString invoked with the arguments.
  // Tracks active objects so cyclic arrays return '' instead of overflowing.
  const activeArrays = [];
  function JoinIndexedProperties(o, length, locales, options) {
    if (call($includes, activeArrays, o)) return '';
    call($push, activeArrays, o);
    try {
      let r = '';
      for (let k = 0; k < length; k++) {
        if (k > 0) r += ',';
        const next = o[$String(k)];
        if (next !== undefined && next !== null) {
          // Invoke: GetV, then Call with the element itself as this.
          const method = $reflectGet(ToObject(next), 'toLocaleString', next);
          r += ToString(call(method, next, locales, options));
        }
      }
      return r;
    } finally {
      activeArrays.length--;
    }
  }

  // Date's toLocale*String with CreateDateTimeFormat's required and defaults.
  function dateToLocale(date, locales, options, required, defaults) {
    const x = call($dateGetTime, date);
    if (x !== x) return 'Invalid Date';
    const dtf = CreateDateTimeFormat(DateTimeFormat, locales, options, required, defaults);
    return FormatDateTimeParts(call($weakGet, dateTimeFormatSlots, dtf), x, false);
  }

  const localeSensitive = {
    stringToLocaleUpperCase() {
      const string = ToString(RequireObjectCoercible(this));
      return TransformCase(string, arguments[0], true);
    },
    stringToLocaleLowerCase() {
      const string = ToString(RequireObjectCoercible(this));
      return TransformCase(string, arguments[0], false);
    },
    numberToLocaleString() {
      const x = call($numberValueOf, this);
      const nf = new NumberFormat(arguments[0], arguments[1]);
      return FormatParts(call($weakGet, numberFormatSlots, nf), x, false);
    },
    bigIntToLocaleString() {
      const x = call($bigIntValueOf, this);
      const nf = new NumberFormat(arguments[0], arguments[1]);
      return FormatParts(call($weakGet, numberFormatSlots, nf), x, false);
    },
    dateToLocaleString() {
      return dateToLocale(this, arguments[0], arguments[1], 'any', 'all');
    },
    dateToLocaleDateString() {
      return dateToLocale(this, arguments[0], arguments[1], 'date', 'date');
    },
    dateToLocaleTimeString() {
      return dateToLocale(this, arguments[0], arguments[1], 'time', 'time');
    },
    arrayToLocaleString() {
      const array = ToObject(this);
      return JoinIndexedProperties(array, ToLength(array.length), arguments[0], arguments[1]);
    },
    typedArrayToLocaleString() {
      call($typedArrayEntries, this); // ValidateTypedArray
      return JoinIndexedProperties(this, call($typedArrayLength, this), arguments[0], arguments[1]);
    },
  };
  const installMethod = (target, name, key) => {
    const method = localeSensitive[key];
    $defineProperty(method, 'name', { __proto__: null, value: name });
    define(target, name, method, false, true);
  };
  installMethod(String.prototype, 'toLocaleUpperCase', 'stringToLocaleUpperCase');
  installMethod(String.prototype, 'toLocaleLowerCase', 'stringToLocaleLowerCase');
  installMethod(Number.prototype, 'toLocaleString', 'numberToLocaleString');
  installMethod(BigInt.prototype, 'toLocaleString', 'bigIntToLocaleString');
  installMethod(Date.prototype, 'toLocaleString', 'dateToLocaleString');
  installMethod(Date.prototype, 'toLocaleDateString', 'dateToLocaleDateString');
  installMethod(Date.prototype, 'toLocaleTimeString', 'dateToLocaleTimeString');
  installMethod(Array.prototype, 'toLocaleString', 'arrayToLocaleString');
  installMethod(TypedArrayPrototype, 'toLocaleString', 'typedArrayToLocaleString');

  // Intl.RelativeTimeFormat
  const relativeTimeFormatSlots = new $WeakMap();
  function relativeTimeFormatOf(rtf, method) {
    const slots = isObject(rtf) ? call($weakGet, relativeTimeFormatSlots, rtf) : undefined;
    if (slots === undefined) {
      throw new $TypeError('Intl.RelativeTimeFormat.prototype.' + method + ' called on an incompatible receiver');
    }
    return slots;
  }

  // SingularRelativeTimeUnit
  const RELATIVE_UNITS = ['second', 'minute', 'hour', 'day', 'week', 'month', 'quarter', 'year'];
  function SingularRelativeTimeUnit(unit) {
    if (call($includes, RELATIVE_UNITS, unit)) return unit;
    const last = unit.length - 1;
    if (last > 0 && call($charCodeAt, unit, last) === 115) {
      const singular = call($slice, unit, 0, last);
      if (call($includes, RELATIVE_UNITS, singular)) return singular;
    }
    throw new $RangeError('Invalid unit: ' + unit);
  }

  // PartitionRelativeTimePattern, as [type, value, unit] triples.
  function PartitionRelativeTimePattern(slots, value, unit) {
    value = +value;
    unit = ToString(unit);
    if (!$isFinite(value)) throw new $RangeError('Invalid relative time value: ' + value);
    unit = SingularRelativeTimeUnit(unit);
    const x = value === 0 && 1 / value < 0 ? '-0' : $String(value);
    return relativeTimeParts(slots, slots.numberFormat, unit, x);
  }

  function RelativeTimeFormat() {
    if (new.target === undefined) throw new $TypeError("Constructor Intl.RelativeTimeFormat requires 'new'");
    let proto = new.target.prototype;
    if (!isObject(proto)) proto = RelativeTimeFormatPrototype;
    const rtf = $create(proto);
    const requested = CanonicalizeLocaleList(arguments[0]);
    const options = CoerceOptionsToObject(arguments[1]);
    GetOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit');
    let nu = GetOption(options, 'numberingSystem', 'string', undefined, undefined);
    if (nu !== undefined) {
      if (!isUnicodeType(nu)) throw new $RangeError('Invalid numberingSystem: ' + nu);
      nu = asciiCase(nu, false);
    }
    const resolved = resolveNumberFormat(requested, nu, 'RelativeTimeFormat');
    const slots = $create(null);
    slots.locale = resolved[0];
    slots.dataLocale = resolved[1];
    slots.numberingSystem = resolved[2];
    slots.style = GetOption(options, 'style', 'string', ['long', 'short', 'narrow'], 'long');
    slots.numeric = GetOption(options, 'numeric', 'string', ['always', 'auto'], 'always');
    const nf = new NumberFormat(slots.locale, { __proto__: null, numberingSystem: slots.numberingSystem });
    slots.numberFormat = call($weakGet, numberFormatSlots, nf);
    call($weakSet, relativeTimeFormatSlots, rtf, slots);
    return rtf;
  }
  const RelativeTimeFormatPrototype = RelativeTimeFormat.prototype;
  $defineProperty(RelativeTimeFormat, 'prototype', { writable: false });

  class RelativeTimeFormatMethods {
    static supportedLocalesOf(locales) {
      return SupportedLocales('RelativeTimeFormat', locales, arguments[1]);
    }

    format(value, unit) {
      const slots = relativeTimeFormatOf(this, 'format');
      const parts = PartitionRelativeTimePattern(slots, value, unit);
      let out = '';
      for (let i = 0; i < parts.length; i++) out += parts[i][1];
      return out;
    }

    formatToParts(value, unit) {
      const slots = relativeTimeFormatOf(this, 'formatToParts');
      const parts = PartitionRelativeTimePattern(slots, value, unit);
      const result = [];
      for (let i = 0; i < parts.length; i++) {
        const part = {};
        define(part, 'type', parts[i][0], true, true);
        define(part, 'value', parts[i][1], true, true);
        if (parts[i][2] !== '') define(part, 'unit', parts[i][2], true, true);
        define(result, $String(i), part, true, true);
      }
      return result;
    }

    resolvedOptions() {
      const s = relativeTimeFormatOf(this, 'resolvedOptions');
      const options = {};
      define(options, 'locale', s.locale, true, true);
      define(options, 'style', s.style, true, true);
      define(options, 'numeric', s.numeric, true, true);
      define(options, 'numberingSystem', s.numberingSystem, true, true);
      return options;
    }
  }
  define(RelativeTimeFormat, 'supportedLocalesOf', RelativeTimeFormatMethods.supportedLocalesOf, false, true);
  define(RelativeTimeFormatPrototype, 'format', RelativeTimeFormatMethods.prototype.format, false, true);
  define(RelativeTimeFormatPrototype, 'formatToParts', RelativeTimeFormatMethods.prototype.formatToParts, false, true);
  define(RelativeTimeFormatPrototype, 'resolvedOptions', RelativeTimeFormatMethods.prototype.resolvedOptions, false, true);
  $defineProperty(RelativeTimeFormatPrototype, $toStringTag, {
    __proto__: null, value: 'Intl.RelativeTimeFormat', writable: false, enumerable: false, configurable: true,
  });

  // Intl.ListFormat
  const listFormatSlots = new $WeakMap();
  function listFormatOf(lf, method) {
    const slots = isObject(lf) ? call($weakGet, listFormatSlots, lf) : undefined;
    if (slots === undefined) {
      throw new $TypeError('Intl.ListFormat.prototype.' + method + ' called on an incompatible receiver');
    }
    return slots;
  }

  // StringListFromIterable, through the iterator protocol a script sees.
  function StringListFromIterable(iterable) {
    const list = [];
    if (iterable === undefined) return list;
    const method = iterable[$iterator];
    if (typeof method !== 'function') throw new $TypeError('The list is not iterable');
    const iterator = call(method, iterable);
    if (!isObject(iterator)) throw new $TypeError('The iterator is not an object');
    const next = iterator.next;
    for (;;) {
      const result = call(next, iterator);
      if (!isObject(result)) throw new $TypeError('The iterator result is not an object');
      if (result.done) return list;
      const value = result.value;
      if (typeof value !== 'string') {
        const error = new $TypeError('The list holds a value that is not a string');
        // IteratorClose with a throw completion: the TypeError wins.
        try {
          const close = iterator.return;
          if (close !== undefined && close !== null) call(close, iterator);
        } catch (e) {
          // The original completion is thrown.
        }
        throw error;
      }
      call($push, list, value);
    }
  }

  function ListFormat() {
    if (new.target === undefined) throw new $TypeError("Constructor Intl.ListFormat requires 'new'");
    let proto = new.target.prototype;
    if (!isObject(proto)) proto = ListFormatPrototype;
    const lf = $create(proto);
    const requested = CanonicalizeLocaleList(arguments[0]);
    const options = GetOptionsObject(arguments[1]);
    GetOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit');
    const slots = $create(null);
    slots.locale = resolveLocale('ListFormat', requested);
    slots.type = GetOption(options, 'type', 'string', ['conjunction', 'disjunction', 'unit'], 'conjunction');
    slots.style = GetOption(options, 'style', 'string', ['long', 'short', 'narrow'], 'long');
    call($weakSet, listFormatSlots, lf, slots);
    return lf;
  }
  const ListFormatPrototype = ListFormat.prototype;
  $defineProperty(ListFormat, 'prototype', { writable: false });

  class ListFormatMethods {
    static supportedLocalesOf(locales) {
      return SupportedLocales('ListFormat', locales, arguments[1]);
    }

    format(list) {
      const slots = listFormatOf(this, 'format');
      const parts = listParts(slots, StringListFromIterable(list));
      let out = '';
      for (let i = 0; i < parts.length; i++) out += parts[i][1];
      return out;
    }

    formatToParts(list) {
      const slots = listFormatOf(this, 'formatToParts');
      const parts = listParts(slots, StringListFromIterable(list));
      const result = [];
      for (let i = 0; i < parts.length; i++) {
        const part = {};
        define(part, 'type', parts[i][0], true, true);
        define(part, 'value', parts[i][1], true, true);
        define(result, $String(i), part, true, true);
      }
      return result;
    }

    resolvedOptions() {
      const s = listFormatOf(this, 'resolvedOptions');
      const options = {};
      define(options, 'locale', s.locale, true, true);
      define(options, 'type', s.type, true, true);
      define(options, 'style', s.style, true, true);
      return options;
    }
  }
  define(ListFormat, 'supportedLocalesOf', ListFormatMethods.supportedLocalesOf, false, true);
  define(ListFormatPrototype, 'format', ListFormatMethods.prototype.format, false, true);
  define(ListFormatPrototype, 'formatToParts', ListFormatMethods.prototype.formatToParts, false, true);
  define(ListFormatPrototype, 'resolvedOptions', ListFormatMethods.prototype.resolvedOptions, false, true);
  $defineProperty(ListFormatPrototype, $toStringTag, {
    __proto__: null, value: 'Intl.ListFormat', writable: false, enumerable: false, configurable: true,
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
  define(Intl, 'DateTimeFormat', DateTimeFormat, false, true);
  define(Intl, 'NumberFormat', NumberFormat, false, true);
  define(Intl, 'PluralRules', PluralRules, false, true);
  define(Intl, 'RelativeTimeFormat', RelativeTimeFormat, false, true);
  define(Intl, 'ListFormat', ListFormat, false, true);
  $defineProperty(globalThis, 'Intl', {
    value: Intl, writable: true, enumerable: false, configurable: true,
  });

  // The helpers the constructors build on, kept out of reach of scripts.
  return {
    CanonicalizeLocaleList, GetOptionsObject, CoerceOptionsToObject, GetOption,
    GetNumberOption, DefaultNumberOption, REQUIRED, defaultLocale, settingsGeneration,
  };
})
