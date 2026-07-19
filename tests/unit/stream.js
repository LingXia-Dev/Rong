describe("ReadableStream (constructor + reader)", () => {
  test("hides internal stream helper classes from global scope", () => {
    assert.equal(typeof ReadableStreamDefaultReader, "undefined");
    assert.equal(typeof ReadableStreamDefaultController, "undefined");
    assert.equal(typeof WritableStreamDefaultWriter, "undefined");

    const reader = new ReadableStream({}).getReader();
    let readerFailed = false;
    try {
      new reader.constructor();
    } catch (e) {
      readerFailed = true;
    }
    expect(readerFailed).toBe(true);
    reader.releaseLock();

    const writer = new WritableStream({}).getWriter();
    let writerFailed = false;
    try {
      new writer.constructor();
    } catch (e) {
      writerFailed = true;
    }
    expect(writerFailed).toBe(true);
    writer.releaseLock();
  });

  test("new ReadableStream(start) enqueues and closes", async () => {
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("a"));
        controller.enqueue(new TextEncoder().encode("b"));
        controller.close();
      },
    });

    const reader = rs.getReader();
    const r1 = await reader.read();
    expect(r1.done).toBe(false);
    expect(new TextDecoder().decode(r1.value)).toBe("a");

    const r2 = await reader.read();
    expect(r2.done).toBe(false);
    expect(new TextDecoder().decode(r2.value)).toBe("b");

    const r3 = await reader.read();
    expect(r3.done).toBe(true);
  });

  test("getReader locking + releaseLock", async () => {
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("x"));
        controller.close();
      },
    });

    const r1 = rs.getReader();
    let threw = false;
    try {
      rs.getReader();
    } catch (e) {
      threw = true;
      expect(e instanceof TypeError).toBe(true);
    }
    if (!threw) throw new Error("expected getReader to throw while locked");

    await r1.releaseLock();
    const r2 = rs.getReader();
    const { value, done } = await r2.read();
    expect(done).toBe(false);
    expect(new TextDecoder().decode(value)).toBe("x");
  });
});

describe("WritableStream (sink + writer)", () => {
  test("collects chunks via sink.write and close", async () => {
    const collected = [];
    const ws = new WritableStream({
      write(chunk) {
        const u8 = new Uint8Array(chunk);
        collected.push(...u8);
      },
      close() {
        // no-op
      },
    });

    const writer = ws.getWriter();
    const enc = new TextEncoder();
    await writer.write(enc.encode("hello"));
    await writer.write(enc.encode("world"));
    await writer.close();

    const text = new TextDecoder().decode(new Uint8Array(collected));
    expect(text).toBe("helloworld");
  });

  test("supports async sink.write/close (promises)", async () => {
    const collected = [];
    const ws = new WritableStream({
      async write(chunk) {
        await new Promise((r) => setTimeout(r, 5));
        const u8 = new Uint8Array(chunk);
        collected.push(...u8);
      },
      async close() {
        await new Promise((r) => setTimeout(r, 5));
      },
    });

    const writer = ws.getWriter();
    const enc = new TextEncoder();
    await writer.write(enc.encode("foo"));
    await writer.write(enc.encode("bar"));
    await writer.close();

    const text = new TextDecoder().decode(new Uint8Array(collected));
    expect(text).toBe("foobar");
  });
});

describe("ReadableStream pipeTo", () => {
  test("pipes readable to writable (sink)", async () => {
    const enc = new TextEncoder();
    const dec = new TextDecoder();

    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(enc.encode("hello"));
        controller.enqueue(enc.encode("world"));
        controller.close();
      },
    });

    const collected = [];
    const ws = new WritableStream({
      write(chunk) {
        const u8 = new Uint8Array(chunk);
        collected.push(...u8);
      },
      close() {},
    });

    await rs.pipeTo(ws);
    const out = new TextDecoder().decode(new Uint8Array(collected));
    expect(out).toBe("helloworld");
  });

  test("respects preventClose option", async () => {
    const enc = new TextEncoder();
    let closed = false;
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(enc.encode("x"));
        controller.close();
      },
    });
    const ws = new WritableStream({
      write(_) {},
      close() {
        closed = true;
      },
    });
    await rs.pipeTo(ws, { preventClose: true });
    expect(closed).toBe(false);
  });

  test("aborts the destination when write fails", async () => {
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("boom"));
        controller.close();
      },
    });

    let aborted = false;
    const ws = new WritableStream({
      write() {
        throw new Error("sink write failed");
      },
      abort() {
        aborted = true;
      },
    });

    let failed = false;
    try {
      await rs.pipeTo(ws);
    } catch (error) {
      failed = true;
      expect(error instanceof Error).toBe(true);
    }

    expect(failed).toBe(true);
    expect(aborted).toBe(true);
  });

  test("respects preventAbort when write fails", async () => {
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("boom"));
        controller.close();
      },
    });

    let aborted = false;
    const ws = new WritableStream({
      write() {
        throw new Error("sink write failed");
      },
      abort() {
        aborted = true;
      },
    });

    let failed = false;
    try {
      await rs.pipeTo(ws, { preventAbort: true });
    } catch (error) {
      failed = true;
      expect(error instanceof Error).toBe(true);
    }

    expect(failed).toBe(true);
    expect(aborted).toBe(false);
  });

  test("releases locks when the abort signal is already aborted", async () => {
    const controller = new AbortController();
    controller.abort("stop");

    const rs = new ReadableStream({});
    const ws = new WritableStream({});

    let failed = false;
    try {
      await rs.pipeTo(ws, { signal: controller.signal });
    } catch (error) {
      failed = true;
    }

    expect(failed).toBe(true);

    const reader = rs.getReader();
    reader.releaseLock();

    const writer = ws.getWriter();
    await writer.releaseLock();
  });
});

describe("ReadableStream pipeThrough", () => {
  test("pipes through a transform pair and returns the readable side", async () => {
    const enc = new TextEncoder();
    const dec = new TextDecoder();

    const source = new ReadableStream({
      start(controller) {
        controller.enqueue(enc.encode("hello"));
        controller.enqueue(enc.encode(" world"));
        controller.close();
      },
    });

    const transformedChunks = [];
    let transformController = null;
    const transform = {
      writable: new WritableStream({
        write(chunk) {
          transformedChunks.push(dec.decode(chunk).toUpperCase());
        },
        close() {
          transformController.enqueue(
            enc.encode(transformedChunks.join("")),
          );
          transformController.close();
        },
      }),
      readable: new ReadableStream({
        start(controller) {
          transformController = controller;
        },
      }),
    };

    const output = source.pipeThrough(transform);
    expect(output).toBe(transform.readable);

    const reader = output.getReader();
    const { done, value } = await reader.read();
    expect(done).toBe(false);
    expect(dec.decode(value)).toBe("HELLO WORLD");

    const finalRead = await reader.read();
    expect(finalRead.done).toBe(true);
  });

  test("validates transform.readable", () => {
    const rs = new ReadableStream({});

    expect(() =>
      rs.pipeThrough({
        readable: {},
        writable: new WritableStream({}),
      }),
    ).toThrow(TypeError);
  });

  test("throws when the source stream is already locked", () => {
    const rs = new ReadableStream({});
    rs.getReader();

    expect(() =>
      rs.pipeThrough(new CompressionStream("gzip")),
    ).toThrow(TypeError);
  });

  test("throws when transform.writable is already locked", () => {
    const source = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("hello"));
        controller.close();
      },
    });
    const transform = new CompressionStream("gzip");
    transform.writable.getWriter();

    expect(() => source.pipeThrough(transform)).toThrow(TypeError);
  });

  test("respects preventClose for the transform writable side", async () => {
    const source = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("x"));
        controller.close();
      },
    });

    let closed = false;
    const transform = {
      writable: new WritableStream({
        write() {},
        close() {
          closed = true;
        },
      }),
      readable: new ReadableStream({}),
    };

    source.pipeThrough(transform, { preventClose: true });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(closed).toBe(false);
  });

  test("aborts transform.writable when piping fails", async () => {
    const source = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("boom"));
        controller.close();
      },
    });

    let aborted = false;
    const transform = {
      writable: new WritableStream({
        write() {
          throw new Error("transform write failed");
        },
        abort() {
          aborted = true;
        },
      }),
      readable: new ReadableStream({}),
    };

    source.pipeThrough(transform);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(aborted).toBe(true);
  });

  test("respects preventAbort when transform.writable fails", async () => {
    const source = new ReadableStream({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("boom"));
        controller.close();
      },
    });

    let aborted = false;
    const transform = {
      writable: new WritableStream({
        write() {
          throw new Error("transform write failed");
        },
        abort() {
          aborted = true;
        },
      }),
      readable: new ReadableStream({}),
    };

    source.pipeThrough(transform, { preventAbort: true });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(aborted).toBe(false);
  });

  test("does not leak locks when the abort signal is already aborted", async () => {
    const controller = new AbortController();
    controller.abort("stop");

    const source = new ReadableStream({});
    const transform = new CompressionStream("gzip");

    const output = source.pipeThrough(transform, { signal: controller.signal });
    expect(output).toBe(transform.readable);

    await new Promise((resolve) => setTimeout(resolve, 20));

    const reader = source.getReader();
    reader.releaseLock();

    const writer = transform.writable.getWriter();
    await writer.releaseLock();
  });
});

describe("ReadableStream async iterator", () => {
  test("iterates chunks with for await...of", async () => {
    const enc = new TextEncoder();
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(enc.encode("A"));
        controller.enqueue(enc.encode("B"));
        controller.close();
      },
    });

    const collected = [];
    for await (const chunk of rs) {
      const u8 = new Uint8Array(chunk);
      collected.push(...u8);
    }
    const out = new TextDecoder().decode(new Uint8Array(collected));
    expect(out).toBe("AB");
  });
});

describe("ReadableStream tee", () => {
  test("duplicates chunks to both branches", async () => {
    const enc = new TextEncoder();
    const dec = new TextDecoder();

    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(enc.encode("hi"));
        controller.enqueue(enc.encode("!"));
        controller.close();
      },
    });

    const [b1, b2] = rs.tee();
    expect(b1 instanceof ReadableStream).toBe(true);
    expect(b2 instanceof ReadableStream).toBe(true);

    const r1 = b1.getReader();
    const r2 = b2.getReader();

    let s1 = "";
    while (true) {
      const { done, value } = await r1.read();
      if (done) break;
      s1 += dec.decode(value);
    }

    let s2 = "";
    while (true) {
      const { done, value } = await r2.read();
      if (done) break;
      s2 += dec.decode(value);
    }

    expect(s1).toBe("hi!");
    expect(s2).toBe("hi!");
  });

  test("locks the original stream after tee", async () => {
    const enc = new TextEncoder();
    const rs = new ReadableStream({
      start(controller) {
        controller.enqueue(enc.encode("x"));
        controller.close();
      },
    });

    rs.tee();

    let threw = false;
    try {
      rs.getReader();
    } catch (e) {
      threw = true;
      expect(e instanceof TypeError).toBe(true);
    }
    if (!threw) throw new Error("expected getReader to throw after tee() (locked)");
  });
});
