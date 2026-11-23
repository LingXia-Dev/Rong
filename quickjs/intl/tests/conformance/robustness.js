// Hostile and odd inputs: each must throw the right error or return, never
// crash, hang or grow without bound. Collection after drop, several
// contexts and a blob without a constructor's data are checked from Rust
// (conformance.rs, data_errors.rs).

const SERVICES = [
  ['NumberFormat', (l, o) => new Intl.NumberFormat(l, o), (f) => f.format(1234.5)],
  ['PluralRules', (l, o) => new Intl.PluralRules(l, o), (f) => f.select(1)],
];

for (const [name, make, use] of SERVICES) {
  test('robustness', name + ': a very long tag is a RangeError', () => {
    assert.throws(RangeError, () => make('en-' + 'a'.repeat(1 << 20)));
    assert.throws(RangeError, () => make('a'.repeat(1 << 20)));
  });

  test('robustness', name + ': many valid subtags', () => {
    const tag = 'en-x-' + Array.from({ length: 2000 }, (_, i) => 'p' + (i % 1000)).join('-');
    use(make(tag));
    use(make('de-' + Array.from({ length: 8 }, (_, i) => 'v' + String(1000 + i)).join('-')));
  });

  only('rong', 'the reference caps the variants of a tag', 'robustness', name + ': hundreds of variants', () => {
    use(make('de-' + Array.from({ length: 300 }, (_, i) => 'v' + String(1000 + i)).join('-')));
  });

  test('robustness', name + ': lone surrogates and null bytes in tags and options', () => {
    for (const bad of ['en-\ud800', '\udc00', 'en\u0000', 'en-US\u0000x', 'e\ud83dn']) {
      assert.throws(RangeError, () => make(bad), JSON.stringify(bad));
    }
    assert.throws(RangeError, () => make('en', { localeMatcher: 'lookup\u0000' }));
    assert.throws(RangeError, () => make('en', { localeMatcher: '\ud800' }));
  });

  test('robustness', name + ': a very long option value is a RangeError', () => {
    assert.throws(RangeError, () => make('en', { localeMatcher: 'x'.repeat(1 << 20) }));
    assert.throws(RangeError, () => make('en', { roundingMode: 'y'.repeat(1 << 20) }));
  });

  test('robustness', name + ': cyclic and deeply nested option objects', () => {
    const cyclic = {};
    cyclic.self = cyclic;
    cyclic.localeMatcher = 'lookup';
    use(make('en', cyclic));
    let deep = {};
    for (let i = 0; i < 10000; i++) deep = { next: deep };
    use(make('en', deep));
    // A value whose conversion never reaches a primitive.
    assert.throws(TypeError, () => make('en', { localeMatcher: { toString: () => ({}), valueOf: () => ({}) } }));
  });

  test('robustness', name + ': a Proxy as options sees each trap it should', () => {
    const traps = [];
    const proxy = new Proxy({}, {
      get(t, k) { traps.push('get'); return undefined; },
      has(t, k) { traps.push('has'); return false; },
      ownKeys() { traps.push('ownKeys'); return []; },
      getOwnPropertyDescriptor() { traps.push('gopd'); return undefined; },
    });
    use(make('en', proxy));
    assert.ok(traps.length > 0 && traps.every((t) => t === 'get'), 'only [[Get]]: ' + traps.join(','));
  });

  test('robustness', name + ': a revoked Proxy as options is a TypeError', () => {
    const { proxy, revoke } = Proxy.revocable({}, {});
    revoke();
    assert.throws(TypeError, () => make('en', proxy));
    assert.throws(TypeError, () => make(proxy));
  });

  test('robustness', name + ': options mutated while being read', () => {
    const options = {
      get localeMatcher() {
        delete this.minimumIntegerDigits;
        this.maximumFractionDigits = 1;
        return 'lookup';
      },
      minimumIntegerDigits: 50,
    };
    const f = make('en', options);
    assert.sameValue(f.resolvedOptions().minimumIntegerDigits, 1, 'the deleted option is not read');
    assert.sameValue(f.resolvedOptions().maximumFractionDigits, 1, 'the added option is read');
  });

  test('robustness', name + ': re-entrant construction from an option getter', () => {
    let inner;
    const f = make('en', {
      get localeMatcher() {
        inner = make('fr', { get localeMatcher() { return 'lookup'; } });
        return 'best fit';
      },
    });
    use(f);
    use(inner);
    assert.sameValue(inner.resolvedOptions().locale, 'fr');
  });

  test('robustness', name + ': prototype pollution reaches GetOption only', () => {
    const saved = Object.getOwnPropertyDescriptor(Object.prototype, 'minimumIntegerDigits');
    Object.prototype.minimumIntegerDigits = 4;
    try {
      assert.sameValue(make('en', {}).resolvedOptions().minimumIntegerDigits, 4, 'options inherit, per [[Get]]');
      assert.sameValue(make('en').resolvedOptions().minimumIntegerDigits, 1, 'no options object, nothing read');
      use(make('en', {}));
    } finally {
      delete Object.prototype.minimumIntegerDigits;
      if (saved) Object.defineProperty(Object.prototype, 'minimumIntegerDigits', saved);
    }
  });

  test('robustness', name + ': thousands of formatters created and dropped', () => {
    const tags = ['en', 'fr', 'ar', 'ja', 'de-CH', 'hi', 'fa', 'zh-Hant'];
    for (let i = 0; i < 3000; i++) {
      const f = make(tags[i % tags.length], { minimumFractionDigits: i % 5, roundingMode: i % 2 ? 'floor' : 'ceil' });
      use(f);
    }
  });
}

test('robustness', 'NumberFormat: re-entrant format from valueOf', () => {
  const nf = new Intl.NumberFormat('en');
  let inner;
  const outer = nf.format({ valueOf() { inner = nf.format(2); return 1; } });
  assert.sameValue(outer, '1');
  assert.sameValue(inner, '2');
  const pr = new Intl.PluralRules('en');
  assert.sameValue(pr.select({ valueOf() { pr.select(5); return 1; } }), 'one');
});

test('robustness', 'NumberFormat: a getter calling resolvedOptions on a half-built formatter', () => {
  const nf = new Intl.NumberFormat('en');
  let seen;
  new Intl.NumberFormat('en', { get style() { seen = nf.resolvedOptions().style; return 'percent'; } });
  assert.sameValue(seen, 'decimal');
});

only('rong', 'the reference formats a non-decimal literal past the Number range exactly', 'sec-tointlmathematicalvalue',
  'RoundMVResult: a long hex literal is an infinity', () => {
    const nf = new Intl.NumberFormat('en');
    assert.sameValue(nf.format('0x' + 'f'.repeat(1000)), '∞');
    assert.sameValue(nf.format('0b' + '1'.repeat(5000)), '∞');
  });

test('robustness', 'NumberFormat: long numeric strings', () => {
  const nf = new Intl.NumberFormat('en', { maximumFractionDigits: 3 });
  assert.sameValue(nf.format('0.' + '0'.repeat(100000) + '1'), '0');
  assert.sameValue(nf.format('1e999999999'), nf.format('1e999999999'), 'a huge exponent returns');
  assert.sameValue(nf.format('1e-999999999'), '0');
  assert.sameValue(nf.format('-1e-999999999'), '-0');
  assert.sameValue(nf.format('9'.repeat(100000)), '∞', 'RoundMVResult overflows');
  assert.sameValue(nf.format('-' + '9'.repeat(100000)), '-∞');
  assert.sameValue(nf.format('0x' + 'f'.repeat(100)), nf.format(BigInt('0x' + 'f'.repeat(100))), 'within range it is exact');
  assert.sameValue(nf.format('1' + '0'.repeat(400) + 'x'), 'NaN');
});

test('robustness', 'NumberFormat: large BigInts', () => {
  const nf = new Intl.NumberFormat('en', { useGrouping: false });
  const big = 7n ** 5000n;
  assert.sameValue(nf.format(big), String(big));
  assert.sameValue(nf.format(-big), '-' + String(big));
});

test('robustness', 'NumberFormat: the extreme digit options at once', () => {
  const nf = new Intl.NumberFormat('en', { minimumIntegerDigits: 21, minimumFractionDigits: 100, maximumFractionDigits: 100 });
  const out = nf.format(Number.MAX_VALUE);
  assert.ok(out.length > 400);
  assert.sameValue(nf.format(5e-324).length, 21 + 6 + 1 + 100);
  const sig = new Intl.NumberFormat('en', { minimumSignificantDigits: 21, maximumSignificantDigits: 21 });
  assert.sameValue(sig.format(Number.MIN_VALUE), '0.' + '0'.repeat(323) + '500000000000000000000', 'the shortest round-trip decimal of the Number');
});

test('robustness', 'intrinsics replaced by a script do not reach the formatters', () => {
  const saved = {
    push: Array.prototype.push, map: Array.prototype.map, iterator: Array.prototype[Symbol.iterator],
    lower: String.prototype.toLowerCase, slice: String.prototype.slice, keys: Object.keys,
    defineProperty: Object.defineProperty, getPrototypeOf: Object.getPrototypeOf,
  };
  const boom = function () { throw new Error('intrinsic used'); };
  try {
    Array.prototype.push = boom;
    Array.prototype.map = boom;
    Array.prototype[Symbol.iterator] = boom;
    String.prototype.toLowerCase = boom;
    String.prototype.slice = boom;
    Object.keys = boom;
    Object.defineProperty = boom;
    Object.getPrototypeOf = boom;
    const nf = new Intl.NumberFormat(['EN-us', 'fr'], { style: 'percent', numberingSystem: 'ARAB' });
    nf.format(0.5);
    nf.formatToParts(-0.5);
    nf.resolvedOptions();
    new Intl.PluralRules('en', { type: 'ordinal' }).select(2);
    new Intl.PluralRules('en').resolvedOptions();
    Intl.getCanonicalLocales(['en-us', 'IW']);
    Intl.NumberFormat.supportedLocalesOf(['en', 'fr']);
  } finally {
    Array.prototype.push = saved.push;
    Array.prototype.map = saved.map;
    Array.prototype[Symbol.iterator] = saved.iterator;
    String.prototype.toLowerCase = saved.lower;
    String.prototype.slice = saved.slice;
    Object.keys = saved.keys;
    Object.defineProperty = saved.defineProperty;
    Object.getPrototypeOf = saved.getPrototypeOf;
  }
});

test('robustness', 'Intl and its constructors can be replaced and deleted by a script', () => {
  const saved = globalThis.Intl;
  const nf = new saved.NumberFormat('en');
  const desc = Object.getOwnPropertyDescriptor(globalThis, 'Intl');
  try {
    delete globalThis.Intl;
    assert.sameValue(typeof globalThis.Intl, 'undefined');
    assert.sameValue(nf.format(1), '1', 'a formatter outlives the namespace');
    globalThis.Intl = { NumberFormat: 1 };
    assert.sameValue(nf.format(2), '2');
  } finally {
    Object.defineProperty(globalThis, 'Intl', desc);
  }
});
