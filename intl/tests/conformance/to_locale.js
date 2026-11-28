// The locale-sensitive methods of other built-ins, per ECMA-402 chapter
// 19 (String, Number, BigInt, Date, Array and TypedArray): each makes the
// service with the given locales and options, and keeps its own this check.

const AT = Date.UTC(2026, 0, 5, 13, 4, 5, 678);
const UTC = { timeZone: 'UTC' };

const METHODS = [
  [String.prototype, 'toLocaleUpperCase', 0],
  [String.prototype, 'toLocaleLowerCase', 0],
  [Number.prototype, 'toLocaleString', 0],
  [BigInt.prototype, 'toLocaleString', 0],
  [Date.prototype, 'toLocaleString', 0],
  [Date.prototype, 'toLocaleDateString', 0],
  [Date.prototype, 'toLocaleTimeString', 0],
  [Array.prototype, 'toLocaleString', 0],
  [Object.getPrototypeOf(Int8Array.prototype), 'toLocaleString', 0],
];
for (const [object, key, length] of METHODS) {
  test('sup-properties-of-other-objects', key + ' of ' + (object.constructor.name || 'TypedArray') + ': attributes', () => {
    verifyMethod(object, key, length);
  });
}

const VALUES = [
  ['sup-date.prototype.tolocalestring', () => new Date(AT).toLocaleString('en', UTC), '1/5/2026, 1:04:05 PM'],
  ['sup-date.prototype.tolocaledatestring', () => new Date(AT).toLocaleDateString('en', UTC), '1/5/2026'],
  ['sup-date.prototype.tolocaletimestring', () => new Date(AT).toLocaleTimeString('en', UTC), '1:04:05 PM'],
  ['sup-date.prototype.tolocalestring', () => new Date(AT).toLocaleString('de', UTC), '5.1.2026, 13:04:05',
    [[{ source: 'icu4x' }, '05.01.2026, 13:04:05', 'ICU4X YMD short pattern for de zero-pads day and month']]],
  ['sup-date.prototype.tolocaledatestring', () => new Date(AT).toLocaleDateString('ja', UTC), '2026/1/5',
    [[{ source: 'icu4x' }, '2026/01/05', 'ICU4X YMD short pattern for ja zero-pads month and day']]],
  ['sup-date.prototype.tolocaletimestring', () => new Date(AT).toLocaleTimeString('ko', UTC), '\uc624\ud6c4 1:04:05'],
  ['sup-date.prototype.tolocaledatestring', () => new Date(AT).toLocaleDateString('en', { timeZone: 'UTC', hour: 'numeric' }), '1/5/2026, 1 PM'],
  ['sup-date.prototype.tolocaletimestring', () => new Date(AT).toLocaleTimeString('en', { timeZone: 'UTC', year: 'numeric' }), '2026, 1:04:05 PM',
    [[{ source: 'icu4x' }, 'Jan 5, 2026, 1:04:05 PM', 'ICU4X promotes year+time without month/day to YMD+time']]],
  ['sup-number.prototype.tolocalestring', () => (1234.5).toLocaleString('de'), '1.234,5'],
  ['sup-number.prototype.tolocalestring', () => (-0).toLocaleString('en'), '-0'],
  ['sup-number.prototype.tolocalestring', () => (0.256).toLocaleString('en', { style: 'percent' }), '26%'],
  ['sup-bigint.prototype.tolocalestring', () => (12345678901234567890n).toLocaleString('en'), '12,345,678,901,234,567,890'],
  ['sup-bigint.prototype.tolocalestring', () => (-(10n ** 30n)).toLocaleString('en', { useGrouping: false }), '-1' + '0'.repeat(30)],
  ['sup-array.prototype.tolocalestring', () => [1234, new Date(AT), null, undefined, 5n].toLocaleString('de', UTC), '1.234,5.1.2026, 13:04:05,,,5',
    [[{ source: 'icu4x' }, '1.234,05.01.2026, 13:04:05,,,5', 'ICU4X YMD short pattern for de zero-pads day and month']]],
  ['sup-array.prototype.tolocalestring', () => new Float64Array([1234.5, -0]).toLocaleString('fr'), '1\u202f234,5,-0'],
  ['sup-string.prototype.tolocalelowercase', () => 'I'.toLocaleLowerCase('tr'), '\u0131'],
  ['sup-string.prototype.tolocaleuppercase', () => 'i'.toLocaleUpperCase('tr'), '\u0130'],
  ['sup-string.prototype.tolocaleuppercase', () => 'i'.toLocaleUpperCase('az'), '\u0130'],
  ['sup-string.prototype.tolocaleuppercase', () => '\u00df'.toLocaleUpperCase('de'), 'SS'],
  ['sup-string.prototype.tolocalelowercase', () => '\u03a3\u0391\u03a3'.toLocaleLowerCase('el'), '\u03c3\u03b1\u03c2'],
  ['sup-string.prototype.tolocaleuppercase', () => 'i'.toLocaleUpperCase(['en', 'tr']), 'I'],
  ['sup-string.prototype.tolocalelowercase', () => 'I'.toLocaleLowerCase('tr-u-co-search'), '\u0131'],
];
VALUES.forEach(([section, run, want, variants], i) => {
  dataTest(section, 'value ' + i + ': ' + String(run).replace(/^\(\) => /, ''), want, variants, (expected) => {
    // V8 and JavaScriptCore write a space where CLDR has U+202F before AM/PM.
    assert.sameValue(section.startsWith('sup-date') ? run().replace(/\u202f/g, ' ') : run(), expected);
  });
});

test('sup-date.prototype.tolocalestring', 'an invalid date is "Invalid Date", options still read', () => {
  const log = [];
  assert.sameValue(new Date(NaN).toLocaleString('en', loggingOptions(log, { timeZone: 'UTC' })), 'Invalid Date');
  assert.sameValue(new Date(NaN).toLocaleDateString(), 'Invalid Date');
  assert.sameValue(new Date(NaN).toLocaleTimeString(), 'Invalid Date');
  assert.throws(RangeError, () => new Date(AT).toLocaleString('en', { timeZone: 'Mars/Phobos' }));
});

test('sup-date.prototype.tolocalestring', 'the required and default components', () => {
  assert.throws(TypeError, () => new Date(AT).toLocaleDateString('en', { timeStyle: 'short' }));
  assert.throws(TypeError, () => new Date(AT).toLocaleTimeString('en', { dateStyle: 'short' }));
  assert.sameValue(new Date(AT).toLocaleString('en', { timeZone: 'UTC', dateStyle: 'short' }), '1/5/26');
});

test('sup-properties-of-other-objects', 'this checks', () => {
  for (const bad of ['1', {}, null, undefined, 1n]) {
    assert.throws(TypeError, () => Number.prototype.toLocaleString.call(bad), show(bad));
  }
  for (const bad of ['1', {}, 1]) assert.throws(TypeError, () => BigInt.prototype.toLocaleString.call(bad), show(bad));
  for (const bad of [{}, 1, AT]) assert.throws(TypeError, () => Date.prototype.toLocaleString.call(bad), show(bad));
  for (const bad of [null, undefined]) {
    assert.throws(TypeError, () => String.prototype.toLocaleUpperCase.call(bad));
    assert.throws(TypeError, () => Array.prototype.toLocaleString.call(bad));
  }
  assert.throws(TypeError, () => Int8Array.prototype.toLocaleString.call([1]));
  assert.sameValue(Number.prototype.toLocaleString.call(Object(5), 'en'), '5');
  assert.sameValue(String.prototype.toLocaleUpperCase.call(true, 'en'), 'TRUE');
});

test('sup-string.prototype.tolocaleuppercase', 'locales are validated, only the first is used', () => {
  assert.throws(RangeError, () => 'a'.toLocaleUpperCase('x-'));
  assert.throws(TypeError, () => 'a'.toLocaleUpperCase([5]));
  assert.sameValue('a'.toLocaleUpperCase([]), 'A');
  assert.sameValue('\ud800a'.toLocaleUpperCase('en'), '\ud800A', 'a lone surrogate stays');
  assert.sameValue('a\u0000b'.toLocaleUpperCase('en'), 'A\u0000B');
});

test('sup-array.prototype.tolocalestring', 'each element\'s own toLocaleString, with the arguments', () => {
  const seen = [];
  const element = { toLocaleString(...args) { seen.push(args); return 'x'; } };
  assert.sameValue([element, element].toLocaleString('fr', { a: 1 }), 'x,x');
  assert.sameValue(seen.length, 2);
  assert.sameValue(seen[0][0], 'fr');
  assert.sameValue(seen[0][1].a, 1);
  assert.throws(TypeError, () => [{ toLocaleString: 1 }].toLocaleString());
  const cyclic = [1];
  cyclic.push(cyclic);
  assert.sameValue(cyclic.toLocaleString('en'), '1,', 'a cycle is empty');
});

test('sup-number.prototype.tolocalestring', 'options and errors are NumberFormat\'s', () => {
  assert.throws(RangeError, () => (1).toLocaleString('en', { style: 'bogus' }));
  assert.throws(RangeError, () => (1).toLocaleString('x-'));
  assert.throws(TypeError, () => (1).toLocaleString('en', null));
  const log = [];
  (1).toLocaleString('en', readLog(log));
  assert.ok(log.length > 10 && log[0] === 'localeMatcher');
});
