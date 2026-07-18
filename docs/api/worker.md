# Worker — Web Workers

Run JavaScript in a dedicated OS thread with its own JS runtime and event loop. Communication via JSON-serialized messages.

## Basic Usage

```javascript
// main.js
const worker = new Worker("./task.js");

worker.onmessage = (event) => {
  console.log("Result:", event.data);
};

worker.postMessage({ numbers: [1, 2, 3, 4, 5] });
```

```javascript
// task.js
onmessage = (event) => {
  const sum = event.data.numbers.reduce((a, b) => a + b, 0);
  postMessage({ sum });
};
```

## Main Thread API

### Constructor

```javascript
const worker = new Worker("./script.js");
```

Path can be absolute or relative to `cwd`.

### Send Message

```javascript
worker.postMessage(data);
```

`data` is serialized via `JSON.stringify` and deserialized via `JSON.parse` on the worker side.

### Receive Message

```javascript
worker.onmessage = (event) => {
  console.log(event.data);
};
```

### Error Handling

```javascript
worker.onerror = (event) => {
  console.error("Worker error:", event.message);
};
```

`onerror` receives an object with `type` and `message` fields.

### Terminate

```javascript
worker.terminate();
```

`terminate()` returns immediately and permanently closes the worker to new
messages. QuickJS and preemptive JavaScriptCore builds also stop currently
running non-yielding JavaScript. ArkJS and cooperative JavaScriptCore builds can
stop running code only at the next engine yield; if code never yields, its
isolated worker thread may remain active. Expected termination does not emit
`onerror`.

## Worker-Side API

Global APIs available inside worker scripts:

| API | Description |
|-----|-------------|
| `onmessage = (event) => {}` | Receive messages from main thread |
| `postMessage(data)` | Send message to main thread |
| `close()` | Close the worker |
| `self` | Worker global scope reference |

## Notes

- Each worker runs in a **dedicated OS thread** with its own JS engine instance
- Messages are JSON-serialized — **no shared memory**
- Workers have `console` by default; other modules can be configured via `rong_worker::set_initializer`
- After `terminate()`, the worker stops processing new messages and cannot be reused
