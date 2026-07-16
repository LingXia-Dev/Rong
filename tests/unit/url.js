describe("URL and URLSearchParams", () => {
  describe("URL Basic functionality", () => {
    it("should parse URL with query parameters", () => {
      const url = new URL("https://example.com/path?name=Alice&age=30");
      expect(url.href).toBe("https://example.com/path?name=Alice&age=30");
      expect(url.search).toBe("?name=Alice&age=30");
    });

    it("should handle URL with empty query", () => {
      const url = new URL("https://example.com/");
      expect(url.search).toBe("");
      expect(url.searchParams.size).toBe(0);
    });

    it("should handle URL with hash", () => {
      const url = new URL("https://example.com/#section1");
      expect(url.hash).toBe("#section1");
      url.hash = "#section2";
      expect(url.href).toBe("https://example.com/#section2");
    });

    it("should handle URL with base", () => {
      const url = new URL("/path", "https://example.com");
      expect(url.href).toBe("https://example.com/path");
    });

    it("should handle URL with invalid base", () => {
      expect(() => new URL("/path", "invalid")).toThrow(/Invalid base URL/);
    });

    it("should handle URL with invalid URL", () => {
      expect(() => new URL("invalid")).toThrow(/Invalid URL/);
    });

    it("should handle URL with pathname", () => {
      const url = new URL("https://example.com/path");
      expect(url.pathname).toBe("/path");
      url.pathname = "/new-path";
      expect(url.href).toBe("https://example.com/new-path");
    });

    it("should handle URL with port", () => {
      const url = new URL("https://example.com:8080");
      expect(url.port).toBe("8080");
      url.port = "9090";
      expect(url.href).toBe("https://example.com:9090/");
    });

    it("should handle URL with protocol", () => {
      const url = new URL("https://example.com");
      expect(url.protocol).toBe("https:");
      url.protocol = "http:";
      expect(url.href).toBe("http://example.com/");
    });

    it("should handle URL with username and password", () => {
      const url = new URL("https://user:pass@example.com");
      expect(url.username).toBe("user");
      expect(url.password).toBe("pass");
      url.username = "new-user";
      url.password = "new-pass";
      expect(url.href).toBe("https://new-user:new-pass@example.com/");
    });

    it("should handle URL with origin", () => {
      const url = new URL("https://example.com");
      expect(url.origin).toBe("https://example.com");
    });

    it("should handle URL with toJSON", () => {
      const url = new URL("https://example.com");
      expect(url.toJSON()).toBe("https://example.com/");
    });

    it("should handle URL with toString", () => {
      const url = new URL("https://example.com");
      expect(url.toString()).toBe("https://example.com/");
    });

    it("should remove an empty query from the serialized URL", () => {
      const url = new URL("https://example.com/?a=1");
      url.search = "";
      expect(url.search).toBe("");
      expect(url.href).toBe("https://example.com/");

      url.search = "b=2";
      url.searchParams.delete("b");
      expect(url.search).toBe("");
      expect(url.href).toBe("https://example.com/");
    });
  });

  describe("URLSearchParams functionality", () => {
    it("should parse query parameters from URL", () => {
      const url = new URL("https://example.com/?name=Alice&age=30");
      const params = url.searchParams;
      expect(params.get("name")).toBe("Alice");
      expect(params.get("age")).toBe("30");
    });

    it("should build URL with query parameters", () => {
      const url = new URL("https://example.com/search");
      const params = new URLSearchParams();
      params.append("q", "rust");
      params.append("lang", "en");
      url.search = params.toString();
      expect(url.href).toBe("https://example.com/search?q=rust&lang=en");
    });

    it("should modify existing query parameters", () => {
      const url = new URL("https://example.com/?a=1&b=2");
      const params = url.searchParams;
      params.set("a", "10");
      params.delete("b");
      params.append("c", "3");
      expect(url.href).toBe("https://example.com/?a=10&c=3");
    });

    it("should sync URL when modifying URLSearchParams", () => {
      const url = new URL("https://example.com/?x=1&y=2");
      const params = url.searchParams;
      params.set("x", "10");
      params.append("z", "3");
      expect(url.href).toBe("https://example.com/?x=10&y=2&z=3");
    });

    it("should iterate over URLSearchParams", () => {
      const url = new URL("https://example.com/?x=10&y=2&z=3");
      const params = url.searchParams;
      const collected = [];
      params.forEach((value, key) => {
        collected.push(`${key}=${value}`);
      });
      expect(collected.join(";")).toBe("x=10;y=2;z=3");
    });

    it("should create URLSearchParams from object", () => {
      const params = new URLSearchParams({ a: "1", b: "2" });
      expect(params.toString()).toBe("a=1&b=2");
    });

    it("should create URLSearchParams from array", () => {
      const params = new URLSearchParams([
        ["a", "1"],
        ["b", "2"],
      ]);
      expect(params.toString()).toBe("a=1&b=2");
    });

    it("should ignore one leading question mark in a query string", () => {
      const params = new URLSearchParams("?a=1&b=2");
      expect(params.get("a")).toBe("1");
      expect(params.get("?a")).toBe(null);
      expect(params.toString()).toBe("a=1&b=2");
    });

    it("should sort URLSearchParams", () => {
      const params = new URLSearchParams("c=3&a=1&b=2");
      params.sort();
      expect(params.toString()).toBe("a=1&b=2&c=3");
    });

    it("should handle URLSearchParams with getAll", () => {
      const params = new URLSearchParams("a=1&a=2&b=3");
      expect(params.getAll("a")).toEqual(["1", "2"]);
    });

    it("should handle URLSearchParams with has", () => {
      const params = new URLSearchParams("a=1&b=2");
      expect(params.has("a")).toBe(true);
      expect(params.has("c")).toBe(false);
    });

    it("should handle URLSearchParams with keys", () => {
      const params = new URLSearchParams("a=1&b=2");
      expect(params.keys()).toEqual(["a", "b"]);
    });

    it("should handle URLSearchParams with values", () => {
      const params = new URLSearchParams("a=1&b=2");
      expect(params.values()).toEqual(["1", "2"]);
    });

    it("should handle URLSearchParams with entries", () => {
      const params = new URLSearchParams("a=1&b=2");
      const entries = params.entries();
      // Convert entries to an array and compare
      const entriesArray = Array.from(entries);
      expect(entriesArray.length).toBe(2);
      expect(entriesArray[0][0]).toBe("a");
      expect(entriesArray[0][1]).toBe("1");
      expect(entriesArray[1][0]).toBe("b");
      expect(entriesArray[1][1]).toBe("2");
    });
  });
});
