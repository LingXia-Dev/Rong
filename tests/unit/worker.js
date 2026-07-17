describe("Worker", () => {
  describe("Basic functionality", () => {
    it("should create a worker", () => {
      const worker = new Worker("tests/unit/worker-echo.js");
      assert.ok(worker instanceof Worker, "Should create a Worker instance");
      worker.terminate();
    });

    it("should send and receive messages", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");

      const messagePromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: hello", "Should receive echoed message");
          worker.terminate();
          resolve();
        };
      });

      worker.postMessage("hello");
      await messagePromise;
    });

    it("should handle multiple messages", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");
      const received = [];

      const messagesPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          received.push(event.data);

          if (received.length === 3) {
            assert.equal(received[0], "echo: first", "Should receive first message");
            assert.equal(received[1], "echo: second", "Should receive second message");
            assert.equal(received[2], "echo: third", "Should receive third message");
            worker.terminate();
            resolve();
          }
        };
      });

      worker.postMessage("first");
      worker.postMessage("second");
      worker.postMessage("third");
      await messagesPromise;
    });

    it("should terminate worker", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");
      let messageCount = 0;

      worker.onmessage = (event) => {
        messageCount++;
      };

      worker.postMessage("test");

      await new Promise((resolve) => setTimeout(resolve, 50));
      worker.terminate();

      await new Promise((resolve) => setTimeout(resolve, 50));
      // Message after terminate should not be processed
      worker.postMessage("after-terminate");

      await new Promise((resolve) => setTimeout(resolve, 100));
      // Only the first message should have been received
      assert.ok(messageCount <= 1, "Should not receive messages after terminate");
    });
  });

  describe("Worker computations", () => {
    it("should perform calculations in worker", async () => {
      const worker = new Worker("tests/unit/worker-compute.js");

      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, 55, "Should compute sum of 1-10 correctly");
          worker.terminate();
          resolve();
        };
      });

      worker.postMessage({ type: "sum", max: 10 });
      await resultPromise;
    });
  });

  describe("Data types support", () => {
    it("should handle number", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");

      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: 42", "Should receive echoed number");
          worker.terminate();
          resolve();
        };
      });

      worker.postMessage(42);
      await resultPromise;
    });

    it("should handle boolean", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");

      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: true", "Should receive echoed boolean");
          worker.terminate();
          resolve();
        };
      });

      worker.postMessage(true);
      await resultPromise;
    });

    it("should handle array", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");

      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: 1,2,3", "Should receive echoed array");
          worker.terminate();
          resolve();
        };
      });

      worker.postMessage([1, 2, 3]);
      await resultPromise;
    });

    it("should handle null", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");

      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: null", "Should receive echoed null");
          worker.terminate();
          resolve();
        };
      });

      worker.postMessage(null);
      await resultPromise;
    });

    it("should handle undefined", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");
      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: undefined");
          worker.terminate();
          resolve();
        };
      });
      worker.postMessage(undefined);
      await Promise.race([
        resultPromise,
        new Promise((_, reject) =>
          setTimeout(() => {
            worker.terminate();
            reject(new Error("undefined message timeout"));
          }, 500),
        ),
      ]);
    });

    it("should handle bigint", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");
      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: 42");
          worker.terminate();
          resolve();
        };
      });
      worker.postMessage(42n);
      await Promise.race([
        resultPromise,
        new Promise((_, reject) =>
          setTimeout(() => {
            worker.terminate();
            reject(new Error("bigint message timeout"));
          }, 500),
        ),
      ]);
    });

    it("should handle non-finite numbers", async () => {
      const worker = new Worker("tests/unit/worker-echo.js");
      const resultPromise = new Promise((resolve) => {
        worker.onmessage = (event) => {
          assert.equal(event.data, "echo: NaN");
          worker.terminate();
          resolve();
        };
      });
      worker.postMessage(NaN);
      await Promise.race([
        resultPromise,
        new Promise((_, reject) =>
          setTimeout(() => {
            worker.terminate();
            reject(new Error("non-finite message timeout"));
          }, 500),
        ),
      ]);
    });
  });

  describe("Error handling", () => {
    it("should report worker script errors to onerror", async () => {
      const worker = new Worker("tests/unit/worker-fail.js");

      await new Promise((resolve, reject) => {
        worker.onerror = (event) => {
          assert.equal(event.type, "error");
          assert.equal(typeof event.message, "string");
          assert.equal(event.message.includes("boom"), true, `unexpected error: ${event.message}`);
          worker.terminate();
          resolve();
        };

        setTimeout(() => {
          worker.terminate();
          reject(new Error("timeout"));
        }, 5000);
      });
    });
  });
});
