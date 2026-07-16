postMessage('ready');

globalThis.onmessage = async () => {
  postMessage('started');
  await new Promise(() => {});
};
