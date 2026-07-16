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
    it("should initialize with empty headers when no arguments provided", () => {
      const emptyHeader = new Headers();
      expect(emptyHeader.has("Content-Type")).toBe(false);
      expect(emptyHeader.has("Accept")).toBe(false);
    });

    it("should initialize from another Headers instance", () => {
      const original = new Headers();
      original.set("Content-Type", "text/plain");
      original.set("X-Custom", "test");

      const newHeader = new Headers(original);
      expect(newHeader.get("Content-Type")).toBe("text/plain");
      expect(newHeader.get("X-Custom")).toBe("test");
    });

    it("should initialize from array of key-value pairs", () => {
      const pairs = [
        ["Content-Type", "application/json"],
        ["Accept", "text/plain"],
      ];
      const header = new Headers(pairs);
      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("Accept")).toBe("text/plain");
    });

    it("should initialize from object literal", () => {
      const init = {
        "Content-Type": "application/json",
        Accept: "text/plain",
      };
      const header = new Headers(init);
      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("Accept")).toBe("text/plain");
    });

    it("should handle case-insensitive headers during initialization", () => {
      const init = {
        "CONTENT-TYPE": "application/json",
        accept: "text/plain",
      };
      const header = new Headers(init);
      expect(header.get("content-type")).toBe("application/json");
      expect(header.get("ACCEPT")).toBe("text/plain");
    });

    it("should throw TypeError for invalid input types", () => {
      expect(() => new Headers(42)).toThrow(TypeError);
      expect(() => new Headers("invalid")).toThrow(TypeError);
      expect(() => new Headers(true)).toThrow(TypeError);
    });

    it("should throw TypeError for invalid header names in input", () => {
      expect(() => new Headers({ "": "empty" })).toThrow(TypeError);
      expect(() => new Headers({ "Invalid:Name": "value" })).toThrow(TypeError);
      expect(() => new Headers([["", "empty"]])).toThrow(TypeError);
    });

    it("should throw TypeError for invalid header values in input", () => {
      expect(() => new Headers({ "X-Test": "" })).toThrow(TypeError);
      expect(() => new Headers({ "X-Test": null })).toThrow(TypeError);
      expect(() => new Headers([["X-Test", ""]])).toThrow(TypeError);
    });
  });

  describe("set", () => {
    it("should set a single header", () => {
      header.set("Content-Type", "application/json");
      expect(header.get("Content-Type")).toBe("application/json");
    });

    it("should set multiple headers", () => {
      header.set("Content-Type", "application/json");
      header.set("Accept", "text/plain");

      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("Accept")).toBe("text/plain");
    });

    it("should override existing headers", () => {
      header.set("Content-Type", "text/plain");
      header.set("Content-Type", "application/json");
      expect(header.get("Content-Type")).toBe("application/json");
    });

    it("should handle case-insensitive header names", () => {
      header.set("content-type", "application/json");
      expect(header.get("Content-Type")).toBe("application/json");
      expect(header.get("content-TYPE")).toBe("application/json");
    });

    it("should throw TypeError for invalid header name", () => {
      expect(() => header.set("", "value")).toThrow(TypeError);
      expect(() => header.set("Invalid:Name", "value")).toThrow(TypeError);
    });

    it("should throw TypeError for invalid header value", () => {
      expect(() => header.set("Content-Type", "")).toThrow(TypeError);
      expect(() => header.set("Content-Type", null)).toThrow(TypeError);
    });
  });

  describe("append", () => {
    it("should throw TypeError for an invalid header name", () => {
      let error;
      try {
        header.append("Invalid:Name", "value");
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });

    it("should throw TypeError for an invalid header value", () => {
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

    it("should throw TypeError when called without arguments", () => {
      expect(() => header.get()).toThrow(TypeError);
    });

    it("should get a specific header value as string", () => {
      const value = header.get("Content-Type");
      expect(typeof value).toBe("string");
      expect(value).toBe("application/json");
    });

    it("should return null for non-existent headers", () => {
      expect(header.get("X-Custom")).toBe(null);
    });

    it("should be case-insensitive when getting headers", () => {
      expect(header.get("content-type")).toBe("application/json");
      expect(header.get("ACCEPT")).toBe("text/plain");
    });

    it("should throw TypeError for invalid header name", () => {
      expect(() => header.get("")).toThrow(TypeError);
      expect(() => header.get("Invalid:Name")).toThrow(TypeError);
    });
  });

  describe("has", () => {
    beforeEach(() => {
      header.set("Content-Type", "application/json");
    });

    it("should return true for existing headers", () => {
      expect(header.has("Content-Type")).toBe(true);
    });

    it("should return false for non-existent headers", () => {
      expect(header.has("X-Custom")).toBe(false);
    });

    it("should be case-insensitive", () => {
      expect(header.has("content-type")).toBe(true);
      expect(header.has("CONTENT-TYPE")).toBe(true);
    });

    it("should throw TypeError when called without arguments", () => {
      expect(() => header.has()).toThrow(TypeError);
    });

    it("should throw TypeError for invalid header name", () => {
      expect(() => header.has("")).toThrow(TypeError);
      expect(() => header.has("Invalid:Name")).toThrow(TypeError);
    });
  });

  describe("delete", () => {
    beforeEach(() => {
      header.set("Content-Type", "application/json");
      header.set("Accept", "text/plain");
    });

    it("should delete a specific header", () => {
      header.delete("Content-Type");
      expect(header.has("Content-Type")).toBe(false);
      expect(header.get("Accept")).toBe("text/plain");
    });

    it("should be case-insensitive when deleting", () => {
      header.delete("content-type");
      expect(header.has("Content-Type")).toBe(false);
    });

    it("should silently ignore deleting non-existent headers", () => {
      const beforeDelete = new Headers(header);
      header.delete("X-Custom");

      // Verify state remains unchanged
      expect(header.has("Content-Type")).toBe(beforeDelete.has("Content-Type"));
      expect(header.get("Content-Type")).toBe(beforeDelete.get("Content-Type"));
    });

    it("should throw TypeError when called without arguments", () => {
      expect(() => header.delete()).toThrow(TypeError);
    });

    it("should throw TypeError for an invalid header name", () => {
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
      it("should return an iterator of all header names", () => {
        const expectedKeys = new Set(["content-type", "accept", "x-custom"]);
        const foundKeys = new Set();

        for (const key of header.keys()) {
          expect(typeof key).toBe("string");
          expect(expectedKeys.has(key)).toBe(true);
          foundKeys.add(key);
        }

        expect(foundKeys.size).toBe(expectedKeys.size);
      });

      it("should return header names in lower case", () => {
        for (const key of header.keys()) {
          expect(key).toBe(key.toLowerCase());
        }
      });

      it("should support multiple iterations", () => {
        const iter1 = [...header.keys()];
        const iter2 = [...header.keys()];
        expect(iter1).toEqual(iter2);
      });
    });

    describe("values", () => {
      it("should return an iterator of all header values", () => {
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

      it("should support multiple iterations", () => {
        const iter1 = [...header.values()];
        const iter2 = [...header.values()];
        expect(iter1).toEqual(iter2);
      });
    });

    describe("entries", () => {
      it("should return an iterator of header [name, value] pairs", () => {
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

      it("should return header names in lower case", () => {
        for (const [key] of header.entries()) {
          expect(key).toBe(key.toLowerCase());
        }
      });

      it("should support multiple iterations", () => {
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
      it("should iterate over all headers", () => {
        const collected = new Map();
        header.forEach((value, key) => {
          collected.set(key, value);
        });

        expect(collected.get("content-type")).toBe("application/json");
        expect(collected.get("accept")).toBe("text/plain");
        expect(collected.get("x-custom")).toBe("test");
      });

      it("should call callback with correct this context when thisArg provided", () => {
        const thisArg = { test: true };
        header.forEach(function () {
          expect(this).toBe(thisArg);
        }, thisArg);
      });

      it("should use undefined as this when thisArg not provided", () => {
        header.forEach(function () {
          expect(this).toBeUndefined();
        });
      });

      it("should provide value, key, and headers object to callback", () => {
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

    it("should return empty array when no Set-Cookie headers present", () => {
      expect(header.getSetCookie()).toEqual([]);
    });

    it("should return array of Set-Cookie header values", () => {
      header.append("Set-Cookie", "cookie1=value1; Path=/");
      header.append("Set-Cookie", "cookie2=value2; Secure");

      const cookies = header.getSetCookie();
      expect(cookies).toEqual([
        "cookie1=value1; Path=/",
        "cookie2=value2; Secure",
      ]);
    });

    it("should preserve original Set-Cookie header values", () => {
      const cookie = "SessionId=123; Path=/; Secure; HttpOnly";
      header.append("Set-Cookie", cookie);

      expect(header.getSetCookie()).toEqual([cookie]);
    });

    it("should handle multiple Set-Cookie headers case-insensitively", () => {
      header.append("Set-Cookie", "cookie1=value1");
      header.append("set-cookie", "cookie2=value2");
      header.append("SET-COOKIE", "cookie3=value3");

      expect(header.getSetCookie().length).toBe(3);
    });
  });
});
