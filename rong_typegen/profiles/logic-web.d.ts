// Versioned `logic-web` runtime profile published with rong_typegen.
// Do not edit downstream copies by hand.
//
// Ambient globals implemented by the Rong Logic runtime without requiring the
// browser DOM library or the @rongjs/rong TypeScript package.

declare function setTimeout(callback: () => void, delay?: number): number;
declare function clearTimeout(id: number): void;
declare function setInterval(callback: () => void, delay?: number): number;
declare function clearInterval(id: number): void;

declare function atob(data: string): string;
declare function btoa(data: string): string;

interface TextEncoder {
  readonly encoding: string;
  encode(input?: string): Uint8Array;
  encodeInto(source: string, destination: Uint8Array): { read: number; written: number };
}
declare var TextEncoder: { new (): TextEncoder; prototype: TextEncoder };

interface TextDecoderOptions { fatal?: boolean; ignoreBOM?: boolean }
interface TextDecoder {
  readonly encoding: string;
  readonly fatal: boolean;
  readonly ignoreBOM: boolean;
  decode(input?: ArrayBuffer | ArrayBufferView, options?: { stream?: boolean }): string;
}
declare var TextDecoder: { new (label?: string, options?: TextDecoderOptions): TextDecoder; prototype: TextDecoder };

interface URLSearchParams {
  append(name: string, value: string): void;
  delete(name: string): void;
  get(name: string): string | null;
  getAll(name: string): string[];
  has(name: string): boolean;
  set(name: string, value: string): void;
  sort(): void;
  entries(): Array<[string, string]>;
  keys(): string[];
  values(): string[];
  forEach(callback: (value: string, key: string) => void, thisArg?: any): void;
  toString(): string;
  readonly size: number;
}
declare var URLSearchParams: {
  new (init?: string | Array<[string, string]> | Record<string, string>): URLSearchParams;
  prototype: URLSearchParams;
};

interface URL {
  hash: string;
  host: string;
  hostname: string;
  href: string;
  readonly origin: string;
  password: string;
  pathname: string;
  port: string;
  protocol: string;
  search: string;
  username: string;
  readonly searchParams: URLSearchParams;
  toString(): string;
  toJSON(): string;
}
declare var URL: { new (url: string, base?: string): URL; prototype: URL };

interface EventOptions { bubbles?: boolean; cancelable?: boolean; composed?: boolean }
interface Event {
  readonly type: string;
  readonly bubbles: boolean;
  readonly cancelable: boolean;
  readonly composed: boolean;
}
declare var Event: { new (type: string, options?: EventOptions): Event; prototype: Event };

interface CustomEventOptions extends EventOptions { detail?: any }
interface CustomEvent extends Event { readonly detail: any }
declare var CustomEvent: { new (type: string, options?: CustomEventOptions): CustomEvent; prototype: CustomEvent };

type EventListener = (event: Event) => void;
interface AddEventListenerOptions { once?: boolean; capture?: boolean; passive?: boolean }
interface EventTarget {
  addEventListener(type: string, listener: EventListener, options?: boolean | AddEventListenerOptions): void;
  removeEventListener(type: string, listener: EventListener, options?: boolean | AddEventListenerOptions): void;
  dispatchEvent(event: Event): boolean;
}
declare var EventTarget: { new (): EventTarget; prototype: EventTarget };

type EventName = string | symbol;
type EventEmitterListener = (...args: any[]) => void;
interface EventEmitter extends EventTarget {
  on(eventName: EventName, listener: EventEmitterListener): this;
  addListener(eventName: EventName, listener: EventEmitterListener): this;
  once(eventName: EventName, listener: EventEmitterListener): this;
  off(eventName: EventName, listener: EventEmitterListener): this;
  removeListener(eventName: EventName, listener: EventEmitterListener): this;
  removeAllListeners(eventName?: EventName): this;
  prependListener(eventName: EventName, listener: EventEmitterListener): this;
  prependOnceListener(eventName: EventName, listener: EventEmitterListener): this;
  emit(eventName: EventName, ...args: any[]): boolean;
  eventNames(): EventName[];
  setMaxListeners(count: number): this;
  getMaxListeners(): number;
  listenerCount(eventName: EventName, listener?: EventEmitterListener): number;
}
declare var EventEmitter: { new (): EventEmitter; prototype: EventEmitter };

interface AbortSignal extends EventTarget {
  readonly aborted: boolean;
  readonly reason: any;
  onabort: ((event: Event) => void) | null;
  throwIfAborted(): void;
}
declare var AbortSignal: {
  prototype: AbortSignal;
  any(signals: AbortSignal[]): AbortSignal;
  abort(reason?: any): AbortSignal;
  timeout(milliseconds: number): AbortSignal;
};

interface AbortController {
  readonly signal: AbortSignal;
  abort(reason?: any): void;
}
declare var AbortController: { new (): AbortController; prototype: AbortController };

type DOMExceptionName =
  | 'IndexSizeError' | 'HierarchyRequestError' | 'InvalidCharacterError'
  | 'NoModificationAllowedError' | 'NotFoundError' | 'NotSupportedError'
  | 'InvalidStateError' | 'SyntaxError' | 'InvalidModificationError'
  | 'NamespaceError' | 'InvalidAccessError' | 'TypeMismatchError'
  | 'SecurityError' | 'NetworkError' | 'AbortError' | 'URLMismatchError'
  | 'QuotaExceededError' | 'TimeoutError' | 'InvalidNodeTypeError'
  | 'DataCloneError' | 'Error';
interface DOMException extends Error {
  readonly name: string;
  readonly message: string;
  readonly stack: string;
}
declare var DOMException: { new (message?: string, name?: DOMExceptionName): DOMException; prototype: DOMException };

type BlobPart = Blob | ArrayBuffer | ArrayBufferView | string;
interface BlobOptions { type?: string; endings?: 'transparent' | 'native' }
interface Blob {
  readonly size: number;
  readonly type: string;
  slice(start?: number, end?: number, contentType?: string): Blob;
  arrayBuffer(): Promise<ArrayBuffer>;
  text(): Promise<string>;
  bytes(): Promise<Uint8Array>;
}
declare var Blob: { new (blobParts?: BlobPart[], options?: BlobOptions): Blob; prototype: Blob };

interface FileOptions extends BlobOptions { lastModified?: number }
interface File extends Blob {
  readonly name: string;
  readonly lastModified: number;
}
declare var File: { new (fileBits: BlobPart[], fileName: string, options?: FileOptions): File; prototype: File };

interface FormData {
  append(name: string, value: string | Blob, filename?: string): void;
  delete(name: string): void;
  get(name: string): string | File | null;
  getAll(name: string): Array<string | File>;
  has(name: string): boolean;
  set(name: string, value: string | Blob, filename?: string): void;
  entries(): IterableIterator<[string, string | File]>;
  keys(): IterableIterator<string>;
  values(): IterableIterator<string | File>;
  forEach(
    callback: (value: string | File, key: string, parent: FormData) => void,
    thisArg?: any,
  ): void;
}
declare var FormData: { new (): FormData; prototype: FormData };

interface ReadableStreamDefaultReader<R = any> {
  read(): Promise<{ done: boolean; value: R }>;
  releaseLock(): void;
  cancel(reason?: any): Promise<void>;
}
interface ReadableStream<R = any> {
  getReader(): ReadableStreamDefaultReader<R>;
  cancel(reason?: any): Promise<void>;
  pipeTo(destination: WritableStream<R>, options?: { preventClose?: boolean; preventAbort?: boolean; preventCancel?: boolean; signal?: AbortSignal }): Promise<void>;
  pipeThrough<T>(transform: { writable: WritableStream<R>; readable: ReadableStream<T> }): ReadableStream<T>;
}
declare var ReadableStream: { new <R = any>(underlyingSource?: any): ReadableStream<R>; prototype: ReadableStream };

interface WritableStreamDefaultWriter<W = any> {
  write(chunk?: W): Promise<void>;
  close(): Promise<void>;
  abort(reason?: any): Promise<void>;
  releaseLock(): void;
}
interface WritableStream<W = any> {
  getWriter(): WritableStreamDefaultWriter<W>;
  abort(reason?: any): Promise<void>;
}
declare var WritableStream: { new <W = any>(underlyingSink?: any): WritableStream<W>; prototype: WritableStream };

type CompressionFormat = 'gzip' | 'deflate' | 'deflate-raw';
interface CompressionStream {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
}
declare var CompressionStream: {
  new (format: CompressionFormat): CompressionStream;
  prototype: CompressionStream;
};
interface DecompressionStream {
  readonly readable: ReadableStream<Uint8Array>;
  readonly writable: WritableStream<Uint8Array>;
}
declare var DecompressionStream: {
  new (format: CompressionFormat): DecompressionStream;
  prototype: DecompressionStream;
};

type HeadersInit = Record<string, string> | Array<[string, string]> | Headers;
interface Headers {
  append(name: string, value: string): void;
  delete(name: string): void;
  get(name: string): string | null;
  has(name: string): boolean;
  set(name: string, value: string): void;
  forEach(callback: (value: string, name: string, self: Headers) => void, thisArg?: any): void;
  entries(): IterableIterator<[string, string]>;
  keys(): IterableIterator<string>;
  values(): IterableIterator<string>;
  getSetCookie(): string[];
}
declare var Headers: { new (init?: HeadersInit): Headers; prototype: Headers };

type BodyInit = string | Blob | ArrayBuffer | ArrayBufferView | FormData | URLSearchParams | ReadableStream<Uint8Array>;
interface Body {
  readonly bodyUsed: boolean;
  readonly body: ReadableStream<Uint8Array> | null;
  text(): Promise<string>;
  json<T = any>(): Promise<T>;
  arrayBuffer(): Promise<ArrayBuffer>;
  formData(): Promise<FormData>;
}

interface RequestInit {
  method?: string;
  headers?: HeadersInit | Headers;
  body?: BodyInit | null;
  redirect?: 'follow' | 'error' | 'manual';
  signal?: AbortSignal | null;
}
type RequestInfo = string | Request | URL;
interface Request extends Body {
  readonly method: string;
  readonly headers: Headers;
  readonly redirect: string;
  readonly signal: AbortSignal | null;
  readonly url: string;
  readonly cache: string;
  readonly keepalive: boolean;
  clone(): Request;
}
declare var Request: { new (input: RequestInfo | string, init?: RequestInit): Request; prototype: Request };

interface ResponseInit { status?: number; statusText?: string; headers?: HeadersInit }
interface Response extends Body {
  readonly status: number;
  readonly statusText: string;
  readonly ok: boolean;
  readonly headers: Headers;
  readonly type: string;
  readonly redirected: boolean;
  readonly url: string;
  blob(): Promise<Blob>;
  clone(): Response;
}
declare var Response: {
  new (body?: BodyInit | null, init?: ResponseInit): Response;
  prototype: Response;
  error(): Response;
  redirect(url: string, status?: number): Response;
};

declare function fetch(url: RequestInfo | URL, options?: RequestInit): Promise<Response>;

interface Console {
  log(...args: any[]): void;
  error(...args: any[]): void;
  warn(...args: any[]): void;
  info(...args: any[]): void;
  debug(...args: any[]): void;
  assert(condition?: any, ...args: any[]): void;
  dir(value?: any, options?: { depth?: number; maxArrayLength?: number; maxObjectKeys?: number }): void;
  trace(...args: any[]): void;
  time(label?: string): void;
  timeLog(label?: string, ...args: any[]): void;
  timeEnd(label?: string): void;
  count(label?: string): void;
  countReset(label?: string): void;
  clear(): void;
}
declare var console: Console;
