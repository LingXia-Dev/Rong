describe("Request", () => {
  describe("constructor", () => {
    test("should create a request with minimum required parameters", () => {
      const request = new Request("https://example.com");
      expect(request.url).toBe("https://example.com/");
      expect(request.method).toBe("GET");
      expect(request.headers instanceof Headers).toBe(true);
    });

    test("should create a request with all parameters", () => {
      const init = {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ key: "value" }),
        redirect: "follow",
      };
      const request = new Request("https://api.example.com", init);

      expect(request.url).toBe("https://api.example.com/");
      expect(request.method).toBe("POST");
      expect(request.headers.get("Content-Type")).toBe("application/json");
      expect(request.cache).toBe("no-cache");
      expect(request.redirect).toBe("follow");
    });

    test("should create a request from URL object", () => {
      const url = new URL("https://example.com/path?query=value");
      const request = new Request(url);
      expect(request.url).toBe("https://example.com/path?query=value");
      expect(request.method).toBe("GET");
      expect(request.headers instanceof Headers).toBe(true);
    });

    test("should throw TypeError for invalid URL", () => {
      let error;
      try {
        new Request("not-a-url");
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });

    test("should create a request from another Request", () => {
      const original = new Request("https://example.com/path", {
        method: "POST",
        body: "payload",
      });
      const copy = new Request(original);
      expect(copy.url).toBe(original.url);
      expect(copy.method).toBe("POST");
    });
  });

  describe("method validation", () => {
    test("should allow standard HTTP methods", () => {
      const methods = [
        "GET",
        "POST",
        "PUT",
        "DELETE",
        "HEAD",
        "OPTIONS",
        "PATCH",
      ];
      methods.forEach((method) => {
        const request = new Request("https://example.com", { method });
        expect(request.method).toBe(method);
      });
    });

    test("should normalize standard HTTP methods", () => {
      const request = new Request("https://example.com", { method: "post" });
      expect(request.method).toBe("POST");
    });

    test("should reject forbidden HTTP methods", () => {
      for (const method of ["CONNECT", "TRACE", "TRACK"]) {
        let error;
        try {
          new Request("https://example.com", { method });
        } catch (caught) {
          error = caught;
        }
        expect(error instanceof TypeError).toBe(true);
      }
    });

    test("should throw TypeError for invalid HTTP methods", () => {
      let error;
      try {
        new Request("https://example.com", { method: "BAD METHOD" });
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });

    test("should reject a signal that is not an AbortSignal", () => {
      let error;
      try {
        new Request("https://example.com", { signal: {} });
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });
  });

  describe("body handling", () => {
    test("should not allow body for GET/HEAD requests", () => {
      for (const method of ["GET", "HEAD"]) {
        let error;
        try {
          new Request("https://example.com", { method, body: "test" });
        } catch (caught) {
          error = caught;
        }
        expect(error instanceof TypeError).toBe(true);
      }
    });

    test("should allow body for POST requests", () => {
      const bodies = [
        JSON.stringify({ test: "data" }),
        new URLSearchParams("key=value"),
        "plain text",
      ];

      bodies.forEach((body) => {
        const request = new Request("https://example.com", {
          method: "POST",
          body,
        });
        expect(request.method).toBe("POST");
      });
    });

    describe("text()", () => {
      test("should handle string body", async () => {
        const body = "Hello, World!";
        const request = new Request("https://example.com", {
          method: "POST",
          body,
        });
        const text = await request.text();
        expect(text).toBe(body);
      });

      test("should handle URLSearchParams body", async () => {
        const params = new URLSearchParams();
        params.append("key1", "value1");
        params.append("key2", "value2");
        const request = new Request("https://example.com", {
          method: "POST",
          body: params,
        });
        const text = await request.text();
        expect(text).toBe("key1=value1&key2=value2");
      });

      test("should handle ArrayBuffer body", async () => {
        const text = "Hello, ArrayBuffer!";
        const encoder = new TextEncoder();
        const buffer = encoder.encode(text).buffer;
        const request = new Request("https://example.com", {
          method: "POST",
          body: buffer,
        });
        const result = await request.text();
        expect(result).toBe(text);
      });

      test("should convert other body values to strings", async () => {
        const request = new Request("https://example.com", {
          method: "POST",
          body: {
            toString() {
              return "custom body";
            },
          },
        });
        expect(await request.text()).toBe("custom body");
      });

      test("should return empty string for null body", async () => {
        const request = new Request("https://example.com");
        const text = await request.text();
        expect(text).toBe("");
      });
    });

    describe("arrayBuffer()", () => {
      test("should handle string body", async () => {
        const body = "Hello, World!";
        const request = new Request("https://example.com", {
          method: "POST",
          body,
        });
        const buffer = await request.arrayBuffer();
        const text = new TextDecoder().decode(buffer);
        expect(text).toBe(body);
      });

      test("should handle URLSearchParams body", async () => {
        const params = new URLSearchParams();
        params.append("key1", "value1");
        params.append("key2", "value2");
        const request = new Request("https://example.com", {
          method: "POST",
          body: params,
        });
        const buffer = await request.arrayBuffer();
        const text = new TextDecoder().decode(buffer);
        expect(text).toBe("key1=value1&key2=value2");
      });

      test("should handle ArrayBuffer body", async () => {
        const text = "Hello, ArrayBuffer!";
        const encoder = new TextEncoder();
        const originalBuffer = encoder.encode(text).buffer;
        const request = new Request("https://example.com", {
          method: "POST",
          body: originalBuffer,
        });
        const buffer = await request.arrayBuffer();
        const result = new TextDecoder().decode(buffer);
        expect(result).toBe(text);
      });

      test("should return empty ArrayBuffer for null body", async () => {
        const request = new Request("https://example.com");
        const buffer = await request.arrayBuffer();
        expect(buffer.byteLength).toBe(0);
      });
    });
  });

  describe("clone", () => {
    test("should create an identical copy of the request", () => {
      const original = new Request("https://example.com", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ test: "data" }),
      });

      const clone = original.clone();

      expect(clone.url).toBe(original.url);
      expect(clone.method).toBe(original.method);
      expect(clone.headers.get("Content-Type")).toBe(
        original.headers.get("Content-Type"),
      );
      expect(clone).not.toBe(original);
    });

    test("should reject cloning after the body is consumed", async () => {
      const request = new Request("https://example.com", {
        method: "POST",
        body: "payload",
      });
      await request.text();
      expect(request.bodyUsed).toBe(true);
      let error;
      try {
        request.clone();
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });
  });
});
