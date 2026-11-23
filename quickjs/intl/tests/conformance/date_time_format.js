// Intl.DateTimeFormat: constructor, options, time zones, resolvedOptions,
// format, formatToParts and the ranges, per ECMA-402's DateTimeFormat
// clauses; the zone rows check the offsets the zone data gives.

const DTF = Intl.DateTimeFormat;
const DTF_ORDER = ['localeMatcher', 'calendar', 'numberingSystem', 'hour12', 'hourCycle', 'timeZone',
  'weekday', 'era', 'year', 'month', 'day', 'dayPeriod', 'hour', 'minute', 'second',
  'fractionalSecondDigits', 'timeZoneName', 'formatMatcher', 'dateStyle', 'timeStyle'];
const D = Date.UTC(2026, 0, 5, 13, 4, 5, 678);

test('sec-intl-datetimeformat-constructor', 'name, length, prototype', () => {
  verifyFunction(DTF, 'DateTimeFormat', 0, true);
  verifyProperty(DTF, 'prototype', { writable: false, enumerable: false, configurable: false });
  verifyProperty(DTF.prototype, 'constructor', { value: DTF, writable: true, enumerable: false, configurable: true });
  verifyToStringTag(DTF.prototype, 'Intl.DateTimeFormat');
  assert.sameValue(Object.prototype.toString.call(new DTF()), '[object Intl.DateTimeFormat]');
});

test('sec-intl-datetimeformat-constructor', 'methods and the format getter', () => {
  verifyMethod(DTF, 'supportedLocalesOf', 1);
  verifyMethod(DTF.prototype, 'formatToParts', 1);
  verifyMethod(DTF.prototype, 'formatRange', 2);
  verifyMethod(DTF.prototype, 'formatRangeToParts', 2);
  verifyMethod(DTF.prototype, 'resolvedOptions', 0);
  verifyGetter(DTF.prototype, 'format');
});

test('sec-intl-datetimeformat-constructor', 'called as a function it constructs', () => {
  const f = DTF('en', { timeZone: 'UTC' });
  assert.sameValue(Object.getPrototypeOf(f), DTF.prototype);
  assert.sameValue(f.format(D), '1/5/2026');
});

test('sec-chaindatetimeformat', 'called on a DateTimeFormat-derived object it wraps (legacy mode)', () => {
  const host = Object.create(DTF.prototype);
  assert.sameValue(DTF.call(host, 'en', { timeZone: 'UTC' }), host);
  assert.sameValue(host.format(D), '1/5/2026', 'format unwraps');
  assert.sameValue(host.resolvedOptions().timeZone, 'UTC', 'resolvedOptions unwraps');
  assert.throws(TypeError, () => host.formatToParts(D), 'formatToParts does not');
  assert.throws(TypeError, () => host.formatRange(D, D), 'formatRange does not');
});

test('sec-intl-datetimeformat-constructor', 'subclassing and a NewTarget without a prototype', () => {
  class Sub extends DTF {}
  const s = new Sub('en', { timeZone: 'UTC' });
  assert.sameValue(Object.getPrototypeOf(s), Sub.prototype);
  assert.sameValue(s.format(D), '1/5/2026');
  const nt = function () {}.bind();
  Object.defineProperty(nt, 'prototype', { value: null });
  assert.sameValue(Object.getPrototypeOf(Reflect.construct(DTF, [], nt)), DTF.prototype);
});

test('sec-createdatetimeformat', 'options are read once each, in the spec order', () => {
  const log = [];
  new DTF('en', readLog(log));
  assert.deepEqual(log, DTF_ORDER);
});

test('sec-createdatetimeformat', 'options: null is a TypeError, a primitive is coerced', () => {
  assert.throws(TypeError, () => new DTF('en', null));
  assert.sameValue(new DTF('en', 5).resolvedOptions().locale, 'en');
});

const DTF_ENUMS = [
  ['weekday', ['narrow', 'short', 'long']],
  ['era', ['narrow', 'short', 'long']],
  ['year', ['2-digit', 'numeric']],
  ['month', ['2-digit', 'numeric', 'narrow', 'short', 'long']],
  ['day', ['2-digit', 'numeric']],
  ['dayPeriod', ['narrow', 'short', 'long']],
  ['hour', ['2-digit', 'numeric']],
  ['minute', ['2-digit', 'numeric']],
  ['second', ['2-digit', 'numeric']],
  ['timeZoneName', ['short', 'long', 'shortOffset', 'longOffset', 'shortGeneric', 'longGeneric']],
  ['hourCycle', ['h11', 'h12', 'h23', 'h24']],
  ['formatMatcher', ['basic', 'best fit']],
  ['localeMatcher', ['lookup', 'best fit']],
  ['dateStyle', ['full', 'long', 'medium', 'short']],
  ['timeStyle', ['full', 'long', 'medium', 'short']],
];
for (const [key, values] of DTF_ENUMS) {
  test('sec-getoption', key + ': every valid value, and invalid ones', () => {
    for (const v of values) new DTF('en', { [key]: v, timeZone: 'UTC' }).format(D);
    for (const bad of ['', 'LONG', values[0] + ' ', 'auto', 'none']) {
      assert.throws(RangeError, () => new DTF('en', { [key]: bad }), key + ' ' + show(bad));
    }
    assert.throws(TypeError, () => new DTF('en', { [key]: Symbol() }));
  });
}

test('sec-createdatetimeformat', 'fractionalSecondDigits is 1 to 3', () => {
  for (const v of [1, 2, 3, 1.9, '2']) new DTF('en', { fractionalSecondDigits: v });
  for (const v of [0, 4, 0.5, NaN, -1, Infinity]) {
    assert.throws(RangeError, () => new DTF('en', { fractionalSecondDigits: v }), show(v));
  }
  const parts = new DTF('en', { second: 'numeric', fractionalSecondDigits: 2, timeZone: 'UTC' }).formatToParts(D);
  assert.sameValue(parts.find((p) => p.type === 'fractionalSecond').value, '67', 'truncated, not rounded');
});

test('sec-createdatetimeformat', 'calendar and numberingSystem must be a Unicode type', () => {
  for (const bad of ['', 'gr', 'gregory!', 'abcdefghi', 'gregory-', '-gregory', 'greg_ory']) {
    assert.throws(RangeError, () => new DTF('en', { calendar: bad }), 'calendar ' + show(bad));
    assert.throws(RangeError, () => new DTF('en', { numberingSystem: bad }), 'numberingSystem ' + show(bad));
  }
  const ro = new DTF('en', { calendar: 'unknown', numberingSystem: 'unknown', timeZone: 'UTC' }).resolvedOptions();
  assert.sameValue(ro.calendar, 'gregory', 'an unsupported calendar falls back');
  assert.sameValue(ro.numberingSystem, 'latn');
});

test('sec-createdatetimeformat', 'calendar and numberingSystem from the tag or the options', () => {
  assert.sameValue(new DTF('en-u-nu-arab', { timeZone: 'UTC' }).resolvedOptions().locale, 'en-u-nu-arab');
  assert.sameValue(new DTF('en-u-nu-arab', { timeZone: 'UTC' }).format(D), '\u0661/\u0665/\u0662\u0660\u0662\u0666');
  const opt = new DTF('en-u-nu-thai', { numberingSystem: 'arab', timeZone: 'UTC' }).resolvedOptions();
  assert.sameValue(opt.numberingSystem, 'arab');
  assert.sameValue(opt.locale, 'en', 'an option that overrides the tag removes the keyword');
});

test('sec-createdatetimeformat', 'dateStyle and timeStyle exclude the components', () => {
  for (const key of ['weekday', 'era', 'year', 'month', 'day', 'dayPeriod', 'hour', 'minute', 'second', 'timeZoneName']) {
    const value = key === 'timeZoneName' ? 'short' : DTF_ENUMS.find((e) => e[0] === key)[1][0];
    assert.throws(TypeError, () => new DTF('en', { dateStyle: 'short', [key]: value }), key);
  }
  assert.throws(TypeError, () => new DTF('en', { timeStyle: 'short', fractionalSecondDigits: 1 }));
  new DTF('en', { dateStyle: 'short', hour12: true, hourCycle: 'h23', timeZone: 'UTC' });
});

test('sec-intl.datetimeformat.prototype.resolvedoptions', 'keys and order, defaults', () => {
  const ro = new DTF('en', { timeZone: 'UTC' }).resolvedOptions();
  assert.deepEqual(Object.keys(ro), ['locale', 'calendar', 'numberingSystem', 'timeZone', 'year', 'month', 'day']);
  assert.deepEqual([ro.year, ro.month, ro.day], ['numeric', 'numeric', 'numeric']);
  const hour = new DTF('en', { timeZone: 'UTC', hour: 'numeric' }).resolvedOptions();
  assert.deepEqual(Object.keys(hour), ['locale', 'calendar', 'numberingSystem', 'timeZone', 'hourCycle', 'hour12', 'hour']);
  const styled = new DTF('en', { timeZone: 'UTC', dateStyle: 'short', timeStyle: 'long' }).resolvedOptions();
  assert.deepEqual(Object.keys(styled),
    ['locale', 'calendar', 'numberingSystem', 'timeZone', 'hourCycle', 'hour12', 'dateStyle', 'timeStyle']);
  const dateOnly = new DTF('en', { timeZone: 'UTC', dateStyle: 'long' }).resolvedOptions();
  assert.sameValue('hourCycle' in dateOnly, false, 'no hour, no hourCycle');
  const props = Object.getOwnPropertyDescriptor(ro, 'locale');
  assert.ok(props.writable && props.enumerable && props.configurable);
});

test('sec-intl.datetimeformat.prototype.resolvedoptions', 'fractionalSecondDigits is a Number', () => {
  const ro = new DTF('en', { timeZone: 'UTC', second: 'numeric', fractionalSecondDigits: '2' }).resolvedOptions();
  assert.sameValue(ro.fractionalSecondDigits, 2);
});

const HOUR_CYCLES = [
  ['en', {}, 'en/h12/true'],
  ['en', { hour12: false }, 'en/h23/false'],
  ['en', { hour12: true }, 'en/h12/true'],
  ['ja', {}, 'ja/h23/false'],
  ['de', {}, 'de/h23/false'],
  ['de', { hour12: true }, 'de/h12/true'],
  ['ko', {}, 'ko/h12/true'],
  ['en-u-hc-h23', {}, 'en-u-hc-h23/h23/false'],
  ['en-u-hc-h23', { hour12: true }, 'en/h12/true'],
  ['en-u-hc-h23', { hourCycle: 'h11' }, 'en/h11/true'],
  ['en', { hourCycle: 'h24' }, 'en/h24/false'],
  ['en', { hourCycle: 'h24', hour12: false }, 'en/h23/false'],
  ['en-u-hc-h24', {}, 'en-u-hc-h24/h24/false'],
];
for (const [locale, options, want] of HOUR_CYCLES) {
  test('sec-createdatetimeformat', 'hour cycle of ' + locale + ' ' + JSON.stringify(options), () => {
    const r = new DTF(locale, Object.assign({ hour: 'numeric', timeZone: 'UTC' }, options)).resolvedOptions();
    assert.sameValue([r.locale, r.hourCycle, r.hour12].join('/'), want);
  });
}

test('sec-formatdatetimepattern', 'h24 and h11 at midnight', () => {
  const at = (hourCycle) => new DTF('en', { hour: 'numeric', minute: '2-digit', hourCycle, timeZone: 'UTC' })
    .format(Date.UTC(2026, 0, 5, 0, 4, 5));
  assert.sameValue(at('h24'), '24:04');
  assert.sameValue(at('h11').replace('\u202f', ' '), '0:04 AM', 'the space is CLDR data');
  assert.sameValue(at('h23'), '00:04');
  assert.sameValue(at('h12').replace('\u202f', ' '), '12:04 AM');
});

const ZONE_IDS = [
  ['UTC', 'UTC'], ['utc', 'UTC'], ['Etc/UTC', 'UTC'], ['GMT', 'UTC'], ['Etc/GMT', 'UTC'],
  ['etc/gmt+5', 'Etc/GMT+5'], ['america/new_york', 'America/New_York'], ['US/Eastern', 'America/New_York'],
  ['+05:30', '+05:30'], ['+0530', '+05:30'], ['-00:00', '+00:00'], ['+05', '+05:00'], ['-0330', '-03:30'],
];
for (const [input, want] of ZONE_IDS) {
  test('sec-createdatetimeformat', 'timeZone ' + input, () => {
    assert.sameValue(new DTF('en', { timeZone: input }).resolvedOptions().timeZone, want);
  });
}

test('sec-createdatetimeformat', 'invalid time zones', () => {
  for (const bad of ['', 'Mars/Phobos', 'Z', '+05:30:00', '+24:00', '+05:30.5', 'America/New York', '+5', 'UTC+1', '\u0000']) {
    assert.throws(RangeError, () => new DTF('en', { timeZone: bad }), show(bad));
  }
});

only('rong', 'the reference keeps the name given; ECMA-402 stores the primary identifier', 'sec-createdatetimeformat',
  'timeZone: a link resolves to its primary identifier', () => {
    assert.sameValue(new DTF('en', { timeZone: 'Asia/Calcutta' }).resolvedOptions().timeZone, 'Asia/Kolkata');
    assert.sameValue(new DTF('en', { timeZone: 'Asia/Kolkata' }).resolvedOptions().timeZone, 'Asia/Kolkata');
  });

test('sec-createdatetimeformat', 'the default time zone is a valid one', () => {
  const tz = new DTF().resolvedOptions().timeZone;
  assert.sameValue(new DTF('en', { timeZone: tz }).resolvedOptions().timeZone, tz);
});

// Instants either side of transitions, odd offsets, past and future rules:
// the wall time and the offset the zone data gives.
const ZONES = [
  ['America/New_York', 1772953199000, '03/08/2026, 01:59:59 GMT-05:00'],
  ['America/New_York', 1772953200000, '03/08/2026, 03:00:00 GMT-04:00'],
  ['America/New_York', 1793512799000, '11/01/2026, 01:59:59 GMT-04:00'],
  ['America/New_York', 1793512800000, '11/01/2026, 01:00:00 GMT-05:00'],
  ['Europe/London', 1774745999000, '03/29/2026, 00:59:59 GMT+00:00'],
  ['Europe/London', 1774746000000, '03/29/2026, 02:00:00 GMT+01:00'],
  ['Australia/Lord_Howe', 1768435200000, '01/15/2026, 11:00:00 GMT+11:00'],
  ['Australia/Lord_Howe', 1784073600000, '07/15/2026, 10:30:00 GMT+10:30'],
  ['Asia/Kathmandu', 1767225600000, '01/01/2026, 05:45:00 GMT+05:45'],
  ['Pacific/Chatham', 1767225600000, '01/01/2026, 13:45:00 GMT+13:45'],
  ['Pacific/Chatham', 1782864000000, '07/01/2026, 12:45:00 GMT+12:45'],
  ['Asia/Kolkata', 1767225600000, '01/01/2026, 05:30:00 GMT+05:30'],
  ['America/St_Johns', 1767225600000, '12/31/2025, 20:30:00 GMT-03:30'],
  ['America/New_York', 4118083200000, '06/30/2100, 20:00:00 GMT-04:00'],
  ['Australia/Sydney', 1767225600000, '01/01/2026, 11:00:00 GMT+11:00'],
  ['Australia/Sydney', 1782864000000, '07/01/2026, 10:00:00 GMT+10:00'],
  ['Pacific/Kiritimati', 1767225600000, '01/01/2026, 14:00:00 GMT+14:00'],
  ['Pacific/Pago_Pago', 1767225600000, '12/31/2025, 13:00:00 GMT-11:00'],
  ['Etc/GMT+5', 1767225600000, '12/31/2025, 19:00:00 GMT-05:00'],
  ['+05:45', 1767225600000, '01/01/2026, 05:45:00 GMT+05:45'],
  ['-03:30', 1767225600000, '12/31/2025, 20:30:00 GMT-03:30'],
  ['Asia/Tokyo', 0, '01/01/1970, 09:00:00 GMT+09:00'],
  ['America/Sao_Paulo', 1541300400000, '11/04/2018, 01:00:00 GMT-02:00'],
  ['Europe/Moscow', 1356998400000, '01/01/2013, 04:00:00 GMT+04:00'],
  ['Europe/Moscow', 1451606400000, '01/01/2016, 03:00:00 GMT+03:00'],
];
for (const [zone, ms, want] of ZONES) {
  test('sec-formatdatetimepattern', 'zone ' + zone + ' at ' + ms, () => {
    const f = new DTF('en-US', { timeZone: zone, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit',
      minute: '2-digit', second: '2-digit', hourCycle: 'h23', timeZoneName: 'longOffset' });
    assert.sameValue(f.format(ms), want);
  });
}

test('sec-formatdatetimepattern', 'shortOffset names', () => {
  const name = (zone, ms) => new DTF('en', { timeZone: zone, timeZoneName: 'shortOffset' }).formatToParts(ms)
    .find((p) => p.type === 'timeZoneName').value;
  assert.sameValue(name('Asia/Kathmandu', D), 'GMT+5:45');
  assert.sameValue(name('America/New_York', D), 'GMT-5');
  assert.sameValue(name('+05:30', D), 'GMT+5:30');
});

test('sec-formatdatetime', 'TimeClip: the range of a time value', () => {
  const f = new DTF('en', { timeZone: 'UTC' });
  assert.sameValue(f.format(8.64e15), '9/13/275760');
  f.format(-8.64e15);
  for (const bad of [8.64e15 + 1, -8.64e15 - 1, NaN, Infinity, -Infinity]) {
    assert.throws(RangeError, () => f.format(bad), show(bad));
    assert.throws(RangeError, () => f.formatToParts(bad), show(bad));
  }
  assert.sameValue(f.format(-0.9), f.format(0), 'truncated toward zero');
  assert.throws(TypeError, () => f.format(Symbol()));
  assert.throws(TypeError, () => f.format(1n));
});

test('sec-formatdatetime', 'dates, numbers, strings and undefined', () => {
  const f = new DTF('en', { timeZone: 'UTC' });
  assert.sameValue(f.format(new Date(D)), '1/5/2026');
  assert.sameValue(f.format(String(D)), '1/5/2026');
  assert.sameValue(f.format({ valueOf: () => D }), '1/5/2026');
  assert.sameValue(f.format(), f.format(Date.now()), 'undefined is now');
  assert.sameValue(f.formatToParts().length > 0, true);
  assert.throws(RangeError, () => f.format(new Date(NaN)));
});

test('sec-intl.datetimeformat.prototype.format', 'the bound format', () => {
  const f = new DTF('en', { timeZone: 'UTC' });
  const bound = f.format;
  assert.sameValue(bound, f.format, 'the same function each time');
  assert.sameValue(bound.name, '');
  assert.sameValue(bound.length, 1);
  assert.sameValue(isConstructor(bound), false);
  assert.sameValue(bound(D), '1/5/2026');
  assert.sameValue([D, 0].map(bound).join(' '), '1/5/2026 1/1/1970');
});

test('sec-intl.datetimeformat.prototype.format', 'receivers', () => {
  const getter = Object.getOwnPropertyDescriptor(DTF.prototype, 'format').get;
  for (const bad of [undefined, null, 1, 'x', {}, new Intl.NumberFormat(), DTF.prototype]) {
    assert.throws(TypeError, () => getter.call(bad), show(bad));
    assert.throws(TypeError, () => DTF.prototype.formatToParts.call(bad, 0));
    assert.throws(TypeError, () => DTF.prototype.formatRange.call(bad, 0, 1));
    assert.throws(TypeError, () => DTF.prototype.resolvedOptions.call(bad));
  }
});

test('sec-formatdatetimetoparts', 'part objects', () => {
  const parts = new DTF('en', { timeZone: 'UTC', dateStyle: 'full', timeStyle: 'full' }).formatToParts(D);
  assert.ok(Array.isArray(parts));
  for (const p of parts) {
    assert.deepEqual(Object.keys(p), ['type', 'value']);
    assert.sameValue(Object.getPrototypeOf(p), Object.prototype);
  }
  const types = parts.map((p) => p.type);
  for (const t of ['weekday', 'month', 'day', 'year', 'hour', 'minute', 'second', 'dayPeriod', 'timeZoneName']) {
    assert.ok(types.includes(t), t);
  }
});

const DTF_TABLE = [
  ["ar", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05\u200f/01\u200f/2026", "day literal month literal year"],
  ["ar", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["ar", {month: 'long', day: 'numeric'}, "5 \u064a\u0646\u0627\u064a\u0631", "day literal month"],
  ["ar", {weekday: 'short'}, "\u0627\u0644\u0627\u062b\u0646\u064a\u0646", "weekday"],
  ["ar", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["br", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["br", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["br", {month: 'long', day: 'numeric'}, "5 Genver", "day literal month"],
  ["br", {weekday: 'short'}, "Lun", "weekday"],
  ["br", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["cs", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05. 01. 2026", "day literal month literal year"],
  ["cs", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["cs", {month: 'long', day: 'numeric'}, "5. ledna", "day literal month"],
  ["cs", {weekday: 'short'}, "po", "weekday"],
  ["cs", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["cy", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["cy", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["cy", {month: 'long', day: 'numeric'}, "Ionawr 5", "month literal day"],
  ["cy", {weekday: 'short'}, "Llun", "weekday"],
  ["cy", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["de", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05.01.2026", "day literal month literal year"],
  ["de", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["de", {month: 'long', day: 'numeric'}, "5. Januar", "day literal month"],
  ["de", {weekday: 'short'}, "Mo", "weekday"],
  ["de", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["en-GB", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["en-GB", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["en-GB", {month: 'long', day: 'numeric'}, "5 January", "day literal month"],
  ["en-GB", {weekday: 'short'}, "Mon", "weekday"],
  ["en-GB", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["en-IN", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["en-IN", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["en-IN", {month: 'long', day: 'numeric'}, "5 January", "day literal month"],
  ["en-IN", {weekday: 'short'}, "Mon", "weekday"],
  ["en-IN", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["en-US", {year: 'numeric', month: '2-digit', day: '2-digit'}, "01/05/2026", "month literal day literal year"],
  ["en-US", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["en-US", {month: 'long', day: 'numeric'}, "January 5", "month literal day"],
  ["en-US", {weekday: 'short'}, "Mon", "weekday"],
  ["en-US", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["en", {year: 'numeric', month: '2-digit', day: '2-digit'}, "01/05/2026", "month literal day literal year"],
  ["en", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["en", {month: 'long', day: 'numeric'}, "January 5", "month literal day"],
  ["en", {weekday: 'short'}, "Mon", "weekday"],
  ["en", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["es", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["es", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["es", {month: 'long', day: 'numeric'}, "5 de enero", "day literal month"],
  ["es", {weekday: 'short'}, "lun", "weekday"],
  ["es", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["fa", {year: 'numeric', month: '2-digit', day: '2-digit'}, "\u06f1\u06f4\u06f0\u06f4/\u06f1\u06f0/\u06f1\u06f5", "year literal month literal day"],
  ["fa", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "\u06f1\u06f3:\u06f0\u06f4", "hour literal minute"],
  ["fa", {month: 'long', day: 'numeric'}, "\u06f1\u06f5 \u062f\u06cc", "day literal month"],
  ["fa", {weekday: 'short'}, "\u062f\u0648\u0634\u0646\u0628\u0647", "weekday"],
  ["fa", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "\u06f4:\u06f0\u06f5\u066b\u06f6\u06f7\u06f8", "minute literal second literal fractionalSecond"],
  ["fr", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["fr", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["fr", {month: 'long', day: 'numeric'}, "5 janvier", "day literal month"],
  ["fr", {weekday: 'short'}, "lun.", "weekday"],
  ["fr", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["ga", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["ga", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["ga", {month: 'long', day: 'numeric'}, "5 Ean\u00e1ir", "day literal month"],
  ["ga", {weekday: 'short'}, "Luan", "weekday"],
  ["ga", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["gv", {year: 'numeric', month: '2-digit', day: '2-digit'}, "2026-01-05", "year literal month literal day"],
  ["gv", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["gv", {month: 'long', day: 'numeric'}, "Jerrey-geuree 5", "month literal day"],
  ["gv", {weekday: 'short'}, "Jel", "weekday"],
  ["gv", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["he", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05.01.2026", "day literal month literal year"],
  ["he", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["he", {month: 'long', day: 'numeric'}, "5 \u05d1\u05d9\u05e0\u05d5\u05d0\u05e8", "day literal month"],
  ["he", {weekday: 'short'}, "\u05d9\u05d5\u05dd \u05d1\u05f3", "weekday"],
  ["he", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["hi", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["hi", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["hi", {month: 'long', day: 'numeric'}, "5 \u091c\u0928\u0935\u0930\u0940", "day literal month"],
  ["hi", {weekday: 'short'}, "\u0938\u094b\u092e", "weekday"],
  ["hi", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["it", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["it", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["it", {month: 'long', day: 'numeric'}, "5 gennaio", "day literal month"],
  ["it", {weekday: 'short'}, "lun", "weekday"],
  ["it", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["ja", {year: 'numeric', month: '2-digit', day: '2-digit'}, "2026/01/05", "year literal month literal day"],
  ["ja", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["ja", {month: 'long', day: 'numeric'}, "1\u67085\u65e5", "month literal day literal"],
  ["ja", {weekday: 'short'}, "\u6708", "weekday"],
  ["ja", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["ko", {year: 'numeric', month: '2-digit', day: '2-digit'}, "2026. 01. 05.", "year literal month literal day literal"],
  ["ko", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["ko", {month: 'long', day: 'numeric'}, "1\uc6d4 5\uc77c", "month literal day literal"],
  ["ko", {weekday: 'short'}, "\uc6d4", "weekday"],
  ["ko", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["lt", {year: 'numeric', month: '2-digit', day: '2-digit'}, "2026-01-05", "year literal month literal day"],
  ["lt", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["lt", {month: 'long', day: 'numeric'}, "sausio 5 d.", "month literal day literal"],
  ["lt", {weekday: 'short'}, "pr", "weekday"],
  ["lt", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["mt", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["mt", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["mt", {month: 'long', day: 'numeric'}, "5 ta\u2019 Jannar", "day literal month"],
  ["mt", {weekday: 'short'}, "Tne", "weekday"],
  ["mt", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["pl", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05.01.2026", "day literal month literal year"],
  ["pl", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["pl", {month: 'long', day: 'numeric'}, "5 stycznia", "day literal month"],
  ["pl", {weekday: 'short'}, "pon.", "weekday"],
  ["pl", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["pt-PT", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["pt-PT", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["pt-PT", {month: 'long', day: 'numeric'}, "5 de janeiro", "day literal month"],
  ["pt-PT", {weekday: 'short'}, "segunda", "weekday"],
  ["pt-PT", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["pt", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05/01/2026", "day literal month literal year"],
  ["pt", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["pt", {month: 'long', day: 'numeric'}, "5 de janeiro", "day literal month"],
  ["pt", {weekday: 'short'}, "seg.", "weekday"],
  ["pt", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["ro", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05.01.2026", "day literal month literal year"],
  ["ro", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["ro", {month: 'long', day: 'numeric'}, "5 ianuarie", "day literal month"],
  ["ro", {weekday: 'short'}, "lun.", "weekday"],
  ["ro", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["ru", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05.01.2026", "day literal month literal year"],
  ["ru", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["ru", {month: 'long', day: 'numeric'}, "5 \u044f\u043d\u0432\u0430\u0440\u044f", "day literal month"],
  ["ru", {weekday: 'short'}, "\u043f\u043d", "weekday"],
  ["ru", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["sl", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05. 01. 2026", "day literal month literal year"],
  ["sl", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["sl", {month: 'long', day: 'numeric'}, "5. januar", "day literal month"],
  ["sl", {weekday: 'short'}, "pon.", "weekday"],
  ["sl", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["uk", {year: 'numeric', month: '2-digit', day: '2-digit'}, "05.01.2026", "day literal month literal year"],
  ["uk", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["uk", {month: 'long', day: 'numeric'}, "5 \u0441\u0456\u0447\u043d\u044f", "day literal month"],
  ["uk", {weekday: 'short'}, "\u043f\u043d", "weekday"],
  ["uk", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05,678", "minute literal second literal fractionalSecond"],
  ["zh-Hant", {year: 'numeric', month: '2-digit', day: '2-digit'}, "2026/01/05", "year literal month literal day"],
  ["zh-Hant", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["zh-Hant", {month: 'long', day: 'numeric'}, "1\u67085\u65e5", "month literal day literal"],
  ["zh-Hant", {weekday: 'short'}, "\u9031\u4e00", "weekday"],
  ["zh-Hant", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
  ["zh", {year: 'numeric', month: '2-digit', day: '2-digit'}, "2026/01/05", "year literal month literal day"],
  ["zh", {hour: '2-digit', minute: '2-digit', hourCycle: 'h23'}, "13:04", "hour literal minute"],
  ["zh", {month: 'long', day: 'numeric'}, "1\u67085\u65e5", "month literal day literal"],
  ["zh", {weekday: 'short'}, "\u5468\u4e00", "weekday"],
  ["zh", {minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3}, "04:05.678", "minute literal second literal fractionalSecond"],
];
for (const [locale, options, want, types] of DTF_TABLE) {
  test('sec-formatdatetimepattern', locale + ' ' + JSON.stringify(options), () => {
    const f = new DTF(locale, Object.assign({ timeZone: 'UTC' }, options));
    assert.sameValue(f.format(D), want);
    const parts = f.formatToParts(D);
    assert.sameValue(joinParts(parts), want, 'formatToParts round trip');
    assert.sameValue(parts.map((p) => p.type).join(' '), types);
  });
}

test('sec-formatdatetimerange', 'arguments', () => {
  const f = new DTF('en', { timeZone: 'UTC' });
  assert.throws(TypeError, () => f.formatRange(D));
  assert.throws(TypeError, () => f.formatRange(undefined, D));
  assert.throws(TypeError, () => f.formatRangeToParts(D));
  assert.throws(RangeError, () => f.formatRange(D, NaN));
  assert.throws(RangeError, () => f.formatRange(NaN, D));
  assert.sameValue(f.formatRange(D + 86400000 * 3, D), '1/8/2026\u2009\u2013\u20091/5/2026', 'an end before the start is allowed');
  assert.sameValue(f.formatRange(D, D), '1/5/2026', 'equal dates collapse');
});

test('sec-formatdatetimerangetoparts', 'sources', () => {
  const f = new DTF('en', { month: 'short', day: 'numeric', timeZone: 'UTC' });
  const parts = f.formatRangeToParts(Date.UTC(2026, 0, 5, 13), Date.UTC(2026, 0, 7, 9));
  assert.deepEqual(parts.map((p) => [p.type, p.value, p.source]), [
    ['month', 'Jan', 'shared'], ['literal', ' ', 'shared'], ['day', '5', 'startRange'],
    ['literal', '\u2009\u2013\u2009', 'shared'], ['day', '7', 'endRange']]);
  for (const p of parts) assert.deepEqual(Object.keys(p), ['type', 'value', 'source']);
  const same = f.formatRangeToParts(D, D);
  assert.ok(same.every((p) => p.source === 'shared'));
  const years = new DTF('en', { year: 'numeric', month: 'short', timeZone: 'UTC' })
    .formatRangeToParts(Date.UTC(2025, 11, 1), Date.UTC(2026, 1, 1));
  assert.ok(years.some((p) => p.type === 'year' && p.source === 'startRange'));
  assert.ok(years.some((p) => p.type === 'year' && p.source === 'endRange'));
});

test('sec-intl.datetimeformat.supportedlocalesof', 'localeMatcher', () => {
  for (const m of ['lookup', 'best fit', undefined]) {
    assert.deepEqual(DTF.supportedLocalesOf(['en-US', 'fr'], { localeMatcher: m }), ['en-US', 'fr']);
  }
  assert.throws(RangeError, () => DTF.supportedLocalesOf('en', { localeMatcher: 'x' }));
  assert.throws(TypeError, () => DTF.supportedLocalesOf('en', null));
  assert.deepEqual(DTF.supportedLocalesOf([]), []);
});
