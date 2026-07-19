describe("Timer", () => {
  describe("setTimeout", () => {
    test("should execute callback after delay", async () => {
      await new Promise((resolve, reject) => {
        const start = Date.now();
        const timeoutId = setTimeout(() => {
          try {
            const elapsed = Date.now() - start;
            assert.ok(
              elapsed >= 100 && elapsed <= 180,
              "setTimeout should wait between 100-180ms",
            );
            resolve();
          } catch (error) {
            reject(error);
          }
        }, 100);
        assert.ok(
          typeof timeoutId === "number",
          "setTimeout should return a number id",
        );
      });
    });

    test("should handle clearing timeout", async () => {
      await new Promise((resolve, reject) => {
        let called = false;
        const timeoutId = setTimeout(() => {
          called = true;
        }, 50);

        clearTimeout(timeoutId);

        setTimeout(() => {
          try {
            assert.ok(!called, "Callback should not be called after clearTimeout");
            resolve();
          } catch (error) {
            reject(error);
          }
        }, 100);
      });
    });
  });

  describe("setInterval", () => {
    test("should execute callback repeatedly", async () => {
      await new Promise((resolve, reject) => {
        const results = [];
        const intervalId = setInterval(() => {
          results.push(Date.now());
          if (results.length >= 3) {
            clearInterval(intervalId);
            try {
              for (let i = 1; i < results.length; i++) {
                const diff = results[i] - results[i - 1];
                assert.ok(
                  diff >= 45,
                  "Interval between values should be at least 45ms",
                );
              }
              resolve();
            } catch (error) {
              reject(error);
            }
          }
        }, 50);

        assert.ok(
          typeof intervalId === "number",
          "setInterval should return a number id",
        );
      });
    });

    test("should handle clearing interval", async () => {
      await new Promise((resolve, reject) => {
        let count = 0;
        const intervalId = setInterval(() => {
          count++;
        }, 50);

        setTimeout(() => {
          clearInterval(intervalId);
          const currentCount = count;

          setTimeout(() => {
            try {
              assert.equal(
                count,
                currentCount,
                "Count should not increase after clearInterval",
              );
              resolve();
            } catch (error) {
              reject(error);
            }
          }, 100);
        }, 125);
      });
    });
  });

  describe("edge cases", () => {
    test("should handle clearing non-existent timers", () => {
      clearTimeout(999999);
      clearInterval(999999);
    });

    test("should not expose the removed timers namespace", () => {
      assert.equal(globalThis.timers, undefined);
    });
  });
});
