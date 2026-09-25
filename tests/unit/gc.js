// Garbage collection on a Rong worker, seen from JavaScript.
//
// A WeakRef is cleared only once the engine has collected its target. On
// JavaScriptCore, collections of an idle heap come from timers on the worker
// thread's run loop, so this passes only while the worker drives them. On
// QuickJS, `test.gc()` collects.
//
// The test watches WeakRefs, not FinalizationRegistry callbacks: Apple's
// JavaScriptCore collects the targets here but has not been seen to deliver
// the callbacks to a C API context, which no Rong change can fix.
//
// `sleep(ms)` is provided by the Rust driver. Every wait is bounded, so a
// missing collection fails the test instead of hanging it.

const OBJECTS = 64;
const DEADLINE_MS = 90000;
const POLL_MS = 50;

// Create `count` unreachable objects, each in a self-cycle with a payload,
// and return WeakRefs to them. Kept in its own function so no local survives
// it. The targets stay alive until the current job ends, as WeakRef requires.
function createGarbage(count) {
  const refs = [];
  for (let i = 0; i < count; i++) {
    const node = { index: i, payload: new Array(256).fill(i) };
    node.self = node;
    node.next = () => node;
    refs.push(new WeakRef(node));
  }
  return refs;
}

function countCleared(refs) {
  return refs.filter((ref) => ref.deref() === undefined).length;
}

describe("Garbage collection on a worker", () => {
  test("WeakRef targets are collected for dropped objects", async () => {
    const refs = createGarbage(OBJECTS);

    const start = Date.now();
    let cleared = 0;
    while (cleared < OBJECTS / 2) {
      if (Date.now() - start > DEADLINE_MS) {
        throw new Error(
          `${cleared} of ${OBJECTS} objects collected after ${DEADLINE_MS} ms`,
        );
      }
      if (typeof test.gc === "function") test.gc();
      await sleep(POLL_MS);
      cleared = countCleared(refs);
    }

    expect(cleared >= OBJECTS / 2).toBe(true);
  });
});
