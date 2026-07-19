describe("Filesystem", () => {
  const isWindows = !!(
    typeof process !== "undefined" &&
    process &&
    process.env &&
    (process.env.OS === "Windows_NT" || !!process.env.ComSpec)
  );

  function normalizePath(p) {
    let normalized = String(p).replace(/\\/g, "/");
    normalized = normalized.replace(/^\/\/\?\//, "");
    if (isWindows) {
      normalized = normalized.toLowerCase();
    }
    return normalized;
  }

  function getTempPath(filename) {
    return `${WORKSPACE_ROOT}/target/test-tmp/${filename}`;
  }

  async function ensureTempDir() {
    try {
      await Rong.mkdir(`${WORKSPACE_ROOT}/target/test-tmp`, {
        recursive: true,
      });
    } catch (e) {
      // Directory might already exist
    }
  }

  async function cleanupTempDir() {
    try {
      await Rong.remove(`${WORKSPACE_ROOT}/target/test-tmp`, {
        recursive: true,
      });
    } catch (e) {
      // Directory might not exist or be already removed
    }
  }

  // ==================== Rong.file() basics ====================

  test("Rong.file() returns lazy RongFile with name", () => {
    const f = Rong.file("/some/path.txt");
    assert(f !== null && f !== undefined, "file() should return an object");
    assert.equal(f.name, "/some/path.txt", "name getter should return path");
  });

  test("RongFile is hidden from global scope", () => {
    assert.equal(typeof RongFile, "undefined");
    assert.equal(globalThis.RongFile, undefined);

    const file = Rong.file("/some/path.txt");
    let failed = false;
    try {
      new file.constructor();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Should not construct RongFile via instance.constructor");
  });

  // ==================== Rong.write() + RongFile.text() ====================

  test("Rong.write(path, string) and file.text()", async () => {
    await ensureTempDir();
    const testFile = getTempPath("write_text.txt");
    const content = "Hello, World!";

    const bytesWritten = await Rong.write(testFile, content);
    assert.equal(bytesWritten, content.length, "Should return bytes written");

    const readBack = await Rong.file(testFile).text();
    assert.equal(readBack, content, "Content should match");

    await cleanupTempDir();
  });

  test("Rong.write(path, Uint8Array) and file.arrayBuffer()", async () => {
    await ensureTempDir();
    const testFile = getTempPath("write_bin.bin");
    const data = new Uint8Array([1, 2, 3, 4, 5, 255, 0, 128]);

    const bytesWritten = await Rong.write(testFile, data);
    assert.equal(bytesWritten, data.length, "Should return bytes written");

    const ab = await Rong.file(testFile).arrayBuffer();
    const readData = new Uint8Array(ab);
    assert.equal(readData.length, data.length, "Length should match");
    for (let i = 0; i < data.length; i++) {
      assert.equal(readData[i], data[i], `Byte ${i} should match`);
    }

    await cleanupTempDir();
  });

  test("Rong.write(path, ArrayBuffer)", async () => {
    await ensureTempDir();
    const testFile = getTempPath("write_ab.bin");
    const data = new Uint8Array([10, 20, 30]).buffer;

    const bytesWritten = await Rong.write(testFile, data);
    assert.equal(bytesWritten, 3, "Should return bytes written");

    const readBack = new Uint8Array(await Rong.file(testFile).arrayBuffer());
    assert.equal(readBack[0], 10);
    assert.equal(readBack[1], 20);
    assert.equal(readBack[2], 30);

    await cleanupTempDir();
  });

  test("Rong.write(dest, RongFile) copies file", async () => {
    await ensureTempDir();
    const src = getTempPath("copy_src.txt");
    const dst = getTempPath("copy_dst.txt");
    const content = "Copy test content";

    await Rong.write(src, content);
    const bytesCopied = await Rong.write(dst, Rong.file(src));
    assert.equal(bytesCopied, content.length, "Should return bytes copied");

    const copied = await Rong.file(dst).text();
    assert.equal(copied, content, "Copied content should match");

    await cleanupTempDir();
  });

  test("Rong.write(RongFile, data) accepts RongFile as dest", async () => {
    await ensureTempDir();
    const testFile = getTempPath("write_rf_dest.txt");
    const f = Rong.file(testFile);

    await Rong.write(f, "hello via RongFile dest");
    const text = await f.text();
    assert.equal(text, "hello via RongFile dest");

    await cleanupTempDir();
  });

  test("Rong.write() overwrites by default", async () => {
    await ensureTempDir();
    const testFile = getTempPath("overwrite.txt");

    await Rong.write(testFile, "first");
    await Rong.write(testFile, "second");
    const text = await Rong.file(testFile).text();
    assert.equal(text, "second", "Should overwrite, not append");

    await cleanupTempDir();
  });

  test("Rong.write() rejects invalid data types", async () => {
    await ensureTempDir();
    const testFile = getTempPath("invalid_data.txt");
    let failed = false;
    try {
      await Rong.write(testFile, 12345);
    } catch (e) {
      failed = true;
    }
    assert(failed, "Should reject non-string/buffer/RongFile data");
    await cleanupTempDir();
  });

  // ==================== RongFile.json() ====================

  test("RongFile.json() parses JSON file", async () => {
    await ensureTempDir();
    const testFile = getTempPath("data.json");
    const obj = { name: "test", value: 42, nested: { arr: [1, 2, 3] } };

    await Rong.write(testFile, JSON.stringify(obj));
    const parsed = await Rong.file(testFile).json();

    assert.equal(parsed.name, "test");
    assert.equal(parsed.value, 42);
    assert.equal(parsed.nested.arr.length, 3);
    assert.equal(parsed.nested.arr[0], 1);

    await cleanupTempDir();
  });

  test("RongFile.json() throws on invalid JSON", async () => {
    await ensureTempDir();
    const testFile = getTempPath("bad.json");
    await Rong.write(testFile, "not valid json {{{");

    let failed = false;
    try {
      await Rong.file(testFile).json();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Should throw on invalid JSON");

    await cleanupTempDir();
  });

  // ==================== RongFile.bytes() ====================

  test("RongFile.bytes() returns Uint8Array", async () => {
    await ensureTempDir();
    const testFile = getTempPath("bytes.bin");
    const data = new Uint8Array([10, 20, 30, 40, 50]);
    await Rong.write(testFile, data);

    const result = await Rong.file(testFile).bytes();
    assert(result instanceof Uint8Array, "Should return Uint8Array");
    assert.equal(result.length, data.length);
    for (let i = 0; i < data.length; i++) {
      assert.equal(result[i], data[i], `Byte ${i} should match`);
    }

    await cleanupTempDir();
  });

  // ==================== RongFile.exists() ====================

  test("RongFile.exists() checks file existence", async () => {
    await ensureTempDir();
    const testFile = getTempPath("exists_test.txt");

    assert.equal(
      await Rong.file(testFile).exists(),
      false,
      "Non-existent file should return false",
    );

    await Rong.write(testFile, "content");
    assert.equal(
      await Rong.file(testFile).exists(),
      true,
      "Existing file should return true",
    );

    await cleanupTempDir();
  });

  // ==================== RongFile.delete() ====================

  test("RongFile.delete() removes file", async () => {
    await ensureTempDir();
    const testFile = getTempPath("delete_test.txt");
    await Rong.write(testFile, "to be deleted");

    await Rong.file(testFile).delete();
    assert.equal(
      await Rong.file(testFile).exists(),
      false,
      "File should not exist after delete",
    );

    await cleanupTempDir();
  });

  test("RongFile.delete() throws for non-existent file", async () => {
    let failed = false;
    try {
      await Rong.file(getTempPath("no_such_file.txt")).delete();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Deleting non-existent file should throw");
  });

  // ==================== RongFile.stat() / lstat() ====================

  test("RongFile.stat() returns file metadata", async () => {
    await ensureTempDir();
    const testFile = getTempPath("stat.txt");
    const content = "Test content";
    await Rong.write(testFile, content);

    const info = await Rong.file(testFile).stat();
    assert(info.isFile, "Should be a file");
    assert(!info.isDirectory, "Should not be a directory");
    assert(!info.isSymlink, "Should not be a symlink");
    assert.equal(info.size, content.length, "Size should match content length");
    assert(typeof info.modified === "number", "modified should be a number");
    assert(typeof info.accessed === "number", "accessed should be a number");
    if (info.mode != null) {
      assert(typeof info.mode === "number", "mode should be a number");
    }

    await cleanupTempDir();
  });

  test("RongFile.lstat() returns symlink metadata", async () => {
    await ensureTempDir();
    const testFile = getTempPath("lstat.txt");
    await Rong.write(testFile, "Test content");

    const info = await Rong.file(testFile).lstat();
    assert(info.isFile);
    assert(!info.isDirectory);
    assert(!info.isSymlink);

    await cleanupTempDir();
  });

  // ==================== RongFile.stream() ====================

  test("RongFile.stream() returns ReadableStream", async () => {
    await ensureTempDir();
    const testFile = getTempPath("stream_test.txt");

    // Create large content for multiple chunks
    const chunk = "chunk_" + "A".repeat(1024);
    let text = "";
    for (let i = 0; i < 200; i++) text += chunk + "\n";
    await Rong.write(testFile, text);

    const rs = Rong.file(testFile).stream();
    assert(rs instanceof ReadableStream, "Should return ReadableStream");

    const reader = rs.getReader();
    let total = 0;
    const bufs = [];
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      bufs.push(new Uint8Array(value));
      total += value.byteLength;
    }

    const all = new Uint8Array(total);
    let off = 0;
    for (const b of bufs) {
      all.set(b, off);
      off += b.byteLength;
    }
    const decoded = new TextDecoder().decode(all);
    assert.equal(decoded.length, text.length, "Stream should read all content");
    assert(decoded.includes("chunk_"), "Content should be correct");

    await cleanupTempDir();
  });

  // ==================== RongFile.open() → FileHandle ====================

  test("FileHandle - basic operations (stat, truncate, sync, close)", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_basic.txt");
    await Rong.write(testFile, "Hello from FileHandle!");

    const handle = await Rong.file(testFile).open({ read: true, write: true });

    const stats = await handle.stat();
    assert(stats.isFile && !stats.isDirectory && !stats.isSymlink);
    assert(stats.size > 0 && typeof stats.modified === "number");

    await handle.truncate(5);
    const statAfter = await handle.stat();
    assert.equal(statAfter.size, 5, "File should be truncated to 5 bytes");

    await handle.sync();
    await handle.close();

    await cleanupTempDir();
  });

  test("FileHandle - read/write operations", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_rw.dat");
    const testData = new Uint8Array([1, 2, 3, 4, 5, 255, 0, 128]);

    // Write binary data
    const file1 = await Rong.file(testFile).open({ write: true, create: true });
    const arrayBuffer = testData.buffer.slice(
      testData.byteOffset,
      testData.byteOffset + testData.byteLength,
    );
    const bytesWritten = await file1.write(arrayBuffer);
    assert.equal(bytesWritten, 8, "Should write 8 bytes");
    await file1.close();

    // Read and verify
    const file2 = await Rong.file(testFile).open({ read: true });
    const readBuffer = new ArrayBuffer(8);
    const bytesRead = await file2.read(readBuffer);
    assert.equal(bytesRead, 8, "Should read 8 bytes");

    const readArray = new Uint8Array(readBuffer);
    for (let i = 0; i < testData.length; i++) {
      assert.equal(readArray[i], testData[i], `Byte ${i} should match`);
    }

    // Test EOF
    const eofResult = await file2.read(new ArrayBuffer(4));
    assert.equal(eofResult, null, "Should return null at EOF");
    await file2.close();

    // Test append mode
    const file3 = await Rong.file(testFile).open({
      write: true,
      append: true,
    });
    const appendData = new Uint8Array([9, 10]);
    await file3.write(
      appendData.buffer.slice(
        appendData.byteOffset,
        appendData.byteOffset + appendData.byteLength,
      ),
    );
    await file3.close();

    // Verify append
    const file4 = await Rong.file(testFile).open({ read: true });
    const fullBuffer = new ArrayBuffer(10);
    const totalRead = await file4.read(fullBuffer);
    assert.equal(totalRead, 10, "Should read 10 bytes after append");
    await file4.close();

    await cleanupTempDir();
  });

  test("FileHandle - seek operations", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_seek.txt");
    const testData = "Hello, World! This is a test file for seeking.";
    await Rong.write(testFile, testData);

    const handle = await Rong.file(testFile).open({
      read: true,
      write: true,
    });

    // Seek from start
    let pos = await handle.seek(7, Rong.SeekMode.Start);
    assert.equal(pos, 7, "Should seek to position 7");

    const buf1 = new ArrayBuffer(6);
    await handle.read(buf1);
    assert.equal(
      new TextDecoder().decode(new Uint8Array(buf1)),
      "World!",
      "Should read 'World!' from position 7",
    );

    // Seek from current
    pos = await handle.seek(-6, Rong.SeekMode.Current);
    assert.equal(pos, 7, "Should be back at position 7");

    // Seek from end
    pos = await handle.seek(-5, Rong.SeekMode.End);
    const buf2 = new ArrayBuffer(5);
    await handle.read(buf2);
    assert.equal(
      new TextDecoder().decode(new Uint8Array(buf2)),
      testData.slice(-5),
      "Should read last 5 chars",
    );

    // Seek to start
    pos = await handle.seek(0, Rong.SeekMode.Start);
    assert.equal(pos, 0, "Should be at start");
    const buf3 = new ArrayBuffer(5);
    await handle.read(buf3);
    assert.equal(
      new TextDecoder().decode(new Uint8Array(buf3)),
      "Hello",
      "Should read 'Hello' from start",
    );

    // Seek beyond EOF
    pos = await handle.seek(1000, Rong.SeekMode.Start);
    assert.equal(pos, 1000, "Should seek beyond EOF");
    const eofResult = await handle.read(new ArrayBuffer(10));
    assert.equal(eofResult, null, "Reading beyond EOF returns null");

    // Invalid whence
    let errorThrown = false;
    try {
      await handle.seek(0, 999);
    } catch (e) {
      errorThrown = true;
    }
    assert(errorThrown, "Should throw for invalid whence");

    await handle.close();
    await cleanupTempDir();
  });

  test("FileHandle - OpenOptions (createNew, truncate)", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_opts.dat");

    // createNew should create new file
    const f1 = await Rong.file(testFile).open({
      write: true,
      createNew: true,
    });
    await f1.write(new TextEncoder().encode("new file").buffer);
    await f1.close();

    // createNew should fail if file exists
    let failed = false;
    try {
      await Rong.file(testFile).open({ write: true, createNew: true });
    } catch (e) {
      failed = true;
    }
    assert(failed, "createNew should fail if file exists");

    // truncate should clear file
    const f2 = await Rong.file(testFile).open({
      write: true,
      truncate: true,
    });
    await f2.write(new TextEncoder().encode("truncated").buffer);
    await f2.close();

    const content = await Rong.file(testFile).text();
    assert.equal(content, "truncated", "File should be truncated");

    await cleanupTempDir();
  });

  test("FileHandle is hidden from global scope", () => {
    assert.equal(typeof FileHandle, "undefined");
    assert.equal(globalThis.FileHandle, undefined);
  });

  test("FileHandle cannot be constructed via instance.constructor", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_indirect_ctor.txt");
    await Rong.write(testFile, "hello");

    const handle = await Rong.file(testFile).open({ read: true });
    let failed = false;
    try {
      new handle.constructor();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Should not construct FileHandle via instance.constructor");
    await handle.close();
    await cleanupTempDir();
  });

  test("FileHandle - opening non-existent file fails", async () => {
    let failed = false;
    try {
      await Rong.file(getTempPath("nonexistent.txt")).open({ read: true });
    } catch (e) {
      failed = true;
    }
    assert(failed, "Opening non-existent file should fail");
  });

  test("FileHandle.close() invalidates the handle", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_close_invalidates.txt");
    await Rong.write(testFile, "hello");

    const handle = await Rong.file(testFile).open({ read: true, write: true });
    await handle.close();

    let failed = false;
    try {
      await handle.stat();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Closed handle should reject further operations");

    await cleanupTempDir();
  });

  // ==================== FileHandle streams ====================

  test("FileHandle.readable returns ReadableStream", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_readable.bin");

    // >64KiB to cross chunk boundaries
    const total = 128 * 1024 + 123;
    const src = new Uint8Array(total);
    for (let i = 0; i < total; i++) src[i] = i % 251;
    await Rong.write(testFile, src);

    const handle = await Rong.file(testFile).open({ read: true });
    const rs = handle.readable;
    assert(rs instanceof ReadableStream);

    const reader = rs.getReader();
    const chunks = [];
    let size = 0;
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      const part = new Uint8Array(value);
      chunks.push(part);
      size += part.byteLength;
    }
    await reader.releaseLock();
    await handle.close();

    const out = new Uint8Array(size);
    let offset = 0;
    for (const c of chunks) {
      out.set(c, offset);
      offset += c.byteLength;
    }
    assert.equal(size, total, "Total bytes should match");
    for (let i = 0; i < total; i++) {
      assert.equal(out[i], src[i], `Byte ${i} should match`);
    }

    await cleanupTempDir();
  });

  test("FileHandle.readable.pipeTo WritableStream (copy)", async () => {
    await ensureTempDir();
    const srcPath = getTempPath("handle_pipe_src.txt");
    const dstPath = getTempPath("handle_pipe_dst.txt");

    const payload = "PIPE_".repeat(50_000); // ~250kB
    await Rong.write(srcPath, payload);

    const src = await Rong.file(srcPath).open({ read: true });
    const dst = await Rong.file(dstPath).open({
      write: true,
      create: true,
      truncate: true,
    });

    await src.readable.pipeTo(dst.writable);
    await src.close();
    await dst.close();

    const copied = await Rong.file(dstPath).text();
    assert.equal(copied.length, payload.length);
    assert(copied.startsWith("PIPE_"));

    await cleanupTempDir();
  });

  test("FileHandle.readable supports async iteration (for-await)", async () => {
    await ensureTempDir();
    const srcPath = getTempPath("handle_for_await.txt");

    let text = "";
    for (let i = 0; i < 100; i++) {
      const line = `iter_${String(i).padStart(4, "0")}\n`;
      text += line.repeat(512);
    }
    await Rong.write(srcPath, text);

    const handle = await Rong.file(srcPath).open({ read: true });
    const rs = handle.readable;
    assert(rs instanceof ReadableStream);

    const ai = rs[Symbol.asyncIterator]?.call(rs);
    expect(ai === rs).toBe(true);

    const decoder = new TextDecoder();
    let total = 0;
    let acc = "";
    for await (const chunk of rs) {
      total += chunk.byteLength;
      acc += decoder.decode(chunk);
    }
    await handle.close();

    assert(total > 0);
    assert(acc.includes("iter_0000"));
    assert(acc.includes("iter_0099"));
    assert.equal(acc.length, text.length);

    await cleanupTempDir();
  });

  test("FileHandle.writable returns WritableStream", async () => {
    await ensureTempDir();
    const testFile = getTempPath("handle_writable.txt");

    const handle = await Rong.file(testFile).open({
      write: true,
      create: true,
      truncate: true,
    });
    const ws = handle.writable;
    assert(ws instanceof WritableStream);

    const writer = ws.getWriter();
    const enc = new TextEncoder();
    await writer.write(enc.encode("hello"));
    await writer.write(enc.encode("_stream_"));
    await writer.write(enc.encode("world"));
    await writer.close();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "hello_stream_world");

    await cleanupTempDir();
  });

  // ==================== RongFile.writer() / FileSink ====================

  test("FileSink - basic write and end", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_basic.txt");

    const w = await Rong.file(testFile).writer();
    const enc = new TextEncoder();
    await w.write(enc.encode("hello "));
    await w.write(enc.encode("world"));
    await w.end();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "hello world");

    await cleanupTempDir();
  });

  test("FileSink - write with string", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_writestr.txt");

    const w = await Rong.file(testFile).writer();
    const bytes1 = await w.write("line 1\n");
    const bytes2 = await w.write("line 2\n");
    await w.end();

    assert.equal(bytes1, 7, "write(string) should return bytes written");
    assert.equal(bytes2, 7, "write(string) should return bytes written");

    const text = await Rong.file(testFile).text();
    assert.equal(text, "line 1\nline 2\n");

    await cleanupTempDir();
  });

  test("FileSink - append mode", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_append.txt");

    // Initial write (truncate)
    await Rong.write(testFile, "first");

    // Append via writer
    const w = await Rong.file(testFile).writer({ append: true });
    await w.write("_second");
    await w.end();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "first_second", "Should append to existing content");

    await cleanupTempDir();
  });

  test("FileSink - default truncates existing content", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_truncate.txt");

    await Rong.write(testFile, "old content that is long");

    const w = await Rong.file(testFile).writer(); // default: truncate
    await w.write("new");
    await w.end();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "new", "Default writer should truncate");

    await cleanupTempDir();
  });

  test("FileSink - flush without end", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_flush.txt");

    const w = await Rong.file(testFile).writer();
    await w.write("buffered data");
    await w.flush();

    // Data should be on disk after flush
    const text = await Rong.file(testFile).text();
    assert.equal(text, "buffered data", "flush should persist data");

    await w.end();
    await cleanupTempDir();
  });

  test("FileSink is hidden from global scope", () => {
    assert.equal(typeof FileSink, "undefined");
    assert.equal(globalThis.FileSink, undefined);
  });

  test("FileSink cannot be constructed via instance.constructor", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_indirect_ctor.txt");
    const sink = await Rong.file(testFile).writer();

    let failed = false;
    try {
      new sink.constructor();
    } catch (e) {
      failed = true;
    }
    assert(failed, "Should not construct FileSink via instance.constructor");
    await sink.end();
    await cleanupTempDir();
  });

  test("FileSink.end() invalidates the writer", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_closed.txt");

    const w = await Rong.file(testFile).writer();
    await w.write("hello");
    await w.end();

    let failed = false;
    try {
      await w.write("world");
    } catch (e) {
      failed = true;
    }
    assert(failed, "Ended FileSink should reject further writes");

    await cleanupTempDir();
  });

  // ==================== Edge cases & error paths ====================

  test("RongFile.text() throws for non-existent file", async () => {
    let failed = false;
    try {
      await Rong.file(getTempPath("no_such.txt")).text();
    } catch (e) {
      failed = true;
    }
    assert(failed, "text() on non-existent file should throw");
  });

  test("RongFile.json() with various JSON types", async () => {
    await ensureTempDir();
    const testFile = getTempPath("json_types.json");

    // Array
    await Rong.write(testFile, "[1,2,3]");
    const arr = await Rong.file(testFile).json();
    assert(Array.isArray(arr), "Should parse array");
    assert.equal(arr.length, 3);

    // String
    await Rong.write(testFile, '"hello"');
    const str = await Rong.file(testFile).json();
    assert.equal(str, "hello");

    // Number
    await Rong.write(testFile, "42");
    const num = await Rong.file(testFile).json();
    assert.equal(num, 42);

    // Boolean
    await Rong.write(testFile, "true");
    const bool = await Rong.file(testFile).json();
    assert.equal(bool, true);

    // Null
    await Rong.write(testFile, "null");
    const nul = await Rong.file(testFile).json();
    assert.equal(nul, null);

    await cleanupTempDir();
  });

  test("RongFile.bytes() on empty file returns empty Uint8Array", async () => {
    await ensureTempDir();
    const testFile = getTempPath("empty.bin");
    await Rong.write(testFile, "");

    const result = await Rong.file(testFile).bytes();
    assert(result instanceof Uint8Array);
    assert.equal(result.length, 0);

    await cleanupTempDir();
  });

  test("RongFile.text() with UTF-8 content", async () => {
    await ensureTempDir();
    const testFile = getTempPath("utf8.txt");
    const content = "你好世界 🌍 café résumé";
    await Rong.write(testFile, content);

    const text = await Rong.file(testFile).text();
    assert.equal(text, content, "UTF-8 content should round-trip");

    await cleanupTempDir();
  });

  test("RongFile.stat() on directory", async () => {
    await ensureTempDir();
    const testDir = getTempPath("stat_dir");
    await Rong.mkdir(testDir);

    const info = await Rong.file(testDir).stat();
    assert(!info.isFile, "Should not be a file");
    assert(info.isDirectory, "Should be a directory");

    await Rong.remove(testDir);
    await cleanupTempDir();
  });

  test("Rong.write() with large data", async () => {
    await ensureTempDir();
    const testFile = getTempPath("large.bin");

    // 1MB of data
    const size = 1024 * 1024;
    const data = new Uint8Array(size);
    for (let i = 0; i < size; i++) data[i] = i % 256;

    const written = await Rong.write(testFile, data);
    assert.equal(written, size, "Should write all bytes");

    const info = await Rong.file(testFile).stat();
    assert.equal(info.size, size, "File size should match");

    // Spot check a few bytes
    const readBack = await Rong.file(testFile).bytes();
    assert.equal(readBack[0], 0);
    assert.equal(readBack[255], 255);
    assert.equal(readBack[256], 0);
    assert.equal(readBack[size - 1], (size - 1) % 256);

    await cleanupTempDir();
  });

  test("Rong.write() rejects invalid dest types", async () => {
    let failed = false;
    try {
      await Rong.write(12345, "data");
    } catch (e) {
      failed = true;
    }
    assert(failed, "Should reject non-string/RongFile dest");
  });

  test("FileSink - multiple append sessions", async () => {
    await ensureTempDir();
    const testFile = getTempPath("multi_append.txt");
    await Rong.write(testFile, "start");

    // First append session
    const w1 = await Rong.file(testFile).writer({ append: true });
    await w1.write("_mid");
    await w1.end();

    // Second append session
    const w2 = await Rong.file(testFile).writer({ append: true });
    await w2.write("_end");
    await w2.end();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "start_mid_end", "Multiple appends should accumulate");

    await cleanupTempDir();
  });

  test("FileSink.write() returns bytes written", async () => {
    await ensureTempDir();
    const testFile = getTempPath("sink_return.txt");

    const w = await Rong.file(testFile).writer();
    const enc = new TextEncoder();
    const n = await w.write(enc.encode("abc"));
    assert.equal(n, 3, "write() should return byte count");
    await w.end();

    await cleanupTempDir();
  });

  test("RongFile.stream() on empty file", async () => {
    await ensureTempDir();
    const testFile = getTempPath("stream_empty.txt");
    await Rong.write(testFile, "");

    const rs = Rong.file(testFile).stream();
    const reader = rs.getReader();
    const { done } = await reader.read();
    assert(done, "Empty file stream should be done immediately");

    await cleanupTempDir();
  });

  test("FileHandle.truncate() to 0 clears file", async () => {
    await ensureTempDir();
    const testFile = getTempPath("truncate_zero.txt");
    await Rong.write(testFile, "1234567890");

    const handle = await Rong.file(testFile).open({ read: true, write: true });
    await handle.truncate(); // default to 0
    const stat = await handle.stat();
    assert.equal(stat.size, 0, "File should be empty after truncate(0)");
    await handle.close();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "", "Content should be empty");

    await cleanupTempDir();
  });

  test("FileHandle.truncate() to specific length", async () => {
    await ensureTempDir();
    const testFile = getTempPath("truncate_len.txt");
    await Rong.write(testFile, "1234567890");

    const handle = await Rong.file(testFile).open({ read: true, write: true });
    await handle.truncate(5);
    await handle.close();

    const text = await Rong.file(testFile).text();
    assert.equal(text, "12345", "Should truncate to 5 bytes");

    await cleanupTempDir();
  });

  // ==================== Directory operations (unchanged) ====================

  test("mkdir and readDir", async () => {
    await ensureTempDir();
    const testDir = getTempPath("test_dir");
    const nestedDir = getTempPath("test_dir/nested");
    const testFile = getTempPath("test_dir/test.txt");

    await Rong.mkdir(testDir);
    await Rong.mkdir(nestedDir, { recursive: true });
    await Rong.write(testFile, "");

    const entries = [];
    const dirEntries = await Rong.readDir(testDir);
    for await (const entry of dirEntries) {
      entries.push(entry);
    }

    assert.equal(entries.length, 2);
    assert(
      entries.some((e) => e.name === "nested" && e.isDirectory),
      "Should have nested directory",
    );
    assert(
      entries.some((e) => e.name === "test.txt" && e.isFile),
      "Should have test file",
    );

    await Rong.remove(testFile);
    await Rong.remove(nestedDir);
    await Rong.remove(testDir);

    await cleanupTempDir();
  });

  test("remove", async () => {
    await ensureTempDir();
    const testFile = getTempPath("remove.txt");
    await Rong.write(testFile, "To be removed");
    await Rong.remove(testFile);

    assert.equal(
      await Rong.file(testFile).exists(),
      false,
      "File should be removed",
    );

    // Recursive directory removal
    const testDir = getTempPath("rm_dir");
    const nestedFile = getTempPath("rm_dir/nested/test.txt");
    await Rong.mkdir(getTempPath("rm_dir/nested"), { recursive: true });
    await Rong.write(nestedFile, "Test");

    let failed = false;
    try {
      await Rong.remove(testDir);
    } catch (e) {
      failed = true;
    }
    assert(failed, "Removing non-empty dir without recursive should fail");

    await Rong.remove(testDir, { recursive: true });

    failed = false;
    try {
      await Rong.readDir(testDir);
    } catch (e) {
      failed = true;
    }
    assert(failed, "Removed directory should not be readable");

    await cleanupTempDir();
  });

  test("rename", async () => {
    await ensureTempDir();
    const oldPath = getTempPath("old.txt");
    const newPath = getTempPath("new.txt");
    const content = "Rename test content";

    await Rong.write(oldPath, content);
    await Rong.rename(oldPath, newPath);

    const text = await Rong.file(newPath).text();
    assert.equal(text, content);

    assert.equal(await Rong.file(oldPath).exists(), false);

    await cleanupTempDir();
  });

  test("realPath", async () => {
    await ensureTempDir();
    const testFile = getTempPath("real.txt");
    await Rong.write(testFile, "");

    const realPath = await Rong.realPath(testFile);
    assert.equal(normalizePath(realPath), normalizePath(testFile));

    await cleanupTempDir();
  });

  test("symlink and readlink", async () => {
    await ensureTempDir();
    const targetFile = getTempPath("target.txt");
    const linkFile = getTempPath("link.txt");
    const content = "Symlink test content";

    await Rong.write(targetFile, content);

    const targetAbsPath = await Rong.realPath(targetFile);
    const linkAbsPath = getTempPath("link.txt");

    try {
      await Rong.symlink(targetAbsPath, linkAbsPath);
    } catch (e) {
      const message = String(e?.message || e);
      if (
        /not permitted|operation not permitted|privilege|not held by the client|1314|所需的特权/i.test(
          message,
        )
      ) {
        return;
      }
      throw e;
    }
    const linkTarget = await Rong.readlink(linkAbsPath);
    assert.equal(normalizePath(linkTarget), normalizePath(targetAbsPath));

    const linkContent = await Rong.file(linkAbsPath).text();
    assert.equal(linkContent, content);

    await cleanupTempDir();
  });

  test("chmod", async () => {
    await ensureTempDir();
    const testFile = getTempPath("chmod.txt");
    await Rong.write(testFile, "Test content");

    if (typeof Rong.chmod === "function") {
      await Rong.chmod(testFile, 0o600);
      const info = await Rong.file(testFile).stat();
      assert.equal(info.mode & 0o777, 0o600);
    }

    await cleanupTempDir();
  });

  test("chown", async () => {
    await ensureTempDir();
    const testFile = getTempPath("chown.txt");
    await Rong.write(testFile, "Test content");

    if (typeof Rong.chown === "function") {
      try {
        await Rong.chown(testFile, process.getuid(), process.getgid());
      } catch (e) {
        // Ignore permission errors
      }
    }

    await cleanupTempDir();
  });

  test("chdir", async () => {
    await ensureTempDir();
    const testDir = getTempPath("chdir_test");
    await Rong.mkdir(testDir);

    const oldPath = await Rong.realPath(".");
    await Rong.chdir(testDir);

    const newPath = await Rong.realPath(".");
    assert(newPath.endsWith("chdir_test"));

    await Rong.chdir(oldPath);
    await Rong.remove(testDir);

    await cleanupTempDir();
  });

  test("utime", async () => {
    await ensureTempDir();
    const testFile = getTempPath("utime.txt");
    await Rong.write(testFile, "Test content");

    const now = Date.now();
    await Rong.utime(testFile, { accessed: now, modified: now });

    const info = await Rong.file(testFile).stat();
    assert(Math.abs(info.accessed - now) < 5000);
    assert(Math.abs(info.modified - now) < 5000);

    await cleanupTempDir();
  });

  test("utime preserves timestamps before the Unix epoch", async () => {
    await ensureTempDir();
    const testFile = getTempPath("utime_pre_epoch.txt");
    await Rong.write(testFile, "Test content");

    await Rong.utime(testFile, { accessed: -1000, modified: -1000 });
    const info = await Rong.file(testFile).stat();
    assert.equal(info.accessed, -1000);
    assert.equal(info.modified, -1000);

    await cleanupTempDir();
  });

  test("utime preserves millisecond precision", async () => {
    await ensureTempDir();
    const testFile = getTempPath("utime_milliseconds.txt");
    await Rong.write(testFile, "Test content");

    const timestamp = 1700000000500;
    await Rong.utime(testFile, { accessed: timestamp, modified: timestamp });
    const info = await Rong.file(testFile).stat();
    assert.equal(info.accessed, timestamp);
    assert.equal(info.modified, timestamp);

    await cleanupTempDir();
  });
});
