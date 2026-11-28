// One named case for each defect found so far, so none comes back.

const PERCENT_SIGNS = [
  ['ar-EG', {}, '\u061c-\u0662\u0665\u066a\u061c'],
  ['en', { numberingSystem: 'arab' }, '\u061c-\u0662\u0665\u066a\u061c'],
  ['ar-EG', { numberingSystem: 'latn' }, '\u200e-25\u200e%\u200e'],
  ['fa', {}, '\u200e\u2212\u06f2\u06f5\u066a'],
];
for (const [locale, options, want] of PERCENT_SIGNS) {
  test('regression', 'percent sign of ' + locale + ' ' + JSON.stringify(options), () => {
    const nf = new Intl.NumberFormat(locale, Object.assign({ style: 'percent' }, options));
    assert.sameValue(nf.format(-0.25), want);
    assert.sameValue(joinParts(nf.formatToParts(-0.25)), want);
    assert.ok(nf.formatToParts(-0.25).some((p) => p.type === 'percentSign'));
  });
}

only('rong', 'ICU writes the bidi marks around a sign as literal parts; ICU4X\'s symbols carry them', 'regression',
  'the arab percent and minus signs carry their marks', () => {
    const parts = new Intl.NumberFormat('en', { style: 'percent', numberingSystem: 'arab' }).formatToParts(-0.25);
    assert.deepEqual(parts.map((p) => [p.type, p.value]),
      [['minusSign', '\u061c-'], ['integer', '\u0662\u0665'], ['percentSign', '\u066a\u061c']]);
  });

test('regression', 'a pt year and long month keep their digits, alone and in a range', () => {
  // ICU4X 2.4 loads no decimal formatter for this pattern (one variant quotes its year).
  const f = new Intl.DateTimeFormat('pt', { year: 'numeric', month: 'long', timeZone: 'UTC' });
  assert.sameValue(f.format(Date.UTC(2026, 0, 5)), 'janeiro de 2026');
  assert.deepEqual(f.formatToParts(Date.UTC(2026, 0, 5)).map((p) => p.type), ['month', 'literal', 'year']);
  const range = f.formatRangeToParts(Date.UTC(2026, 10, 5), Date.UTC(2026, 11, 5));
  assert.ok(range.some((p) => p.type === 'year' && p.value === '2026'));
  assert.ok(range.some((p) => p.type === 'month' && p.value === 'novembro' && p.source === 'startRange'));
  assert.ok(range.some((p) => p.type === 'month' && p.value === 'dezembro' && p.source === 'endRange'));
});

test('regression', 'NaN under signDisplay takes the positive-zero pattern', () => {
  const nan = (signDisplay) => new Intl.NumberFormat('en', { signDisplay }).format(NaN);
  assert.sameValue(nan('auto'), 'NaN');
  assert.sameValue(nan('always'), '+NaN');
  assert.sameValue(nan('exceptZero'), 'NaN');
  assert.sameValue(nan('negative'), 'NaN');
  assert.sameValue(nan('never'), 'NaN');
});

test('regression', 'the default locale is one every constructor supports, and they agree', () => {
  const nf = new Intl.NumberFormat().resolvedOptions().locale;
  const pr = new Intl.PluralRules().resolvedOptions().locale;
  assert.sameValue(nf, pr);
  assert.deepEqual(Intl.NumberFormat.supportedLocalesOf(nf), [nf]);
  assert.deepEqual(Intl.PluralRules.supportedLocalesOf(pr), [pr]);
  assert.sameValue(new Intl.NumberFormat([]).resolvedOptions().locale, nf);
  assert.sameValue(new Intl.NumberFormat(undefined).resolvedOptions().locale, nf);
});

test('regression', 'a long non-decimal literal is bounded, and returns', () => {
  const nf = new Intl.NumberFormat('en', { useGrouping: false });
  assert.sameValue(nf.format('0x' + 'f'.repeat(64)), BigInt('0x' + 'f'.repeat(64)).toString());
  assert.sameValue(nf.format('0b' + '1'.repeat(200000)), nf.format('0b' + '1'.repeat(200000)));
  assert.sameValue(nf.format('0x'), 'NaN');
  assert.sameValue(nf.format('-0x10'), 'NaN', 'no sign on a non-decimal literal');
  assert.sameValue(new Intl.PluralRules('en').select('0x1'), 'one');
});

test('regression', 'a Symbol option is a TypeError, not a crash', () => {
  assert.throws(TypeError, () => new Intl.NumberFormat('en', { style: Symbol('percent') }));
  assert.throws(TypeError, () => new Intl.NumberFormat('en', { minimumIntegerDigits: Symbol() }));
  assert.throws(TypeError, () => new Intl.NumberFormat('en', { useGrouping: Symbol() }));
  assert.throws(TypeError, () => new Intl.PluralRules('en', { type: Symbol() }));
  assert.throws(TypeError, () => new Intl.PluralRules([Symbol()]));
  assert.throws(TypeError, () => Intl.NumberFormat.supportedLocalesOf('en', { localeMatcher: Symbol() }));
});

test('regression', 'minimumIntegerDigits pads to the full count', () => {
  assert.sameValue(new Intl.NumberFormat('en', { minimumIntegerDigits: 3 }).format(5), '005');
  assert.sameValue(new Intl.NumberFormat('en', { minimumIntegerDigits: 3 }).format(1234), '1,234');
  assert.sameValue(new Intl.NumberFormat('en', { minimumIntegerDigits: 5 }).format(-5), '-00,005');
  assert.sameValue(new Intl.NumberFormat('en', { minimumIntegerDigits: 2, minimumFractionDigits: 1 }).format(0.5), '00.5');
  assert.sameValue(new Intl.NumberFormat('en', { minimumIntegerDigits: 4, useGrouping: false }).format(0), '0000');
  assert.sameValue(new Intl.NumberFormat('en', { minimumIntegerDigits: 3, style: 'percent' }).format(0.05), '005%');
});

test('regression', 'slots left unset by the digit options are read as absent', () => {
  const nf = new Intl.NumberFormat('en', { maximumSignificantDigits: 2 });
  assert.sameValue(nf.format(1234), '1,200');
  const o = nf.resolvedOptions();
  assert.sameValue('minimumFractionDigits' in o, false);
  assert.sameValue('maximumFractionDigits' in o, false);
  const pr = new Intl.PluralRules('en', { minimumSignificantDigits: 2 });
  assert.sameValue(pr.select(1), 'other', '1.0 shows a fraction digit');
  assert.sameValue('minimumFractionDigits' in pr.resolvedOptions(), false);
});

test('regression', 'GetOption converts with ToString once', () => {
  let calls = 0;
  const nf = new Intl.NumberFormat('en', { style: { toString() { calls++; return 'percent'; } } });
  assert.sameValue(calls, 1);
  assert.sameValue(nf.format(0.5), '50%');
});

test('regression', 'percent digits default to zero fraction digits', () => {
  assert.sameValue(new Intl.NumberFormat('en', { style: 'percent' }).format(0.1234), '12%');
  assert.sameValue(new Intl.NumberFormat('en', { style: 'percent', maximumFractionDigits: 2 }).format(0.123456), '12.35%');
});

test('regression', 'a BigInt is exact, never an infinity of RoundMVResult', () => {
  const nf = new Intl.NumberFormat('en', { useGrouping: false });
  const big = 10n ** 400n + 7n;
  assert.sameValue(nf.format(big), String(big));
  assert.sameValue(nf.format(-big), String(-big));
  assert.sameValue(nf.format('n1'), 'NaN', 'a string starting with n is not a BigInt');
});

test('regression', 'every integer digit of a long numeric string is kept', () => {
  const nf = new Intl.NumberFormat('en', { useGrouping: false, maximumFractionDigits: 2 });
  const digits = '1' + '23456789'.repeat(37);
  assert.sameValue(nf.format(digits), digits);
  assert.sameValue(nf.format(digits + '.125'), digits + '.13');
});

only('rong', 'the reference reads select\'s value with ToNumber, from before ToIntlMathematicalValue', 'sec-intl.pluralrules.prototype.select',
  'PluralRules selects a BigInt exactly', () => {
    const pr = new Intl.PluralRules('en');
    assert.sameValue(pr.select(1n), 'one');
    assert.sameValue(pr.select(10n ** 400n + 1n), 'other');
  });

only('rong', 'the reference has no such bound', 'regression', 'a BigInt past 32767 digits is an infinity (implementation limit)', () => {
  assert.sameValue(new Intl.NumberFormat('en').format(10n ** 40000n), '\u221e');
});
