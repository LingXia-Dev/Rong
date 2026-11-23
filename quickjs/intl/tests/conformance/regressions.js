// One named case for each defect found so far, so none comes back.

const PERCENT_SIGNS = [
  ['ar-EG', {}, '؜-٢٥٪؜'],
  ['en', { numberingSystem: 'arab' }, '؜-٢٥٪؜'],
  ['ar-EG', { numberingSystem: 'latn' }, '‎-25‎%‎'],
  ['fa', {}, '‎−۲۵٪'],
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
      [['minusSign', '؜-'], ['integer', '٢٥'], ['percentSign', '٪؜']]);
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
