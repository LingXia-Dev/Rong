// Driven by rong_http's `test_network_observer` (feature `network-observer`):
// `__setNetworkOwner(name)` sets this context's owner ("" clears it),
// `__denyHost(host)` installs an access guard denying that host ("" denies
// none), and `__observedRequests(owner, urlContains)` returns the recorded
// traces as JSON.

async function observed(owner, count, urlContains = "") {
  for (let i = 0; i < 300; i++) {
    const list = JSON.parse(__observedRequests(owner, urlContains));
    for (const trace of list) {
      if (trace.orderError !== null) {
        throw new Error(`trace ${trace.url}: ${trace.orderError}`);
      }
    }
    if (list.length >= count && list.every((t) => t.outcome !== null)) {
      return list;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(
    `observer traces did not settle: ${__observedRequests(owner, urlContains)}`,
  );
}

describe("network observer", () => {
  test("attributes a small fetch to the context owner", async () => {
    __setNetworkOwner("js-small");
    const response = await fetch(`${TEST_SERVER_URL}/ip`);
    const text = await response.text();
    expect(response.status).toBe(200);

    const [trace] = await observed("js-small", 1);
    assert.equal(trace.owner, "js-small");
    assert.equal(trace.kind, "fetch");
    assert.equal(trace.method, "GET");
    assert.equal(trace.url, `${TEST_SERVER_URL}/ip`);
    assert.equal(trace.status, 200);
    assert.equal(trace.redirectedFrom, null);
    assert.equal(trace.responseBytes, new TextEncoder().encode(text).length);
    assert.equal(trace.bodyText, text);
    assert.equal(trace.outcome, "completed");
  });

  test("tees a large streamed body without changing what fetch returns", async () => {
    __setNetworkOwner("js-large");
    const response = await fetch(`${TEST_SERVER_URL}/large`);
    const reader = response.body.getReader();
    let appBytes = 0;
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      appBytes += value.byteLength;
    }
    assert.equal(appBytes, 100 * 11 * 1024);

    const [trace] = await observed("js-large", 1);
    assert.equal(trace.responseBytes, appBytes);
    assert.equal(trace.endBytes, appBytes);
    assert.equal(trace.chunks > 1, true);
    assert.equal(trace.outcome, "completed");
  });

  test("reports gzip bodies as wire bytes while fetch decodes them", async () => {
    __setNetworkOwner("js-gzip");
    const response = await fetch(`${TEST_SERVER_URL}/gzip`);
    const data = await response.json();
    expect(data.gzipped).toBe(true);

    const [trace] = await observed("js-gzip", 1);
    assert.equal(trace.contentEncoding, "gzip");
    assert.equal(trace.firstBytes, "1f8b");
    assert.equal(trace.responseBytes, trace.contentLength);
  });

  test("reports each redirect hop in order under one owner", async () => {
    __setNetworkOwner("js-redirect");
    const response = await fetch(`${TEST_SERVER_URL}/redirect?n=2`);
    expect(response.redirected).toBe(true);
    expect((await response.json()).origin).toBe("127.0.0.1");

    const hops = await observed("js-redirect", 4);
    expect(hops.map((h) => h.url.slice(TEST_SERVER_URL.length))).toEqual([
      "/redirect?n=2",
      "/redirect?n=1",
      "/redirect?n=0",
      "/ip",
    ]);
    for (let i = 0; i < hops.length; i++) {
      assert.equal(hops[i].owner, "js-redirect");
      assert.equal(hops[i].kind, "fetch");
      assert.equal(hops[i].redirectedFrom, i === 0 ? null : hops[i - 1].id);
    }
    for (let i = 0; i < 3; i++) {
      // A hop's head carries the redirect: status plus Location.
      assert.equal(hops[i].status, 302);
      assert.equal(
        new URL(hops[i].location, hops[i].url).href,
        new URL(hops[i + 1].url).href,
      );
    }
    assert.equal(hops[3].status, 200);
    assert.equal(hops[3].location, null);
    assert.equal(hops[3].outcome, "completed");
  });

  test("reports an abort during the body read", async () => {
    __setNetworkOwner("js-abort");
    const controller = new AbortController();
    const response = await fetch(`${TEST_SERVER_URL}/large`, {
      signal: controller.signal,
    });
    const reader = response.body.getReader();
    await reader.read();
    controller.abort();
    let rejected = false;
    try {
      while (!(await reader.read()).done) {}
    } catch {
      rejected = true;
    }
    assert.equal(rejected, true);

    const [trace] = await observed("js-abort", 1);
    assert.equal(trace.outcome, "aborted");
  });

  test("reports a refused connection as a failure", async () => {
    __setNetworkOwner("js-refused");
    let caught = null;
    try {
      await fetch(CLOSED_URL);
    } catch (error) {
      caught = error;
    }
    assert.equal(caught instanceof TypeError, true);

    const [trace] = await observed("js-refused", 1);
    assert.equal(trace.status, null);
    assert.equal(trace.outcome.startsWith("failed: request failed"), true);
  });

  test("reports the request body", async () => {
    __setNetworkOwner("js-upload");
    const body = "x".repeat(4096);
    const response = await fetch(`${TEST_SERVER_URL}/upload`, {
      method: "PUT",
      body,
    });
    expect((await response.json()).received).toBe(4096);

    const [trace] = await observed("js-upload", 1);
    assert.equal(trace.method, "PUT");
    assert.equal(trace.bodySize, 4096);
    assert.equal(trace.requestBytes, 4096);
  });

  test("attributes Rong.SSE streams and reports their wire chunks", async () => {
    __setNetworkOwner("js-sse");
    const sse = new Rong.SSE(`${TEST_SERVER_URL}/sse/basic`, {
      reconnect: { enabled: false },
    });
    const events = [];
    for await (const event of sse) {
      events.push(event.data);
      if (events.length === 2) break;
    }
    expect(events).toEqual(["hello", "world"]);

    const [trace] = await observed("js-sse", 1);
    assert.equal(trace.kind, "sse");
    assert.equal(trace.bodyText.includes("data: hello"), true);
    assert.equal(trace.bodyText.includes("data: world"), true);
  });

  test("follows owner changes and clearing", async () => {
    __setNetworkOwner("js-before");
    await (await fetch(`${TEST_SERVER_URL}/ip?phase=before`)).text();
    __setNetworkOwner("");
    await (await fetch(`${TEST_SERVER_URL}/ip?phase=cleared`)).text();

    const [before] = await observed("js-before", 1);
    assert.equal(before.url.endsWith("phase=before"), true);
    const [cleared] = await observed("", 1, "phase=cleared");
    assert.equal(cleared.owner, null);
    assert.equal(cleared.kind, "fetch");
  });

  test("reports guard-denied requests as blocked", async () => {
    __setNetworkOwner("js-blocked");
    __denyHost("blocked.invalid");
    try {
      let caught = null;
      try {
        await fetch("http://blocked.invalid/direct");
      } catch (error) {
        caught = error;
      }
      assert.equal(caught !== null, true);

      caught = null;
      try {
        await fetch(`${TEST_SERVER_URL}/redirect-blocked`);
      } catch (error) {
        caught = error;
      }
      assert.equal(caught !== null, true);

      caught = null;
      try {
        new Rong.SSE("http://blocked.invalid/sse");
      } catch (error) {
        caught = error;
      }
      assert.equal(caught !== null, true);
    } finally {
      __denyHost("");
    }

    const BLOCKED = "blocked: E_PERMISSION_DENIED: denied by policy";
    const traces = await observed("js-blocked", 4);
    expect(traces.map((t) => [t.kind, t.url, t.outcome])).toEqual([
      ["fetch", "http://blocked.invalid/direct", BLOCKED],
      ["fetch", `${TEST_SERVER_URL}/redirect-blocked`, "completed"],
      ["fetch", "http://blocked.invalid/next", BLOCKED],
      ["sse", "http://blocked.invalid/sse", BLOCKED],
    ]);
    assert.equal(traces[2].redirectedFrom, traces[1].id);
    assert.equal(traces[0].status, null);
  });
});
