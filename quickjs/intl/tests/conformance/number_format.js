// Intl.NumberFormat: constructor, options, resolvedOptions, format,
// formatToParts, per ECMA-402's NumberFormat clauses. The currency and unit
// styles, the notations and the ranges are checked where they land.

const NF = Intl.NumberFormat;
const NF_ORDER = ['localeMatcher', 'numberingSystem', 'style', 'currency', 'currencyDisplay',
  'currencySign', 'unit', 'unitDisplay', 'notation', 'minimumIntegerDigits', 'minimumFractionDigits',
  'maximumFractionDigits', 'minimumSignificantDigits', 'maximumSignificantDigits', 'roundingIncrement',
  'roundingMode', 'roundingPriority', 'trailingZeroDisplay', 'compactDisplay', 'useGrouping', 'signDisplay'];

test('sec-intl-numberformat-constructor', 'name, length, prototype', () => {
  verifyFunction(NF, 'NumberFormat', 0, true);
  verifyProperty(NF, 'prototype', { writable: false, enumerable: false, configurable: false });
  verifyProperty(NF.prototype, 'constructor', { value: NF, writable: true, enumerable: false, configurable: true });
  verifyToStringTag(NF.prototype, 'Intl.NumberFormat');
  assert.sameValue(Object.prototype.toString.call(new NF()), '[object Intl.NumberFormat]');
});

test('sec-intl-numberformat-constructor', 'called as a function it constructs', () => {
  const nf = NF('en');
  assert.sameValue(Object.getPrototypeOf(nf), NF.prototype);
  assert.sameValue(nf.format(1234), '1,234');
});

test('sec-chainnumberformat', 'called on a NumberFormat-derived object it wraps (legacy mode)', () => {
  const host = Object.create(NF.prototype);
  const back = NF.call(host, 'en', { minimumFractionDigits: 1 });
  assert.sameValue(back, host);
  const symbols = Object.getOwnPropertySymbols(host);
  assert.sameValue(symbols.length, 1);
  verifyProperty(host, symbols[0], { writable: false, enumerable: false, configurable: false });
  assert.sameValue(host.format(2), '2.0', 'format unwraps');
  assert.sameValue(host.resolvedOptions().minimumFractionDigits, 1, 'resolvedOptions unwraps');
  assert.throws(TypeError, () => host.formatToParts(2), 'formatToParts does not');
  const plain = {};
  assert.notSameValue(NF.call(plain, 'en'), plain, 'an unrelated this is ignored');
});

test('sec-intl-numberformat-constructor', 'subclassing', () => {
  class Sub extends NF {
    twice(x) { return this.format(x) + this.format(x); }
  }
  const s = new Sub('en');
  assert.sameValue(Object.getPrototypeOf(s), Sub.prototype);
  assert.sameValue(s.twice(1), '11');
  assert.sameValue(s.formatToParts(1)[0].type, 'integer');
});

test('sec-intl-numberformat-constructor', 'a NewTarget whose prototype is not an object falls back', () => {
  const nt = function () {}.bind();
  Object.defineProperty(nt, 'prototype', { value: 1 });
  assert.sameValue(Object.getPrototypeOf(Reflect.construct(NF, [], nt)), NF.prototype);
});

test('sec-intl-numberformat-constructor', 'options are read once each, in the spec order', () => {
  const log = [];
  new NF('en', readLog(log));
  assert.deepEqual(log, NF_ORDER);
});

test('sec-intl-numberformat-constructor', 'reads stop at the first invalid value', () => {
  const log = [];
  assert.throws(RangeError, () => new NF('en', loggingOptions(log, {
    localeMatcher: 'lookup', numberingSystem: 'latn', style: 'bogus', currency: 'USD',
  })));
  assert.deepEqual(log, ['localeMatcher', 'numberingSystem', 'style']);
});

test('sec-coerceoptionstoobject', 'options: undefined, primitives and null', () => {
  assert.sameValue(new NF('en', undefined).resolvedOptions().style, 'decimal');
  assert.sameValue(new NF('en', 'x').resolvedOptions().style, 'decimal');
  assert.sameValue(new NF('en', true).resolvedOptions().style, 'decimal');
  assert.throws(TypeError, () => new NF('en', null));
});

const ENUMS = [
  ['localeMatcher', ['lookup', 'best fit'], ['bestfit', 'Lookup']],
  ['style', ['decimal', 'percent'], ['Decimal', 'number', '']],
  ['currencyDisplay', ['code', 'symbol', 'narrowSymbol', 'name'], ['narrowsymbol', 'long']],
  ['currencySign', ['standard', 'accounting'], ['Accounting']],
  ['unitDisplay', ['short', 'narrow', 'long'], ['full']],
  ['notation', ['standard', 'scientific', 'engineering', 'compact'], ['Compact', 'exponential']],
  ['compactDisplay', ['short', 'long'], ['medium']],
  ['signDisplay', ['auto', 'never', 'always', 'exceptZero', 'negative'], ['exceptzero', 'none']],
  ['roundingMode', ['ceil', 'floor', 'expand', 'trunc', 'halfCeil', 'halfFloor', 'halfExpand', 'halfTrunc', 'halfEven'], ['halfUp', 'round']],
  ['roundingPriority', ['auto', 'morePrecision', 'lessPrecision'], ['most']],
  ['trailingZeroDisplay', ['auto', 'stripIfInteger'], ['strip']],
];
for (const [key, good, bad] of ENUMS) {
  for (const value of good) {
    test('sec-getoption', key + ' ' + JSON.stringify(value), () => { new NF('en', { [key]: value }); });
  }
  for (const value of bad) {
    test('sec-getoption', key + ' ' + JSON.stringify(value) + ' is a RangeError', () => {
      assert.throws(RangeError, () => new NF('en', { [key]: value }));
    });
  }
  test('sec-getoption', key + ' Symbol is a TypeError', () => {
    assert.throws(TypeError, () => new NF('en', { [key]: Symbol() }));
  });
}

test('sec-setnumberformatunitoptions', 'currency is required for the currency style, and well formed', () => {
  assert.throws(TypeError, () => new NF('en', { style: 'currency' }));
  for (const bad of ['US', 'USDX', 'US$', '123', 'u5d', '\u0130SD', '']) {
    assert.throws(RangeError, () => new NF('en', { currency: bad }), JSON.stringify(bad));
  }
  assert.sameValue(new NF('en', { style: 'currency', currency: 'eur' }).resolvedOptions().currency, 'EUR');
  assert.sameValue(new NF('en', { style: 'currency', currency: 'XYZ' }).resolvedOptions().currency, 'XYZ', 'unknown but well formed');
  assert.sameValue(new NF('en', { currency: 'EUR' }).resolvedOptions().currency, undefined, 'ignored by other styles');
});

test('sec-setnumberformatunitoptions', 'unit is required for the unit style, and sanctioned', () => {
  assert.throws(TypeError, () => new NF('en', { style: 'unit' }));
  for (const bad of ['meters', 'METER', 'kilometer-per-', '-per-hour', 'meter-per-meter-per-second', 'furlong', 'percent-per-', '']) {
    assert.throws(RangeError, () => new NF('en', { unit: bad }), JSON.stringify(bad));
  }
  assert.sameValue(new NF('en', { style: 'unit', unit: 'kilometer-per-hour' }).resolvedOptions().unit, 'kilometer-per-hour');
  assert.sameValue(new NF('en', { unit: 'meter' }).resolvedOptions().unit, undefined);
});

for (const [currency, digits] of [['USD', 2], ['EUR', 2], ['JPY', 0], ['KRW', 0], ['BHD', 3], ['KWD', 3], ['CLF', 4], ['XYZ', 2]]) {
  test('sec-currencydigits', currency + ' has ' + digits + ' fraction digits', () => {
    const o = new NF('en', { style: 'currency', currency }).resolvedOptions();
    assert.sameValue(o.minimumFractionDigits, digits);
    assert.sameValue(o.maximumFractionDigits, digits);
  });
}

test('sec-intl-numberformat-constructor', 'defaults per style and notation', () => {
  const digits = (o) => [o.minimumFractionDigits, o.maximumFractionDigits, o.minimumSignificantDigits, o.maximumSignificantDigits, o.roundingPriority];
  assert.deepEqual(digits(new NF('en').resolvedOptions()), [0, 3, undefined, undefined, 'auto']);
  assert.deepEqual(digits(new NF('en', { style: 'percent' }).resolvedOptions()), [0, 0, undefined, undefined, 'auto']);
  assert.deepEqual(digits(new NF('en', { notation: 'compact' }).resolvedOptions()), [0, 0, 1, 2, 'morePrecision']);
  assert.deepEqual(digits(new NF('en', { notation: 'compact', maximumFractionDigits: 1 }).resolvedOptions()), [0, 1, undefined, undefined, 'auto']);
  assert.sameValue(new NF('en', { notation: 'compact' }).resolvedOptions().useGrouping, 'min2');
  assert.deepEqual(digits(new NF('en', { style: 'currency', currency: 'USD', notation: 'compact' }).resolvedOptions()), [0, 0, 1, 2, 'morePrecision']);
});

const GROUPING = [
  [undefined, 'auto'], [true, 'always'], [false, false], [null, false], [0, false], ['', false],
  ['always', 'always'], ['auto', 'auto'], ['min2', 'min2'], ['true', 'auto'], ['false', 'auto'],
];
for (const [value, want] of GROUPING) {
  test('sec-getbooleanorstringnumberformatoption', 'useGrouping ' + show(value), () => {
    assert.sameValue(new NF('en', { useGrouping: value }).resolvedOptions().useGrouping, want);
  });
}
test('sec-getbooleanorstringnumberformatoption', 'useGrouping rejects other strings', () => {
  assert.throws(RangeError, () => new NF('en', { useGrouping: 'never' }));
  assert.throws(RangeError, () => new NF('en', { useGrouping: 1 }), 'ToString(1) is "1"');
  assert.throws(TypeError, () => new NF('en', { useGrouping: Symbol() }));
});

test('sec-intl-numberformat-constructor', 'numberingSystem: option, extension and validation', () => {
  assert.sameValue(new NF('en', { numberingSystem: 'arab' }).resolvedOptions().numberingSystem, 'arab');
  assert.sameValue(new NF('en-u-nu-thai').resolvedOptions().numberingSystem, 'thai');
  assert.sameValue(new NF('en-u-nu-thai').resolvedOptions().locale, 'en-u-nu-thai');
  assert.sameValue(new NF('en-u-nu-thai', { numberingSystem: 'arab' }).resolvedOptions().locale, 'en', 'an option that differs drops the extension');
  assert.sameValue(new NF('en-u-nu-thai', { numberingSystem: 'thai' }).resolvedOptions().locale, 'en-u-nu-thai');
  assert.sameValue(new NF('en', { numberingSystem: 'bogus' }).resolvedOptions().numberingSystem, 'latn', 'unsupported is ignored');
  assert.sameValue(new NF('en-u-nu-bogus').resolvedOptions().locale, 'en');
  for (const bad of ['latn-', 'ab', 'abcdefghi', 'latn_x', '', 'la tn']) {
    assert.throws(RangeError, () => new NF('en', { numberingSystem: bad }), JSON.stringify(bad));
  }
});

only('rong', 'the reference compares option values without CanonicalizeUValue', 'sec-resolvelocale',
  'a numberingSystem option is lowercased (CanonicalizeUValue)', () => {
    assert.sameValue(new NF('en', { numberingSystem: 'ARAB' }).resolvedOptions().numberingSystem, 'arab');
  });

test('sec-intl-numberformat-constructor', 'default numbering systems', () => {
  assert.sameValue(new NF('fa').resolvedOptions().numberingSystem, 'arabext');
  assert.sameValue(new NF('en').resolvedOptions().numberingSystem, 'latn');
  assert.sameValue(new NF('hi').resolvedOptions().numberingSystem, 'latn');
  assert.sameValue(new NF('fa').format(12), '\u06f1\u06f2');
  assert.sameValue(new NF('en', { numberingSystem: 'thai' }).format(1234.5), '\u0e51,\u0e52\u0e53\u0e54.\u0e55');
  assert.sameValue(new NF('en', { numberingSystem: 'hanidec' }).format(1203), '\u4e00,\u4e8c\u3007\u4e09');
});

test('sec-intl.numberformat.prototype', 'prototype methods and getter', () => {
  verifyGetter(NF.prototype, 'format');
  verifyMethod(NF.prototype, 'formatToParts', 1);
  verifyMethod(NF.prototype, 'resolvedOptions', 0);
  verifyMethod(NF, 'supportedLocalesOf', 1);
});

test('sec-intl.numberformat.prototype.format', 'the bound format is cached, anonymous, length 1', () => {
  const nf = new NF('en');
  const f = nf.format;
  assert.sameValue(nf.format, f);
  verifyFunction(f, '', 1, false);
  assert.sameValue(f(1234), '1,234', 'works detached');
  assert.sameValue([1, 22].map(nf.format).join('|'), '1|22');
  assert.notSameValue(new NF('en').format, f);
});

test('sec-intl.numberformat.prototype.format', 'the getter on a wrong receiver is a TypeError', () => {
  const get = Object.getOwnPropertyDescriptor(NF.prototype, 'format').get;
  for (const receiver of [undefined, null, 1, {}, NF.prototype, new Intl.PluralRules('en'), Object.create(NF.prototype)]) {
    assert.throws(TypeError, () => get.call(receiver), show(receiver));
  }
});

for (const method of ['formatToParts', 'resolvedOptions']) {
  test('sec-intl.numberformat.prototype', method + ' on a wrong receiver is a TypeError', () => {
    for (const receiver of [undefined, null, 1, {}, NF.prototype, new Intl.PluralRules('en')]) {
      assert.throws(TypeError, () => NF.prototype[method].call(receiver, 1), show(receiver));
    }
  });
}

test('sec-intl.numberformat.prototype.resolvedoptions', 'keys in table order', () => {
  assert.deepEqual(Object.keys(new NF('en').resolvedOptions()), ['locale', 'numberingSystem', 'style',
    'minimumIntegerDigits', 'minimumFractionDigits', 'maximumFractionDigits', 'useGrouping', 'notation',
    'signDisplay', 'roundingIncrement', 'roundingMode', 'roundingPriority', 'trailingZeroDisplay']);
  assert.deepEqual(Object.keys(new NF('en', { style: 'currency', currency: 'EUR', maximumSignificantDigits: 3, notation: 'compact' }).resolvedOptions()),
    ['locale', 'numberingSystem', 'style', 'currency', 'currencyDisplay', 'currencySign', 'minimumIntegerDigits',
      'minimumSignificantDigits', 'maximumSignificantDigits', 'useGrouping', 'notation', 'compactDisplay',
      'signDisplay', 'roundingIncrement', 'roundingMode', 'roundingPriority', 'trailingZeroDisplay']);
  assert.deepEqual(Object.keys(new NF('en', { style: 'unit', unit: 'meter' }).resolvedOptions()).slice(0, 5),
    ['locale', 'numberingSystem', 'style', 'unit', 'unitDisplay']);
});

test('sec-intl.numberformat.prototype.resolvedoptions', 'a fresh ordinary object with data properties', () => {
  const nf = new NF('de', { minimumIntegerDigits: 4, signDisplay: 'negative', roundingMode: 'trunc' });
  const a = nf.resolvedOptions();
  assert.notSameValue(a, nf.resolvedOptions());
  assert.sameValue(Object.getPrototypeOf(a), Object.prototype);
  verifyProperty(a, 'minimumIntegerDigits', { value: 4, writable: true, enumerable: true, configurable: true });
  assert.sameValue(a.locale, 'de');
  assert.sameValue(a.signDisplay, 'negative');
  assert.sameValue(a.roundingMode, 'trunc');
  a.minimumIntegerDigits = 1;
  assert.sameValue(nf.resolvedOptions().minimumIntegerDigits, 4);
});

// format across the blob's locales and option sets, for:
// 0 -0 1 -1 1234.5678 -1234567.891 0.000123 1e21 5e-324 NaN Infinity -Infinity.
const NF_VALUES = [0, -0, 1, -1, 1234.5678, -1234567.891, 0.000123, 1e21, 5e-324, NaN, Infinity, -Infinity];
const NF_TABLE = [
  ["en", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["en", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["en", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["en", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["en", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["en", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["en", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["en", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["en-US", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["en-US", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["en-US", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["en-US", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["en-US", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["en-US", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["en-US", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["en-US", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["en-GB", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["en-GB", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["en-GB", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["en-GB", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["en-GB", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["en-GB", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["en-GB", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["en-GB", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["fr", {},
    ["0","-0","1","-1","1\u202f234,568","-1\u202f234\u202f567,891","0","1\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000","0","NaN","\u221e","-\u221e"]],
  ["fr", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1\u202f235","-1\u202f234\u202f568","0","1\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000","0","NaN","\u221e","-\u221e"]],
  ["fr", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1\u202f234,57","-1\u202f234\u202f567,89","0,00","1\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000,00","0,00","NaN","\u221e","-\u221e"]],
  ["fr", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1\u202f230","-1\u202f230\u202f000","0,000123","1\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["fr", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["fr", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1\u202f234,568","-1\u202f234\u202f567,891","+0","+1\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000","+0","+NaN","+\u221e","-\u221e"]],
  ["fr", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123\u202f457\u00a0%","-123\u202f456\u202f789\u00a0%","0\u00a0%","100\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["fr", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123\u202f456,8\u00a0%","-123\u202f456\u202f789,1\u00a0%","0\u00a0%","+100\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u202f000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["de", {},
    ["0","-0","1","-1","1.234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["de", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1.235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["de", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1.234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","\u221e","-\u221e"]],
  ["de", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1.230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["de", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["de", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1.234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+\u221e","-\u221e"]],
  ["de", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123.457\u00a0%","-123.456.789\u00a0%","0\u00a0%","100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["de", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123.456,8\u00a0%","-123.456.789,1\u00a0%","0\u00a0%","+100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["es", {},
    ["0","-0","1","-1","1234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["es", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["es", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","\u221e","-\u221e"]],
  ["es", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["es", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["es", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+\u221e","-\u221e"]],
  ["es", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123.457\u00a0%","-123.456.789\u00a0%","0\u00a0%","100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["es", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123.456,8\u00a0%","-123.456.789,1\u00a0%","0\u00a0%","+100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["it", {},
    ["0","-0","1","-1","1234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["it", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["it", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","\u221e","-\u221e"]],
  ["it", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["it", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["it", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+\u221e","-\u221e"]],
  ["it", {style: "percent"},
    ["0%","-0%","100%","-100%","123.457%","-123.456.789%","0%","100.000.000.000.000.000.000.000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["it", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123.456,8%","-123.456.789,1%","0%","+100.000.000.000.000.000.000.000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["pt", {},
    ["0","-0","1","-1","1.234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["pt", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1.235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["pt", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1.234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","\u221e","-\u221e"]],
  ["pt", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1.230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["pt", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["pt", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1.234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+\u221e","-\u221e"]],
  ["pt", {style: "percent"},
    ["0%","-0%","100%","-100%","123.457%","-123.456.789%","0%","100.000.000.000.000.000.000.000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["pt", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123.456,8%","-123.456.789,1%","0%","+100.000.000.000.000.000.000.000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["ru", {},
    ["0","-0","1","-1","1\u00a0234,568","-1\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e","\u221e","-\u221e"]],
  ["ru", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1\u00a0235","-1\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e","\u221e","-\u221e"]],
  ["ru", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1\u00a0234,57","-1\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e","\u221e","-\u221e"]],
  ["ru", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1\u00a0230","-1\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e","\u221e","-\u221e"]],
  ["ru", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e","\u221e","-\u221e"]],
  ["ru", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1\u00a0234,568","-1\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e","+\u221e","-\u221e"]],
  ["ru", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123\u00a0457\u00a0%","-123\u00a0456\u00a0789\u00a0%","0\u00a0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["ru", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123\u00a0456,8\u00a0%","-123\u00a0456\u00a0789,1\u00a0%","0\u00a0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","\u043d\u0435\u00a0\u0447\u0438\u0441\u043b\u043e\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["pl", {},
    ["0","-0","1","-1","1234,568","-1\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["pl", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["pl", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","NaN","\u221e","-\u221e"]],
  ["pl", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["pl", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["pl", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+NaN","+\u221e","-\u221e"]],
  ["pl", {style: "percent"},
    ["0%","-0%","100%","-100%","123\u00a0457%","-123\u00a0456\u00a0789%","0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["pl", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123\u00a0456,8%","-123\u00a0456\u00a0789,1%","0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["uk", {},
    ["0","-0","1","-1","1\u00a0234,568","-1\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["uk", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1\u00a0235","-1\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["uk", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1\u00a0234,57","-1\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","NaN","\u221e","-\u221e"]],
  ["uk", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1\u00a0230","-1\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["uk", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["uk", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1\u00a0234,568","-1\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+NaN","+\u221e","-\u221e"]],
  ["uk", {style: "percent"},
    ["0%","-0%","100%","-100%","123\u00a0457%","-123\u00a0456\u00a0789%","0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["uk", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123\u00a0456,8%","-123\u00a0456\u00a0789,1%","0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["cs", {},
    ["0","-0","1","-1","1\u00a0234,568","-1\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["cs", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1\u00a0235","-1\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["cs", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1\u00a0234,57","-1\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","NaN","\u221e","-\u221e"]],
  ["cs", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1\u00a0230","-1\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["cs", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["cs", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1\u00a0234,568","-1\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+NaN","+\u221e","-\u221e"]],
  ["cs", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123\u00a0457\u00a0%","-123\u00a0456\u00a0789\u00a0%","0\u00a0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["cs", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123\u00a0456,8\u00a0%","-123\u00a0456\u00a0789,1\u00a0%","0\u00a0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["ar", {},
    ["0","\u200e-0","1","\u200e-1","1,234.568","\u200e-1,234,567.891","0","1,000,000,000,000,000,000,000","0","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627","\u221e","\u200e-\u221e"]],
  ["ar", {maximumFractionDigits: 0},
    ["0","\u200e-0","1","\u200e-1","1,235","\u200e-1,234,568","0","1,000,000,000,000,000,000,000","0","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627","\u221e","\u200e-\u221e"]],
  ["ar", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","\u200e-0.00","1.00","\u200e-1.00","1,234.57","\u200e-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627","\u221e","\u200e-\u221e"]],
  ["ar", {maximumSignificantDigits: 3},
    ["0","\u200e-0","1","\u200e-1","1,230","\u200e-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627","\u221e","\u200e-\u221e"]],
  ["ar", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","\u200e-000","001","\u200e-001","1234.568","\u200e-1234567.891","000","1000000000000000000000","000","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627","\u221e","\u200e-\u221e"]],
  ["ar", {signDisplay: "always"},
    ["\u200e+0","\u200e-0","\u200e+1","\u200e-1","\u200e+1,234.568","\u200e-1,234,567.891","\u200e+0","\u200e+1,000,000,000,000,000,000,000","\u200e+0","\u200e+\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627","\u200e+\u221e","\u200e-\u221e"]],
  ["ar", {style: "percent"},
    ["0\u200e%\u200e","\u200e-0\u200e%\u200e","100\u200e%\u200e","\u200e-100\u200e%\u200e","123,457\u200e%\u200e","\u200e-123,456,789\u200e%\u200e","0\u200e%\u200e","100,000,000,000,000,000,000,000\u200e%\u200e","0\u200e%\u200e","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627\u200e%\u200e","\u221e\u200e%\u200e","\u200e-\u221e\u200e%\u200e"]],
  ["ar", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u200e%\u200e","0\u200e%\u200e","\u200e+100\u200e%\u200e","\u200e-100\u200e%\u200e","\u200e+123,456.8\u200e%\u200e","\u200e-123,456,789.1\u200e%\u200e","0\u200e%\u200e","\u200e+100,000,000,000,000,000,000,000\u200e%\u200e","0\u200e%\u200e","\u0644\u064a\u0633\u00a0\u0631\u0642\u0645\u064b\u0627\u200e%\u200e","\u200e+\u221e\u200e%\u200e","\u200e-\u221e\u200e%\u200e"]],
  ["he", {},
    ["0","\u200e-0","1","\u200e-1","1,234.568","\u200e-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","\u200e-\u221e"]],
  ["he", {maximumFractionDigits: 0},
    ["0","\u200e-0","1","\u200e-1","1,235","\u200e-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","\u200e-\u221e"]],
  ["he", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","\u200e-0.00","1.00","\u200e-1.00","1,234.57","\u200e-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","\u200e-\u221e"]],
  ["he", {maximumSignificantDigits: 3},
    ["0","\u200e-0","1","\u200e-1","1,230","\u200e-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","\u200e-\u221e"]],
  ["he", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","\u200e-000","001","\u200e-001","1234.568","\u200e-1234567.891","000","1000000000000000000000","000","NaN","\u221e","\u200e-\u221e"]],
  ["he", {signDisplay: "always"},
    ["\u200e+0","\u200e-0","\u200e+1","\u200e-1","\u200e+1,234.568","\u200e-1,234,567.891","\u200e+0","\u200e+1,000,000,000,000,000,000,000","\u200e+0","\u200e+NaN","\u200e+\u221e","\u200e-\u221e"]],
  ["he", {style: "percent"},
    ["0%","\u200e-0%","100%","\u200e-100%","123,457%","\u200e-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","\u200e-\u221e%"]],
  ["he", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","\u200e+100%","\u200e-100%","\u200e+123,456.8%","\u200e-123,456,789.1%","0%","\u200e+100,000,000,000,000,000,000,000%","0%","NaN%","\u200e+\u221e%","\u200e-\u221e%"]],
  ["ja", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["ja", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["ja", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["ja", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["ja", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["ja", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["ja", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["ja", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["zh", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["zh", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["zh", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["zh", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["zh", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["zh", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["zh", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["zh", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["ko", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["ko", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["ko", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["ko", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["ko", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["ko", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["ko", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["ko", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["hi", {},
    ["0","-0","1","-1","1,234.568","-12,34,567.891","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","\u221e","-\u221e"]],
  ["hi", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-12,34,568","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","\u221e","-\u221e"]],
  ["hi", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-12,34,567.89","0.00","1,00,00,00,00,00,00,00,00,00,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["hi", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-12,30,000","0.000123","1,00,00,00,00,00,00,00,00,00,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["hi", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["hi", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-12,34,567.891","+0","+1,00,00,00,00,00,00,00,00,00,000","+0","+NaN","+\u221e","-\u221e"]],
  ["hi", {style: "percent"},
    ["0%","-0%","100%","-100%","1,23,457%","-12,34,56,789%","0%","1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["hi", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+1,23,456.8%","-12,34,56,789.1%","0%","+1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["cy", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["cy", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["cy", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["cy", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["cy", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["cy", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["cy", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["cy", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["sl", {},
    ["0","\u22120","1","\u22121","1234,568","\u22121.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","\u2212\u221e"]],
  ["sl", {maximumFractionDigits: 0},
    ["0","\u22120","1","\u22121","1235","\u22121.234.568","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","\u2212\u221e"]],
  ["sl", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","\u22120,00","1,00","\u22121,00","1234,57","\u22121.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","\u221e","\u2212\u221e"]],
  ["sl", {maximumSignificantDigits: 3},
    ["0","\u22120","1","\u22121","1230","\u22121.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","\u2212\u221e"]],
  ["sl", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","\u2212000","001","\u2212001","1234,568","\u22121234567,891","000","1000000000000000000000","000","NaN","\u221e","\u2212\u221e"]],
  ["sl", {signDisplay: "always"},
    ["+0","\u22120","+1","\u22121","+1234,568","\u22121.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+\u221e","\u2212\u221e"]],
  ["sl", {style: "percent"},
    ["0\u00a0%","\u22120\u00a0%","100\u00a0%","\u2212100\u00a0%","123.457\u00a0%","\u2212123.456.789\u00a0%","0\u00a0%","100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","\u2212\u221e\u00a0%"]],
  ["sl", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","\u2212100\u00a0%","+123.456,8\u00a0%","\u2212123.456.789,1\u00a0%","0\u00a0%","+100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","\u2212\u221e\u00a0%"]],
  ["lt", {},
    ["0","\u22120","1","\u22121","1\u00a0234,568","\u22121\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","\u2212\u221e"]],
  ["lt", {maximumFractionDigits: 0},
    ["0","\u22120","1","\u22121","1\u00a0235","\u22121\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","\u2212\u221e"]],
  ["lt", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","\u22120,00","1,00","\u22121,00","1\u00a0234,57","\u22121\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","NaN","\u221e","\u2212\u221e"]],
  ["lt", {maximumSignificantDigits: 3},
    ["0","\u22120","1","\u22121","1\u00a0230","\u22121\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","\u2212\u221e"]],
  ["lt", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","\u2212000","001","\u2212001","1234,568","\u22121234567,891","000","1000000000000000000000","000","NaN","\u221e","\u2212\u221e"]],
  ["lt", {signDisplay: "always"},
    ["+0","\u22120","+1","\u22121","+1\u00a0234,568","\u22121\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+NaN","+\u221e","\u2212\u221e"]],
  ["lt", {style: "percent"},
    ["0\u00a0%","\u22120\u00a0%","100\u00a0%","\u2212100\u00a0%","123\u00a0457\u00a0%","\u2212123\u00a0456\u00a0789\u00a0%","0\u00a0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","\u2212\u221e\u00a0%"]],
  ["lt", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","\u2212100\u00a0%","+123\u00a0456,8\u00a0%","\u2212123\u00a0456\u00a0789,1\u00a0%","0\u00a0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","\u2212\u221e\u00a0%"]],
  ["ga", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","Nuimh","\u221e","-\u221e"]],
  ["ga", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","Nuimh","\u221e","-\u221e"]],
  ["ga", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","Nuimh","\u221e","-\u221e"]],
  ["ga", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","Nuimh","\u221e","-\u221e"]],
  ["ga", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","Nuimh","\u221e","-\u221e"]],
  ["ga", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+Nuimh","+\u221e","-\u221e"]],
  ["ga", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","Nuimh%","\u221e%","-\u221e%"]],
  ["ga", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","Nuimh%","+\u221e%","-\u221e%"]],
  ["br", {},
    ["0","-0","1","-1","1\u00a0234,568","-1\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["br", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1\u00a0235","-1\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["br", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1\u00a0234,57","-1\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","NaN","\u221e","-\u221e"]],
  ["br", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1\u00a0230","-1\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["br", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["br", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1\u00a0234,568","-1\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+NaN","+\u221e","-\u221e"]],
  ["br", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123\u00a0457\u00a0%","-123\u00a0456\u00a0789\u00a0%","0\u00a0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["br", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123\u00a0456,8\u00a0%","-123\u00a0456\u00a0789,1\u00a0%","0\u00a0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["mt", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["mt", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["mt", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["mt", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["mt", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["mt", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["mt", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["mt", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["ro", {},
    ["0","-0","1","-1","1.234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["ro", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1.235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","\u221e","-\u221e"]],
  ["ro", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1.234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","\u221e","-\u221e"]],
  ["ro", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1.230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["ro", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["ro", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1.234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+\u221e","-\u221e"]],
  ["ro", {style: "percent"},
    ["0\u00a0%","-0\u00a0%","100\u00a0%","-100\u00a0%","123.457\u00a0%","-123.456.789\u00a0%","0\u00a0%","100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","\u221e\u00a0%","-\u221e\u00a0%"]],
  ["ro", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0\u00a0%","0\u00a0%","+100\u00a0%","-100\u00a0%","+123.456,8\u00a0%","-123.456.789,1\u00a0%","0\u00a0%","+100.000.000.000.000.000.000.000\u00a0%","0\u00a0%","NaN\u00a0%","+\u221e\u00a0%","-\u221e\u00a0%"]],
  ["fa", {},
    ["\u06f0","\u200e\u2212\u06f0","\u06f1","\u200e\u2212\u06f1","\u06f1\u066c\u06f2\u06f3\u06f4\u066b\u06f5\u06f6\u06f8","\u200e\u2212\u06f1\u066c\u06f2\u06f3\u06f4\u066c\u06f5\u06f6\u06f7\u066b\u06f8\u06f9\u06f1","\u06f0","\u06f1\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0","\u06f0","\u0646\u0627\u0639\u062f\u062f","\u221e","\u200e\u2212\u221e"]],
  ["fa", {maximumFractionDigits: 0},
    ["\u06f0","\u200e\u2212\u06f0","\u06f1","\u200e\u2212\u06f1","\u06f1\u066c\u06f2\u06f3\u06f5","\u200e\u2212\u06f1\u066c\u06f2\u06f3\u06f4\u066c\u06f5\u06f6\u06f8","\u06f0","\u06f1\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0","\u06f0","\u0646\u0627\u0639\u062f\u062f","\u221e","\u200e\u2212\u221e"]],
  ["fa", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["\u06f0\u066b\u06f0\u06f0","\u200e\u2212\u06f0\u066b\u06f0\u06f0","\u06f1\u066b\u06f0\u06f0","\u200e\u2212\u06f1\u066b\u06f0\u06f0","\u06f1\u066c\u06f2\u06f3\u06f4\u066b\u06f5\u06f7","\u200e\u2212\u06f1\u066c\u06f2\u06f3\u06f4\u066c\u06f5\u06f6\u06f7\u066b\u06f8\u06f9","\u06f0\u066b\u06f0\u06f0","\u06f1\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066b\u06f0\u06f0","\u06f0\u066b\u06f0\u06f0","\u0646\u0627\u0639\u062f\u062f","\u221e","\u200e\u2212\u221e"]],
  ["fa", {maximumSignificantDigits: 3},
    ["\u06f0","\u200e\u2212\u06f0","\u06f1","\u200e\u2212\u06f1","\u06f1\u066c\u06f2\u06f3\u06f0","\u200e\u2212\u06f1\u066c\u06f2\u06f3\u06f0\u066c\u06f0\u06f0\u06f0","\u06f0\u066b\u06f0\u06f0\u06f0\u06f1\u06f2\u06f3","\u06f1\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0","\u06f0\u066b\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f5","\u0646\u0627\u0639\u062f\u062f","\u221e","\u200e\u2212\u221e"]],
  ["fa", {useGrouping: false, minimumIntegerDigits: 3},
    ["\u06f0\u06f0\u06f0","\u200e\u2212\u06f0\u06f0\u06f0","\u06f0\u06f0\u06f1","\u200e\u2212\u06f0\u06f0\u06f1","\u06f1\u06f2\u06f3\u06f4\u066b\u06f5\u06f6\u06f8","\u200e\u2212\u06f1\u06f2\u06f3\u06f4\u06f5\u06f6\u06f7\u066b\u06f8\u06f9\u06f1","\u06f0\u06f0\u06f0","\u06f1\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0\u06f0","\u06f0\u06f0\u06f0","\u0646\u0627\u0639\u062f\u062f","\u221e","\u200e\u2212\u221e"]],
  ["fa", {signDisplay: "always"},
    ["\u200e+\u06f0","\u200e\u2212\u06f0","\u200e+\u06f1","\u200e\u2212\u06f1","\u200e+\u06f1\u066c\u06f2\u06f3\u06f4\u066b\u06f5\u06f6\u06f8","\u200e\u2212\u06f1\u066c\u06f2\u06f3\u06f4\u066c\u06f5\u06f6\u06f7\u066b\u06f8\u06f9\u06f1","\u200e+\u06f0","\u200e+\u06f1\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0","\u200e+\u06f0","\u200e+\u0646\u0627\u0639\u062f\u062f","\u200e+\u221e","\u200e\u2212\u221e"]],
  ["fa", {style: "percent"},
    ["\u06f0\u066a","\u200e\u2212\u06f0\u066a","\u06f1\u06f0\u06f0\u066a","\u200e\u2212\u06f1\u06f0\u06f0\u066a","\u06f1\u06f2\u06f3\u066c\u06f4\u06f5\u06f7\u066a","\u200e\u2212\u06f1\u06f2\u06f3\u066c\u06f4\u06f5\u06f6\u066c\u06f7\u06f8\u06f9\u066a","\u06f0\u066a","\u06f1\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066a","\u06f0\u066a","\u0646\u0627\u0639\u062f\u062f\u066a","\u221e\u066a","\u200e\u2212\u221e\u066a"]],
  ["fa", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["\u06f0\u066a","\u06f0\u066a","\u200e+\u06f1\u06f0\u06f0\u066a","\u200e\u2212\u06f1\u06f0\u06f0\u066a","\u200e+\u06f1\u06f2\u06f3\u066c\u06f4\u06f5\u06f6\u066b\u06f8\u066a","\u200e\u2212\u06f1\u06f2\u06f3\u066c\u06f4\u06f5\u06f6\u066c\u06f7\u06f8\u06f9\u066b\u06f1\u066a","\u06f0\u066a","\u200e+\u06f1\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066c\u06f0\u06f0\u06f0\u066a","\u06f0\u066a","\u0646\u0627\u0639\u062f\u062f\u066a","\u200e+\u221e\u066a","\u200e\u2212\u221e\u066a"]],
  ["gv", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["gv", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","\u221e","-\u221e"]],
  ["gv", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["gv", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["gv", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["gv", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+\u221e","-\u221e"]],
  ["gv", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["gv", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["zh-Hant", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","\u975e\u6578\u503c","\u221e","-\u221e"]],
  ["zh-Hant", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","\u975e\u6578\u503c","\u221e","-\u221e"]],
  ["zh-Hant", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","\u975e\u6578\u503c","\u221e","-\u221e"]],
  ["zh-Hant", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","\u975e\u6578\u503c","\u221e","-\u221e"]],
  ["zh-Hant", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","\u975e\u6578\u503c","\u221e","-\u221e"]],
  ["zh-Hant", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+\u975e\u6578\u503c","+\u221e","-\u221e"]],
  ["zh-Hant", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","\u975e\u6578\u503c%","\u221e%","-\u221e%"]],
  ["zh-Hant", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","\u975e\u6578\u503c%","+\u221e%","-\u221e%"]],
  ["en-IN", {},
    ["0","-0","1","-1","1,234.568","-12,34,567.891","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","\u221e","-\u221e"]],
  ["en-IN", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-12,34,568","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","\u221e","-\u221e"]],
  ["en-IN", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-12,34,567.89","0.00","1,00,00,00,00,00,00,00,00,00,000.00","0.00","NaN","\u221e","-\u221e"]],
  ["en-IN", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-12,30,000","0.000123","1,00,00,00,00,00,00,00,00,00,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["en-IN", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["en-IN", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-12,34,567.891","+0","+1,00,00,00,00,00,00,00,00,00,000","+0","+NaN","+\u221e","-\u221e"]],
  ["en-IN", {style: "percent"},
    ["0%","-0%","100%","-100%","1,23,457%","-12,34,56,789%","0%","1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["en-IN", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+1,23,456.8%","-12,34,56,789.1%","0%","+1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","+\u221e%","-\u221e%"]],
  ["pt-PT", {},
    ["0","-0","1","-1","1234,568","-1\u00a0234\u00a0567,891","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["pt-PT", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1\u00a0234\u00a0568","0","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0","NaN","\u221e","-\u221e"]],
  ["pt-PT", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1\u00a0234\u00a0567,89","0,00","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000,00","0,00","NaN","\u221e","-\u221e"]],
  ["pt-PT", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1\u00a0230\u00a0000","0,000123","1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","\u221e","-\u221e"]],
  ["pt-PT", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","\u221e","-\u221e"]],
  ["pt-PT", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1\u00a0234\u00a0567,891","+0","+1\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000","+0","+NaN","+\u221e","-\u221e"]],
  ["pt-PT", {style: "percent"},
    ["0%","-0%","100%","-100%","123\u00a0457%","-123\u00a0456\u00a0789%","0%","100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000%","0%","NaN%","\u221e%","-\u221e%"]],
  ["pt-PT", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123\u00a0456,8%","-123\u00a0456\u00a0789,1%","0%","+100\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000\u00a0000%","0%","NaN%","+\u221e%","-\u221e%"]],
];
// The table holds CLDR 46+ data: CLDR 46 made latn the default for ar and
// set minimumGroupingDigits 2 for it and sl. An engine on older data skips
// those rows.
const NF_CLDR46 = { ar: true, it: true, sl: true };
for (const [locale, options, want] of NF_TABLE) {
  const variants = NF_CLDR46[locale] ? [[{ cldrBelow: 46 }, SKIP_DATA, 'the ' + locale + ' rows hold CLDR 46 data']] : [];
  dataTest('sec-formatnumber', locale + ' ' + JSON.stringify(options), want, variants, (want) => {
    const nf = new NF(locale, options);
    const got = NF_VALUES.map((v) => nf.format(v));
    for (let i = 0; i < got.length; i++) assert.sameValue(got[i], want[i], show(NF_VALUES[i]));
    for (let i = 0; i < got.length; i++) {
      assert.sameValue(joinParts(nf.formatToParts(NF_VALUES[i])), got[i], 'formatToParts of ' + show(NF_VALUES[i]));
    }
  });
}

const ROUNDING = [
  // mode, values -2.5 -1.5 -0.5 0.5 1.5 2.5 1.4 -1.6 at 0 fraction digits
  ['ceil', '-2 -1 -0 1 2 3 2 -1'],
  ['floor', '-3 -2 -1 0 1 2 1 -2'],
  ['expand', '-3 -2 -1 1 2 3 2 -2'],
  ['trunc', '-2 -1 -0 0 1 2 1 -1'],
  ['halfCeil', '-2 -1 -0 1 2 3 1 -2'],
  ['halfFloor', '-3 -2 -1 0 1 2 1 -2'],
  ['halfExpand', '-3 -2 -1 1 2 3 1 -2'],
  ['halfTrunc', '-2 -1 -0 0 1 2 1 -2'],
  ['halfEven', '-2 -2 -0 0 2 2 1 -2'],
];
for (const [roundingMode, want] of ROUNDING) {
  test('sec-applyunsignedroundingmode', roundingMode, () => {
    const nf = new NF('en', { roundingMode, maximumFractionDigits: 0 });
    assert.sameValue([-2.5, -1.5, -0.5, 0.5, 1.5, 2.5, 1.4, -1.6].map((v) => nf.format(v)).join(' '), want);
  });
}

const INCREMENT_CASES = [
  [{ roundingIncrement: 5, minimumFractionDigits: 2, maximumFractionDigits: 2 }, 1.23, '1.25'],
  [{ roundingIncrement: 5, minimumFractionDigits: 2, maximumFractionDigits: 2 }, 1.22, '1.20'],
  [{ roundingIncrement: 25, minimumFractionDigits: 2, maximumFractionDigits: 2 }, 1.3, '1.25'],
  [{ roundingIncrement: 50, minimumFractionDigits: 1, maximumFractionDigits: 1 }, 7.6, '10.0'],
  [{ roundingIncrement: 5000, minimumFractionDigits: 3, maximumFractionDigits: 3 }, 2.6, '5.000'],
  [{ roundingIncrement: 5, minimumFractionDigits: 2, maximumFractionDigits: 2, roundingMode: 'floor' }, 1.29, '1.25'],
  [{ roundingIncrement: 2, minimumFractionDigits: 0, maximumFractionDigits: 0 }, 3, '4'],
];
for (const [options, value, want] of INCREMENT_CASES) {
  test('sec-intl-numberformat-constructor', 'roundingIncrement ' + JSON.stringify(options) + ' ' + value, () => {
    assert.sameValue(new NF('en', options).format(value), want);
  });
}

const DIGIT_CASES = [
  [{ minimumIntegerDigits: 3 }, 5, '005'],
  [{ minimumIntegerDigits: 21 }, 1, '000,000,000,000,000,000,001'],
  [{ minimumIntegerDigits: 3, useGrouping: false }, 1234, '1234'],
  // Engines read a Number through its shortest round-trip decimal.
  [{ maximumFractionDigits: 100 }, 0.1, '0.1'],
  [{ maximumFractionDigits: 100 }, '0.1000000000000000055511151231257827021181583404541015625', '0.1000000000000000055511151231257827021181583404541015625'],
  [{ minimumFractionDigits: 3 }, 1, '1.000'],
  [{ maximumSignificantDigits: 1 }, 987, '1,000'],
  [{ minimumSignificantDigits: 5 }, 1.5, '1.5000'],
  [{ maximumSignificantDigits: 21 }, 0.1, '0.1'],
  [{ minimumSignificantDigits: 21 }, 0.1, '0.100000000000000000000'],
  [{ maximumSignificantDigits: 2, maximumFractionDigits: 2, roundingPriority: 'morePrecision' }, 1.234, '1.23'],
  [{ maximumSignificantDigits: 2, maximumFractionDigits: 2, roundingPriority: 'lessPrecision' }, 1.234, '1.2'],
  [{ trailingZeroDisplay: 'stripIfInteger', minimumFractionDigits: 2 }, 1, '1'],
  [{ trailingZeroDisplay: 'stripIfInteger', minimumFractionDigits: 2 }, 1.5, '1.50'],
  [{ signDisplay: 'never' }, -1, '1'],
  [{ signDisplay: 'exceptZero' }, 0, '0'],
  [{ signDisplay: 'exceptZero' }, -0, '0'],
  [{ signDisplay: 'exceptZero' }, -0.0001, '0', ],
  [{ signDisplay: 'negative' }, -0, '0'],
  [{ signDisplay: 'negative' }, -1, '-1'],
  [{ signDisplay: 'auto' }, -0, '-0'],
  [{ signDisplay: 'always' }, 0, '+0'],
  [{ signDisplay: 'always' }, -0, '-0'],
  [{ useGrouping: 'min2' }, 1234, '1234'],
  [{ useGrouping: 'min2' }, 12345, '12,345'],
  [{ useGrouping: 'always' }, 1234, '1,234'],
];
for (const [options, value, want] of DIGIT_CASES) {
  test('sec-formatnumber', 'en ' + JSON.stringify(options) + ' ' + show(value), () => {
    assert.sameValue(new NF('en', options).format(value), want);
  });
}

test('sec-formatnumber', 'grouping follows the locale', () => {
  assert.sameValue(new NF('es').format(1234), '1234', 'es groups from five digits');
  assert.sameValue(new NF('es').format(12345), '12.345');
  assert.sameValue(new NF('es', { useGrouping: 'always' }).format(1234), '1.234');
  assert.sameValue(new NF('pl').format(1234), '1234');
  assert.sameValue(new NF('en-IN').format(1234567), '12,34,567');
  assert.sameValue(new NF('hi').format(1234567), '12,34,567');
});

test('sec-tointlmathematicalvalue', 'BigInt and decimal strings are exact', () => {
  const nf = new NF('en', { maximumFractionDigits: 20 });
  assert.sameValue(nf.format(12345678901234567890n), '12,345,678,901,234,567,890');
  assert.sameValue(nf.format(-(10n ** 30n)), '-1,000,000,000,000,000,000,000,000,000,000');
  assert.sameValue(nf.format('12345678901234567890.123456789'), '12,345,678,901,234,567,890.123456789');
  assert.sameValue(nf.format('0.1'), '0.1');
  assert.sameValue(nf.format('-0'), '-0');
  assert.sameValue(nf.format(' 12 '), '12');
  assert.sameValue(nf.format('1e3'), '1,000');
  assert.sameValue(nf.format('.5'), '0.5');
  assert.sameValue(nf.format('5.'), '5');
  assert.sameValue(nf.format('0x1F'), '31');
  assert.sameValue(nf.format('0b11'), '3');
  assert.sameValue(nf.format('0o17'), '15');
  assert.sameValue(nf.format(''), '0');
  assert.sameValue(nf.format('abc'), 'NaN');
  assert.sameValue(nf.format('Infinity'), '\u221e');
  assert.sameValue(nf.format('-Infinity'), '-\u221e');
  assert.sameValue(nf.format('1_000'), 'NaN');
});

test('sec-tointlmathematicalvalue', 'RoundMVResult: strings past the Number range are infinities or zeros', () => {
  const nf = new NF('en', { maximumFractionDigits: 100 });
  assert.sameValue(nf.format('1e400'), '\u221e');
  assert.sameValue(nf.format('-1e400'), '-\u221e');
  assert.sameValue(nf.format('1e-400'), '0');
  assert.sameValue(nf.format('-1e-400'), '-0');
  assert.sameValue(nf.format('1e-100'), '0.' + '0'.repeat(99) + '1', 'in range it stays exact');
});

test('sec-tointlmathematicalvalue', 'other values', () => {
  const nf = new NF('en');
  assert.sameValue(nf.format(), 'NaN');
  assert.sameValue(nf.format(null), '0');
  assert.sameValue(nf.format(true), '1');
  assert.sameValue(nf.format([]), '0');
  assert.sameValue(nf.format([5]), '5');
  assert.sameValue(nf.format({ valueOf: () => 3n }), '3');
  assert.sameValue(nf.format({ [Symbol.toPrimitive]: () => '7' }), '7');
  assert.throws(TypeError, () => nf.format(Symbol()));
  assert.throws(TypeError, () => nf.format({ valueOf: () => ({}), toString: () => ({}) }));
  assert.sameValue(nf.format(Number.MAX_VALUE).length, 309 + 102);
  assert.sameValue(nf.format(Number.MAX_SAFE_INTEGER), '9,007,199,254,740,991');
});

test('sec-formatnumbertoparts', 'part types and order', () => {
  const parts = new NF('en', { minimumFractionDigits: 2 }).formatToParts(-1234.5);
  assert.deepEqual(parts.map((p) => [p.type, p.value]), [
    ['minusSign', '-'], ['integer', '1'], ['group', ','], ['integer', '234'], ['decimal', '.'], ['fraction', '50'],
  ]);
  assert.deepEqual(new NF('en', { style: 'percent', signDisplay: 'always' }).formatToParts(0.5).map((p) => p.type),
    ['plusSign', 'integer', 'percentSign']);
  assert.deepEqual(new NF('en').formatToParts(NaN).map((p) => p.type), ['nan']);
  assert.deepEqual(new NF('en').formatToParts(-Infinity).map((p) => p.type), ['minusSign', 'infinity']);
  assert.deepEqual(new NF('de', { style: 'percent' }).formatToParts(0.25).map((p) => [p.type, p.value]),
    [['integer', '25'], ['literal', '\u00a0'], ['percentSign', '%']]);
});

test('sec-formatnumbertoparts', 'each part is a fresh ordinary object with type then value', () => {
  const parts = new NF('en').formatToParts(1);
  assert.ok(Array.isArray(parts));
  assert.deepEqual(Object.keys(parts[0]), ['type', 'value']);
  verifyProperty(parts[0], 'type', { writable: true, enumerable: true, configurable: true });
  assert.sameValue(Object.getPrototypeOf(parts[0]), Object.prototype);
  assert.deepEqual(new NF('en').formatToParts().map((p) => p.type), ['nan'], 'undefined is NaN');
});

test('sec-intl.numberformat.supportedlocalesof', 'each localeMatcher', () => {
  for (const localeMatcher of ['lookup', 'best fit', undefined]) {
    assert.deepEqual(NF.supportedLocalesOf(['fr-CA', 'ar', 'zh-Hant-TW'], { localeMatcher }), ['fr-CA', 'ar', 'zh-Hant-TW']);
  }
  assert.throws(RangeError, () => NF.supportedLocalesOf('en', { localeMatcher: 'none' }));
  assert.throws(TypeError, () => NF.supportedLocalesOf('en', null));
  const result = NF.supportedLocalesOf('en');
  assert.ok(Array.isArray(result));
  verifyProperty(result, 0, { writable: true, enumerable: true, configurable: true });
});
