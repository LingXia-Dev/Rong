// Delivery guarantees for setTimeout/setInterval under load.
//
// Every wait here is bounded and polled with `Rong.sleep`, which does not use
// the callback-timer path under test, so a lost callback fails the test
// instead of hanging it.

const STALL_MS = 3000;
const DEADLINE_MS = 60000;

// Poll `progress()` until `isDone()` holds. Fails when progress stops for
// STALL_MS or the whole wait exceeds DEADLINE_MS.
async function waitFor(what, isDone, progress) {
  const start = Date.now();
  let last = progress();
  let lastChange = start;
  while (!isDone()) {
    await Rong.sleep(10);
    const now = Date.now();
    const current = progress();
    if (current !== last) {
      last = current;
      lastChange = now;
    }
    if (now - lastChange > STALL_MS) {
      throw new Error(`${what}: stalled at ${current} for ${STALL_MS} ms`);
    }
    if (now - start > DEADLINE_MS) {
      throw new Error(`${what}: at ${current} after ${DEADLINE_MS} ms`);
    }
  }
}

// Schedule `count` timers with `delay` at once and wait until all fire.
async function burst(count, delay) {
  const hits = new Uint32Array(count);
  let fired = 0;
  for (let i = 0; i < count; i++) {
    setTimeout(() => {
      hits[i]++;
      fired++;
    }, delay);
  }
  await waitFor(
    `${count} x setTimeout(fn, ${delay})`,
    () => fired >= count,
    () => fired,
  );
  // Give a duplicate a chance to show up.
  await Rong.sleep(20);
  for (let i = 0; i < count; i++) {
    assert.equal(hits[i], 1, `timer ${i} of ${count} fired ${hits[i]} times`);
  }
  assert.equal(fired, count);
}

describe("Timer delivery", () => {
  test("a sequential zero-delay await loop completes", async () => {
    const iterations = 20000;
    let done = 0;
    let failure;
    (async () => {
      for (let i = 0; i < iterations; i++) {
        await new Promise((resolve) => setTimeout(resolve, 0));
        done = i + 1;
      }
    })().catch((error) => {
      failure = error;
    });
    await waitFor(
      "sequential setTimeout(resolve, 0)",
      () => done === iterations || failure !== undefined,
      () => done,
    );
    assert.equal(failure, undefined);
    assert.equal(done, iterations);
  });

  test("a sequential await loop with an omitted delay completes", async () => {
    const iterations = 5000;
    let done = 0;
    (async () => {
      for (let i = 0; i < iterations; i++) {
        await new Promise((resolve) => setTimeout(resolve));
        done = i + 1;
      }
    })();
    await waitFor(
      "sequential setTimeout(resolve)",
      () => done === iterations,
      () => done,
    );
  });

  test("10,000 timers with delay 0 each fire exactly once", async () => {
    await burst(10000, 0);
  });

  test("10,000 timers with delay 1 each fire exactly once", async () => {
    await burst(10000, 1);
  });

  test("2,000 timers with delay 5 each fire exactly once", async () => {
    await burst(2000, 5);
  });

  test("a timer cleared before it fires never fires", async () => {
    const count = 2000;
    const hits = new Uint32Array(count);
    let kept = 0;
    for (let i = 0; i < count; i++) {
      const id = setTimeout(() => {
        hits[i]++;
        if (i % 2 === 1) kept++;
      }, i % 3);
      if (i % 2 === 0) clearTimeout(id);
    }
    await waitFor("uncleared timers", () => kept >= count / 2, () => kept);
    await Rong.sleep(30);
    for (let i = 0; i < count; i++) {
      assert.equal(hits[i], i % 2 === 0 ? 0 : 1, `timer ${i} fired ${hits[i]} times`);
    }
  });

  for (const delay of [0, 5]) {
    test(`clearTimeout from a callback cancels timers due together (delay ${delay})`, async () => {
      const count = 500;
      const ids = [];
      let fired = 0;
      for (let i = 0; i < count; i++) {
        ids.push(
          setTimeout(() => {
            fired++;
            // The first one to run cancels all others, including those
            // whose expiry is already queued.
            for (const id of ids) clearTimeout(id);
          }, delay),
        );
      }
      await waitFor("the first callback", () => fired > 0, () => fired);
      await Rong.sleep(50);
      assert.equal(fired, 1, `${fired} callbacks ran after the first cleared the rest`);
    });
  }

  test("clearTimeout of a timer's own id from its callback is harmless", async () => {
    let fired = 0;
    const id = setTimeout(() => {
      fired++;
      clearTimeout(id);
    }, 0);
    await waitFor("self-clearing timer", () => fired > 0, () => fired);
    await Rong.sleep(20);
    assert.equal(fired, 1);
  });

  test("a chain of nested setTimeout(0) calls completes", async () => {
    const depth = 5000;
    let reached = 0;
    const step = () => {
      reached++;
      if (reached < depth) setTimeout(step, 0);
    };
    setTimeout(step, 0);
    await waitFor("nested setTimeout(0)", () => reached === depth, () => reached);
  });

  test("setInterval keeps firing and clearInterval stops it", async () => {
    let count = 0;
    const id = setInterval(() => {
      count++;
    }, 1);
    await waitFor("setInterval(fn, 1)", () => count >= 100, () => count);
    clearInterval(id);
    const stopped = count;
    await Rong.sleep(30);
    assert.equal(count, stopped, "the interval fired after clearInterval");
  });

  test("clearInterval from its own callback stops it", async () => {
    let count = 0;
    const id = setInterval(() => {
      count++;
      if (count === 50) clearInterval(id);
    }, 0);
    await waitFor("self-clearing setInterval", () => count >= 50, () => count);
    await Rong.sleep(30);
    assert.equal(count, 50);
  });

  test("many timers due together while an await loop runs all fire", async () => {
    const iterations = 3000;
    const timers = iterations; // 100 every 100 iterations
    let fired = 0;
    let done = 0;
    const hits = new Uint32Array(timers);
    (async () => {
      for (let i = 0; i < iterations; i++) {
        if (i % 100 === 0) {
          for (let j = 0; j < 100; j++) {
            const index = i + j;
            setTimeout(() => {
              hits[index]++;
              fired++;
            }, j % 4);
          }
        }
        await new Promise((resolve) => setTimeout(resolve, i % 2));
        done = i + 1;
      }
    })();
    await waitFor(
      "timers mixed with an await loop",
      () => done === iterations && fired >= timers,
      () => done + fired,
    );
    await Rong.sleep(20);
    for (let i = 0; i < timers; i++) {
      assert.equal(hits[i], 1, `timer ${i} fired ${hits[i]} times`);
    }
  });

  test("a sequential Rong.sleep(0) loop completes", async () => {
    const iterations = 2000;
    let done = 0;
    (async () => {
      for (let i = 0; i < iterations; i++) {
        await Rong.sleep(0);
        done = i + 1;
      }
    })();
    await waitFor("sequential Rong.sleep(0)", () => done === iterations, () => done);
  });
});
