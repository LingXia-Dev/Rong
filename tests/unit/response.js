describe("Response", () => {
  describe("constructor", () => {
    it("should create an empty response", () => {
      const response = new Response();
      expect(response.ok).toBe(true);
      expect(response.status).toBe(200);
      expect(response.statusText).toBe("");
      expect(response.headers instanceof Headers).toBe(true);
    });

    it("should create a response with body and init parameters", () => {
      const init = {
        status: 201,
        statusText: "Created",
        headers: { "Content-Type": "application/json" },
      };
      const body = JSON.stringify({ id: 1 });
      const response = new Response(body, init);

      expect(response.status).toBe(201);
      expect(response.statusText).toBe("Created");
      expect(response.headers.get("Content-Type")).toBe("application/json");
      expect(response.ok).toBe(true);
    });
  });

  describe("status validation", () => {
    it("should reject informational status codes", () => {
      let error;
      try {
        new Response(null, { status: 199 });
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof RangeError).toBe(true);
    });

    it("should set ok to true for successful status codes", () => {
      const successCodes = [200, 201, 202, 203, 204, 206, 207, 299];
      successCodes.forEach((status) => {
        const response = new Response(null, { status });
        expect(response.ok).toBe(true);
      });
    });

    it("should set ok to false for error status codes", () => {
      const errorCodes = [400, 404, 500, 503];
      errorCodes.forEach((status) => {
        const response = new Response(null, { status });
        expect(response.ok).toBe(false);
      });
    });

    it("should throw TypeError for invalid status codes", () => {
      expect(() => new Response(null, { status: 99 })).toThrow(TypeError);
      expect(() => new Response(null, { status: 600 })).toThrow(TypeError);
    });
  });

  describe("body handling", () => {
    it("should reject a body for a null-body status", () => {
      let error;
      try {
        new Response("body", { status: 204 });
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });

    it("should handle different body types", async () => {
      const testCases = [
        {
          body: JSON.stringify({ test: "data" }),
          type: "application/json",
          check: async (response) => {
            const data = await response.json();
            expect(data.test).toBe("data");
          },
        },
        {
          body: "plain text",
          type: "text/plain",
          check: async (response) => {
            const text = await response.text();
            expect(text).toBe("plain text");
          },
        },
        {
          body: new ArrayBuffer(8),
          type: "application/octet-stream",
          check: async (response) => {
            const buffer = await response.arrayBuffer();
            expect(buffer instanceof ArrayBuffer).toBe(true);
            expect(buffer.byteLength).toBe(8);
          },
        },
        {
          body: "blob content",
          type: "text/plain",
          check: async (response) => {
            const blob = await response.blob();
            expect(blob instanceof Blob).toBe(true);
            expect(blob.type).toBe("text/plain");
            const text = await blob.text();
            expect(text).toBe("blob content");
          },
        },
      ];

      for (const testCase of testCases) {
        const response = new Response(testCase.body, {
          headers: { "Content-Type": testCase.type },
        });
        await testCase.check(response);
      }
    });

    it("should return the same body stream object across getter calls (buffered)", () => {
      const response = new Response("hello world");
      const b1 = response.body;
      const b2 = response.body;
      expect(b1 === b2).toBe(true);
    });

    it("should return the same body stream object across getter calls (JS body)", () => {
      const buf = new Uint8Array([1, 2, 3]);
      const response = new Response(buf);
      const b1 = response.body;
      const b2 = response.body;
      expect(b1 === b2).toBe(true);
    });

    it("should not allow multiple body reads", async () => {
      const response = new Response(JSON.stringify({ ok: true }), {
        headers: { "Content-Type": "application/json" },
      });

      const a = await response.json();
      expect(a.ok).toBe(true);

      // Second read must fail per spec.
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

    it("should parse formData from urlencoded body", async () => {
      const params = new URLSearchParams();
      params.append("name", "Alice");
      params.append("age", "30");

      const response = new Response(params.toString(), {
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
      });

      const form = await response.formData();
      expect(form.get("name")).toBe("Alice");
      expect(form.get("age")).toBe("30");
    });

    it("should parse formData from multipart body", async () => {
      const boundary = "----RongBoundaryTest";
      const body =
        `--${boundary}\r\n` +
        `Content-Disposition: form-data; name=\"field\"\r\n\r\n` +
        `value\r\n` +
        `--${boundary}\r\n` +
        `Content-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\n` +
        `Content-Type: text/plain\r\n\r\n` +
        `hello\r\n` +
        `--${boundary}--\r\n`;

      const response = new Response(body, {
        headers: { "Content-Type": `multipart/form-data; boundary=${boundary}` },
      });

      const form = await response.formData();
      expect(form.get("field")).toBe("value");

      const file = form.get("file");
      expect(file instanceof File).toBe(true);
      expect(file.name).toBe("test.txt");
      const text = await file.text();
      expect(text).toBe("hello");
    });
  });

  describe("clone", () => {
    it("should create an identical copy of the response", async () => {
      const original = new Response("test data", {
        status: 200,
        headers: { "Content-Type": "text/plain" },
      });

      const clone = original.clone();

      expect(clone.status).toBe(original.status);
      expect(clone.headers.get("Content-Type")).toBe(
        original.headers.get("Content-Type"),
      );
      expect(clone).not.toBe(original);

      const originalText = await original.text();
      const cloneText = await clone.text();
      expect(cloneText).toBe(originalText);
    });

    it("should reject cloning after the body is consumed", async () => {
      const response = new Response("payload");
      await response.text();
      let error;
      try {
        response.clone();
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });
  });

  describe("error handling", () => {
    it("should reject line breaks in statusText", () => {
      let error;
      try {
        new Response(null, { statusText: "bad\ntext" });
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });

    it("should create error responses", () => {
      const response = Response.error();
      expect(response.status).toBe(0);
      expect(response.statusText).toBe("");
      expect(response.ok).toBe(false);
    });

    it("should create redirect responses", () => {
      const url = "https://example.com/new-location";
      const response = Response.redirect(url, 301);
      expect(response.status).toBe(301);
      expect(response.headers.get("Location")).toBe(url);
    });

    it("should throw TypeError for invalid redirect status", () => {
      expect(() => Response.redirect("https://example.com", 200)).toThrow(
        TypeError,
      );
    });

    it("should reject a relative redirect URL", () => {
      let error;
      try {
        Response.redirect("/relative", 302);
      } catch (caught) {
        error = caught;
      }
      expect(error instanceof TypeError).toBe(true);
    });
  });
});
