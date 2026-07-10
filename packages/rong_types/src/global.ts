/**
 * Global API declarations for Rong JavaScript Runtime
 *
 * This file declares all globally available APIs injected by the Rong runtime.
 * These declarations enable IDE autocomplete and TypeScript type checking.
 */

import type { AssertFunction } from './assert';
import type {
  RongGzipCompressOptions,
  RongCompressionInput,
  RongZstdCompressOptions,
} from './compression';
import type {
  RongEnvMap,
  RongOutputHandle,
  RongShellError,
  RongShellTag,
  RongReadableProcessStream,
  RongSpawnOptions,
  RongSpawnOptionsWithCmd,
  RongSubprocess,
  RongSyncSubprocess,
} from './command';
import type { RongSleepValue } from './timer';
import type { RongCronFunction } from './cron';
import type { RedisClientConstructor } from './redis';
import type { SSEConstructor } from './sse';

declare global {
  /**
   * Rong runtime namespace - host APIs exposed by the Rong runtime
   */
  interface RongNamespace {
    // Runtime APIs
    readonly version: string;
    readonly revision: string;
    readonly argv: string[];
    readonly args: string[];
    readonly env: RongEnvMap;
    readonly stdin: RongReadableProcessStream;
    readonly stdout: RongOutputHandle;
    readonly stderr: RongOutputHandle;
    spawn(cmd: string[], options?: RongSpawnOptions): RongSubprocess;
    spawn(options: RongSpawnOptionsWithCmd): RongSubprocess;
    spawnSync(cmd: string[], options?: RongSpawnOptions): RongSyncSubprocess;
    spawnSync(options: RongSpawnOptionsWithCmd): RongSyncSubprocess;
    sleep(delay?: RongSleepValue): Promise<void>;
    sleepSync(delay?: number): void;
    /**
     * Rong in-process cron API.
     *
     * `Rong.cron(schedule, handler)` synchronously returns a CronJob handle.
     * `Rong.cron.parse(expression, relativeDate?)` returns the next matching
     * UTC Date or null.
     */
    cron: RongCronFunction;
    zstdCompress(
      data: RongCompressionInput,
      options?: RongZstdCompressOptions
    ): Promise<Uint8Array>;
    zstdCompressSync(
      data: RongCompressionInput,
      options?: RongZstdCompressOptions
    ): Uint8Array;
    zstdDecompress(data: RongCompressionInput): Promise<Uint8Array>;
    zstdDecompressSync(data: RongCompressionInput): Uint8Array;
    gzip(
      data: RongCompressionInput,
      options?: RongGzipCompressOptions
    ): Promise<Uint8Array>;
    gzipSync(
      data: RongCompressionInput,
      options?: RongGzipCompressOptions
    ): Uint8Array;
    gunzip(data: RongCompressionInput): Promise<Uint8Array>;
    gunzipSync(data: RongCompressionInput): Uint8Array;
    readonly $: RongShellTag;
    readonly ShellError: {
      new (message: string): RongShellError;
      prototype: RongShellError;
    };
    readonly RedisClient: RedisClientConstructor;
    readonly SSE: SSEConstructor;
  }

  const Rong: RongNamespace;

  /**
   * Base64 decode - Decode base64 string to binary string
   */
  function atob(data: string): string;

  /**
   * Base64 encode - Encode binary string to base64
   */
  function btoa(data: string): string;

  /**
   * Assert function - Test assertions (Node.js compatible)
   */
  const assert: AssertFunction;

  const Bun: {
    /** Alias of `Rong.cron`. */
    cron: RongCronFunction;
  };

}

export {};
