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
  for (const bad of ['US', 'USDX', 'US$', '123', 'u5d', 'İSD', '']) {
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
  assert.sameValue(new NF('fa').format(12), '۱۲');
  assert.sameValue(new NF('en', { numberingSystem: 'thai' }).format(1234.5), '๑,๒๓๔.๕');
  assert.sameValue(new NF('en', { numberingSystem: 'hanidec' }).format(1203), '一,二〇三');
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
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["en", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["en", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["en", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["en", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["en", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["en", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["en", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["en-US", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["en-US", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["en-US", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["en-US", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["en-US", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["en-US", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["en-US", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["en-US", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["en-GB", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["en-GB", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["en-GB", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["en-GB", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["en-GB", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["en-GB", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["en-GB", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["en-GB", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["fr", {},
    ["0","-0","1","-1","1 234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["fr", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1 235","-1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["fr", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1 234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","-∞"]],
  ["fr", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1 230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["fr", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["fr", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1 234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","-∞"]],
  ["fr", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123 457 %","-123 456 789 %","0 %","100 000 000 000 000 000 000 000 %","0 %","NaN %","∞ %","-∞ %"]],
  ["fr", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123 456,8 %","-123 456 789,1 %","0 %","+100 000 000 000 000 000 000 000 %","0 %","NaN %","+∞ %","-∞ %"]],
  ["de", {},
    ["0","-0","1","-1","1.234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["de", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1.235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["de", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1.234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","∞","-∞"]],
  ["de", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1.230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["de", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["de", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1.234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+∞","-∞"]],
  ["de", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123.457 %","-123.456.789 %","0 %","100.000.000.000.000.000.000.000 %","0 %","NaN %","∞ %","-∞ %"]],
  ["de", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123.456,8 %","-123.456.789,1 %","0 %","+100.000.000.000.000.000.000.000 %","0 %","NaN %","+∞ %","-∞ %"]],
  ["es", {},
    ["0","-0","1","-1","1234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["es", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["es", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","∞","-∞"]],
  ["es", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["es", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["es", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+∞","-∞"]],
  ["es", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123.457 %","-123.456.789 %","0 %","100.000.000.000.000.000.000.000 %","0 %","NaN %","∞ %","-∞ %"]],
  ["es", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123.456,8 %","-123.456.789,1 %","0 %","+100.000.000.000.000.000.000.000 %","0 %","NaN %","+∞ %","-∞ %"]],
  ["it", {},
    ["0","-0","1","-1","1234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["it", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["it", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","∞","-∞"]],
  ["it", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["it", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["it", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+∞","-∞"]],
  ["it", {style: "percent"},
    ["0%","-0%","100%","-100%","123.457%","-123.456.789%","0%","100.000.000.000.000.000.000.000%","0%","NaN%","∞%","-∞%"]],
  ["it", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123.456,8%","-123.456.789,1%","0%","+100.000.000.000.000.000.000.000%","0%","NaN%","+∞%","-∞%"]],
  ["pt", {},
    ["0","-0","1","-1","1.234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["pt", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1.235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["pt", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1.234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","∞","-∞"]],
  ["pt", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1.230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["pt", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["pt", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1.234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+∞","-∞"]],
  ["pt", {style: "percent"},
    ["0%","-0%","100%","-100%","123.457%","-123.456.789%","0%","100.000.000.000.000.000.000.000%","0%","NaN%","∞%","-∞%"]],
  ["pt", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123.456,8%","-123.456.789,1%","0%","+100.000.000.000.000.000.000.000%","0%","NaN%","+∞%","-∞%"]],
  ["ru", {},
    ["0","-0","1","-1","1 234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","не число","∞","-∞"]],
  ["ru", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1 235","-1 234 568","0","1 000 000 000 000 000 000 000","0","не число","∞","-∞"]],
  ["ru", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1 234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","не число","∞","-∞"]],
  ["ru", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1 230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","не число","∞","-∞"]],
  ["ru", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","не число","∞","-∞"]],
  ["ru", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1 234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+не число","+∞","-∞"]],
  ["ru", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123 457 %","-123 456 789 %","0 %","100 000 000 000 000 000 000 000 %","0 %","не число %","∞ %","-∞ %"]],
  ["ru", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123 456,8 %","-123 456 789,1 %","0 %","+100 000 000 000 000 000 000 000 %","0 %","не число %","+∞ %","-∞ %"]],
  ["pl", {},
    ["0","-0","1","-1","1234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["pl", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["pl", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","-∞"]],
  ["pl", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["pl", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["pl", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","-∞"]],
  ["pl", {style: "percent"},
    ["0%","-0%","100%","-100%","123 457%","-123 456 789%","0%","100 000 000 000 000 000 000 000%","0%","NaN%","∞%","-∞%"]],
  ["pl", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123 456,8%","-123 456 789,1%","0%","+100 000 000 000 000 000 000 000%","0%","NaN%","+∞%","-∞%"]],
  ["uk", {},
    ["0","-0","1","-1","1 234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["uk", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1 235","-1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["uk", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1 234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","-∞"]],
  ["uk", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1 230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["uk", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["uk", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1 234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","-∞"]],
  ["uk", {style: "percent"},
    ["0%","-0%","100%","-100%","123 457%","-123 456 789%","0%","100 000 000 000 000 000 000 000%","0%","NaN%","∞%","-∞%"]],
  ["uk", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123 456,8%","-123 456 789,1%","0%","+100 000 000 000 000 000 000 000%","0%","NaN%","+∞%","-∞%"]],
  ["cs", {},
    ["0","-0","1","-1","1 234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["cs", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1 235","-1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["cs", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1 234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","-∞"]],
  ["cs", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1 230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["cs", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["cs", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1 234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","-∞"]],
  ["cs", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123 457 %","-123 456 789 %","0 %","100 000 000 000 000 000 000 000 %","0 %","NaN %","∞ %","-∞ %"]],
  ["cs", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123 456,8 %","-123 456 789,1 %","0 %","+100 000 000 000 000 000 000 000 %","0 %","NaN %","+∞ %","-∞ %"]],
  ["ar", {},
    ["0","‎-0","1","‎-1","1,234.568","‎-1,234,567.891","0","1,000,000,000,000,000,000,000","0","ليس رقمًا","∞","‎-∞"]],
  ["ar", {maximumFractionDigits: 0},
    ["0","‎-0","1","‎-1","1,235","‎-1,234,568","0","1,000,000,000,000,000,000,000","0","ليس رقمًا","∞","‎-∞"]],
  ["ar", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","‎-0.00","1.00","‎-1.00","1,234.57","‎-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","ليس رقمًا","∞","‎-∞"]],
  ["ar", {maximumSignificantDigits: 3},
    ["0","‎-0","1","‎-1","1,230","‎-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","ليس رقمًا","∞","‎-∞"]],
  ["ar", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","‎-000","001","‎-001","1234.568","‎-1234567.891","000","1000000000000000000000","000","ليس رقمًا","∞","‎-∞"]],
  ["ar", {signDisplay: "always"},
    ["‎+0","‎-0","‎+1","‎-1","‎+1,234.568","‎-1,234,567.891","‎+0","‎+1,000,000,000,000,000,000,000","‎+0","‎+ليس رقمًا","‎+∞","‎-∞"]],
  ["ar", {style: "percent"},
    ["0‎%‎","‎-0‎%‎","100‎%‎","‎-100‎%‎","123,457‎%‎","‎-123,456,789‎%‎","0‎%‎","100,000,000,000,000,000,000,000‎%‎","0‎%‎","ليس رقمًا‎%‎","∞‎%‎","‎-∞‎%‎"]],
  ["ar", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0‎%‎","0‎%‎","‎+100‎%‎","‎-100‎%‎","‎+123,456.8‎%‎","‎-123,456,789.1‎%‎","0‎%‎","‎+100,000,000,000,000,000,000,000‎%‎","0‎%‎","ليس رقمًا‎%‎","‎+∞‎%‎","‎-∞‎%‎"]],
  ["he", {},
    ["0","‎-0","1","‎-1","1,234.568","‎-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","‎-∞"]],
  ["he", {maximumFractionDigits: 0},
    ["0","‎-0","1","‎-1","1,235","‎-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","‎-∞"]],
  ["he", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","‎-0.00","1.00","‎-1.00","1,234.57","‎-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","‎-∞"]],
  ["he", {maximumSignificantDigits: 3},
    ["0","‎-0","1","‎-1","1,230","‎-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","‎-∞"]],
  ["he", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","‎-000","001","‎-001","1234.568","‎-1234567.891","000","1000000000000000000000","000","NaN","∞","‎-∞"]],
  ["he", {signDisplay: "always"},
    ["‎+0","‎-0","‎+1","‎-1","‎+1,234.568","‎-1,234,567.891","‎+0","‎+1,000,000,000,000,000,000,000","‎+0","‎+NaN","‎+∞","‎-∞"]],
  ["he", {style: "percent"},
    ["0%","‎-0%","100%","‎-100%","123,457%","‎-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","‎-∞%"]],
  ["he", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","‎+100%","‎-100%","‎+123,456.8%","‎-123,456,789.1%","0%","‎+100,000,000,000,000,000,000,000%","0%","NaN%","‎+∞%","‎-∞%"]],
  ["ja", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["ja", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["ja", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["ja", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["ja", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["ja", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["ja", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["ja", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["zh", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["zh", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["zh", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["zh", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["zh", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["zh", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["zh", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["zh", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["ko", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["ko", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["ko", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["ko", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["ko", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["ko", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["ko", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["ko", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["hi", {},
    ["0","-0","1","-1","1,234.568","-12,34,567.891","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","∞","-∞"]],
  ["hi", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-12,34,568","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","∞","-∞"]],
  ["hi", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-12,34,567.89","0.00","1,00,00,00,00,00,00,00,00,00,000.00","0.00","NaN","∞","-∞"]],
  ["hi", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-12,30,000","0.000123","1,00,00,00,00,00,00,00,00,00,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["hi", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["hi", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-12,34,567.891","+0","+1,00,00,00,00,00,00,00,00,00,000","+0","+NaN","+∞","-∞"]],
  ["hi", {style: "percent"},
    ["0%","-0%","100%","-100%","1,23,457%","-12,34,56,789%","0%","1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","∞%","-∞%"]],
  ["hi", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+1,23,456.8%","-12,34,56,789.1%","0%","+1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","+∞%","-∞%"]],
  ["cy", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["cy", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["cy", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["cy", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["cy", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["cy", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["cy", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["cy", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["sl", {},
    ["0","−0","1","−1","1234,568","−1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","∞","−∞"]],
  ["sl", {maximumFractionDigits: 0},
    ["0","−0","1","−1","1235","−1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","∞","−∞"]],
  ["sl", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","−0,00","1,00","−1,00","1234,57","−1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","∞","−∞"]],
  ["sl", {maximumSignificantDigits: 3},
    ["0","−0","1","−1","1230","−1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","−∞"]],
  ["sl", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","−000","001","−001","1234,568","−1234567,891","000","1000000000000000000000","000","NaN","∞","−∞"]],
  ["sl", {signDisplay: "always"},
    ["+0","−0","+1","−1","+1234,568","−1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+∞","−∞"]],
  ["sl", {style: "percent"},
    ["0 %","−0 %","100 %","−100 %","123.457 %","−123.456.789 %","0 %","100.000.000.000.000.000.000.000 %","0 %","NaN %","∞ %","−∞ %"]],
  ["sl", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","−100 %","+123.456,8 %","−123.456.789,1 %","0 %","+100.000.000.000.000.000.000.000 %","0 %","NaN %","+∞ %","−∞ %"]],
  ["lt", {},
    ["0","−0","1","−1","1 234,568","−1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","−∞"]],
  ["lt", {maximumFractionDigits: 0},
    ["0","−0","1","−1","1 235","−1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","−∞"]],
  ["lt", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","−0,00","1,00","−1,00","1 234,57","−1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","−∞"]],
  ["lt", {maximumSignificantDigits: 3},
    ["0","−0","1","−1","1 230","−1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","−∞"]],
  ["lt", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","−000","001","−001","1234,568","−1234567,891","000","1000000000000000000000","000","NaN","∞","−∞"]],
  ["lt", {signDisplay: "always"},
    ["+0","−0","+1","−1","+1 234,568","−1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","−∞"]],
  ["lt", {style: "percent"},
    ["0 %","−0 %","100 %","−100 %","123 457 %","−123 456 789 %","0 %","100 000 000 000 000 000 000 000 %","0 %","NaN %","∞ %","−∞ %"]],
  ["lt", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","−100 %","+123 456,8 %","−123 456 789,1 %","0 %","+100 000 000 000 000 000 000 000 %","0 %","NaN %","+∞ %","−∞ %"]],
  ["ga", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","Nuimh","∞","-∞"]],
  ["ga", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","Nuimh","∞","-∞"]],
  ["ga", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","Nuimh","∞","-∞"]],
  ["ga", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","Nuimh","∞","-∞"]],
  ["ga", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","Nuimh","∞","-∞"]],
  ["ga", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+Nuimh","+∞","-∞"]],
  ["ga", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","Nuimh%","∞%","-∞%"]],
  ["ga", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","Nuimh%","+∞%","-∞%"]],
  ["br", {},
    ["0","-0","1","-1","1 234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["br", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1 235","-1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["br", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1 234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","-∞"]],
  ["br", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1 230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["br", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["br", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1 234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","-∞"]],
  ["br", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123 457 %","-123 456 789 %","0 %","100 000 000 000 000 000 000 000 %","0 %","NaN %","∞ %","-∞ %"]],
  ["br", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123 456,8 %","-123 456 789,1 %","0 %","+100 000 000 000 000 000 000 000 %","0 %","NaN %","+∞ %","-∞ %"]],
  ["mt", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["mt", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["mt", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["mt", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["mt", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["mt", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["mt", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["mt", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["ro", {},
    ["0","-0","1","-1","1.234,568","-1.234.567,891","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["ro", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1.235","-1.234.568","0","1.000.000.000.000.000.000.000","0","NaN","∞","-∞"]],
  ["ro", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1.234,57","-1.234.567,89","0,00","1.000.000.000.000.000.000.000,00","0,00","NaN","∞","-∞"]],
  ["ro", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1.230","-1.230.000","0,000123","1.000.000.000.000.000.000.000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["ro", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["ro", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1.234,568","-1.234.567,891","+0","+1.000.000.000.000.000.000.000","+0","+NaN","+∞","-∞"]],
  ["ro", {style: "percent"},
    ["0 %","-0 %","100 %","-100 %","123.457 %","-123.456.789 %","0 %","100.000.000.000.000.000.000.000 %","0 %","NaN %","∞ %","-∞ %"]],
  ["ro", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0 %","0 %","+100 %","-100 %","+123.456,8 %","-123.456.789,1 %","0 %","+100.000.000.000.000.000.000.000 %","0 %","NaN %","+∞ %","-∞ %"]],
  ["fa", {},
    ["۰","‎−۰","۱","‎−۱","۱٬۲۳۴٫۵۶۸","‎−۱٬۲۳۴٬۵۶۷٫۸۹۱","۰","۱٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰","۰","ناعدد","∞","‎−∞"]],
  ["fa", {maximumFractionDigits: 0},
    ["۰","‎−۰","۱","‎−۱","۱٬۲۳۵","‎−۱٬۲۳۴٬۵۶۸","۰","۱٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰","۰","ناعدد","∞","‎−∞"]],
  ["fa", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["۰٫۰۰","‎−۰٫۰۰","۱٫۰۰","‎−۱٫۰۰","۱٬۲۳۴٫۵۷","‎−۱٬۲۳۴٬۵۶۷٫۸۹","۰٫۰۰","۱٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٫۰۰","۰٫۰۰","ناعدد","∞","‎−∞"]],
  ["fa", {maximumSignificantDigits: 3},
    ["۰","‎−۰","۱","‎−۱","۱٬۲۳۰","‎−۱٬۲۳۰٬۰۰۰","۰٫۰۰۰۱۲۳","۱٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰","۰٫۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۵","ناعدد","∞","‎−∞"]],
  ["fa", {useGrouping: false, minimumIntegerDigits: 3},
    ["۰۰۰","‎−۰۰۰","۰۰۱","‎−۰۰۱","۱۲۳۴٫۵۶۸","‎−۱۲۳۴۵۶۷٫۸۹۱","۰۰۰","۱۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰۰","۰۰۰","ناعدد","∞","‎−∞"]],
  ["fa", {signDisplay: "always"},
    ["‎+۰","‎−۰","‎+۱","‎−۱","‎+۱٬۲۳۴٫۵۶۸","‎−۱٬۲۳۴٬۵۶۷٫۸۹۱","‎+۰","‎+۱٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰","‎+۰","‎+ناعدد","‎+∞","‎−∞"]],
  ["fa", {style: "percent"},
    ["۰٪","‎−۰٪","۱۰۰٪","‎−۱۰۰٪","۱۲۳٬۴۵۷٪","‎−۱۲۳٬۴۵۶٬۷۸۹٪","۰٪","۱۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٪","۰٪","ناعدد٪","∞٪","‎−∞٪"]],
  ["fa", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["۰٪","۰٪","‎+۱۰۰٪","‎−۱۰۰٪","‎+۱۲۳٬۴۵۶٫۸٪","‎−۱۲۳٬۴۵۶٬۷۸۹٫۱٪","۰٪","‎+۱۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٬۰۰۰٪","۰٪","ناعدد٪","‎+∞٪","‎−∞٪"]],
  ["gv", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["gv", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","NaN","∞","-∞"]],
  ["gv", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","NaN","∞","-∞"]],
  ["gv", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["gv", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["gv", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+NaN","+∞","-∞"]],
  ["gv", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","NaN%","∞%","-∞%"]],
  ["gv", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","NaN%","+∞%","-∞%"]],
  ["zh-Hant", {},
    ["0","-0","1","-1","1,234.568","-1,234,567.891","0","1,000,000,000,000,000,000,000","0","非數值","∞","-∞"]],
  ["zh-Hant", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-1,234,568","0","1,000,000,000,000,000,000,000","0","非數值","∞","-∞"]],
  ["zh-Hant", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-1,234,567.89","0.00","1,000,000,000,000,000,000,000.00","0.00","非數值","∞","-∞"]],
  ["zh-Hant", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-1,230,000","0.000123","1,000,000,000,000,000,000,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","非數值","∞","-∞"]],
  ["zh-Hant", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","非數值","∞","-∞"]],
  ["zh-Hant", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-1,234,567.891","+0","+1,000,000,000,000,000,000,000","+0","+非數值","+∞","-∞"]],
  ["zh-Hant", {style: "percent"},
    ["0%","-0%","100%","-100%","123,457%","-123,456,789%","0%","100,000,000,000,000,000,000,000%","0%","非數值%","∞%","-∞%"]],
  ["zh-Hant", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123,456.8%","-123,456,789.1%","0%","+100,000,000,000,000,000,000,000%","0%","非數值%","+∞%","-∞%"]],
  ["en-IN", {},
    ["0","-0","1","-1","1,234.568","-12,34,567.891","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","∞","-∞"]],
  ["en-IN", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1,235","-12,34,568","0","1,00,00,00,00,00,00,00,00,00,000","0","NaN","∞","-∞"]],
  ["en-IN", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0.00","-0.00","1.00","-1.00","1,234.57","-12,34,567.89","0.00","1,00,00,00,00,00,00,00,00,00,000.00","0.00","NaN","∞","-∞"]],
  ["en-IN", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1,230","-12,30,000","0.000123","1,00,00,00,00,00,00,00,00,00,000","0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["en-IN", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234.568","-1234567.891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["en-IN", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1,234.568","-12,34,567.891","+0","+1,00,00,00,00,00,00,00,00,00,000","+0","+NaN","+∞","-∞"]],
  ["en-IN", {style: "percent"},
    ["0%","-0%","100%","-100%","1,23,457%","-12,34,56,789%","0%","1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","∞%","-∞%"]],
  ["en-IN", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+1,23,456.8%","-12,34,56,789.1%","0%","+1,00,00,00,00,00,00,00,00,00,00,000%","0%","NaN%","+∞%","-∞%"]],
  ["pt-PT", {},
    ["0","-0","1","-1","1234,568","-1 234 567,891","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["pt-PT", {maximumFractionDigits: 0},
    ["0","-0","1","-1","1235","-1 234 568","0","1 000 000 000 000 000 000 000","0","NaN","∞","-∞"]],
  ["pt-PT", {minimumFractionDigits: 2, maximumFractionDigits: 2},
    ["0,00","-0,00","1,00","-1,00","1234,57","-1 234 567,89","0,00","1 000 000 000 000 000 000 000,00","0,00","NaN","∞","-∞"]],
  ["pt-PT", {maximumSignificantDigits: 3},
    ["0","-0","1","-1","1230","-1 230 000","0,000123","1 000 000 000 000 000 000 000","0,000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005","NaN","∞","-∞"]],
  ["pt-PT", {useGrouping: false, minimumIntegerDigits: 3},
    ["000","-000","001","-001","1234,568","-1234567,891","000","1000000000000000000000","000","NaN","∞","-∞"]],
  ["pt-PT", {signDisplay: "always"},
    ["+0","-0","+1","-1","+1234,568","-1 234 567,891","+0","+1 000 000 000 000 000 000 000","+0","+NaN","+∞","-∞"]],
  ["pt-PT", {style: "percent"},
    ["0%","-0%","100%","-100%","123 457%","-123 456 789%","0%","100 000 000 000 000 000 000 000%","0%","NaN%","∞%","-∞%"]],
  ["pt-PT", {style: "percent", signDisplay: "exceptZero", maximumFractionDigits: 1},
    ["0%","0%","+100%","-100%","+123 456,8%","-123 456 789,1%","0%","+100 000 000 000 000 000 000 000%","0%","NaN%","+∞%","-∞%"]],
];
// CLDR 46 made latn the default for ar and set minimumGroupingDigits 2 for
// it and sl; an older ICU (JavaScriptCore on Linux has ICU 74) differs.
const OLD_DATA = {
  ar: () => new NF('ar').format(0) !== '0',
  it: () => new NF('it').format(1234) !== '1234',
  sl: () => new NF('sl').format(1234) !== '1234',
};
for (const [locale, options, want] of NF_TABLE) {
  const stale = ENGINE === 'reference' && OLD_DATA[locale] !== undefined && OLD_DATA[locale]();
  unlessOldData(stale, 'the reference\'s ICU predates CLDR 46 for ' + locale, 'sec-formatnumber', locale + ' ' + JSON.stringify(options), () => {
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
  assert.sameValue(nf.format('Infinity'), '∞');
  assert.sameValue(nf.format('-Infinity'), '-∞');
  assert.sameValue(nf.format('1_000'), 'NaN');
});

test('sec-tointlmathematicalvalue', 'RoundMVResult: strings past the Number range are infinities or zeros', () => {
  const nf = new NF('en', { maximumFractionDigits: 100 });
  assert.sameValue(nf.format('1e400'), '∞');
  assert.sameValue(nf.format('-1e400'), '-∞');
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
    [['integer', '25'], ['literal', ' '], ['percentSign', '%']]);
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
