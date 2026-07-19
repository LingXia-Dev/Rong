describe("Headers", () => {
  let header;

  beforeEach(() => {
    // Ensure we create a fresh Headers instance for each test
    header = new Headers();
  });

  afterEach(() => {
    // Clean up after each test
    header = null;
  });

  describe("constructor", () => {
    test("should initialize with empty headers when no arguments provided", () => {
      const emptyHeader = new Headers();
      expect(emptyHeader.has("Content-Type")).toBe(false);
      expect(emptyHeader.has("Accept")).toBe(false);
    });

    test("should initialize from another Headers instance", () => {
      const original = new Headers();
      original.set("Content-Type", "text/plain");
      original.set("X-Custom", "test");

      const newHeader = new Headers(original);
      expect(newHeader.get("Content-Type")).toBe("text/plain");
      expect(newHeader.get("X-Custom")).toBe("test");
    });

    test("should initialize from array of key-value pairs", () => {
      const pairs = [
        ["Content-Type", "application/json"],
        ["Accept", "text/plain"],
      ];
      const header = new Headers(pairs);
      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("Accept")).toBe("text/plain");
    });

    test("should initialize from object literal", () => {
      const init = {
        "Content-Type": "application/json",
        Accept: "text/plain",
      };
      const header = new Headers(init);
      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("Accept")).toBe("text/plain");
    });

    test("should handle case-insensitive headers during initialization", () => {
      const init = {
        "CONTENT-TYPE": "application/json",
        accept: "text/plain",
      };
      const header = new Headers(init);
      expect(header.get("content-type")).toBe("application/json");
      expect(header.get("ACCEPT")).toBe("text/plain");
    });

    test("should throw TypeError for invalid input types", () => {
      expect(() => new Headers(42)).toThrow(TypeError);
      expect(() => new Headers("invalid")).toThrow(TypeError);
      expect(() => new Headers(true)).toThrow(TypeError);
    });

    test("should throw TypeError for invalid header names in input", () => {
      expect(() => new Headers({ "": "empty" })).toThrow(TypeError);
      expect(() => new Headers({ "Invalid:Name": "value" })).toThrow(TypeError);
      expect(() => new Headers([["", "empty"]])).toThrow(TypeError);
    });

    test("should accept empty and coerce null header values in input", () => {
      expect(new Headers({ "X-Test": "" }).get("X-Test")).toBe("");
      expect(new Headers({ "X-Test": null }).get("X-Test")).toBe("null");
      expect(new Headers([["X-Test", ""]]).get("X-Test")).toBe("");
    });
  });

  describe("set", () => {
    test("should set a single header", () => {
      header.set("Content-Type", "application/json");
      expect(header.get("Content-Type")).toBe("application/json");
    });

    test("should set multiple headers", () => {
      header.set("Content-Type", "application/json");
      header.set("Accept", "text/plain");

      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("Accept")).toBe("text/plain");
    });

    test("should override existing headers", () => {
      header.set("Content-Type", "text/plain");
      header.set("Content-Type", "application/json");
      expect(header.get("Content-Type")).toBe("application/json");
    });

    test("should handle case-insensitive header names", () => {
      header.set("content-type", "application/json");
      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("content-TYPE")).toBe("application/json");
    });

    test("should throw TypeError for invalid header name", () => {
      expect(() => header.set("", "value")).toThrow(TypeError);
      expect(() => header.set("Invalid:Name", "value")).toThrow(TypeError);
    });

    test("should accept empty and coerce null header values", () => {
      header.set("Content-Type", "");
      expect(header.get("Content-Type")).toBe("");
      header.set("Content-Type", null);
      expect(header.get("Content-Type")).toBe("null");
    });
  });

  describe("append", () => {
    test("should throw TypeError for an invalid header name", () => {
      let error;
      try {
        header.append("Invalid:Name", "value");
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });

    test("should throw TypeError for an invalid header value", () => {
      let error;
      try {
        header.append("X-Test", "bad\nvalue");
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });
  });

  describe("get", () => {
    beforeEach(() => {
      header.set("Content-Type", "application/json");
      header.set("Accept", "text/plain");
    });

    test("should throw TypeError when called without arguments", () => {
      expect(() => header.get()).toThrow(TypeError);
    });

    test("should get a specific header value as string", () => {
      const value = header.get("Content-Type");
      expect(typeof value).toBe("string");
      expect(value).toBe("application/json");
    });

    test("should return null for non-existent headers", () => {
      expect(header.get("X-Custom")).toBe(null);
    });

    test("should be case-insensitive when getting headers", () => {
      expect(header.get("content-type")).toBe("application/json");
      expect(header.get("ACCEPT")).toBe("text/plain");
    });

    test("should throw TypeError for invalid header name", () => {
      expect(() => header.get("")).toThrow(TypeError);
      expect(() => header.get("Invalid:Name")).toThrow(TypeError);
    });
  });

  describe("has", () => {
    beforeEach(() => {
      header.set("Content-Type", "application/json");
    });

    test("should return true for existing headers", () => {
      expect(header.has("Content-Type")).toBe(true);
    });

    test("should return false for non-existent headers", () => {
      expect(header.has("X-Custom")).toBe(false);
    });

    test("should be case-insensitive", () => {
      expect(header.has("content-type")).toBe(true);
      expect(header.has("CONTENT-TYPE")).toBe(true);
    });

    test("should throw TypeError when called without arguments", () => {
      expect(() => header.has()).toThrow(TypeError);
    });

    test("should throw TypeError for invalid header name", () => {
      expect(() => header.has("")).toThrow(TypeError);
      expect(() => header.has("Invalid:Name")).toThrow(TypeError);
    });
  });

  describe("delete", () => {
    beforeEach(() => {
      header.set("Content-Type", "application/json");
      header.set("Accept", "text/plain");
    });

    test("should delete a specific header", () => {
      header.delete("Content-Type");
      expect(header.has("Content-Type")).toBe(false);
      expect(header.get("Accept")).toBe("text/plain");
    });

    test("should be case-insensitive when deleting", () => {
      header.delete("content-type");
      expect(header.has("Content-Type")).toBe(false);
    });

    test("should silently ignore deleting non-existent headers", () => {
      const beforeDelete = new Headers(header);
      header.delete("X-Custom");

      // Verify state remains unchanged
      expect(header.has("Content-Type")).toBe(beforeDelete.has("Content-Type"));
      expect(header.get("Content-Type")).toBe(beforeDelete.get("Content-Type"));
    });

    test("should throw TypeError when called without arguments", () => {
      expect(() => header.delete()).toThrow(TypeError);
    });

    test("should throw TypeError for an invalid header name", () => {
      let error;
      try {
        header.delete("Invalid:Name");
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });
  });

  describe("iteration methods", () => {
    beforeEach(() => {
      header.set("Content-Type", "application/json");
      header.set("Accept", "text/plain");
      header.set("X-Custom", "test");
    });

    describe("keys", () => {
      test("should return an iterator of all header names", () => {
        const expectedKeys = new Set(["content-type", "accept", "x-custom"]);
        const foundKeys = new Set();

        for (const key of header.keys()) {
          expect(typeof key).toBe("string");
          expect(expectedKeys.has(key)).toBe(true);
          foundKeys.add(key);
        }

        expect(foundKeys.size).toBe(expectedKeys.size);
      });

      test("should return header names in lower case", () => {
        for (const key of header.keys()) {
          expect(key).toBe(key.toLowerCase());
        }
      });

      test("should support multiple iterations", () => {
        const iter1 = [...header.keys()];
        const iter2 = [...header.keys()];
        expect(iter1).toEqual(iter2);
      });
    });

    describe("values", () => {
      test("should return an iterator of all header values", () => {
        const expectedValues = new Set([
          "application/json",
          "text/plain",
          "test",
        ]);
        const foundValues = new Set();

        for (const value of header.values()) {
          expect(typeof value).toBe("string");
          expect(expectedValues.has(value)).toBe(true);
          foundValues.add(value);
        }

        expect(foundValues.size).toBe(expectedValues.size);
      });

      test("should support multiple iterations", () => {
        const iter1 = [...header.values()];
        const iter2 = [...header.values()];
        expect(iter1).toEqual(iter2);
      });
    });

    describe("entries", () => {
      test("should return an iterator of header [name, value] pairs", () => {
        const expectedEntries = new Map([
          ["content-type", "application/json"],
          ["accept", "text/plain"],
          ["x-custom", "test"],
        ]);
        const foundEntries = new Map();

        for (const [key, value] of header.entries()) {
          expect(typeof key).toBe("string");
          expect(typeof value).toBe("string");
          expect(expectedEntries.get(key)).toBe(value);
          foundEntries.set(key, value);
        }

        expect(foundEntries.size).toBe(expectedEntries.size);
      });

      test("should return header names in lower case", () => {
        for (const [key] of header.entries()) {
          expect(key).toBe(key.toLowerCase());
        }
      });

      test("should support multiple iterations", () => {
        const iter1 = [...header.entries()];
        const iter2 = [...header.entries()];
        expect(iter1.length).toBe(iter2.length);
        for (let i = 0; i < iter1.length; i++) {
          expect(iter1[i][0]).toBe(iter2[i][0]);
          expect(iter1[i][1]).toBe(iter2[i][1]);
        }
      });
    });

    describe("forEach", () => {
      test("should iterate over all headers", () => {
        const collected = new Map();
        header.forEach((value, key) => {
          collected.set(key, value);
        });

        expect(collected.get("content-type")).toBe("application/json");
        expect(collected.get("accept")).toBe("text/plain");
        expect(collected.get("x-custom")).toBe("test");
      });

      test("should call callback with correct this context when thisArg provided", () => {
        const thisArg = { test: true };
        header.forEach(function () {
          expect(this).toBe(thisArg);
        }, thisArg);
      });

      test("should use undefined as this when thisArg not provided", () => {
        header.forEach(function () {
          expect(this).toBeUndefined();
        });
      });

      test("should provide value, key, and headers object to callback", () => {
        header.forEach((value, key, hdrs) => {
          expect(typeof value).toBe("string");
          expect(typeof key).toBe("string");
          expect(hdrs).toBe(header);
        });
      });
    });
  });

  describe("getSetCookie", () => {
    beforeEach(() => {
      header = new Headers();
    });

    test("should return empty array when no Set-Cookie headers present", () => {
      expect(header.getSetCookie()).toEqual([]);
    });

    test("should return array of Set-Cookie header values", () => {
      header.append("Set-Cookie", "cookie1=value1; Path=/");
      header.append("Set-Cookie", "cookie2=value2; Secure");

      const cookies = header.getSetCookie();
      expect(cookies).toEqual([
        "cookie1=value1; Path=/",
        "cookie2=value2; Secure",
      ]);
    });

    test("should preserve original Set-Cookie header values", () => {
      const cookie = "SessionId=123; Path=/; Secure; HttpOnly";
      header.append("Set-Cookie", cookie);

      expect(header.getSetCookie()).toEqual([cookie]);
    });

    test("should handle multiple Set-Cookie headers case-insensitively", () => {
      header.append("Set-Cookie", "cookie1=value1");
      header.append("set-cookie", "cookie2=value2");
      header.append("SET-COOKIE", "cookie3=value3");

      expect(header.getSetCookie().length).toBe(3);
    });
  });
});
