describe("SSE", () => {
  test("does not expose SSE on globalThis", () => {
    assert.equal(typeof SSE, "undefined");
    assert.equal(globalThis.SSE, undefined);
    assert.equal(typeof Rong.SSE, "function");
  });

  test("should receive events via for-await-of", async () => {
    const url = TEST_SERVER_URL + "/sse/basic";
    const sse = new Rong.SSE(url);

    const events = [];
    for await (const event of sse) {
      events.push(event);
      if (events.length === 2) break;
    }

    assert.equal(events.length, 2);
    assert.equal(events[0].type, "message");
    assert.equal(events[0].data, "hello");
    assert.equal(events[0].id, "1");
    assert.equal(events[1].data, "world");
    assert.equal(events[1].id, "2");
  });

  test("should reconnect transparently across iterations", async () => {
    const url = TEST_SERVER_URL + "/sse/reconnect";
    const sse = new Rong.SSE(url, {
      reconnect: {
        enabled: true,
        baseDelayMs: 10,
        maxDelayMs: 100,
      },
    });

    const ids = [];
    const payloads = [];

    for await (const event of sse) {
      ids.push(event.id);
      payloads.push(event.data);
      if (event.id === "2") break;
    }

    assert.equal(ids.length >= 2, true);
    assert.equal(ids[0], "1");
    assert.equal(payloads[0], "first");
    assert.equal(ids.includes("2"), true);
  });

  test("should expose url property", () => {
    const url = TEST_SERVER_URL + "/sse/basic";
    const sse = new Rong.SSE(url);
    assert.equal(sse.url, url);
    sse.close();
  });

  test("should provide origin in event objects", async () => {
    const url = TEST_SERVER_URL + "/sse/basic";
    const sse = new Rong.SSE(url);

    for await (const event of sse) {
      assert.equal(typeof event.origin, "string");
      assert.equal(event.origin.length > 0, true);
      assert.equal(event.origin.startsWith("http://"), true);
      break;
    }
  });

  test("should throw on connection error", async () => {
    const url = TEST_SERVER_URL + "/sse/not-event-stream";
    const sse = new Rong.SSE(url, { reconnect: { enabled: false } });

    let threw = false;
    try {
      for await (const event of sse) {
        // should not reach here
      }
    } catch (e) {
      threw = true;
      assert.equal(e.message.includes("content-type") || e.message.includes("status"), true);
    }

    assert.equal(threw, true);
  });

  test("should stop receiving after break", async () => {
    const url = TEST_SERVER_URL + "/sse/many";
    const sse = new Rong.SSE(url, { reconnect: { enabled: false } });

    const events = [];
    for await (const event of sse) {
      events.push(event);
      if (events.length === 2) break;
    }

    assert.equal(events.length, 2);
  });

  test("should stop receiving after close()", async () => {
    const url = TEST_SERVER_URL + "/sse/many";
    const sse = new Rong.SSE(url, { reconnect: { enabled: false } });

    const events = [];
    for await (const event of sse) {
      events.push(event);
      if (events.length === 2) {
        sse.close();
      }
    }

    // close() signals the transport to stop, but already-buffered events
    // may still be yielded before the channel drains.
    assert.equal(events.length >= 2, true);
    assert.equal(events.length <= 5, true);
  });

  test("should deliver custom event types", async () => {
    const url = TEST_SERVER_URL + "/sse/custom";
    const sse = new Rong.SSE(url, { reconnect: { enabled: false } });

    const statusEvents = [];
    const progressEvents = [];
    const messageEvents = [];

    for await (const event of sse) {
      if (event.type === "status") statusEvents.push(event);
      else if (event.type === "progress") progressEvents.push(event);
      else if (event.type === "message") {
        messageEvents.push(event);
        break;
      }
    }

    assert.equal(statusEvents.length, 1);
    assert.equal(statusEvents[0].type, "status");
    assert.equal(statusEvents[0].data, "connected");

    assert.equal(progressEvents.length, 1);
    assert.equal(progressEvents[0].type, "progress");
    assert.equal(progressEvents[0].data, "50%");

    assert.equal(messageEvents.length, 1);
    assert.equal(messageEvents[0].data, "default message");
    assert.equal(messageEvents[0].id, "3");
  });

  test("should deliver small events before stream end", async () => {
    const url = TEST_SERVER_URL + "/sse/live-small";
    const sse = new Rong.SSE(url, { reconnect: { enabled: false } });
    const startedAt = Date.now();

    for await (const event of sse) {
      const elapsed = Date.now() - startedAt;
      assert.equal(event.data, "live-small");
      assert.equal(elapsed < 600, true, `message arrived too late: ${elapsed}ms`);
      break;
    }
  });
});
