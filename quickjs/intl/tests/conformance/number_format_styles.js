// Intl.NumberFormat's currency and unit styles, the notations and the
// ranges, per ECMA-402's SetNumberFormatUnitOptions, the notation
// sub-patterns, PartitionNumberRangePattern and FormatApproximately.

const NFS = Intl.NumberFormat;

test('sec-setnumberformatunitoptions', 'currency: required, well formed, upper-cased', () => {
  assert.throws(TypeError, () => new NFS('en', { style: 'currency' }));
  for (const bad of ['', 'US', 'USDX', 'US$', '\u00e9ur', '123', 'u\u0131d']) {
    assert.throws(RangeError, () => new NFS('en', { style: 'currency', currency: bad }), show(bad));
  }
  assert.sameValue(new NFS('en', { style: 'currency', currency: 'usd' }).resolvedOptions().currency, 'USD');
  assert.sameValue(new NFS('en', { style: 'currency', currency: 'XYZ' }).format(1).includes('XYZ'), true, 'an unknown code is its own symbol');
  assert.throws(RangeError, () => new NFS('en', { currency: 'US' }), 'validated without the currency style');
  assert.sameValue('currency' in new NFS('en', { currency: 'USD' }).resolvedOptions(), false);
});

test('sec-setnumberformatunitoptions', 'unit: required, sanctioned or a per-compound', () => {
  assert.throws(TypeError, () => new NFS('en', { style: 'unit' }));
  for (const bad of ['', 'meters', 'Meter', 'kilometer-per-', '-per-hour', 'meter-per-kilometer-per-second', 'parsec', 'per-second']) {
    assert.throws(RangeError, () => new NFS('en', { style: 'unit', unit: bad }), show(bad));
  }
  for (const good of ['meter', 'kilometer-per-hour', 'percent-per-second', 'mile-per-gallon', 'byte']) {
    assert.sameValue(new NFS('en', { style: 'unit', unit: good }).resolvedOptions().unit, good);
  }
  assert.throws(RangeError, () => new NFS('en', { unit: 'parsec' }), 'validated without the unit style');
});

test('sec-intl.numberformat.prototype.resolvedoptions', 'currency, unit and compact keys', () => {
  const cur = new NFS('en', { style: 'currency', currency: 'usd', currencyDisplay: 'narrowSymbol' }).resolvedOptions();
  assert.deepEqual(Object.keys(cur), ['locale', 'numberingSystem', 'style', 'currency', 'currencyDisplay', 'currencySign',
    'minimumIntegerDigits', 'minimumFractionDigits', 'maximumFractionDigits', 'useGrouping', 'notation', 'signDisplay',
    'roundingIncrement', 'roundingMode', 'roundingPriority', 'trailingZeroDisplay']);
  const unit = new NFS('en', { style: 'unit', unit: 'meter' }).resolvedOptions();
  assert.sameValue(unit.unitDisplay, 'short');
  assert.sameValue(unit.maximumFractionDigits, 3);
  const compact = new NFS('en', { notation: 'compact' }).resolvedOptions();
  assert.deepEqual([compact.compactDisplay, compact.useGrouping, compact.roundingPriority,
    compact.minimumSignificantDigits, compact.maximumSignificantDigits, compact.maximumFractionDigits],
  ['short', 'min2', 'morePrecision', 1, 2, 0]);
  assert.sameValue('compactDisplay' in new NFS('en', { compactDisplay: 'long' }).resolvedOptions(), false);
});

test('sec-currencydigits', 'currency digits set the fraction defaults', () => {
  const digits = (currency) => new NFS('en', { style: 'currency', currency }).resolvedOptions().maximumFractionDigits;
  assert.sameValue(digits('USD'), 2);
  assert.sameValue(digits('JPY'), 0);
  assert.sameValue(digits('KWD'), 3);
  assert.sameValue(new NFS('en', { style: 'currency', currency: 'JPY', maximumFractionDigits: 2 }).format(1.239), '\u00a51.24');
});

test('sec-partitionnumberpattern', 'formatToParts: currency, unit, scientific, compact', () => {
  const types = (o, x) => new NFS('en', o).formatToParts(x).map((p) => p.type).join(' ');
  assert.sameValue(types({ style: 'currency', currency: 'USD' }, -1.5), 'minusSign currency integer decimal fraction');
  assert.sameValue(types({ style: 'currency', currency: 'USD', currencySign: 'accounting' }, -1.5),
    'literal currency integer decimal fraction literal');
  assert.sameValue(types({ style: 'unit', unit: 'kilometer-per-hour', unitDisplay: 'long' }, -1.5),
    'minusSign integer decimal fraction literal unit');
  assert.sameValue(types({ notation: 'scientific' }, -0.00012),
    'minusSign integer decimal fraction exponentSeparator exponentMinusSign exponentInteger');
  assert.sameValue(types({ notation: 'compact' }, 1500), 'integer decimal fraction compact');
  assert.sameValue(types({ notation: 'engineering' }, 0), 'integer exponentSeparator exponentInteger');
});

test('sec-partitionnumberpattern', 'notations at the edges', () => {
  const f = (o, x) => new NFS('en', o).format(x);
  assert.sameValue(f({ notation: 'scientific' }, 0), '0E0');
  assert.sameValue(f({ notation: 'scientific' }, -0), '-0E0');
  assert.sameValue(f({ notation: 'scientific' }, Infinity), '\u221e');
  assert.sameValue(f({ notation: 'scientific' }, NaN), 'NaN');
  assert.sameValue(f({ notation: 'scientific' }, 9.9999), '1E1', 'rounding moves the exponent');
  assert.sameValue(f({ notation: 'engineering' }, 999999), '999.999E3');
  assert.sameValue(f({ notation: 'compact' }, 999999), '1M', 'rounding moves the magnitude');
  assert.sameValue(f({ notation: 'compact' }, 1e21), '1,000,000,000T');
  assert.sameValue(f({ notation: 'compact' }, -0.5), '-0.5');
  assert.sameValue(f({ notation: 'scientific' }, 5e-324), '5E-324', 'Number::toString gives the digits');
  assert.sameValue(f({ notation: 'scientific' }, 10n ** 400n), '1E400', 'a BigInt beyond the Number range');
});

only('rong', 'the reference keeps a string\'s exact value past the Number range', 'sec-tointlmathematicalvalue',
  'RoundMVResult under scientific notation', () => {
    assert.sameValue(new NFS('en', { notation: 'scientific' }).format('1.5e-400'), '0E0');
    assert.sameValue(new NFS('en', { notation: 'scientific' }).format('1.5e400'), '\u221e');
  });

const NF_STYLES = [
  ["en", {style: 'currency', currency: 'EUR'}, ["-\u20ac1,234.50", "\u20ac0.00", "\u20ac1.00", "\u20ac2.00", "\u20ac1,500,000.00"]],
  ["en", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-USD\u00a01,234.50", "USD\u00a00.00", "USD\u00a01.00", "USD\u00a02.00", "USD\u00a01,500,000.00"]],
  ["en", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1,235 Japanese yen", "0 Japanese yen", "1 Japanese yen", "2 Japanese yen", "1,500,000 Japanese yen"]],
  ["en", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["($1,234.50)", "$0.00", "$1.00", "$2.00", "$1,500,000.00"]],
  ["en", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1,234.5 km/h", "0 km/h", "1 km/h", "2 km/h", "1,500,000 km/h"]],
  ["en", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1,234.5 liters", "0 liters", "1 liter", "2 liters", "1,500,000 liters"]],
  ["en", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1,234.5B", "0B", "1B", "2B", "1,500,000B"]],
  ["en", {notation: 'compact'}, ["-1.2K", "0", "1", "2", "1.5M"]],
  ["en", {notation: 'compact', compactDisplay: 'long'}, ["-1.2 thousand", "0", "1", "2", "1.5 million"]],
  ["en", {notation: 'scientific'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["en", {notation: 'engineering'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["de", {style: 'currency', currency: 'EUR'}, ["-1.234,50\u00a0\u20ac", "0,00\u00a0\u20ac", "1,00\u00a0\u20ac", "2,00\u00a0\u20ac", "1.500.000,00\u00a0\u20ac"]],
  ["de", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-1.234,50\u00a0USD", "0,00\u00a0USD", "1,00\u00a0USD", "2,00\u00a0USD", "1.500.000,00\u00a0USD"]],
  ["de", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1.235 Japanische Yen", "0 Japanische Yen", "1 Japanischer Yen", "2 Japanische Yen", "1.500.000 Japanische Yen"]],
  ["de", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["-1.234,50\u00a0$", "0,00\u00a0$", "1,00\u00a0$", "2,00\u00a0$", "1.500.000,00\u00a0$"]],
  ["de", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1.234,5 km/h", "0 km/h", "1 km/h", "2 km/h", "1.500.000 km/h"]],
  ["de", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1.234,5 Liter", "0 Liter", "1 Liter", "2 Liter", "1.500.000 Liter"]],
  ["de", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1.234,5 B", "0 B", "1 B", "2 B", "1.500.000 B"]],
  ["de", {notation: 'compact'}, ["-1235", "0", "1", "2", "1,5\u00a0Mio."]],
  ["de", {notation: 'compact', compactDisplay: 'long'}, ["-1,2 Tausend", "0", "1", "2", "1,5 Millionen"]],
  ["de", {notation: 'scientific'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["de", {notation: 'engineering'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["fr", {style: 'currency', currency: 'EUR'}, ["-1\u202f234,50\u00a0\u20ac", "0,00\u00a0\u20ac", "1,00\u00a0\u20ac", "2,00\u00a0\u20ac", "1\u202f500\u202f000,00\u00a0\u20ac"]],
  ["fr", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-1\u202f234,50\u00a0USD", "0,00\u00a0USD", "1,00\u00a0USD", "2,00\u00a0USD", "1\u202f500\u202f000,00\u00a0USD"]],
  ["fr", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1\u202f235 yens japonais", "0 yen japonais", "1 yen japonais", "2 yens japonais", "1\u202f500\u202f000 yens japonais"]],
  ["fr", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["(1\u202f234,50\u00a0$US)", "0,00\u00a0$US", "1,00\u00a0$US", "2,00\u00a0$US", "1\u202f500\u202f000,00\u00a0$US"]],
  ["fr", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1\u202f234,5\u202fkm/h", "0\u202fkm/h", "1\u202fkm/h", "2\u202fkm/h", "1\u202f500\u202f000\u202fkm/h"]],
  ["fr", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1\u202f234,5\u00a0litres", "0\u00a0litre", "1\u00a0litre", "2\u00a0litres", "1\u202f500\u202f000\u00a0litres"]],
  ["fr", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1\u202f234,5o", "0o", "1o", "2o", "1\u202f500\u202f000o"]],
  ["fr", {notation: 'compact'}, ["-1,2\u00a0k", "0", "1", "2", "1,5\u00a0M"]],
  ["fr", {notation: 'compact', compactDisplay: 'long'}, ["-1,2 millier", "0", "1", "2", "1,5 million"]],
  ["fr", {notation: 'scientific'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["fr", {notation: 'engineering'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["ja", {style: 'currency', currency: 'EUR'}, ["-\u20ac1,234.50", "\u20ac0.00", "\u20ac1.00", "\u20ac2.00", "\u20ac1,500,000.00"]],
  ["ja", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-USD\u00a01,234.50", "USD\u00a00.00", "USD\u00a01.00", "USD\u00a02.00", "USD\u00a01,500,000.00"]],
  ["ja", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1,235\u5186", "0\u5186", "1\u5186", "2\u5186", "1,500,000\u5186"]],
  ["ja", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["($1,234.50)", "$0.00", "$1.00", "$2.00", "$1,500,000.00"]],
  ["ja", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1,234.5 km/h", "0 km/h", "1 km/h", "2 km/h", "1,500,000 km/h"]],
  ["ja", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1,234.5 \u30ea\u30c3\u30c8\u30eb", "0 \u30ea\u30c3\u30c8\u30eb", "1 \u30ea\u30c3\u30c8\u30eb", "2 \u30ea\u30c3\u30c8\u30eb", "1,500,000 \u30ea\u30c3\u30c8\u30eb"]],
  ["ja", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1,234.5B", "0B", "1B", "2B", "1,500,000B"]],
  ["ja", {notation: 'compact'}, ["-1235", "0", "1", "2", "150\u4e07"]],
  ["ja", {notation: 'compact', compactDisplay: 'long'}, ["-1235", "0", "1", "2", "150\u4e07"]],
  ["ja", {notation: 'scientific'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["ja", {notation: 'engineering'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["ar", {style: 'currency', currency: 'EUR'}, ["\u200f\u200e-1,234.50\u00a0\u20ac", "\u200f0.00\u00a0\u20ac", "\u200f1.00\u00a0\u20ac", "\u200f2.00\u00a0\u20ac", "\u200f1,500,000.00\u00a0\u20ac"]],
  ["ar", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["\u200f\u200e-1,234.50\u00a0USD", "\u200f0.00\u00a0USD", "\u200f1.00\u00a0USD", "\u200f2.00\u00a0USD", "\u200f1,500,000.00\u00a0USD"]],
  ["ar", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["\u200e-1,235 \u064a\u0646 \u064a\u0627\u0628\u0627\u0646\u064a", "0 \u064a\u0646 \u064a\u0627\u0628\u0627\u0646\u064a", "1 \u064a\u0646 \u064a\u0627\u0628\u0627\u0646\u064a", "2 \u064a\u0646 \u064a\u0627\u0628\u0627\u0646\u064a", "1,500,000 \u064a\u0646 \u064a\u0627\u0628\u0627\u0646\u064a"]],
  ["ar", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["(\u061c1,234.50\u00a0US$)", "\u061c0.00\u00a0US$", "\u061c1.00\u00a0US$", "\u061c2.00\u00a0US$", "\u061c1,500,000.00\u00a0US$"]],
  ["ar", {style: 'unit', unit: 'kilometer-per-hour'}, ["\u200e-1,234.5 \u0643\u0645/\u0633", "0 \u0643\u0645/\u0633", "1 \u0643\u0645/\u0633", "2 \u0643\u0645/\u0633", "1,500,000 \u0643\u0645/\u0633"]],
  ["ar", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["\u200e-1,234.5 \u0644\u062a\u0631", "0 \u0644\u062a\u0631", "\u0644\u062a\u0631", "2 \u0644\u062a\u0631", "1,500,000 \u0644\u062a\u0631"]],
  ["ar", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["\u200e-1,234.5 \u0628", "0 \u0628", "1 \u0628", "2 \u0628", "1,500,000 \u0628"]],
  ["ar", {notation: 'compact'}, ["\u200e-1.2\u00a0\u0623\u0644\u0641", "0", "1", "2", "1.5\u00a0\u0645\u0644\u064a\u0648\u0646"]],
  ["ar", {notation: 'compact', compactDisplay: 'long'}, ["\u200e-1.2 \u0623\u0644\u0641", "0", "1", "2", "1.5 \u0645\u0644\u064a\u0648\u0646"]],
  ["ar", {notation: 'scientific'}, ["\u200e-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["ar", {notation: 'engineering'}, ["\u200e-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["hi", {style: 'currency', currency: 'EUR'}, ["-\u20ac1,234.50", "\u20ac0.00", "\u20ac1.00", "\u20ac2.00", "\u20ac15,00,000.00"]],
  ["hi", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-USD\u00a01,234.50", "USD\u00a00.00", "USD\u00a01.00", "USD\u00a02.00", "USD\u00a015,00,000.00"]],
  ["hi", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1,235 \u091c\u093e\u092a\u093e\u0928\u0940 \u092f\u0947\u0928", "0 \u091c\u093e\u092a\u093e\u0928\u0940 \u092f\u0947\u0928", "1 \u091c\u093e\u092a\u093e\u0928\u0940 \u092f\u0947\u0928", "2 \u091c\u093e\u092a\u093e\u0928\u0940 \u092f\u0947\u0928", "15,00,000 \u091c\u093e\u092a\u093e\u0928\u0940 \u092f\u0947\u0928"]],
  ["hi", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["-$1,234.50", "$0.00", "$1.00", "$2.00", "$15,00,000.00"]],
  ["hi", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1,234.5 \u0915\u093f\u0970\u092e\u0940\u0970/\u0918\u0902\u0970", "0 \u0915\u093f\u0970\u092e\u0940\u0970/\u0918\u0902\u0970", "1 \u0915\u093f\u0970\u092e\u0940\u0970/\u0918\u0902\u0970", "2 \u0915\u093f\u0970\u092e\u0940\u0970/\u0918\u0902\u0970", "15,00,000 \u0915\u093f\u0970\u092e\u0940\u0970/\u0918\u0902\u0970"]],
  ["hi", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1,234.5 \u0932\u0940\u091f\u0930", "0 \u0932\u0940\u091f\u0930", "1 \u0932\u0940\u091f\u0930", "2 \u0932\u0940\u091f\u0930", "15,00,000 \u0932\u0940\u091f\u0930"]],
  ["hi", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1,234.5 \u092c\u093e\u0907\u091f", "0 \u092c\u093e\u0907\u091f", "1 \u092c\u093e\u0907\u091f", "2 \u092c\u093e\u0907\u091f", "15,00,000 \u092c\u093e\u0907\u091f"]],
  ["hi", {notation: 'compact'}, ["-1.2\u00a0\u0939\u091c\u093c\u093e\u0930", "0", "1", "2", "15\u00a0\u0932\u093e\u0916"]],
  ["hi", {notation: 'compact', compactDisplay: 'long'}, ["-1.2 \u0939\u091c\u093c\u093e\u0930", "0", "1", "2", "15 \u0932\u093e\u0916"]],
  ["hi", {notation: 'scientific'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["hi", {notation: 'engineering'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["ru", {style: 'currency', currency: 'EUR'}, ["-1\u00a0234,50\u00a0\u20ac", "0,00\u00a0\u20ac", "1,00\u00a0\u20ac", "2,00\u00a0\u20ac", "1\u00a0500\u00a0000,00\u00a0\u20ac"]],
  ["ru", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-1\u00a0234,50\u00a0USD", "0,00\u00a0USD", "1,00\u00a0USD", "2,00\u00a0USD", "1\u00a0500\u00a0000,00\u00a0USD"]],
  ["ru", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1\u00a0235 \u044f\u043f\u043e\u043d\u0441\u043a\u0438\u0445 \u0438\u0435\u043d", "0 \u044f\u043f\u043e\u043d\u0441\u043a\u0438\u0445 \u0438\u0435\u043d", "1 \u044f\u043f\u043e\u043d\u0441\u043a\u0430\u044f \u0438\u0435\u043d\u0430", "2 \u044f\u043f\u043e\u043d\u0441\u043a\u0438\u0435 \u0438\u0435\u043d\u044b", "1\u00a0500\u00a0000 \u044f\u043f\u043e\u043d\u0441\u043a\u0438\u0445 \u0438\u0435\u043d"]],
  ["ru", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["-1\u00a0234,50\u00a0$", "0,00\u00a0$", "1,00\u00a0$", "2,00\u00a0$", "1\u00a0500\u00a0000,00\u00a0$"]],
  ["ru", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1\u00a0234,5 \u043a\u043c/\u0447", "0 \u043a\u043c/\u0447", "1 \u043a\u043c/\u0447", "2 \u043a\u043c/\u0447", "1\u00a0500\u00a0000 \u043a\u043c/\u0447"]],
  ["ru", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1\u00a0234,5 \u043b\u0438\u0442\u0440\u0430", "0 \u043b\u0438\u0442\u0440\u043e\u0432", "1 \u043b\u0438\u0442\u0440", "2 \u043b\u0438\u0442\u0440\u0430", "1\u00a0500\u00a0000 \u043b\u0438\u0442\u0440\u043e\u0432"]],
  ["ru", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1\u00a0234,5 \u0411", "0 \u0411", "1 \u0411", "2 \u0411", "1\u00a0500\u00a0000 \u0411"]],
  ["ru", {notation: 'compact'}, ["-1,2\u00a0\u0442\u044b\u0441.", "0", "1", "2", "1,5\u00a0\u043c\u043b\u043d"]],
  ["ru", {notation: 'compact', compactDisplay: 'long'}, ["-1,2 \u0442\u044b\u0441\u044f\u0447\u0438", "0", "1", "2", "1,5 \u043c\u0438\u043b\u043b\u0438\u043e\u043d\u0430"]],
  ["ru", {notation: 'scientific'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["ru", {notation: 'engineering'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["pl", {style: 'currency', currency: 'EUR'}, ["-1234,50\u00a0\u20ac", "0,00\u00a0\u20ac", "1,00\u00a0\u20ac", "2,00\u00a0\u20ac", "1\u00a0500\u00a0000,00\u00a0\u20ac"]],
  ["pl", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-1234,50\u00a0USD", "0,00\u00a0USD", "1,00\u00a0USD", "2,00\u00a0USD", "1\u00a0500\u00a0000,00\u00a0USD"]],
  ["pl", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1235 jen\u00f3w japo\u0144skich", "0 jen\u00f3w japo\u0144skich", "1 jen japo\u0144ski", "2 jeny japo\u0144skie", "1\u00a0500\u00a0000 jen\u00f3w japo\u0144skich"]],
  ["pl", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["(1234,50\u00a0USD)", "0,00\u00a0USD", "1,00\u00a0USD", "2,00\u00a0USD", "1\u00a0500\u00a0000,00\u00a0USD"]],
  ["pl", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1234,5 km/godz.", "0 km/godz.", "1 km/godz.", "2 km/godz.", "1\u00a0500\u00a0000 km/godz."]],
  ["pl", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1234,5 litra", "0 litr\u00f3w", "1 litr", "2 litry", "1\u00a0500\u00a0000 litr\u00f3w"]],
  ["pl", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1234,5 B", "0 B", "1 B", "2 B", "1\u00a0500\u00a0000 B"]],
  ["pl", {notation: 'compact'}, ["-1,2\u00a0tys.", "0", "1", "2", "1,5\u00a0mln"]],
  ["pl", {notation: 'compact', compactDisplay: 'long'}, ["-1,2 tysi\u0105ca", "0", "1", "2", "1,5 miliona"]],
  ["pl", {notation: 'scientific'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["pl", {notation: 'engineering'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["zh", {style: 'currency', currency: 'EUR'}, ["-\u20ac1,234.50", "\u20ac0.00", "\u20ac1.00", "\u20ac2.00", "\u20ac1,500,000.00"]],
  ["zh", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-USD\u00a01,234.50", "USD\u00a00.00", "USD\u00a01.00", "USD\u00a02.00", "USD\u00a01,500,000.00"]],
  ["zh", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1,235\u65e5\u5143", "0\u65e5\u5143", "1\u65e5\u5143", "2\u65e5\u5143", "1,500,000\u65e5\u5143"]],
  ["zh", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["(US$1,234.50)", "US$0.00", "US$1.00", "US$2.00", "US$1,500,000.00"]],
  ["zh", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1,234.5 km/h", "0 km/h", "1 km/h", "2 km/h", "1,500,000 km/h"]],
  ["zh", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1,234.5\u5347", "0\u5347", "1\u5347", "2\u5347", "1,500,000\u5347"]],
  ["zh", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1,234.5 B", "0 B", "1 B", "2 B", "1,500,000 B"]],
  ["zh", {notation: 'compact'}, ["-1235", "0", "1", "2", "150\u4e07"]],
  ["zh", {notation: 'compact', compactDisplay: 'long'}, ["-1235", "0", "1", "2", "150\u4e07"]],
  ["zh", {notation: 'scientific'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["zh", {notation: 'engineering'}, ["-1.235E3", "0E0", "1E0", "2E0", "1.5E6"]],
  ["es", {style: 'currency', currency: 'EUR'}, ["-1234,50\u00a0\u20ac", "0,00\u00a0\u20ac", "1,00\u00a0\u20ac", "2,00\u00a0\u20ac", "1.500.000,00\u00a0\u20ac"]],
  ["es", {style: 'currency', currency: 'USD', currencyDisplay: 'code'}, ["-1234,50\u00a0USD", "0,00\u00a0USD", "1,00\u00a0USD", "2,00\u00a0USD", "1.500.000,00\u00a0USD"]],
  ["es", {style: 'currency', currency: 'JPY', currencyDisplay: 'name'}, ["-1235 yenes japoneses", "0 yenes japoneses", "1 yen japon\u00e9s", "2 yenes japoneses", "1.500.000 yenes japoneses"]],
  ["es", {style: 'currency', currency: 'USD', currencySign: 'accounting'}, ["-1234,50\u00a0US$", "0,00\u00a0US$", "1,00\u00a0US$", "2,00\u00a0US$", "1.500.000,00\u00a0US$"]],
  ["es", {style: 'unit', unit: 'kilometer-per-hour'}, ["-1234,5 km/h", "0 km/h", "1 km/h", "2 km/h", "1.500.000 km/h"]],
  ["es", {style: 'unit', unit: 'liter', unitDisplay: 'long'}, ["-1234,5 litros", "0 litros", "1 litro", "2 litros", "1.500.000 litros"]],
  ["es", {style: 'unit', unit: 'byte', unitDisplay: 'narrow'}, ["-1234,5B", "0B", "1B", "2B", "1.500.000B"]],
  ["es", {notation: 'compact'}, ["-1,2\u00a0mil", "0", "1", "2", "1,5\u00a0M"]],
  ["es", {notation: 'compact', compactDisplay: 'long'}, ["-1,2 mil", "0", "1", "2", "1,5 millones"]],
  ["es", {notation: 'scientific'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
  ["es", {notation: 'engineering'}, ["-1,235E3", "0E0", "1E0", "2E0", "1,5E6"]],
];
const NF_STYLE_VALUES = [-1234.5, 0, 1, 2, 1500000];
// CLDR 46 made latn the default for ar and updated es's JPY currency name;
// CLDR 49 uses US$ for pl USD symbol.
for (const [locale, options, want] of NF_STYLES) {
  const variants = [];
  if (locale === 'ar' || (locale === 'es' && options.currencyDisplay === 'name')) {
    variants.push([{ cldrBelow: 46 }, SKIP_DATA, 'the ' + locale + ' row holds CLDR 46+ data']);
  }
  if (locale === 'pl' && options.currency === 'USD' && options.currencySign === 'accounting') {
    variants.push([
      { cldrAtLeast: 49 },
      ['(1234,50\u00a0US$)', '0,00\u00a0US$', '1,00\u00a0US$', '2,00\u00a0US$', '1\u00a0500\u00a0000,00\u00a0US$'],
      'CLDR 49 uses US$ as pl USD symbol',
    ]);
  }
  if (locale === 'ja' && options.unit === 'liter' && options.unitDisplay === 'long') {
    variants.push([
      { vendor: 'apple' },
      want.map((s) => s.replace(' \u30ea\u30c3\u30c8\u30eb', '\u30ea\u30c3\u30c8\u30eb')),
      'Apple ICU omits the space before リットル',
    ]);
  }
  if (locale === 'zh' && options.unit === 'kilometer-per-hour') {
    variants.push([
      { vendor: 'apple' },
      want.map((s) => s.replace(' km/h', '\u516c\u91cc/\u65f6')),
      'Apple ICU writes 公里/时 for zh kilometer-per-hour',
    ]);
  }
  dataTest('sec-formatnumber', locale + ' ' + JSON.stringify(options), want, variants, (expected) => {
    const nf = new NFS(locale, options);
    for (let i = 0; i < expected.length; i++) {
      assert.sameValue(nf.format(NF_STYLE_VALUES[i]), expected[i], show(NF_STYLE_VALUES[i]));
      assert.sameValue(joinParts(nf.formatToParts(NF_STYLE_VALUES[i])), expected[i], 'formatToParts');
    }
  });
}

test('sec-intl.numberformat.prototype.formatrange', 'attributes and arguments', () => {
  verifyMethod(NFS.prototype, 'formatRange', 2);
  verifyMethod(NFS.prototype, 'formatRangeToParts', 2);
  const nf = new NFS('en');
  assert.throws(TypeError, () => nf.formatRange(1));
  assert.throws(TypeError, () => nf.formatRange(undefined, 1));
  assert.throws(TypeError, () => nf.formatRangeToParts(1));
  assert.throws(RangeError, () => nf.formatRange(NaN, 1));
  assert.throws(RangeError, () => nf.formatRange(1, NaN));
  assert.throws(RangeError, () => nf.formatRangeToParts(1, 'x'));
  assert.throws(TypeError, () => NFS.prototype.formatRange.call({}, 1, 2));
  assert.throws(TypeError, () => nf.formatRange(Symbol(), 1));
});

test('sec-partitionnumberrangepattern', 'values', () => {
  const r = (o, x, y, l) => new NFS(l || 'en', o).formatRange(x, y);
  assert.sameValue(r({}, 3, 5), '3\u20135');
  assert.sameValue(r({}, 5, 3), '5\u20133', 'an end below the start is allowed');
  assert.sameValue(r({}, -5, -3), '-5 \u2013 -3');
  assert.sameValue(r({}, 1n, 10n ** 20n), '1\u2013100,000,000,000,000,000,000');
  assert.sameValue(r({ style: 'currency', currency: 'EUR' }, 3, 5), '\u20ac3.00 \u2013 \u20ac5.00');
  assert.sameValue(r({ style: 'currency', currency: 'EUR' }, 3, 5, 'de'), '3,00\u20135,00\u00a0\u20ac');
  assert.sameValue(r({ notation: 'compact' }, 3000, 5000), '3K \u2013 5K');
  assert.sameValue(r({}, 0, Infinity), '0\u2013\u221e');
  assert.sameValue(r({}, -Infinity, Infinity), '-\u221e \u2013 \u221e');
});

test('sec-formatapproximately', 'equal ends after rounding', () => {
  const r = (o, x, y) => new NFS('en', o).formatRange(x, y);
  assert.sameValue(r({}, 3, 3), '~3');
  assert.sameValue(r({ maximumFractionDigits: 0 }, 2.9, 3.1), '~3');
  assert.sameValue(r({ style: 'currency', currency: 'USD' }, 3, 3.001), '~$3.00');
  assert.deepEqual(new NFS('en').formatRangeToParts(3, 3).map((p) => [p.type, p.value, p.source]),
    [['approximatelySign', '~', 'shared'], ['integer', '3', 'shared']]);
});

test('sec-formatnumericrangetoparts', 'sources and keys', () => {
  const parts = new NFS('en', { style: 'currency', currency: 'USD' }).formatRangeToParts(3, 5);
  assert.deepEqual(parts.map((p) => [p.type, p.value, p.source]), [
    ['currency', '$', 'startRange'], ['integer', '3', 'startRange'], ['decimal', '.', 'startRange'], ['fraction', '00', 'startRange'],
    ['literal', ' \u2013 ', 'shared'],
    ['currency', '$', 'endRange'], ['integer', '5', 'endRange'], ['decimal', '.', 'endRange'], ['fraction', '00', 'endRange']]);
  for (const p of parts) assert.deepEqual(Object.keys(p), ['type', 'value', 'source']);
});
