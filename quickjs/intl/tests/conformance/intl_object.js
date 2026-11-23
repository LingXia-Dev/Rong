// The Intl object, Intl.getCanonicalLocales and the locale list
// operations every constructor shares.

test('sec-intl-object', 'Intl is an ordinary non-callable object on the global', () => {
  verifyProperty(globalThis, 'Intl', { writable: true, enumerable: false, configurable: true });
  assert.sameValue(typeof Intl, 'object');
  assert.sameValue(Object.getPrototypeOf(Intl), Object.prototype);
  assert.throws(TypeError, () => Intl());
  assert.throws(TypeError, () => new Intl());
  verifyToStringTag(Intl, 'Intl');
  assert.sameValue(Object.prototype.toString.call(Intl), '[object Intl]');
});

test('sec-constructor-properties-of-the-intl-object', 'constructors are writable, non-enumerable, configurable', () => {
  for (const name of ['NumberFormat', 'PluralRules']) {
    verifyProperty(Intl, name, { writable: true, enumerable: false, configurable: true });
    assert.sameValue(typeof Intl[name], 'function', name);
  }
});

test('sec-intl.getcanonicallocales', 'property, name and length', () => {
  verifyMethod(Intl, 'getCanonicalLocales', 1);
});

test('sec-intl.getcanonicallocales', 'returns a new array each call', () => {
  const a = Intl.getCanonicalLocales('en');
  const b = Intl.getCanonicalLocales('en');
  assert.ok(Array.isArray(a));
  assert.notSameValue(a, b);
  assert.deepEqual(Intl.getCanonicalLocales(), []);
  assert.deepEqual(Intl.getCanonicalLocales(undefined), []);
  assert.deepEqual(Intl.getCanonicalLocales([]), []);
});

const CANONICAL = [
  ['EN-us', 'en-US'],
  ['en-latn-us', 'en-Latn-US'],
  ['zh-hant-tw', 'zh-Hant-TW'],
  ['iw', 'he'],
  ['in', 'id'],
  ['ji', 'yi'],
  ['sgn-GR', 'gss'],
  ['en-u-ca-gregory', 'en-u-ca-gregory'],
  ['en-U-CA-GREGORY', 'en-u-ca-gregory'],
  ['en-u-kn-true', 'en-u-kn'],
  ['en-u-kn-yes', 'en-u-kn'],
  ['de-u-co-phonebk', 'de-u-co-phonebk'],
  ['en-u-cu-usd-ca-gregory', 'en-u-ca-gregory-cu-usd'],
  ['en-a-bbb-u-ccc', 'en-a-bbb-u-ccc'],
  ['en-x-private', 'en-x-private'],
  ['en-US-u-va-posix', 'en-US-u-va-posix'],
  ['sl-rozaj-biske-1994', 'sl-1994-biske-rozaj'],
  ['de-DD', 'de-DE'],
  ['en-t-ja', 'en-t-ja'],
  ['und', 'und'],
  ['und-latn', 'und-Latn'],
];
only('rong', 'JavaScriptCore does not apply CLDR\'s cmn -> zh macrolanguage alias (in CLDR 44 to 49), which CanonicalizeUnicodeLocaleId requires', 'sec-canonicalizelocalelist', 'canonical form of cmn', () => {
  assert.deepEqual(Intl.getCanonicalLocales('cmn'), ['zh']);
});

for (const [input, want] of CANONICAL) {
  test('sec-canonicalizelocalelist', 'canonical form of ' + input, () => {
    assert.deepEqual(Intl.getCanonicalLocales(input), [want]);
  });
}

test('sec-canonicalizelocalelist', 'duplicates after canonicalization are dropped, first kept', () => {
  assert.deepEqual(Intl.getCanonicalLocales(['en-us', 'EN-US', 'fr', 'en-US']), ['en-US', 'fr']);
  assert.deepEqual(Intl.getCanonicalLocales(['iw', 'he']), ['he']);
});

const INVALID_TAGS = ['', ' en', 'en ', 'en_US', 'e', 'en-', '-en', 'en--US', 'abcdefghi',
  'en-u', 'en-u-', 'en-x', 'en-a', 'en-US-US', 'en-latn-latn', 'de-1996-1996',
  'en-u-ca-gregory-ca-buddhist-x', 'en-t-', '123', 'i-klingon', 'en-\u0130', 'en\u0000',
  'en-a-aaa-a-bbb', 'zh-cmn-Hans-CN', 'x-private', 'x', 'root', 'en-u-ca-gregorian-123456789'];
for (const tag of INVALID_TAGS) {
  test('sec-canonicalizelocalelist', 'RangeError for ' + JSON.stringify(tag), () => {
    assert.throws(RangeError, () => Intl.getCanonicalLocales(tag));
  });
}

for (const value of [null, 1, true, Symbol('x'), 1n, undefined]) {
  test('sec-canonicalizelocalelist', 'TypeError for the element ' + show(value), () => {
    assert.throws(TypeError, () => Intl.getCanonicalLocales([value]));
  });
}
test('sec-canonicalizelocalelist', 'null locales is a TypeError', () => {
  assert.throws(TypeError, () => Intl.getCanonicalLocales(null));
});

test('sec-canonicalizelocalelist', 'a string is one tag; other primitives are ToObject-ed', () => {
  assert.deepEqual(Intl.getCanonicalLocales('fr'), ['fr']);
  assert.deepEqual(Intl.getCanonicalLocales(1), []);
  assert.deepEqual(Intl.getCanonicalLocales(true), []);
  assert.deepEqual(Intl.getCanonicalLocales(Symbol('s')), []);
});

test('sec-canonicalizelocalelist', 'an array-like is read by length and index, holes skipped', () => {
  const log = [];
  const list = new Proxy({ length: 3, 0: 'en', 2: 'fr' }, {
    get(target, key) { log.push(String(key)); return target[key]; },
    has(target, key) { log.push('has ' + String(key)); return key in target; },
  });
  assert.deepEqual(Intl.getCanonicalLocales(list), ['en', 'fr']);
  assert.deepEqual(log, ['length', 'has 0', '0', 'has 1', 'has 2', '2']);
});

test('sec-canonicalizelocalelist', 'length goes through ToLength', () => {
  assert.deepEqual(Intl.getCanonicalLocales({ length: -1, 0: 'en' }), []);
  assert.deepEqual(Intl.getCanonicalLocales({ length: 1.9, 0: 'en' }), ['en']);
  assert.deepEqual(Intl.getCanonicalLocales({ length: '1', 0: 'en' }), ['en']);
  assert.deepEqual(Intl.getCanonicalLocales({ length: NaN, 0: 'en' }), []);
  let read = 0;
  assert.deepEqual(Intl.getCanonicalLocales({ get length() { read++; return { valueOf: () => 1 }; }, 0: 'de' }), ['de']);
  assert.sameValue(read, 1);
});

test('sec-canonicalizelocalelist', 'an object element is converted with ToString', () => {
  const log = [];
  assert.deepEqual(Intl.getCanonicalLocales([stringish(log, 'tag', 'EN')]), ['en']);
  assert.deepEqual(log, ['tag']);
  assert.deepEqual(Intl.getCanonicalLocales([new String('ja-jp')]), ['ja-JP']);
});

test('sec-canonicalizelocalelist', 'an element whose conversion throws propagates', () => {
  class Marker extends Error {}
  assert.throws(Marker, () => Intl.getCanonicalLocales([{ toString() { throw new Marker(); } }]));
  assert.throws(Marker, () => Intl.getCanonicalLocales({ get length() { throw new Marker(); } }));
  assert.throws(Marker, () => Intl.getCanonicalLocales({ length: 1, get 0() { throw new Marker(); } }));
});

test('sec-canonicalizelocalelist', 'stops at the first invalid tag', () => {
  const log = [];
  assert.throws(RangeError, () => Intl.getCanonicalLocales([
    stringish(log, 'a', 'en'), stringish(log, 'b', 'not a tag'), stringish(log, 'c', 'fr'),
  ]));
  assert.deepEqual(log, ['a', 'b']);
});
