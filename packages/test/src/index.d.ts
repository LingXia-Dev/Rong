export type TestCaseStatus = "passed" | "failed" | "skipped";

export interface TestError {
  name: string;
  message: string;
  stack: string;
  causes?: TestError[];
}

export interface TestCaseResult {
  name: string;
  full_name: string;
  status: TestCaseStatus;
  duration_ms: number;
  error?: TestError;
}

export interface TestReport {
  total: number;
  passed: number;
  failed: number;
  skipped: number;
  duration_ms: number;
  cases: TestCaseResult[];
}

export type TestEvent =
  | { type: "case_started"; name: string; full_name: string }
  | {
      type: "case_finished";
      name: string;
      full_name: string;
      status: TestCaseStatus;
      duration_ms: number;
      error?: TestError;
    };

export type TestCallback = () => void | Promise<void>;

export interface TestFunction {
  (name: string, run: TestCallback): void;
  skip(name: string, run?: TestCallback): void;
  args?: unknown;
  attach?: (name: string, artifact: unknown) => void | Promise<void>;
}

export interface Matchers<T> {
  readonly not: Matchers<T>;
  toBe(expected: unknown): void;
  toEqual(expected: unknown): void;
  toContain(expected: unknown): void;
  toMatch(expected: string | RegExp): void;
  toBeTruthy(): void;
  toBeFalsy(): void;
  toBeDefined(): void;
  toBeUndefined(): void;
  toBeInstanceOf(expected: Function): void;
  toThrow(expected?: unknown): void;
}

export interface AssertionError extends Error {
  name: "AssertionError";
  matcher: string;
  actual?: unknown;
  expected?: unknown;
}

export const AssertionError: {
  new (matcher: string, actual: unknown, expected: unknown, message: string): AssertionError;
};

export interface RongTestController {
  run(): Promise<TestReport>;
}

export interface RongTestHost {
  args?: unknown;
  attach?: (name: string, artifact: unknown) => void | Promise<void>;
  report?: (event: TestEvent) => void | Promise<void>;
}

export function describe(name: string, define: () => void): void;
export const test: TestFunction;
export function beforeEach(run: TestCallback): void;
export function afterEach(run: TestCallback): void;
export function expect<T>(actual: T): Matchers<T>;

declare global {
  var describe: typeof import("./index.js").describe;
  var test: TestFunction;
  var beforeEach: typeof import("./index.js").beforeEach;
  var afterEach: typeof import("./index.js").afterEach;
  var expect: typeof import("./index.js").expect;
  var __RONG_TEST__: RongTestController;
  var __RONG_TEST_HOST__: RongTestHost | undefined;
}
