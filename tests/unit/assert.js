describe("assert module", () => {
  describe("assert.ok()", () => {
    test("should pass for truthy values", () => {
      assert.ok(true);
      assert.ok(1);
      assert.ok("test");
      assert.ok({});
      assert.ok([]);
    });

    test("should throw for falsy values", () => {
      expect(() => assert.ok(false)).toThrow();
      expect(() => assert.ok(0)).toThrow();
      expect(() => assert.ok("")).toThrow();
      expect(() => assert.ok(null)).toThrow();
      expect(() => assert.ok(undefined)).toThrow();
    });

    test("should include custom message in error", () => {
      const message = "Custom error message";
      try {
        assert.ok(false, message);
      } catch (e) {
        expect(e.message).toContain(message);
      }
    });
  });

  describe("assert.equal()", () => {
    test("should pass for equal values", () => {
      assert.equal(1, 1);
      assert.equal("test", "test");
      assert.equal(null, null);
      assert.equal(undefined, undefined);
    });

    test("should throw for unequal values", () => {
      expect(() => assert.equal(1, 2)).toThrow();
      expect(() => assert.equal("a", "b")).toThrow();
    });

    test("should use loose equality", () => {
      assert.equal(1, "1");
      assert.equal(0, false);
      assert.equal("", false);
    });

    test("should include custom message in error", () => {
      const message = "Custom equality message";
      try {
        assert.equal(1, 2, message);
      } catch (e) {
        expect(e.message).toContain(message);
      }
    });
  });

  describe("assert.fail()", () => {
    test("should always throw", () => {
      expect(() => assert.fail()).toThrow();
    });

    test("should include custom message in error", () => {
      const message = "Custom fail message";
      try {
        assert.fail(message);
      } catch (e) {
        expect(e.message).toContain(message);
      }
    });
  });

  describe("assert.doesNotThrow()", () => {
    test("should pass for functions that don't throw", () => {
      assert.doesNotThrow(() => {
        // This function doesn't throw
        return true;
      });
    });

    test("should throw for non-function arguments", () => {
      expect(() => assert.doesNotThrow("not a function")).toThrow();
      expect(() => assert.doesNotThrow(123)).toThrow();
      expect(() => assert.doesNotThrow({})).toThrow();
    });

    test("should include custom message in error", () => {
      const message = "Custom doesNotThrow message";
      try {
        assert.doesNotThrow(() => {
          throw new Error("Test error");
        }, message);
      } catch (e) {
        expect(e.message).toContain(message);
      }
    });
  });

  describe("assert.fail()", () => {
    test("should always throw", () => {
      expect(() => assert.fail()).toThrow();
    });

    test("should include custom message in error", () => {
      const message = "Custom fail message";
      try {
        assert.fail(message);
      } catch (e) {
        expect(e.message).toContain(message);
      }
    });

    test("should throw with default message when no args", () => {
      try {
        assert.fail();
      } catch (e) {
        expect(e.message).toBe("Failed");
      }
    });

    test("should throw with provided error object", () => {
      const error = new Error("Custom error");
      try {
        assert.fail(error);
      } catch (e) {
        expect(e).toBe(error);
      }
    });
  });
});

describe("Custom message handling", () => {
  describe("assert.ok()", () => {
    test("should use default message when no custom message provided", () => {
      try {
        assert.ok(false);
      } catch (e) {
        expect(e.message).toBe(
          "AssertionError: The expression was evaluated to a falsy value",
        );
      }
    });

    test("should use custom string message", () => {
      const message = "Custom ok message";
      try {
        assert.ok(false, message);
      } catch (e) {
        expect(e.message).toBe(message);
      }
    });

    test("should throw custom error object", () => {
      const error = new Error("Custom error");
      try {
        assert.ok(false, error);
      } catch (e) {
        expect(e).toBe(error);
      }
    });

    test("should throw non-string values as-is", () => {
      const testValues = [123, { key: "value" }];

      testValues.forEach((value) => {
        try {
          assert.ok(false, value);
        } catch (e) {
          expect(e).toBe(value);
        }
      });
    });
  });

  describe("assert.equal()", () => {
    test("should use default message when no custom message provided", () => {
      try {
        assert.equal(1, 2);
      } catch (e) {
        expect(e.message).toBe("AssertionError: It's not equal!");
      }
    });

    test("should use custom string message", () => {
      const message = "Custom equality message";
      try {
        assert.equal(1, 2, message);
      } catch (e) {
        expect(e.message).toBe(message);
      }
    });

    test("should throw custom error object", () => {
      const error = new Error("Custom equality error");
      try {
        assert.equal(1, 2, error);
      } catch (e) {
        expect(e).toBe(error);
      }
    });
  });
});
