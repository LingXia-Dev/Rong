# rong_rt

Async runtime and host-side platform services for RongJS.

This crate provides the executor-facing runtime support used by Rong, including
HTTP client plumbing, async service integration, and transport-related helpers.

Most applications should depend on `rong` instead of using `rong_rt` directly.

## Timeouts

`rong_rt` separates two different timeout scopes:

- `request_timeout`: caps the whole request/response operation
- `connect_timeout`: caps only the socket connect phase

This matters for long-lived or high-latency flows. For example, an SSE stream or
large upload may legitimately run for minutes after the connection is
established, while you still want the initial dial to fail fast if DNS, TCP, or
TLS setup gets stuck.

Typical use cases:

- fail fast when connecting through a flaky proxy or dead IP
- keep SSE handshakes short without limiting the stream lifetime
- allow large uploads/downloads to run for a long time after connection

### Per-request overrides

```rust
use std::time::Duration;

use rong_rt::http::RequestOptions;

let options = RequestOptions::new()
    .with_request_timeout(Duration::from_secs(30))
    .with_connect_timeout(Duration::from_secs(2));
```

If no override is provided, `rong_rt` uses its built-in defaults: a `60s`
request timeout and no extra connect timeout cap.

There is no process-wide mutable timeout configuration. If one call site needs a
different policy, pass it explicitly through that operation's options.

The same split exists on the operation-specific builders:

- `download::DownloadOptions::with_request_timeout`
- `download::DownloadOptions::with_connect_timeout`
- `upload::UploadOptions::with_request_timeout`
- `upload::UploadOptions::with_connect_timeout`
- `sse::SseConnectOptions::with_request_timeout`
- `sse::SseConnectOptions::with_connect_timeout`

Rule of thumb:

- shorten `connect_timeout` when you want to fail fast before the connection is established
- shorten `request_timeout` when the whole request/response lifecycle should be bounded

## Network observer

Enable the `network-observer` feature to see every request sent through the
shared HTTP client, including `fetch` and `Rong.SSE` from `rong_http`:

```rust
use std::sync::Arc;

use rong_rt::http::{NetworkEvent, NetworkObserver, NetworkOwner};

struct Log;

impl NetworkObserver for Log {
    fn on_event(&self, event: NetworkEvent) {
        // Runs on the I/O thread: hand off, never block.
        let _ = event;
    }
}

rong_rt::http::set_network_observer(Some(Arc::new(Log)));

// Attribute Rust-side requests; `rong_http::set_network_owner` does the same
// for a JS context.
let response = rong_rt::http::scope_network_owner(
    NetworkOwner::new("sync-service"),
    rong_rt::http::send(request, rong_rt::http::RequestOptions::new()),
)
.await?;
```

Events for one request share a `NetworkRequestId`: `RequestStart` first,
request-body chunks, `ResponseHead`, response-body chunks, and exactly one
`RequestEnd` last. `RequestStart` runs on the sender's thread (the JS thread for
`fetch`); the rest run on executor threads. A redirect hop's
`RequestStart::redirected_from` names the previous hop, whose head carries the
status and `Location`. Guard-denied requests end with
`NetworkOutcome::Blocked`. Body chunks are teed from the connection, never
buffered. Each chunk is a `NetworkBodyChunk` whose `at` is the monotonic
instant it was observed, taken before the observer is called. Without the
feature the hooks are not compiled.

A plain `rong_rt::http::send*` reports kind `Other` and the scope's owner; tag
the request to say otherwise:

```rust
NetworkRequestTag::new(NetworkRequestKind::Download).insert_into(&mut request);
```

To unit-test an observer, build events with `NetworkRequestStart::new`,
`NetworkResponseHead::new`, `NetworkBodyChunk::new`, `NetworkRequestEnd::new`,
and `NetworkRequestId::from_u64`; the payload types are `#[non_exhaustive]`, so
set any remaining public fields after construction.

The observer tests need a TLS backend feature to build a client:
`cargo test -p rong_rt --features tls-aws-lc,network-observer`.
