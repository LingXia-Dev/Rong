globalThis.onmessage = () => {
  globalThis.__rongWorkerTestStarted();
  while (true) {}
};
