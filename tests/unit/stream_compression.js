async function collectBytes(stream) {
  const reader = stream.getReader();
  const chunks = [];
  let total = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    const chunk = new Uint8Array(value);
    chunks.push(chunk);
    total += chunk.byteLength;
  }
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return out;
}

async function compressBytes(format, input) {
  const stream = new CompressionStream(format);
  const writer = stream.writable.getWriter();
  await writer.write(input);
  await writer.close();
  return collectBytes(stream.readable);
}

async function decompressBytes(format, input) {
  const stream = new DecompressionStream(format);
  const writer = stream.writable.getWriter();
  await writer.write(input);
  await writer.close();
  return collectBytes(stream.readable);
}

describe("Compression Streams", () => {
  test("constructors expose readable and writable sides", () => {
    const compression = new CompressionStream("gzip");
    const decompression = new DecompressionStream("gzip");

    expect(compression.readable instanceof ReadableStream).toBe(true);
    expect(compression.writable instanceof WritableStream).toBe(true);
    expect(decompression.readable instanceof ReadableStream).toBe(true);
    expect(decompression.writable instanceof WritableStream).toBe(true);
  });

  test("rejects unsupported formats", () => {
    let compressionFailed = false;
    try {
      new CompressionStream("brotli");
    } catch (error) {
      compressionFailed = true;
      expect(error instanceof TypeError).toBe(true);
    }
    expect(compressionFailed).toBe(true);

    let decompressionFailed = false;
    try {
      new DecompressionStream("brotli");
    } catch (error) {
      decompressionFailed = true;
      expect(error instanceof TypeError).toBe(true);
    }
    expect(decompressionFailed).toBe(true);
  });

  for (const format of ["gzip", "deflate", "deflate-raw"]) {
    test(`round-trips ${format} data`, async () => {
      const input = new TextEncoder().encode(
        "Rong compression stream test payload repeated twice Rong compression stream test payload repeated twice",
      );

      const compressed = await compressBytes(format, input);
      expect(compressed.byteLength > 0).toBe(true);

      const decompressed = await decompressBytes(format, compressed);
      expect(new TextDecoder().decode(decompressed)).toBe(
        new TextDecoder().decode(input),
      );
    });

    test(`supports pipeThrough() with ${format}`, async () => {
      const input = new TextEncoder().encode(
        `pipeThrough-${format}-payload-pipeThrough-${format}-payload`,
      );

      const source = new ReadableStream({
        start(controller) {
          controller.enqueue(input.slice(0, 10));
          controller.enqueue(input.slice(10));
          controller.close();
        },
      });

      const output = await collectBytes(
        source
          .pipeThrough(new CompressionStream(format))
          .pipeThrough(new DecompressionStream(format)),
      );

      expect(new TextDecoder().decode(output)).toBe(
        new TextDecoder().decode(input),
      );
    });
  }

  test("supports chunked writes across multiple writer.write() calls", async () => {
    const encoder = new TextEncoder();
    const compression = new CompressionStream("gzip");
    const writer = compression.writable.getWriter();
    await writer.write(encoder.encode("hello "));
    await writer.write(encoder.encode("world"));
    await writer.close();

    const compressed = await collectBytes(compression.readable);
    const decompressed = await decompressBytes("gzip", compressed);
    expect(new TextDecoder().decode(decompressed)).toBe("hello world");
  });

  test("errors on invalid compressed input", async () => {
    const stream = new DecompressionStream("gzip");
    const writer = stream.writable.getWriter();
    await writer.write(new Uint8Array([1, 2, 3, 4, 5]));
    await writer.close();

    const reader = stream.readable.getReader();
    let failed = false;
    try {
      while (true) {
        const { done } = await reader.read();
        if (done) break;
      }
    } catch (error) {
      failed = true;
      expect(error instanceof Error).toBe(true);
    }
    expect(failed).toBe(true);
  });
});
