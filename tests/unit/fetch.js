describe("fetch", () => {
  test("should fetch IP from test server", async () => {
    const url = new URL("/ip", TEST_SERVER_URL);
    const response = await fetch(url);
    expect(response instanceof Response).toBe(true);
    expect(response.ok).toBe(true);
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("application/json");
    const data = await response.json();
    console.log(data);
    expect(typeof data.origin).toBe("string");
    expect(data.origin).toBe("127.0.0.1");
  });

  test("should send and receive custom headers", async () => {
    const url = new URL("/headers", TEST_SERVER_URL);
    const customHeaders = {
      "X-Custom-Header": "custom value",
      "X-Test-Header": "test value",
      "User-Agent": "RongJS Test Client",
    };

    const response = await fetch(url, {
      headers: customHeaders,
    });

    expect(response.ok).toBe(true);
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("application/json");

    const data = await response.json();
    console.log("Received headers:", data);

    // Verify our custom headers were received by the server
    for (const [key, value] of Object.entries(customHeaders)) {
      assert.equal(data[key.toLowerCase()], value);
    }
  });

  test("should handle gzipped response", async () => {
    const url = new URL("/gzip", TEST_SERVER_URL);
    const response = await fetch(url);
    expect(response instanceof Response).toBe(true);
    expect(response.ok).toBe(true);
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("application/json");
    expect(response.headers.get("content-encoding")).toBe("gzip");

    const data = await response.json();
    expect(data.gzipped).toBe(true);
    expect(typeof data.method).toBe("string");
    expect(data.method).toBe("GET");
  });

  test("should handle network errors", async () => {
    try {
      await fetch("https://invalid.example.com");
    } catch (error) {
      console.log(error);
      expect(error instanceof TypeError).toBe(true);
    }
  });

  test("should not allow multiple body reads", async () => {
    const url = new URL("/ip", TEST_SERVER_URL);
    const response = await fetch(url);
    const a = await response.json();
    expect(typeof a.origin).toBe("string");

    // Second read must fail per spec
    let threw = false;
    try {
      await response.text();
    } catch (err) {
      threw = true;
      expect(err instanceof TypeError).toBe(true);
      expect(/body used already/.test(String(err))).toBe(true);
    }
    if (!threw) {
      throw new Error("Expected second body read to fail");
    }
  });

  test("should stream upload with ReadableStream body", async () => {
    const url = new URL("/upload", TEST_SERVER_URL);
    const total = 100 * 1024 + 5;
    const chunk = new Uint8Array(4096).fill(0x61); // 'a'
    let sent = 0;

    const rs = new ReadableStream({
      start(controller) {
        (async () => {
          while (sent < total) {
            const n = Math.min(chunk.length, total - sent);
            controller.enqueue(chunk.subarray(0, n));
            sent += n;
            // small yield
            await new Promise((r) => setTimeout(r, 1));
          }
          controller.close();
        })();
      },
    });

    const response = await fetch(url, { method: "PUT", body: rs });
    expect(response.ok).toBe(true);
    const data = await response.json();
    expect(data.received).toBe(total);
  });

  test("should reject when request body conversion throws", async () => {
    const url = new URL("/upload", TEST_SERVER_URL);
    const reason = new Error("body conversion failed");
    let caught;
    try {
      await fetch(url, {
        method: "PUT",
        body: {
          toString() {
            throw reason;
          },
        },
      });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBe(reason);
  });

  test("should read streaming response via Response.body", async () => {
    const url = new URL("/large", TEST_SERVER_URL);
    const response = await fetch(url);
    expect(response instanceof Response).toBe(true);

    const body = response.body;
    const decoder = new TextDecoder();
    let seenStart = false;
    let seenEnd = false;
    let total = 0;
    let text = "";
    for await (const chunk of body) {
      total += chunk.byteLength;
      text += decoder.decode(chunk);
      if (text.includes("chunk_0000")) seenStart = true;
      if (text.includes("chunk_0099")) seenEnd = true;
    }
    expect(total > 0).toBe(true);
    expect(seenStart).toBe(true);
    expect(seenEnd).toBe(true);
  });

  test("should download to file via WritableStream", async () => {
    // Prepare temp dir and file path
    const tmpDir = `${WORKSPACE_ROOT}/target/test-tmp`;
    try {
      await Rong.mkdir(tmpDir, { recursive: true });
    } catch {}
    const outPath = `${tmpDir}/fetch_download_stream.txt`;

    // Open file and get writable stream
    const file = await Rong.file(outPath).open({
      write: true,
      create: true,
      truncate: true,
    });
    const ws = file.writable;
    const writer = ws.getWriter();

    // Fetch large streaming response
    const url = new URL("/large", TEST_SERVER_URL);
    const response = await fetch(url);
    const body = response.body;
    const reader = body.getReader();

    // Pump
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      await writer.write(value);
    }
    await writer.close();
    await file.close();

    // Verify file content contains streamed markers
    const data = await Rong.file(outPath).bytes();
    const text = new TextDecoder().decode(data);
    assert(text.includes("chunk_0000"));
    assert(text.includes("chunk_0099"));

    // Cleanup created file
    await Rong.remove(outPath);
  });

  test("should pipeTo file.writable (download)", async () => {
    const tmpDir = `${WORKSPACE_ROOT}/target/test-tmp`;
    try {
      await Rong.mkdir(tmpDir, { recursive: true });
    } catch {}
    const outPath = `${tmpDir}/fetch_download_pipeTo.txt`;

    const file = await Rong.file(outPath).open({
      write: true,
      create: true,
      truncate: true,
    });
    const ws = file.writable;

    const url = new URL("/large", TEST_SERVER_URL);
    const response = await fetch(url);

    // Use pipeTo
    await response.body.pipeTo(ws);
    await file.close();

    const data = await Rong.file(outPath).bytes();
    const text = new TextDecoder().decode(data);
    assert(text.includes("chunk_0000"));
    assert(text.includes("chunk_0099"));
    await Rong.remove(outPath);
  });

  describe("redirect", () => {
    test("should follow redirects by default", async () => {
      const url = new URL("/redirect?n=2", TEST_SERVER_URL);
      const response = await fetch(url);
      expect(response.ok).toBe(true);
      expect(response.status).toBe(200);
      expect(response.redirected).toBe(true);
      const data = await response.json();
      expect(data.origin).toBe("127.0.0.1");
    });

    test("should handle redirect: manual", async () => {
      const url = new URL("/redirect?n=1", TEST_SERVER_URL);
      const response = await fetch(url, { redirect: "manual" });
      expect(response.type).toBe("basic");
      expect(response.status).toBe(302);
      expect(response.headers.has("location")).toBe(true);
    });

    test("should handle redirect: error", async () => {
      const url = new URL("/redirect?n=1", TEST_SERVER_URL);
      try {
        await fetch(url, { redirect: "error" });
        throw new Error("Should have thrown");
      } catch (e) {
        expect(e instanceof TypeError).toBe(true);
      }
    });

    test("should change POST/PUT to GET on 303 redirect", async () => {
      const url = new URL("/303", TEST_SERVER_URL);
      // PUT /303 -> 303 Location: /ip -> GET /ip
      const response = await fetch(url, { method: "PUT" });
      expect(response.ok).toBe(true);
      expect(response.status).toBe(200);
      const data = await response.json();
      expect(data.origin).toBe("127.0.0.1");
    });

    test("should limit redirects", async () => {
      const url = new URL("/redirect?n=25", TEST_SERVER_URL);
      try {
        await fetch(url);
        throw new Error("Should have thrown");
      } catch (e) {
        // Implementation throws HostError::NETWORK -> NetworkError
        expect(e.name).toBe("NetworkError");
      }
    });
  });
});

describe("Abort to fetch", () => {
  let controller;
  let signal;

  beforeEach(() => {
    controller = new AbortController();
    signal = controller.signal;
  });

  test("should abort fetch request", async () => {
    const fetchPromise = (async () => {
      return await fetch(`${TEST_SERVER_URL}/delay`, { signal });
    })();

    // Abort on the next turn so the request has been created, without racing a
    // short server delay on slower Windows machines.
    await Promise.resolve();
    controller.abort();

    try {
      await fetchPromise;
      assert.fail("fetch should have been aborted");
    } catch (error) {
      assert.ok(error instanceof DOMException);
      assert.equal(error.name, "AbortError");
      console.log("##Got:", error.name);
    }
  });

  test("should abort during response body read", async () => {
    const response = await fetch(`${TEST_SERVER_URL}/large`, { signal });

    // Start reading the body first
    const readPromise = response.arrayBuffer();
    let abortCaught = false; // Add flag to track if we catch the abort

    // Wait just a tiny bit to ensure reading has started
    await new Promise((resolve) => setTimeout(resolve, 10));

    // Then abort
    controller.abort();

    try {
      await readPromise;
      console.log("##Body read completed without being aborted");
    } catch (error) {
      assert.ok(error instanceof DOMException);
      assert.equal(error.name, "AbortError");
      abortCaught = true; // Set flag when we catch the abort
      console.log("##Got: ", error.name);
    }

    // Verify that we actually caught the abort
    if (!abortCaught) {
      throw new Error("Body read was not aborted as expected");
    }
  });

  test("should abort with custom reason", async () => {
    const reason = new Error("Custom abort reason");
    assert.equal(signal.aborted, false);
    assert.equal(signal.reason, undefined);

    const fetchPromise = (async () => {
      return await fetch(`${TEST_SERVER_URL}/delay`, { signal });
    })();

    await Promise.resolve();
    controller.abort(reason);

    assert.equal(signal.aborted, true);
    assert.equal(signal.reason, reason);

    try {
      await fetchPromise;
      assert.fail("fetch should have been aborted");
    } catch (error) {
      assert.equal(error, reason);
      console.log("##Got: ", error);
    }
  });

  test("should abort immediately if signal is already aborted", async () => {
    assert.equal(signal.aborted, false);
    assert.equal(signal.reason, undefined);

    controller.abort();

    assert.equal(signal.aborted, true);
    assert.ok(signal.reason instanceof DOMException);
    assert.equal(signal.reason.name, "AbortError");

    try {
      await fetch(`${TEST_SERVER_URL}/ip`, { signal });
      assert.fail("fetch should have been aborted");
    } catch (error) {
      console.log("##Got: ", error.name);
      assert.ok(error instanceof DOMException);
      assert.equal(error.name, "AbortError");
    }
  });
});
