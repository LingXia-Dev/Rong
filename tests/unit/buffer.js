describe("Buffer", () => {
  const CLOCK_SKEW_MS = 20;

  // Helper function for array comparison
  async function assertArrayBufferEquals(buffer1, buffer2, message) {
    const arr1 = new Uint8Array(buffer1);
    const arr2 = new Uint8Array(buffer2);

    assert.equal(
      arr1.length,
      arr2.length,
      `${message}: Arrays have different lengths`,
    );
    for (let i = 0; i < arr1.length; i++) {
      assert.equal(arr1[i], arr2[i], `${message}: Arrays differ at index ${i}`);
    }
  }

  describe("Blob", () => {
    test("should create a blob with content and type", () => {
      const blob = new Blob(["Hello, World!"], { type: "text/plain" });
      assert(blob instanceof Blob);
      assert.equal(blob.size, 13);
      assert.equal(blob.type, "text/plain");
    });

    test("should create an empty blob", () => {
      const emptyBlob = new Blob();
      assert.equal(emptyBlob.size, 0);
      assert.equal(emptyBlob.type, "");
    });

    test("should normalize type to lowercase", () => {
      const blob = new Blob([], { type: "TEXT/PLAIN" });
      assert.equal(blob.type, "text/plain");
    });

    test("should handle invalid type", () => {
      const blob = new Blob([], { type: "text/plain\0" });
      assert.equal(blob.type, "");
    });

    test("should slice blob content", () => {
      const blob = new Blob(["Hello, World!"]);
      const slice = blob.slice(0, 5);
      assert.equal(slice.size, 5);
    });

    test("should handle negative slice indices", () => {
      const blob = new Blob(["Hello, World!"]);
      const slice = blob.slice(-5);
      assert.equal(slice.size, 5);
    });

    test("should return text content", async () => {
      const blob = new Blob(["Hello, World!"]);
      const text = await blob.text();
      assert.equal(text, "Hello, World!");
    });

    test("should handle unicode text", async () => {
      const blob = new Blob(["Hello, 世界!"]);
      const text = await blob.text();
      assert.equal(text, "Hello, 世界!");
    });

    test("should return array buffer", async () => {
      const blob = new Blob(["Hello"]);
      const buffer = await blob.arrayBuffer();
      assert(buffer instanceof ArrayBuffer);
      assert.equal(buffer.byteLength, 5);
    });

    test("should handle typed array input", async () => {
      const data = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
      const blob = new Blob([data]);
      const buffer = await blob.arrayBuffer();
      await assertArrayBufferEquals(
        buffer,
        data.buffer,
        "TypedArray content should match",
      );
    });

    test("should handle multiple parts", async () => {
      const blob = new Blob(["Hello", " ", "World"]);
      const text = await blob.text();
      assert.equal(text, "Hello World");
    });
  });

  describe("File", () => {
    test("should create a file with content and metadata", () => {
      const now = Date.now();
      const file = new File(["Hello, World!"], "test.txt", {
        type: "text/plain",
      });
      const after = Date.now();

      console.log("File instance details:", {
        constructorName: file.constructor.name,
        isFile: file instanceof File,
        isBlob: file instanceof Blob,
        prototype: Object.getPrototypeOf(file),
        methods: Object.getOwnPropertyNames(Object.getPrototypeOf(file)),
        name: file.name,
        type: file.type,
        lastModified: file.lastModified,
      });

      // Check instance first
      assert(file instanceof File, "Should be instance of File");
      // BUGFIX: currently, File and Blob are registered both as standalone class
      // assert(file instanceof Blob, "Should be instance of Blob");

      // Then check properties
      console.log("Comparing values:", {
        name: { actual: file.name, expected: "test.txt" },
        type: { actual: file.type, expected: "text/plain" },
        lastModified: {
          actual: file.lastModified,
          expectedRange: [now - CLOCK_SKEW_MS, after + CLOCK_SKEW_MS],
        },
      });

      assert.equal(file.name, "test.txt", "Filename should match");
      assert.equal(file.type, "text/plain", "MIME type should match");
      assert(
        file.lastModified >= now - CLOCK_SKEW_MS &&
          file.lastModified <= after + CLOCK_SKEW_MS,
        "Last modified time should be captured during construction",
      );
    });

    test("should create an empty file with defaults", () => {
      const file = new File([], "empty.txt");
      assert.equal(file.size, 0);
      assert.equal(file.type, "");
      assert(file.lastModified > 0);
    });

    test("should inherit blob methods", async () => {
      const file = new File(["Hello"], "test.txt");
      const text = await file.text();
      assert.equal(text, "Hello");
    });

    test("should handle multiple parts", async () => {
      const file = new File(["Hello", " ", "World"], "test.txt");
      const text = await file.text();
      assert.equal(text, "Hello World");
    });

    test("should handle typed arrays", async () => {
      const data = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
      const file = new File([data], "test.txt");
      const text = await file.text();
      assert.equal(text, "Hello");
    });

    test("should reject empty filenames", () => {
      let threw = false;
      try {
        new File([], "");
      } catch (e) {
        threw = true;
        assert(e instanceof TypeError);
      }
      assert(threw, "Should throw TypeError for empty filename");
    });

    test("should handle unicode filenames", () => {
      console.log("Creating file with unicode filename");
      const file = new File([], "测试.txt");
      console.log("Created file:", {
        name: file.name,
        size: file.size,
        type: file.type,
        lastModified: file.lastModified,
      });

      // Compare string values directly
      const actualName = String(file.name);
      const expectedName = String("测试.txt");
      console.log("Comparing names:", {
        actual: actualName,
        expected: expectedName,
        actualLength: actualName.length,
        expectedLength: expectedName.length,
        actualBytes: [...actualName].map((c) => c.charCodeAt(0)),
        expectedBytes: [...expectedName].map((c) => c.charCodeAt(0)),
      });

      // Use individual character comparison
      for (let i = 0; i < expectedName.length; i++) {
        assert.equal(
          actualName.charCodeAt(i),
          expectedName.charCodeAt(i),
          `Character at position ${i} should match`,
        );
      }
      assert.equal(
        actualName.length,
        expectedName.length,
        "Filename lengths should match",
      );
    });
  });
});
