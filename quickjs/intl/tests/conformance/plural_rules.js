// Intl.PluralRules: constructor, options, resolvedOptions, select and
// selectRange, per ECMA-402's PluralRules clauses.

const PR = Intl.PluralRules;
const SPEC_ORDER = ['localeMatcher', 'type', 'notation', 'compactDisplay', 'minimumIntegerDigits',
  'minimumFractionDigits', 'maximumFractionDigits', 'minimumSignificantDigits',
  'maximumSignificantDigits', 'roundingIncrement', 'roundingMode', 'roundingPriority',
  'trailingZeroDisplay'];

test('sec-intl-pluralrules-constructor', 'name, length, constructor', () => {
  verifyFunction(PR, 'PluralRules', 0, true);
  verifyProperty(PR, 'prototype', { writable: false, enumerable: false, configurable: false });
  verifyProperty(PR.prototype, 'constructor', { value: PR, writable: true, enumerable: false, configurable: true });
});

test('sec-intl-pluralrules-constructor', 'called without new is a TypeError', () => {
  assert.throws(TypeError, () => PR());
  assert.throws(TypeError, () => PR('en'));
  assert.throws(TypeError, () => PR.call({}, 'en'));
});

test('sec-intl-pluralrules-constructor', 'subclassing keeps the subclass prototype and the slots', () => {
  class Sub extends PR {
    extra() { return this.select(1); }
  }
  const s = new Sub('en');
  assert.sameValue(Object.getPrototypeOf(s), Sub.prototype);
  assert.ok(s instanceof PR);
  assert.sameValue(s.extra(), 'one');
  assert.sameValue(s.resolvedOptions().locale, 'en');
});

test('sec-intl-pluralrules-constructor', 'a NewTarget whose prototype is not an object falls back', () => {
  const nt = function () {}.bind();
  Object.defineProperty(nt, 'prototype', { value: null });
  const pr = Reflect.construct(PR, ['en'], nt);
  assert.sameValue(Object.getPrototypeOf(pr), PR.prototype);
  assert.sameValue(pr.select(2), 'other');
});

test('sec-intl-pluralrules-constructor', 'a NewTarget prototype is read once, after the options', () => {
  const log = [];
  const nt = new Proxy(function () {}, {
    get(target, key) { log.push(String(key)); return key === 'prototype' ? PR.prototype : target[key]; },
  });
  Reflect.construct(PR, ['en', loggingOptions(log, { type: 'ordinal' })], nt);
  assert.sameValue(log.filter((k) => k === 'prototype').length, 1);
});

only('rong', 'the reference has no PluralRules notation yet', 'sec-intl-pluralrules-constructor',
  'options are read once each, in the spec order', () => {
    const log = [];
    new PR('en', readLog(log));
    assert.deepEqual(log, SPEC_ORDER);
  });

test('sec-intl-pluralrules-constructor', 'options are read in the spec order (shared subset)', () => {
  const log = [];
  new PR('en', readLog(log));
  const shared = log.filter((k) => k !== 'notation' && k !== 'compactDisplay');
  assert.deepEqual(shared, SPEC_ORDER.filter((k) => k !== 'notation' && k !== 'compactDisplay'));
});

test('sec-intl-pluralrules-constructor', 'locales are read before options', () => {
  const log = [];
  const locales = { length: 1, get 0() { log.push('locale'); return 'en'; } };
  new PR(locales, loggingOptions(log, { type: 'cardinal' }));
  assert.deepEqual(log, ['locale', 'type']);
});

test('sec-coerceoptionstoobject', 'options: undefined, primitives and null', () => {
  assert.sameValue(new PR('en', undefined).resolvedOptions().type, 'cardinal');
  assert.sameValue(new PR('en', 'string').resolvedOptions().type, 'cardinal');
  assert.sameValue(new PR('en', 5).resolvedOptions().type, 'cardinal');
  assert.throws(TypeError, () => new PR('en', null));
});

for (const [type, ok] of [['cardinal', true], ['ordinal', true], ['Cardinal', false], ['', false], ['plural', false]]) {
  test('sec-getoption', 'type ' + JSON.stringify(type), () => {
    if (ok) assert.sameValue(new PR('en', { type }).resolvedOptions().type, type);
    else assert.throws(RangeError, () => new PR('en', { type }));
  });
}

test('sec-getoption', 'string options go through ToString', () => {
  const log = [];
  assert.sameValue(new PR('en', { type: stringish(log, 'type', 'ordinal') }).resolvedOptions().type, 'ordinal');
  assert.deepEqual(log, ['type']);
  assert.throws(TypeError, () => new PR('en', { type: Symbol('ordinal') }));
  assert.throws(RangeError, () => new PR('en', { type: null }));
  assert.throws(RangeError, () => new PR('en', { type: true }));
});

test('sec-getoption', 'localeMatcher is validated', () => {
  new PR('en', { localeMatcher: 'lookup' });
  new PR('en', { localeMatcher: 'best fit' });
  assert.throws(RangeError, () => new PR('en', { localeMatcher: 'best-fit' }));
});

test('sec-getoption', 'a throwing getter propagates and stops the reads', () => {
  class Marker extends Error {}
  const log = [];
  const options = {
    get localeMatcher() { log.push('localeMatcher'); return undefined; },
    get type() { log.push('type'); throw new Marker(); },
    get minimumIntegerDigits() { log.push('minimumIntegerDigits'); return 1; },
  };
  assert.throws(Marker, () => new PR('en', options));
  assert.deepEqual(log, ['localeMatcher', 'type']);
});

const DIGIT_RANGES = [
  ['minimumIntegerDigits', 1, 21],
  ['minimumFractionDigits', 0, 100],
  ['maximumFractionDigits', 0, 100],
  ['minimumSignificantDigits', 1, 21],
  ['maximumSignificantDigits', 1, 21],
];
for (const [key, min, max] of DIGIT_RANGES) {
  test('sec-setnfdigitoptions', key + ' bounds ' + min + '..' + max, () => {
    assert.sameValue(new PR('en', { [key]: min }).resolvedOptions()[key], min);
    assert.sameValue(new PR('en', { [key]: max }).resolvedOptions()[key], max);
    assert.sameValue(new PR('en', { [key]: String(max) }).resolvedOptions()[key], max, 'a numeric string');
    assert.sameValue(new PR('en', { [key]: min + 0.9 }).resolvedOptions()[key], min, 'floored');
    assert.throws(RangeError, () => new PR('en', { [key]: max + 0.5 }), 'range checked before flooring');
    assert.throws(RangeError, () => new PR('en', { [key]: min - 1 }), 'below');
    assert.throws(RangeError, () => new PR('en', { [key]: max + 1 }), 'above');
    assert.throws(RangeError, () => new PR('en', { [key]: NaN }), 'NaN');
    assert.throws(RangeError, () => new PR('en', { [key]: Infinity }), 'Infinity');
    assert.throws(TypeError, () => new PR('en', { [key]: Symbol() }), 'Symbol');
    assert.throws(TypeError, () => new PR('en', { [key]: 1n }), 'BigInt');
  });
}

test('sec-setnfdigitoptions', 'maximumFractionDigits below minimumFractionDigits is a RangeError', () => {
  assert.throws(RangeError, () => new PR('en', { minimumFractionDigits: 3, maximumFractionDigits: 2 }));
  const o = new PR('en', { minimumFractionDigits: 5 }).resolvedOptions();
  assert.sameValue(o.maximumFractionDigits, 5, 'the maximum follows a larger minimum');
  const p = new PR('en', { maximumFractionDigits: 0 }).resolvedOptions();
  assert.sameValue(p.minimumFractionDigits, 0);
});

test('sec-setnfdigitoptions', 'maximumSignificantDigits below minimumSignificantDigits is a RangeError', () => {
  assert.throws(RangeError, () => new PR('en', { minimumSignificantDigits: 3, maximumSignificantDigits: 2 }));
  const o = new PR('en', { minimumSignificantDigits: 3 }).resolvedOptions();
  assert.sameValue(o.maximumSignificantDigits, 21);
  assert.sameValue(o.minimumFractionDigits, undefined);
});

const INCREMENTS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 2500, 5000];
for (const increment of [...INCREMENTS, 3, 0, 5001, 15]) {
  test('sec-setnfdigitoptions', 'roundingIncrement ' + increment, () => {
    const options = { roundingIncrement: increment, maximumFractionDigits: 2, minimumFractionDigits: 2 };
    if (INCREMENTS.includes(increment)) {
      assert.sameValue(new PR('en', options).resolvedOptions().roundingIncrement, increment);
    } else {
      assert.throws(RangeError, () => new PR('en', options));
    }
  });
}

test('sec-setnfdigitoptions', 'roundingIncrement needs equal fraction digits and fraction rounding', () => {
  assert.throws(RangeError, () => new PR('en', { roundingIncrement: 5, maximumFractionDigits: 2 }));
  assert.throws(TypeError, () => new PR('en', { roundingIncrement: 5, minimumSignificantDigits: 1 }));
  assert.throws(TypeError, () => new PR('en', { roundingIncrement: 5, roundingPriority: 'morePrecision', minimumFractionDigits: 2, maximumFractionDigits: 2 }));
});

const MODES = ['ceil', 'floor', 'expand', 'trunc', 'halfCeil', 'halfFloor', 'halfExpand', 'halfTrunc', 'halfEven'];
for (const mode of [...MODES, 'HALFEVEN', 'up']) {
  test('sec-setnfdigitoptions', 'roundingMode ' + mode, () => {
    if (MODES.includes(mode)) assert.sameValue(new PR('en', { roundingMode: mode }).resolvedOptions().roundingMode, mode);
    else assert.throws(RangeError, () => new PR('en', { roundingMode: mode }));
  });
}

for (const priority of ['auto', 'morePrecision', 'lessPrecision', 'more']) {
  test('sec-setnfdigitoptions', 'roundingPriority ' + priority, () => {
    if (priority !== 'more') assert.sameValue(new PR('en', { roundingPriority: priority }).resolvedOptions().roundingPriority, priority);
    else assert.throws(RangeError, () => new PR('en', { roundingPriority: priority }));
  });
}

for (const display of ['auto', 'stripIfInteger', 'strip']) {
  test('sec-setnfdigitoptions', 'trailingZeroDisplay ' + display, () => {
    if (display !== 'strip') assert.sameValue(new PR('en', { trailingZeroDisplay: display }).resolvedOptions().trailingZeroDisplay, display);
    else assert.throws(RangeError, () => new PR('en', { trailingZeroDisplay: display }));
  });
}

only('rong', 'the reference has no PluralRules notation yet', 'sec-intl-pluralrules-constructor',
  'notation and compactDisplay', () => {
    for (const notation of ['standard', 'scientific', 'engineering', 'compact']) {
      assert.sameValue(new PR('en', { notation }).resolvedOptions().notation, notation);
    }
    assert.throws(RangeError, () => new PR('en', { notation: 'Compact' }));
    assert.sameValue(new PR('en', { notation: 'compact', compactDisplay: 'long' }).resolvedOptions().compactDisplay, 'long');
    assert.sameValue(new PR('en', { compactDisplay: 'long' }).resolvedOptions().compactDisplay, undefined);
    assert.throws(RangeError, () => new PR('en', { compactDisplay: 'medium' }));
  });

test('sec-intl.pluralrules.prototype', 'prototype properties', () => {
  verifyToStringTag(PR.prototype, 'Intl.PluralRules');
  assert.sameValue(Object.prototype.toString.call(new PR('en')), '[object Intl.PluralRules]');
  verifyMethod(PR.prototype, 'select', 1);
  verifyMethod(PR.prototype, 'selectRange', 2);
  verifyMethod(PR.prototype, 'resolvedOptions', 0);
  verifyMethod(PR, 'supportedLocalesOf', 1);
});

for (const method of ['select', 'selectRange', 'resolvedOptions']) {
  test('sec-intl.pluralrules.prototype', method + ' on a wrong receiver is a TypeError', () => {
    const f = PR.prototype[method];
    for (const receiver of [undefined, null, 1, 'en', {}, PR.prototype, new Intl.NumberFormat('en'), Object.create(PR.prototype)]) {
      assert.throws(TypeError, () => f.call(receiver, 1, 2), show(receiver));
    }
  });
}

only('rong', 'the reference lacks notation in resolvedOptions', 'sec-intl.pluralrules.prototype.resolvedoptions',
  'keys in table order', () => {
    assert.deepEqual(Object.keys(new PR('en').resolvedOptions()), ['locale', 'type', 'notation',
      'minimumIntegerDigits', 'minimumFractionDigits', 'maximumFractionDigits', 'pluralCategories',
      'roundingIncrement', 'roundingMode', 'roundingPriority', 'trailingZeroDisplay']);
    assert.deepEqual(Object.keys(new PR('en', { minimumSignificantDigits: 2, notation: 'compact' }).resolvedOptions()),
      ['locale', 'type', 'notation', 'compactDisplay', 'minimumIntegerDigits', 'minimumSignificantDigits',
        'maximumSignificantDigits', 'pluralCategories', 'roundingIncrement', 'roundingMode',
        'roundingPriority', 'trailingZeroDisplay']);
  });

test('sec-intl.pluralrules.prototype.resolvedoptions', 'values, attributes and a fresh object each call', () => {
  const pr = new PR('fr', { type: 'ordinal', minimumFractionDigits: 1, roundingMode: 'floor' });
  const a = pr.resolvedOptions();
  const b = pr.resolvedOptions();
  assert.notSameValue(a, b);
  assert.notSameValue(a.pluralCategories, b.pluralCategories);
  assert.sameValue(Object.getPrototypeOf(a), Object.prototype);
  verifyProperty(a, 'locale', { value: 'fr', writable: true, enumerable: true, configurable: true });
  assert.sameValue(a.type, 'ordinal');
  assert.sameValue(a.minimumIntegerDigits, 1);
  assert.sameValue(a.minimumFractionDigits, 1);
  assert.sameValue(a.maximumFractionDigits, 3);
  assert.sameValue(a.roundingMode, 'floor');
  assert.sameValue(a.roundingPriority, 'auto');
  assert.deepEqual(a.pluralCategories, ['one', 'other']);
  a.pluralCategories.push('zero');
  assert.deepEqual(pr.resolvedOptions().pluralCategories, ['one', 'other']);
});

test('sec-intl.pluralrules.prototype.resolvedoptions', 'roundingPriority with both kinds of digits', () => {
  const o = new PR('en', { minimumSignificantDigits: 2, maximumFractionDigits: 2, roundingPriority: 'lessPrecision' }).resolvedOptions();
  assert.sameValue(o.roundingPriority, 'lessPrecision');
  assert.sameValue(o.minimumSignificantDigits, 2);
  assert.sameValue(o.maximumFractionDigits, 2);
});

// Categories and selections per locale of the test blob, cardinal and
// ordinal, for: 0 1 2 3 4 5 6 7 11 12 19 21 22 23 100 101 102 1e6 1.5 0.5 -1 -2.
const VALUES = [0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 19, 21, 22, 23, 100, 101, 102, 1000000, 1.5, 0.5, -1, -2];
const TABLE = [
  ["en", "cardinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["en", "ordinal", "one two few other",
    "other one two few other other other other other other other one two few other one two other other other one two"],
  ["en-US", "cardinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["en-US", "ordinal", "one two few other",
    "other one two few other other other other other other other one two few other one two other other other one two"],
  ["en-GB", "cardinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["en-GB", "ordinal", "one two few other",
    "other one two few other other other other other other other one two few other one two other other other one two"],
  ["fr", "cardinal", "one many other",
    "one one other other other other other other other other other other other other other other other many one one one other"],
  ["fr", "ordinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["de", "cardinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["de", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["es", "cardinal", "one many other",
    "other one other other other other other other other other other other other other other other other many other other one other"],
  ["es", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["it", "cardinal", "one many other",
    "other one other other other other other other other other other other other other other other other many other other one other"],
  ["it", "ordinal", "many other",
    "other other other other other other other other many other other other other other other other other other other other other other"],
  ["pt", "cardinal", "one many other",
    "one one other other other other other other other other other other other other other other other many one one one other"],
  ["pt", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ru", "cardinal", "one few many other",
    "many one few few few many many many many many many one few few many one few many other other one few"],
  ["ru", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["pl", "cardinal", "one few many other",
    "many one few few few many many many many many many many few few many many few many other other one few"],
  ["pl", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["uk", "cardinal", "one few many other",
    "many one few few few many many many many many many one few few many one few many other other one few"],
  ["uk", "ordinal", "few other",
    "other other other few other other other other other other other other other few other other other other other other other other"],
  ["cs", "cardinal", "one few many other",
    "other one few few few other other other other other other other other other other other other other many many one few"],
  ["cs", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ar", "cardinal", "zero one two few many other",
    "zero one two few few few few few many many many many many many other other other other other other one two"],
  ["ar", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["he", "cardinal", "one two other",
    "other one two other other other other other other other other other other other other other other other other one one two"],
  ["he", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ja", "cardinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ja", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["zh", "cardinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["zh", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ko", "cardinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ko", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["hi", "cardinal", "one other",
    "one one other other other other other other other other other other other other other other other other other one one other"],
  ["hi", "ordinal", "one two few many other",
    "other one two two few other many other other other other other other other other other other other other other one two"],
  ["cy", "cardinal", "zero one two few many other",
    "zero one two few other other many other other other other other other other other other other other other other one two"],
  ["cy", "ordinal", "zero one two few many other",
    "zero one two few few many many zero other other other other other other other other other other other other one two"],
  ["sl", "cardinal", "one two few other",
    "other one two few few other other other other other other other other other other one two other few few one two"],
  ["sl", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["lt", "cardinal", "one few many other",
    "other one few few few few few few other other other one few few other one few other many many one few"],
  ["lt", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ga", "cardinal", "one two few many other",
    "other one two few few few few many other other other other other other other other other other other other one two"],
  ["ga", "ordinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["br", "cardinal", "one two few many other",
    "other one two few few other other other other other other one two few other one two many other other one two"],
  ["br", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["mt", "cardinal", "one two few many other",
    "few one two few few few few few many many many other other other other other other other other other one two"],
  ["mt", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["ro", "cardinal", "one few other",
    "few one few few few few few few few few few other other other other few few other few few one few"],
  ["ro", "ordinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["fa", "cardinal", "one other",
    "one one other other other other other other other other other other other other other other other other other one one other"],
  ["fa", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["gv", "cardinal", "one two few many other",
    "few one two other other other other other one two other one two other few one two few many many one two"],
  ["gv", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["zh-Hant", "cardinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["zh-Hant", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
  ["en-IN", "cardinal", "one other",
    "other one other other other other other other other other other other other other other other other other other other one other"],
  ["en-IN", "ordinal", "one two few other",
    "other one two few other other other other other other other one two few other one two other other other one two"],
  ["pt-PT", "cardinal", "one many other",
    "other one other other other other other other other other other other other other other other other many other other one other"],
  ["pt-PT", "ordinal", "other",
    "other other other other other other other other other other other other other other other other other other other other other other"],
];
for (const [locale, type, categories, selections] of TABLE) {
  test('sec-intl.pluralrules.prototype.select', locale + ' ' + type, () => {
    const pr = new PR(locale, { type });
    assert.sameValue(pr.resolvedOptions().pluralCategories.join(' '), categories, 'categories');
    assert.sameValue(VALUES.map((v) => pr.select(v)).join(' '), selections, 'selections');
  });
}

test('sec-intl.pluralrules.prototype.select', 'digit options change the category', () => {
  assert.sameValue(new PR('en').select(1), 'one');
  assert.sameValue(new PR('en', { minimumFractionDigits: 1 }).select(1), 'other', '1.0 has visible fraction digits');
  assert.sameValue(new PR('en', { maximumFractionDigits: 0 }).select(1.4), 'one', 'rounded to 1');
  assert.sameValue(new PR('en', { maximumSignificantDigits: 1 }).select(1.4), 'one');
  assert.sameValue(new PR('en', { roundingMode: 'ceil', maximumFractionDigits: 0 }).select(0.2), 'one');
  assert.sameValue(new PR('en', { roundingMode: 'floor', maximumFractionDigits: 0 }).select(1.9), 'one');
  assert.sameValue(new PR('fr').select(1.5), 'one');
  assert.sameValue(new PR('fr', { maximumFractionDigits: 0 }).select(1.5), 'other', 'halfExpand rounds 1.5 to 2');
});

test('sec-intl.pluralrules.prototype.select', 'special values', () => {
  const pr = new PR('en');
  assert.sameValue(pr.select(NaN), 'other');
  assert.sameValue(pr.select(Infinity), 'other');
  assert.sameValue(pr.select(-Infinity), 'other');
  assert.sameValue(pr.select(-0), 'other');
  assert.sameValue(pr.select(Number.MAX_VALUE), 'other');
  assert.sameValue(pr.select(Number.MIN_VALUE), 'other');
  assert.sameValue(pr.select(), 'other', 'undefined is NaN');
  assert.sameValue(pr.select(null), 'other', 'null is 0');
  assert.sameValue(pr.select(true), 'one');
  assert.sameValue(pr.select('1'), 'one');
  assert.sameValue(pr.select(' 1 '), 'one');
  assert.sameValue(pr.select({ valueOf: () => 1 }), 'one');
  assert.throws(TypeError, () => pr.select(Symbol()));
});

only('rong', 'the reference converts with ToNumber, not ToIntlMathematicalValue', 'sec-intl.pluralrules.prototype.select',
  'BigInt and exact decimal strings', () => {
    const pr = new PR('en');
    assert.sameValue(pr.select(1n), 'one');
    assert.sameValue(pr.select(10n ** 30n), 'other');
    assert.sameValue(new PR('en', { maximumFractionDigits: 20 }).select('1.00000000000000000001'), 'other');
    assert.sameValue(new PR('en', { maximumFractionDigits: 20, minimumFractionDigits: 0 }).select('1.000000000000000000000001'), 'one');
  });

test('sec-intl.pluralrules.prototype.selectrange', 'arguments and errors', () => {
  const pr = new PR('en');
  assert.throws(TypeError, () => pr.selectRange());
  assert.throws(TypeError, () => pr.selectRange(1));
  assert.throws(TypeError, () => pr.selectRange(undefined, 1));
  assert.throws(RangeError, () => pr.selectRange(NaN, 1));
  assert.throws(RangeError, () => pr.selectRange(1, NaN));
  assert.throws(TypeError, () => pr.selectRange(Symbol(), 1));
});

test('sec-intl.pluralrules.prototype.selectrange', 'conversion order: start, then end', () => {
  const log = [];
  new PR('en').selectRange({ valueOf() { log.push('start'); return 1; } }, { valueOf() { log.push('end'); return 2; } });
  assert.deepEqual(log, ['start', 'end']);
});

const RANGES = [
  ['en', 1, 2, 'other'], ['en', 0, 1, 'other'], ['en', 2, 1, 'other'],
  ['fr', 0, 1, 'one'], ['fr', 1, 2, 'other'], ['ru', 1, 2, 'few'], ['ru', 1, 5, 'many'],
  ['ru', 2, 21, 'one'], ['pl', 1, 2, 'few'], ['pl', 2, 5, 'many'], ['ar', 0, 1, 'zero'],
  ['ar', 1, 2, 'other'], ['ar', 3, 11, 'many'], ['he', 1, 2, 'other'], ['lt', 1, 2, 'few'],
];
for (const [locale, x, y, want] of RANGES) {
  test('sec-intl.pluralrules.prototype.selectrange', locale + ' ' + x + '-' + y, () => {
    assert.sameValue(new PR(locale).selectRange(x, y), want);
  });
}

test('sec-intl.pluralrules.supportedlocalesof', 'each localeMatcher', () => {
  for (const localeMatcher of ['lookup', 'best fit', undefined]) {
    assert.deepEqual(PR.supportedLocalesOf(['en-US', 'fr', 'de-CH'], { localeMatcher }), ['en-US', 'fr', 'de-CH']);
  }
  assert.throws(RangeError, () => PR.supportedLocalesOf('en', { localeMatcher: 'any' }));
  assert.throws(TypeError, () => PR.supportedLocalesOf('en', null));
  assert.deepEqual(PR.supportedLocalesOf([]), []);
  assert.throws(RangeError, () => PR.supportedLocalesOf('en_US'));
});
