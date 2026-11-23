// Intl.RelativeTimeFormat and Intl.ListFormat, per ECMA-402's clauses for
// each: constructors, options, resolvedOptions, format and formatToParts.

const RTF = Intl.RelativeTimeFormat;
const LF = Intl.ListFormat;

for (const [C, name, tag, methods] of [
  [RTF, 'RelativeTimeFormat', 'Intl.RelativeTimeFormat', [['format', 2], ['formatToParts', 2], ['resolvedOptions', 0]]],
  [LF, 'ListFormat', 'Intl.ListFormat', [['format', 1], ['formatToParts', 1], ['resolvedOptions', 0]]],
]) {
  const section = 'sec-intl-' + name.toLowerCase() + '-constructor';
  test(section, name + ': name, length, prototype, methods', () => {
    verifyFunction(C, name, 0, true);
    verifyProperty(C, 'prototype', { writable: false, enumerable: false, configurable: false });
    verifyProperty(C.prototype, 'constructor', { value: C, writable: true, enumerable: false, configurable: true });
    verifyToStringTag(C.prototype, tag);
    assert.sameValue(Object.getPrototypeOf(C), Function.prototype);
    verifyMethod(C, 'supportedLocalesOf', 1);
    for (const [key, length] of methods) verifyMethod(C.prototype, key, length);
  });
  test(section, name + ': needs new', () => {
    assert.throws(TypeError, () => C());
    assert.throws(TypeError, () => C.call(Object.create(C.prototype)));
  });
  test(section, name + ': subclassing and a NewTarget without a prototype', () => {
    class Sub extends C {}
    assert.sameValue(Object.getPrototypeOf(new Sub('en')), Sub.prototype);
    const nt = function () {}.bind();
    Object.defineProperty(nt, 'prototype', { value: null });
    assert.sameValue(Object.getPrototypeOf(Reflect.construct(C, ['en'], nt)), C.prototype);
  });
  test(section, name + ': a NewTarget prototype is read once, before the options', () => {
    const log = [];
    const nt = new Proxy(function () {}, {
      get(target, key) { log.push(String(key)); return key === 'prototype' ? C.prototype : target[key]; },
    });
    Reflect.construct(C, ['en', loggingOptions(log, { style: 'short' })], nt);
    assert.deepEqual(log, ['prototype', 'style']);
  });
  test(section, name + ': receivers', () => {
    for (const bad of [undefined, null, 1, {}, new Intl.NumberFormat(), C.prototype]) {
      for (const [key] of methods) {
        assert.throws(TypeError, () => C.prototype[key].call(bad, 1, 'day'), key + ' on ' + show(bad));
      }
    }
  });
  test('sec-intl.' + name.toLowerCase() + '.supportedlocalesof', name + ': supportedLocalesOf', () => {
    for (const m of ['lookup', 'best fit', undefined]) {
      assert.deepEqual(C.supportedLocalesOf(['en-US', 'fr'], { localeMatcher: m }), ['en-US', 'fr']);
    }
    assert.throws(RangeError, () => C.supportedLocalesOf('en', { localeMatcher: 'x' }));
  });
}

test('sec-InitializeRelativeTimeFormat', 'options are read once each, in the spec order', () => {
  const log = [];
  new RTF('en', readLog(log));
  assert.deepEqual(log, ['localeMatcher', 'numberingSystem', 'style', 'numeric']);
  assert.sameValue(new RTF('en', 5).resolvedOptions().style, 'long', 'a primitive is coerced');
  assert.throws(TypeError, () => new RTF('en', null));
});

test('sec-InitializeRelativeTimeFormat', 'option values', () => {
  for (const [key, values] of [['style', ['long', 'short', 'narrow']], ['numeric', ['always', 'auto']], ['localeMatcher', ['lookup', 'best fit']]]) {
    for (const v of values) new RTF('en', { [key]: v });
    for (const bad of ['', 'LONG', 'none']) assert.throws(RangeError, () => new RTF('en', { [key]: bad }), key + ' ' + show(bad));
    assert.throws(TypeError, () => new RTF('en', { [key]: Symbol() }));
  }
  for (const bad of ['', 'ab', 'abcdefghi', 'latn!']) assert.throws(RangeError, () => new RTF('en', { numberingSystem: bad }));
});

test('sec-intl.relativetimeformat.prototype.resolvedoptions', 'keys, order and values', () => {
  const ro = new RTF('en').resolvedOptions();
  assert.deepEqual(Object.keys(ro), ['locale', 'style', 'numeric', 'numberingSystem']);
  assert.deepEqual([ro.locale, ro.style, ro.numeric, ro.numberingSystem], ['en', 'long', 'always', 'latn']);
  assert.sameValue(new RTF('en-u-nu-arab').resolvedOptions().numberingSystem, 'arab');
  assert.sameValue(new RTF('en-u-nu-arab').resolvedOptions().locale, 'en-u-nu-arab');
  assert.sameValue(new RTF('en-u-nu-arab', { numberingSystem: 'thai' }).resolvedOptions().locale, 'en');
});

test('sec-singularrelativetimeunit', 'units, singular and plural', () => {
  const rtf = new RTF('en');
  for (const u of ['second', 'minute', 'hour', 'day', 'week', 'month', 'quarter', 'year']) {
    assert.sameValue(rtf.format(2, u), rtf.format(2, u + 's'));
  }
  for (const bad of ['', 'Day', 'fortnight', 'millisecond', 'dayss', 'days ']) {
    assert.throws(RangeError, () => rtf.format(1, bad), show(bad));
  }
  assert.throws(TypeError, () => rtf.format(1, Symbol()));
  assert.sameValue(rtf.format(1, { toString: () => 'day' }), 'in 1 day', 'ToString of the unit');
});

test('sec-PartitionRelativeTimePattern', 'values: ToNumber, finite only', () => {
  const rtf = new RTF('en');
  for (const bad of [NaN, Infinity, -Infinity, 'x', undefined]) {
    assert.throws(RangeError, () => rtf.format(bad, 'day'), show(bad));
  }
  assert.throws(TypeError, () => rtf.format(1n, 'day'));
  assert.throws(TypeError, () => rtf.format(Symbol(), 'day'));
  assert.sameValue(rtf.format('2', 'day'), 'in 2 days');
  assert.sameValue(rtf.format(-0, 'day'), '0 days ago', '-0 is past');
  assert.sameValue(rtf.format(1e21, 'year'), 'in 1,000,000,000,000,000,000,000 years');
  assert.sameValue(rtf.format(1.23456, 'day'), 'in 1.235 days', 'NumberFormat\'s defaults');
  const order = [];
  rtf.format({ valueOf() { order.push('value'); return 1; } }, { toString() { order.push('unit'); return 'day'; } });
  assert.deepEqual(order, ['value', 'unit']);
});

test('sec-FormatRelativeTimeToParts', 'part objects and the unit key', () => {
  const parts = new RTF('en').formatToParts(1234.5, 'day');
  assert.deepEqual(parts.map((p) => [p.type, p.value, p.unit]), [
    ['literal', 'in ', undefined], ['integer', '1', 'day'], ['group', ',', 'day'], ['integer', '234', 'day'],
    ['decimal', '.', 'day'], ['fraction', '5', 'day'], ['literal', ' days', undefined]]);
  assert.deepEqual(Object.keys(parts[0]), ['type', 'value']);
  assert.deepEqual(Object.keys(parts[1]), ['type', 'value', 'unit']);
  assert.deepEqual(new RTF('en', { numeric: 'auto' }).formatToParts(1, 'day').map((p) => [p.type, p.value]),
    [['literal', 'tomorrow']]);
  assert.sameValue(joinParts(new RTF('en').formatToParts(-3, 'hours')), '3 hours ago');
});

const RTF_CASES = [[-1, 'day'], [0, 'day'], [1, 'day'], [2, 'days'], [-3, 'hour'], [5, 'year'], [-0, 'second'],
  [1.5, 'week'], [-1, 'quarter'], [21, 'month']];
const RTF_TABLE = [
  ["en", {}, ["1 day ago", "in 0 days", "in 1 day", "in 2 days", "3 hours ago", "in 5 years", "0 seconds ago", "in 1.5 weeks", "1 quarter ago", "in 21 months"]],
  ["en", {numeric: 'auto'}, ["yesterday", "today", "tomorrow", "in 2 days", "3 hours ago", "in 5 years", "now", "in 1.5 weeks", "last quarter", "in 21 months"]],
  ["en", {style: 'short'}, ["1 day ago", "in 0 days", "in 1 day", "in 2 days", "3 hr. ago", "in 5 yr.", "0 sec. ago", "in 1.5 wk.", "1 qtr. ago", "in 21 mo."]],
  ["en", {style: 'narrow', numeric: 'auto'}, ["yesterday", "today", "tomorrow", "in 2d", "3h ago", "in 5y", "now", "in 1.5w", "last qtr.", "in 21mo"]],
  ["de", {}, ["vor 1 Tag", "in 0 Tagen", "in 1 Tag", "in 2 Tagen", "vor 3 Stunden", "in 5 Jahren", "vor 0 Sekunden", "in 1,5 Wochen", "vor 1 Quartal", "in 21 Monaten"]],
  ["de", {numeric: 'auto'}, ["gestern", "heute", "morgen", "\u00fcbermorgen", "vor 3 Stunden", "in 5 Jahren", "jetzt", "in 1,5 Wochen", "letztes Quartal", "in 21 Monaten"]],
  ["de", {style: 'short'}, ["vor 1 Tag", "in 0 Tagen", "in 1 Tag", "in 2 Tagen", "vor 3 Std.", "in 5 Jahren", "vor 0 Sek.", "in 1,5 Wochen", "vor 1 Quart.", "in 21 Monaten"]],
  ["de", {style: 'narrow', numeric: 'auto'}, ["gestern", "heute", "morgen", "\u00fcbermorgen", "vor 3 Std.", "in 5 Jahren", "jetzt", "in 1,5 Wo.", "letztes Quartal", "in 21 Monaten"]],
  ["fr", {}, ["il y a 1 jour", "dans 0 jour", "dans 1 jour", "dans 2 jours", "il y a 3 heures", "dans 5 ans", "il y a 0 seconde", "dans 1,5 semaine", "il y a 1 trimestre", "dans 21 mois"]],
  ["fr", {numeric: 'auto'}, ["hier", "aujourd\u2019hui", "demain", "apr\u00e8s-demain", "il y a 3 heures", "dans 5 ans", "maintenant", "dans 1,5 semaine", "le trimestre dernier", "dans 21 mois"]],
  ["fr", {style: 'short'}, ["il y a 1\u00a0j", "dans 0\u00a0j", "dans 1\u00a0j", "dans 2\u00a0j", "il y a 3\u00a0h", "dans 5 a", "il y a 0\u00a0s", "dans 1,5 sem.", "il y a 1 trim.", "dans 21 m."]],
  ["fr", {style: 'narrow', numeric: 'auto'}, ["hier", "aujourd\u2019hui", "demain", "apr\u00e8s-demain", "-3 h", "+5 a", "maintenant", "+1,5 sem.", "le trimestre dernier", "+21 m."]],
  ["ja", {}, ["1 \u65e5\u524d", "0 \u65e5\u5f8c", "1 \u65e5\u5f8c", "2 \u65e5\u5f8c", "3 \u6642\u9593\u524d", "5 \u5e74\u5f8c", "0 \u79d2\u524d", "1.5 \u9031\u9593\u5f8c", "1 \u56db\u534a\u671f\u524d", "21 \u304b\u6708\u5f8c"]],
  ["ja", {numeric: 'auto'}, ["\u6628\u65e5", "\u4eca\u65e5", "\u660e\u65e5", "\u660e\u5f8c\u65e5", "3 \u6642\u9593\u524d", "5 \u5e74\u5f8c", "\u4eca", "1.5 \u9031\u9593\u5f8c", "\u524d\u56db\u534a\u671f", "21 \u304b\u6708\u5f8c"]],
  ["ja", {style: 'short'}, ["1 \u65e5\u524d", "0 \u65e5\u5f8c", "1 \u65e5\u5f8c", "2 \u65e5\u5f8c", "3 \u6642\u9593\u524d", "5 \u5e74\u5f8c", "0 \u79d2\u524d", "1.5 \u9031\u9593\u5f8c", "1 \u56db\u534a\u671f\u524d", "21 \u304b\u6708\u5f8c"]],
  ["ja", {style: 'narrow', numeric: 'auto'}, ["\u6628\u65e5", "\u4eca\u65e5", "\u660e\u65e5", "\u660e\u5f8c\u65e5", "3\u6642\u9593\u524d", "5\u5e74\u5f8c", "\u4eca", "1.5\u9031\u9593\u5f8c", "\u524d\u56db\u534a\u671f", "21\u304b\u6708\u5f8c"]],
  ["ar", {}, ["\u0642\u0628\u0644 \u064a\u0648\u0645 \u0648\u0627\u062d\u062f", "\u062e\u0644\u0627\u0644 0 \u064a\u0648\u0645", "\u062e\u0644\u0627\u0644 \u064a\u0648\u0645 \u0648\u0627\u062d\u062f", "\u062e\u0644\u0627\u0644 \u064a\u0648\u0645\u064a\u0646", "\u0642\u0628\u0644 3 \u0633\u0627\u0639\u0627\u062a", "\u062e\u0644\u0627\u0644 5 \u0633\u0646\u0648\u0627\u062a", "\u0642\u0628\u0644 0 \u062b\u0627\u0646\u064a\u0629", "\u062e\u0644\u0627\u0644 1.5 \u0623\u0633\u0628\u0648\u0639", "\u0642\u0628\u0644 \u0631\u0628\u0639 \u0633\u0646\u0629 \u0648\u0627\u062d\u062f", "\u062e\u0644\u0627\u0644 21 \u0634\u0647\u0631\u064b\u0627"]],
  ["ar", {numeric: 'auto'}, ["\u0623\u0645\u0633", "\u0627\u0644\u064a\u0648\u0645", "\u063a\u062f\u064b\u0627", "\u0628\u0639\u062f \u0627\u0644\u063a\u062f", "\u0642\u0628\u0644 3 \u0633\u0627\u0639\u0627\u062a", "\u062e\u0644\u0627\u0644 5 \u0633\u0646\u0648\u0627\u062a", "\u0627\u0644\u0622\u0646", "\u062e\u0644\u0627\u0644 1.5 \u0623\u0633\u0628\u0648\u0639", "\u0627\u0644\u0631\u0628\u0639 \u0627\u0644\u0623\u062e\u064a\u0631", "\u062e\u0644\u0627\u0644 21 \u0634\u0647\u0631\u064b\u0627"]],
  ["ar", {style: 'short'}, ["\u0642\u0628\u0644 \u064a\u0648\u0645 \u0648\u0627\u062d\u062f", "\u062e\u0644\u0627\u0644 0 \u064a\u0648\u0645", "\u062e\u0644\u0627\u0644 \u064a\u0648\u0645 \u0648\u0627\u062d\u062f", "\u062e\u0644\u0627\u0644 \u064a\u0648\u0645\u064a\u0646", "\u0642\u0628\u0644 3 \u0633\u0627\u0639\u0627\u062a", "\u062e\u0644\u0627\u0644 5 \u0633\u0646\u0648\u0627\u062a", "\u0642\u0628\u0644 0 \u062b\u0627\u0646\u064a\u0629", "\u062e\u0644\u0627\u0644 1.5 \u0623\u0633\u0628\u0648\u0639", "\u0642\u0628\u0644 \u0631\u0628\u0639 \u0633\u0646\u0629 \u0648\u0627\u062d\u062f", "\u062e\u0644\u0627\u0644 21 \u0634\u0647\u0631\u064b\u0627"]],
  ["ar", {style: 'narrow', numeric: 'auto'}, ["\u0623\u0645\u0633", "\u0627\u0644\u064a\u0648\u0645", "\u063a\u062f\u064b\u0627", "\u0628\u0639\u062f \u0627\u0644\u063a\u062f", "\u0642\u0628\u0644 3 \u0633\u0627\u0639\u0627\u062a", "\u062e\u0644\u0627\u0644 5 \u0633\u0646\u0648\u0627\u062a", "\u0627\u0644\u0622\u0646", "\u062e\u0644\u0627\u0644 1.5 \u0623\u0633\u0628\u0648\u0639", "\u0627\u0644\u0631\u0628\u0639 \u0627\u0644\u0623\u062e\u064a\u0631", "\u062e\u0644\u0627\u0644 21 \u0634\u0647\u0631\u064b\u0627"]],
  ["ru", {}, ["1 \u0434\u0435\u043d\u044c \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 0 \u0434\u043d\u0435\u0439", "\u0447\u0435\u0440\u0435\u0437 1 \u0434\u0435\u043d\u044c", "\u0447\u0435\u0440\u0435\u0437 2 \u0434\u043d\u044f", "3 \u0447\u0430\u0441\u0430 \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 5 \u043b\u0435\u0442", "0 \u0441\u0435\u043a\u0443\u043d\u0434 \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 1,5 \u043d\u0435\u0434\u0435\u043b\u0438", "1 \u043a\u0432\u0430\u0440\u0442\u0430\u043b \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 21 \u043c\u0435\u0441\u044f\u0446"]],
  ["ru", {numeric: 'auto'}, ["\u0432\u0447\u0435\u0440\u0430", "\u0441\u0435\u0433\u043e\u0434\u043d\u044f", "\u0437\u0430\u0432\u0442\u0440\u0430", "\u043f\u043e\u0441\u043b\u0435\u0437\u0430\u0432\u0442\u0440\u0430", "3 \u0447\u0430\u0441\u0430 \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 5 \u043b\u0435\u0442", "\u0441\u0435\u0439\u0447\u0430\u0441", "\u0447\u0435\u0440\u0435\u0437 1,5 \u043d\u0435\u0434\u0435\u043b\u0438", "\u0432 \u043f\u0440\u043e\u0448\u043b\u043e\u043c \u043a\u0432\u0430\u0440\u0442\u0430\u043b\u0435", "\u0447\u0435\u0440\u0435\u0437 21 \u043c\u0435\u0441\u044f\u0446"]],
  ["ru", {style: 'short'}, ["1 \u0434\u043d. \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 0 \u0434\u043d.", "\u0447\u0435\u0440\u0435\u0437 1 \u0434\u043d.", "\u0447\u0435\u0440\u0435\u0437 2 \u0434\u043d.", "3 \u0447 \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 5 \u043b.", "0 \u0441\u0435\u043a. \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 1,5 \u043d\u0435\u0434.", "1 \u043a\u0432. \u043d\u0430\u0437\u0430\u0434", "\u0447\u0435\u0440\u0435\u0437 21 \u043c\u0435\u0441."]],
  ["ru", {style: 'narrow', numeric: 'auto'}, ["\u0432\u0447\u0435\u0440\u0430", "\u0441\u0435\u0433\u043e\u0434\u043d\u044f", "\u0437\u0430\u0432\u0442\u0440\u0430", "\u043f\u043e\u0441\u043b\u0435\u0437\u0430\u0432\u0442\u0440\u0430", "-3 \u0447", "+5 \u043b.", "\u0441\u0435\u0439\u0447\u0430\u0441", "+1,5 \u043d\u0435\u0434.", "\u043f\u043e\u0441\u043b. \u043a\u0432.", "+21 \u043c\u0435\u0441."]],
  ["pl", {}, ["1 dzie\u0144 temu", "za 0 dni", "za 1 dzie\u0144", "za 2 dni", "3 godziny temu", "za 5 lat", "0 sekund temu", "za 1,5 tygodnia", "1 kwarta\u0142 temu", "za 21 miesi\u0119cy"]],
  ["pl", {numeric: 'auto'}, ["wczoraj", "dzisiaj", "jutro", "pojutrze", "3 godziny temu", "za 5 lat", "teraz", "za 1,5 tygodnia", "w zesz\u0142ym kwartale", "za 21 miesi\u0119cy"]],
  ["pl", {style: 'short'}, ["1 dzie\u0144 temu", "za 0 dni", "za 1 dzie\u0144", "za 2 dni", "3 godz. temu", "za 5 lat", "0 sek. temu", "za 1,5 tyg.", "1 kw. temu", "za 21 mies."]],
  ["pl", {style: 'narrow', numeric: 'auto'}, ["wcz.", "dzi\u015b", "jutro", "pojutrze", "3 g. temu", "za 5 lat", "teraz", "za 1,5 tyg.", "w zesz\u0142ym kwartale", "za 21 mies."]],
  ["zh", {}, ["1\u5929\u524d", "0\u5929\u540e", "1\u5929\u540e", "2\u5929\u540e", "3\u5c0f\u65f6\u524d", "5\u5e74\u540e", "0\u79d2\u949f\u524d", "1.5\u5468\u540e", "1\u4e2a\u5b63\u5ea6\u524d", "21\u4e2a\u6708\u540e"]],
  ["zh", {numeric: 'auto'}, ["\u6628\u5929", "\u4eca\u5929", "\u660e\u5929", "\u540e\u5929", "3\u5c0f\u65f6\u524d", "5\u5e74\u540e", "\u73b0\u5728", "1.5\u5468\u540e", "\u4e0a\u5b63\u5ea6", "21\u4e2a\u6708\u540e"]],
  ["zh", {style: 'short'}, ["1\u5929\u524d", "0\u5929\u540e", "1\u5929\u540e", "2\u5929\u540e", "3\u5c0f\u65f6\u524d", "5\u5e74\u540e", "0\u79d2\u524d", "1.5\u5468\u540e", "1\u4e2a\u5b63\u5ea6\u524d", "21\u4e2a\u6708\u540e"]],
  ["zh", {style: 'narrow', numeric: 'auto'}, ["\u6628\u5929", "\u4eca\u5929", "\u660e\u5929", "\u540e\u5929", "3\u5c0f\u65f6\u524d", "5\u5e74\u540e", "\u73b0\u5728", "1.5\u5468\u540e", "\u4e0a\u5b63\u5ea6", "21\u4e2a\u6708\u540e"]],
  ["es", {}, ["hace 1 d\u00eda", "dentro de 0 d\u00edas", "dentro de 1 d\u00eda", "dentro de 2 d\u00edas", "hace 3 horas", "dentro de 5 a\u00f1os", "hace 0 segundos", "dentro de 1,5 semanas", "hace 1 trimestre", "dentro de 21 meses"]],
  ["es", {numeric: 'auto'}, ["ayer", "hoy", "ma\u00f1ana", "pasado ma\u00f1ana", "hace 3 horas", "dentro de 5 a\u00f1os", "ahora", "dentro de 1,5 semanas", "el trimestre pasado", "dentro de 21 meses"]],
  ["es", {style: 'short'}, ["hace 1 d", "dentro de 0 d", "dentro de 1 d", "dentro de 2 d", "hace 3 h", "dentro de 5 a", "hace 0 s", "dentro de 1,5 sem.", "hace 1 trim.", "dentro de 21 m"]],
  ["es", {style: 'narrow', numeric: 'auto'}, ["ayer", "hoy", "ma\u00f1ana", "pasado ma\u00f1ana", "hace 3 h", "dentro de 5 a", "ahora", "dentro de 1,5 sem.", "el trimestre pasado", "dentro de 21 m"]],
  ["he", {}, ["\u05dc\u05e4\u05e0\u05d9 \u05d9\u05d5\u05dd \u05d0\u05d7\u05d3 (1)", "\u05d1\u05e2\u05d5\u05d3 0 \u05d9\u05de\u05d9\u05dd", "\u05d1\u05e2\u05d5\u05d3 \u05d9\u05d5\u05dd (1)", "\u05d1\u05e2\u05d5\u05d3 \u05d9\u05d5\u05de\u05d9\u05d9\u05dd (2)", "\u05dc\u05e4\u05e0\u05d9 3 \u05e9\u05e2\u05d5\u05ea", "\u05d1\u05e2\u05d5\u05d3 5 \u05e9\u05e0\u05d9\u05dd", "\u05dc\u05e4\u05e0\u05d9 0 \u05e9\u05e0\u05d9\u05d5\u05ea", "\u05d1\u05e2\u05d5\u05d3 1.5 \u05e9\u05d1\u05d5\u05e2\u05d5\u05ea", "\u05dc\u05e4\u05e0\u05d9 \u05e8\u05d1\u05e2\u05d5\u05df \u05d0\u05d7\u05d3 (1)", "\u05d1\u05e2\u05d5\u05d3 21 \u05d7\u05d5\u05d3\u05e9\u05d9\u05dd"]],
  ["he", {numeric: 'auto'}, ["\u05d0\u05ea\u05de\u05d5\u05dc", "\u05d4\u05d9\u05d5\u05dd", "\u05de\u05d7\u05e8", "\u05de\u05d7\u05e8\u05ea\u05d9\u05d9\u05dd", "\u05dc\u05e4\u05e0\u05d9 3 \u05e9\u05e2\u05d5\u05ea", "\u05d1\u05e2\u05d5\u05d3 5 \u05e9\u05e0\u05d9\u05dd", "\u05e2\u05db\u05e9\u05d9\u05d5", "\u05d1\u05e2\u05d5\u05d3 1.5 \u05e9\u05d1\u05d5\u05e2\u05d5\u05ea", "\u05d4\u05e8\u05d1\u05e2\u05d5\u05df \u05d4\u05e7\u05d5\u05d3\u05dd", "\u05d1\u05e2\u05d5\u05d3 21 \u05d7\u05d5\u05d3\u05e9\u05d9\u05dd"]],
  ["he", {style: 'short'}, ["\u05dc\u05e4\u05e0\u05d9 \u05d9\u05d5\u05dd \u05d0\u05d7\u05d3 (1)", "\u05d1\u05e2\u05d5\u05d3 0 \u05d9\u05de\u05d9\u05dd", "\u05d1\u05e2\u05d5\u05d3 \u05d9\u05d5\u05dd \u05d0\u05d7\u05d3 (1)", "\u05d1\u05e2\u05d5\u05d3 \u05d9\u05d5\u05de\u05d9\u05d9\u05dd (2)", "\u05dc\u05e4\u05e0\u05d9 3 \u05e9\u05e2\u05f3", "\u05d1\u05e2\u05d5\u05d3 5 \u05e9\u05e0\u05d9\u05dd", "\u05dc\u05e4\u05e0\u05d9 0 \u05e9\u05e0\u05f3", "\u05d1\u05e2\u05d5\u05d3 1.5 \u05e9\u05d1\u05f3", "\u05dc\u05e4\u05e0\u05d9 \u05e8\u05d1\u05e2\u05f3 \u05d0\u05d7\u05d3 (1)", "\u05d1\u05e2\u05d5\u05d3 21 \u05d7\u05d5\u05d3\u05e9\u05d9\u05dd"]],
  ["he", {style: 'narrow', numeric: 'auto'}, ["\u05d0\u05ea\u05de\u05d5\u05dc", "\u05d4\u05d9\u05d5\u05dd", "\u05de\u05d7\u05e8", "\u05de\u05d7\u05e8\u05ea\u05d9\u05d9\u05dd", "\u05dc\u05e4\u05e0\u05d9 3 \u05e9\u05e2\u05f3", "\u05d1\u05e2\u05d5\u05d3 5 \u05e9\u05e0\u05d9\u05dd", "\u05e2\u05db\u05e9\u05d9\u05d5", "\u05d1\u05e2\u05d5\u05d3 1.5 \u05e9\u05d1\u05f3", "\u05d4\u05e8\u05d1\u05e2\u05d5\u05df \u05d4\u05e7\u05d5\u05d3\u05dd", "\u05d1\u05e2\u05d5\u05d3 21 \u05d7\u05d5\u05f3"]],
];
// CLDR 46 made latn the default for ar and updated he's relative day wording;
// CLDR 49 updated he short seconds to שני׳.
const RTF_CLDR46 = { ar: true, he: true };
for (const [locale, options, want] of RTF_TABLE) {
  const variants = [];
  if (RTF_CLDR46[locale]) {
    variants.push([{ cldrBelow: 46 }, SKIP_DATA, 'the ' + locale + ' row holds CLDR 46+ data']);
  }
  if (locale === 'he' && options.style === 'short') {
    const cldr49 = want.slice();
    cldr49[6] = '\u05dc\u05e4\u05e0\u05d9 0 \u05e9\u05e0\u05d9\u05f3';
    variants.push([{ cldrAtLeast: 49 }, cldr49, 'CLDR 49 updated he short seconds to שני׳']);
  }
  dataTest('sec-PartitionRelativeTimePattern', locale + ' ' + JSON.stringify(options), want, variants, (expected) => {
    const rtf = new RTF(locale, options);
    for (let i = 0; i < expected.length; i++) {
      const [v, u] = RTF_CASES[i];
      assert.sameValue(rtf.format(v, u), expected[i], v + ' ' + u);
      assert.sameValue(joinParts(rtf.formatToParts(v, u)), expected[i], 'formatToParts');
    }
  });
}

test('sec-Intl.ListFormat', 'options are read once each, in the spec order, and must be an object', () => {
  const log = [];
  new LF('en', readLog(log));
  assert.deepEqual(log, ['localeMatcher', 'type', 'style']);
  assert.throws(TypeError, () => new LF('en', 5), 'GetOptionsObject, not CoerceOptionsToObject');
  assert.throws(TypeError, () => new LF('en', null));
  new LF('en', undefined);
});

test('sec-Intl.ListFormat', 'option values', () => {
  for (const [key, values] of [['type', ['conjunction', 'disjunction', 'unit']], ['style', ['long', 'short', 'narrow']]]) {
    for (const v of values) new LF('en', { [key]: v });
    for (const bad of ['', 'and', 'LONG']) assert.throws(RangeError, () => new LF('en', { [key]: bad }), key + ' ' + show(bad));
  }
});

test('sec-Intl.ListFormat.prototype.resolvedoptions', 'keys, order and values', () => {
  const ro = new LF('en', { type: 'unit', style: 'narrow' }).resolvedOptions();
  assert.deepEqual(Object.keys(ro), ['locale', 'type', 'style']);
  assert.deepEqual([ro.locale, ro.type, ro.style], ['en', 'unit', 'narrow']);
  assert.sameValue(new LF('en-u-nu-arab').resolvedOptions().locale, 'en', 'no relevant extension keys');
});

test('sec-createstringlistfromiterable', 'iterables of strings only', () => {
  const lf = new LF('en');
  assert.sameValue(lf.format(), '');
  assert.sameValue(lf.format(new Set(['a', 'b'])), 'a and b');
  assert.sameValue(lf.format('ab'), 'a and b', 'a string iterates its code points');
  assert.sameValue(lf.format('\ud83d\ude00x'), '\ud83d\ude00 and x');
  assert.throws(TypeError, () => lf.format(['a', 1]));
  assert.throws(TypeError, () => lf.format([new String('a')]));
  assert.throws(TypeError, () => lf.format(null));
  assert.throws(TypeError, () => lf.format(5));
  assert.throws(TypeError, () => lf.format({}));
  let closed = 0;
  const iterable = {
    [Symbol.iterator]() {
      let i = 0;
      return { next: () => ({ done: false, value: i++ ? 2 : 'a' }), return: () => { closed++; return {}; } };
    },
  };
  assert.throws(TypeError, () => lf.format(iterable));
  assert.sameValue(closed, 1, 'the iterator is closed on a non-string');
  const huge = Array.from({ length: 10000 }, (_, i) => 'x' + i);
  assert.ok(lf.format(huge).endsWith(', and x9999'));
  assert.sameValue(lf.format(['', '']), ' and ');
});

test('sec-formatlisttoparts', 'part objects', () => {
  const parts = new LF('en').formatToParts(['a', 'b', 'c']);
  assert.deepEqual(parts.map((p) => [p.type, p.value]),
    [['element', 'a'], ['literal', ', '], ['element', 'b'], ['literal', ', and '], ['element', 'c']]);
  for (const p of parts) assert.deepEqual(Object.keys(p), ['type', 'value']);
  assert.deepEqual(new LF('en').formatToParts([]), []);
  const tricky = new LF('en').formatToParts(['{0}', '{1}', ' and ']);
  assert.deepEqual(tricky.filter((p) => p.type === 'element').map((p) => p.value), ['{0}', '{1}', ' and '], 'elements are not patterns');
});

const LF_LISTS = [[], ['A'], ['A', 'B'], ['A', 'B', 'C']];
const LF_TABLE = [
  ["en", {}, ["", "A", "A and B", "A, B, and C"]],
  ["en", {type: 'disjunction'}, ["", "A", "A or B", "A, B, or C"]],
  ["en", {type: 'unit'}, ["", "A", "A, B", "A, B, C"]],
  ["en", {type: 'unit', style: 'narrow'}, ["", "A", "A B", "A B C"]],
  ["en", {style: 'short'}, ["", "A", "A & B", "A, B, & C"]],
  ["de", {}, ["", "A", "A und B", "A, B und C"]],
  ["de", {type: 'disjunction'}, ["", "A", "A oder B", "A, B oder C"]],
  ["de", {type: 'unit'}, ["", "A", "A, B", "A, B und C"]],
  ["de", {type: 'unit', style: 'narrow'}, ["", "A", "A, B", "A, B und C"]],
  ["de", {style: 'short'}, ["", "A", "A und B", "A, B und C"]],
  ["fr", {}, ["", "A", "A et B", "A, B et C"]],
  ["fr", {type: 'disjunction'}, ["", "A", "A ou B", "A, B ou C"]],
  ["fr", {type: 'unit'}, ["", "A", "A et B", "A, B et C"]],
  ["fr", {type: 'unit', style: 'narrow'}, ["", "A", "A B", "A B C"]],
  ["fr", {style: 'short'}, ["", "A", "A et B", "A, B et C"]],
  ["ja", {}, ["", "A", "A\u3001B", "A\u3001B\u3001C"]],
  ["ja", {type: 'disjunction'}, ["", "A", "A\u307e\u305f\u306fB", "A\u3001B\u3001\u307e\u305f\u306fC"]],
  ["ja", {type: 'unit'}, ["", "A", "A B", "A B C"]],
  ["ja", {type: 'unit', style: 'narrow'}, ["", "A", "AB", "ABC"]],
  ["ja", {style: 'short'}, ["", "A", "A\u3001B", "A\u3001B\u3001C"]],
  ["ar", {}, ["", "A", "A \u0648B", "A \u0648B \u0648C"]],
  ["ar", {type: 'disjunction'}, ["", "A", "A \u0623\u0648 B", "A \u0623\u0648 B \u0623\u0648 C"]],
  ["ar", {type: 'unit'}, ["", "A", "A \u0648B", "A\u060c \u0648B\u060c \u0648C"]],
  ["ar", {type: 'unit', style: 'narrow'}, ["", "A", "A \u0648B", "A \u0648B \u0648C"]],
  ["ar", {style: 'short'}, ["", "A", "A \u0648B", "A \u0648B \u0648C"]],
  ["ru", {}, ["", "A", "A \u0438 B", "A, B \u0438 C"]],
  ["ru", {type: 'disjunction'}, ["", "A", "A \u0438\u043b\u0438 B", "A, B \u0438\u043b\u0438 C"]],
  ["ru", {type: 'unit'}, ["", "A", "A B", "A B C"]],
  ["ru", {type: 'unit', style: 'narrow'}, ["", "A", "A B", "A B C"]],
  ["ru", {style: 'short'}, ["", "A", "A \u0438 B", "A, B \u0438 C"]],
  ["pl", {}, ["", "A", "A i B", "A, B i C"]],
  ["pl", {type: 'disjunction'}, ["", "A", "A lub B", "A, B lub C"]],
  ["pl", {type: 'unit'}, ["", "A", "A i B", "A, B i C"]],
  ["pl", {type: 'unit', style: 'narrow'}, ["", "A", "A i B", "A, B i C"]],
  ["pl", {style: 'short'}, ["", "A", "A i B", "A, B i C"]],
  ["zh", {}, ["", "A", "A\u548cB", "A\u3001B\u548cC"]],
  ["zh", {type: 'disjunction'}, ["", "A", "A\u6216B", "A\u3001B\u6216C"]],
  ["zh", {type: 'unit'}, ["", "A", "AB", "ABC"]],
  ["zh", {type: 'unit', style: 'narrow'}, ["", "A", "AB", "ABC"]],
  ["zh", {style: 'short'}, ["", "A", "A\u548cB", "A\u3001B\u548cC"]],
  ["es", {}, ["", "A", "A y B", "A, B y C"]],
  ["es", {type: 'disjunction'}, ["", "A", "A o B", "A, B o C"]],
  ["es", {type: 'unit'}, ["", "A", "A y B", "A, B y C"]],
  ["es", {type: 'unit', style: 'narrow'}, ["", "A", "A B", "A B C"]],
  ["es", {style: 'short'}, ["", "A", "A y B", "A, B y C"]],
  ["he", {}, ["", "A", "A \u05d5-B", "A, B \u05d5-C"]],
  ["he", {type: 'disjunction'}, ["", "A", "A \u05d0\u05d5 B", "A, B \u05d0\u05d5 C"]],
  ["he", {type: 'unit'}, ["", "A", "A, B", "A, B \u05d5-C"]],
  ["he", {type: 'unit', style: 'narrow'}, ["", "A", "A B", "A B C"]],
  ["he", {style: 'short'}, ["", "A", "A \u05d5-B", "A, B \u05d5-C"]],
];
for (const [locale, options, want] of LF_TABLE) {
  test('sec-formatlist', locale + ' ' + JSON.stringify(options), () => {
    const lf = new LF(locale, options);
    for (let i = 0; i < want.length; i++) {
      assert.sameValue(lf.format(LF_LISTS[i]), want[i], JSON.stringify(LF_LISTS[i]));
      assert.sameValue(joinParts(lf.formatToParts(LF_LISTS[i])), want[i], 'formatToParts');
    }
  });
}
