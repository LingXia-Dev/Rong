import "./runtime.js";

const controller = globalThis.__RONG_TEST__;

export const describe = globalThis.describe;
export const test = globalThis.test;
export const beforeEach = globalThis.beforeEach;
export const afterEach = globalThis.afterEach;
export const expect = globalThis.expect;
export const AssertionError = controller.AssertionError;
