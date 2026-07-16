describe("FormData", () => {
  it("constructor", () => {
    const formData = new FormData();
    assert(formData instanceof FormData);
  });

  it("append and get with string values", () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("name", "Jane");
    formData.append("age", "30");

    const nameValue = formData.get("name");
    assert.equal(nameValue, "John");
    const ageValue = formData.get("age");
    assert.equal(ageValue, "30");
    assert.equal(formData.get("nonexistent"), null);

    const allNames = formData.getAll("name");
    assert.equal(allNames.length, 2);
    assert.equal(allNames[0], "John");
    assert.equal(allNames[1], "Jane");
  });

  it("append and get with file values", () => {
    const formData = new FormData();
    const file1 = new File(["content1"], "file1.txt", { type: "text/plain" });
    const file2 = new File(["content2"], "file2.txt", { type: "text/plain" });

    formData.append("file", file1);
    formData.append("file", file2);

    const firstFile = formData.get("file");
    assert(firstFile instanceof File);
    assert.equal(firstFile.name, "file1.txt");

    const allFiles = formData.getAll("file");
    assert.equal(allFiles.length, 2);
    assert(allFiles[0] instanceof File);
    assert(allFiles[1] instanceof File);
    assert.equal(allFiles[0].name, "file1.txt");
    assert.equal(allFiles[1].name, "file2.txt");
  });

  it("set", () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("name", "Jane");
    formData.set("name", "Alice");

    const names = formData.getAll("name");
    assert.equal(names.length, 1);
    assert.equal(names[0], "Alice");

    // Test set with File
    const file = new File(["content"], "test.txt", { type: "text/plain" });
    formData.set("file", file);
    const fileValue = formData.get("file");
    assert(fileValue instanceof File);
    assert.equal(fileValue.name, "test.txt");
  });

  it("set preserves the position of the first matching entry", () => {
    const formData = new FormData();
    formData.append("first", "old");
    formData.append("second", "kept");
    formData.append("first", "duplicate");

    formData.set("first", "new");

    const entries = [...formData.entries()];
    assert.equal(entries.length, 2);
    assert.equal(entries[0][0], "first");
    assert.equal(entries[0][1], "new");
    assert.equal(entries[1][0], "second");
  });

  it("has and delete", () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("file", new File(["content"], "test.txt"));

    assert(formData.has("name"));
    assert(formData.has("file"));
    assert(!formData.has("age"));

    formData.delete("name");
    assert(!formData.has("name"));
    assert.equal(formData.get("name"), null);

    formData.delete("file");
    assert(!formData.has("file"));
    assert.equal(formData.get("file"), null);
  });

  it("entries", async () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("name", "Jane");
    formData.append("file", new File(["content"], "test.txt"));

    const entries = [];
    console.log("Starting entries iteration");
    for await (const entry of formData.entries()) {
      console.log("Entry:", {
        isArray: Array.isArray(entry),
        length: entry.length,
        key: entry[0],
        value: entry[1],
        valueType: entry[1] instanceof File ? "File" : typeof entry[1],
      });
      entries.push(entry);
    }
    console.log("Finished entries iteration, total entries:", entries.length);

    assert.equal(entries.length, 3);
    assert.equal(entries[0][0], "name");
    assert.equal(entries[0][1], "John");
    assert.equal(entries[1][0], "name");
    assert.equal(entries[1][1], "Jane");
    assert.equal(entries[2][0], "file");
    assert(entries[2][1] instanceof File);
    assert.equal(entries[2][1].name, "test.txt");
  });

  it("forEach", () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("file", new File(["content"], "test.txt"));

    const seen = [];
    formData.forEach((value, key) => {
      seen.push([key, value]);
    });

    assert.equal(seen.length, 2);
    assert.equal(seen[0][0], "name");
    assert.equal(seen[0][1], "John");
    assert.equal(seen[1][0], "file");
    assert(seen[1][1] instanceof File);
  });

  it("keys", async () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("name", "Jane");
    formData.append("file", new File(["content"], "test.txt"));

    const keys = [];
    for await (const key of formData.keys()) {
      keys.push(key);
    }

    assert.equal(keys.length, 3);
    assert.equal(keys[0], "name");
    assert.equal(keys[1], "name");
    assert.equal(keys[2], "file");
  });

  it("values", async () => {
    const formData = new FormData();
    formData.append("name", "John");
    formData.append("name", "Jane");
    formData.append("file", new File(["content"], "test.txt"));

    const values = [];
    console.log("Starting values iteration");
    for await (const value of formData.values()) {
      console.log("Value:", {
        type: value instanceof File ? "File" : typeof value,
        isString: typeof value === "string",
        stringValue: typeof value === "string" ? value : undefined,
        isFile: value instanceof File,
        fileName: value instanceof File ? value.name : undefined,
      });
      values.push(value);
    }
    console.log("Finished values iteration, total values:", values.length);

    assert.equal(values.length, 3);
    assert.equal(values[0], "John");
    assert.equal(values[1], "Jane");
    assert(values[2] instanceof File);
    assert.equal(values[2].name, "test.txt");
  });

  it("should handle empty values", () => {
    const formData = new FormData();
    formData.append("empty", "");
    assert.equal(formData.get("empty"), "");
    assert(formData.has("empty"));
  });

  it("should handle non-string values", () => {
    const formData = new FormData();
    formData.append("number", 42);
    formData.append("boolean", true);
    formData.append("object", { key: "value" });
    assert.equal(formData.get("number"), "42");
    assert.equal(formData.get("boolean"), "true");
    assert.equal(formData.get("object"), "[object Object]");
  });

  it("should handle special characters in field names and values", () => {
    const formData = new FormData();
    formData.append("field#name", "value#1");
    formData.append("field/name", "value/2");
    assert.equal(formData.get("field#name"), "value#1");
    assert.equal(formData.get("field/name"), "value/2");
  });

  it("should handle large files", () => {
    const largeContent = new Array(1024 * 1024).fill("a").join(""); // 1MB
    const largeFile = new File([largeContent], "large.txt");
    const formData = new FormData();
    formData.append("file", largeFile);
    const retrievedFile = formData.get("file");
    assert(retrievedFile instanceof File);
    assert.equal(retrievedFile.size, 1024 * 1024);
  });

  it("should handle multiple files with same field name", () => {
    const formData = new FormData();
    const files = [
      new File(["content1"], "file1.txt"),
      new File(["content2"], "file2.txt"),
      new File(["content3"], "file3.txt"),
    ];
    files.forEach((file) => formData.append("files", file));
    const allFiles = formData.getAll("files");
    assert.equal(allFiles.length, 3);
    allFiles.forEach((file, i) => {
      assert(file instanceof File);
      assert.equal(file.name, `file${i + 1}.txt`);
    });
  });

  it("should handle formData iteration with mixed types", async () => {
    const formData = new FormData();
    formData.append("text", "value");
    formData.append("file", new File(["content"], "test.txt"));
    formData.append("number", 123);

    const entries = [];
    for await (const [key, value] of formData.entries()) {
      entries.push({ key, value });
    }

    assert.equal(entries.length, 3);
    assert.equal(entries[0].key, "text");
    assert.equal(entries[0].value, "value");
    assert.equal(entries[1].key, "file");
    assert(entries[1].value instanceof File);
    assert.equal(entries[2].key, "number");
    assert.equal(entries[2].value, "123");
  });
});
