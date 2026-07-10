use futures::StreamExt;
use redis::{AsyncCommands, Client, Value as RedisValue};
use rong::{
    HostError, IntoJSValue, JSArray, JSContext, JSObject, JSResult, JSValue, function::Optional,
    js_class, js_method,
};
use rong_abort::AbortSignal;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

type SharedCloseTx = Arc<Mutex<Option<oneshot::Sender<()>>>>;
type SharedMessageRx = Arc<Mutex<Option<mpsc::Receiver<Result<RedisSubscriptionMessage, String>>>>>;

struct RedisSubscriptionMessage {
    channel: String,
    message: String,
}

#[js_class]
pub struct RedisSubscription {
    id: u64,
    channel: String,
    close_tx: SharedCloseTx,
    rx_slot: SharedMessageRx,
    owner_subs: Rc<RefCell<HashMap<u64, SharedCloseTx>>>,
}

impl RedisSubscription {
    fn done_result(ctx: &JSContext) -> JSResult<JSObject> {
        let result = JSObject::new(ctx);
        result.set("done", true)?;
        result.set("value", JSValue::undefined(ctx))?;
        Ok(result)
    }

    fn value_result(ctx: &JSContext, message: RedisSubscriptionMessage) -> JSResult<JSObject> {
        let result = JSObject::new(ctx);
        let value = JSObject::new(ctx);
        value.set("channel", message.channel.as_str())?;
        value.set("message", message.message.as_str())?;
        result.set("done", false)?;
        result.set("value", value)?;
        Ok(result)
    }

    fn close_internal(&self) {
        self.owner_subs.borrow_mut().remove(&self.id);
        if let Ok(mut guard) = self.close_tx.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.rx_slot.lock() {
            *guard = None;
        }
    }
}

#[js_class]
impl RedisSubscription {
    #[js_method(constructor, private)]
    fn new() -> JSResult<Self> {
        rong::illegal_constructor(
            "Not allowed 'new RedisSubscription()'. Use client.subscribe(channel) instead.",
        )
    }

    #[js_method(getter)]
    fn channel(&self) -> String {
        self.channel.clone()
    }

    #[js_method]
    fn close(&self) {
        self.close_internal();
    }

    #[js_method]
    async fn next(&self, ctx: JSContext) -> JSResult<JSObject> {
        let mut rx = {
            let mut guard = self
                .rx_slot
                .lock()
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Subscription is poisoned"))?;
            guard.take()
        };

        let Some(mut rx) = rx.take() else {
            return Self::done_result(&ctx);
        };

        match rx.recv().await {
            Some(Ok(message)) => {
                if let Ok(mut guard) = self.rx_slot.lock()
                    && guard.is_none()
                {
                    *guard = Some(rx);
                }
                Self::value_result(&ctx, message)
            }
            Some(Err(message)) => {
                self.close_internal();
                Err(HostError::new("E_IO", message).into())
            }
            None => {
                self.close_internal();
                Self::done_result(&ctx)
            }
        }
    }

    #[js_method(rename = "return")]
    async fn r#return(&self, ctx: JSContext) -> JSResult<JSObject> {
        self.close_internal();
        Self::done_result(&ctx)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

#[js_class]
pub struct RedisClient {
    url: String,
    conn: Rc<RefCell<Option<redis::aio::MultiplexedConnection>>>,
    namespace_prefix: Option<String>,
    /// Active subscriptions tracked so `client.close()` can tear them down.
    subs: Rc<RefCell<HashMap<u64, SharedCloseTx>>>,
    next_sub_id: Rc<Cell<u64>>,
}

impl RedisClient {
    fn prefixed_name(&self, name: &str) -> String {
        match self.namespace_prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}{name}"),
            _ => name.to_string(),
        }
    }

    fn namespaced(&self) -> bool {
        matches!(self.namespace_prefix.as_deref(), Some(prefix) if !prefix.is_empty())
    }

    fn ensure_raw_command_allowed(&self) -> JSResult<()> {
        if self.namespaced() {
            return Err(HostError::new(
                "E_INVALID_STATE",
                "RedisClient.send() is disabled on namespaced injected clients",
            )
            .with_name("TypeError")
            .into());
        }
        Ok(())
    }

    /// Create a new `RedisClient` from Rust.
    ///
    /// This is the primary Rust-side API for creating pre-configured clients,
    /// e.g. in environments that inject instances via a platform namespace
    /// instead of exposing the JS constructor.
    pub fn new(url: String, namespace_prefix: Option<String>) -> Self {
        Self {
            url,
            conn: Rc::new(RefCell::new(None)),
            namespace_prefix,
            subs: Rc::new(RefCell::new(HashMap::new())),
            next_sub_id: Rc::new(Cell::new(0)),
        }
    }

    async fn ensure_conn(&self) -> JSResult<redis::aio::MultiplexedConnection> {
        {
            let conn = self.conn.borrow();
            if let Some(c) = conn.as_ref() {
                return Ok(c.clone());
            }
        }

        let client = Client::open(self.url.as_str()).map_err(|e| {
            HostError::new("E_INVALID_ARG", format!("Invalid Redis URL: {}", e))
                .with_name("TypeError")
        })?;

        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| HostError::new("E_IO", format!("Failed to connect to Redis: {}", e)))?;

        *self.conn.borrow_mut() = Some(conn.clone());
        Ok(conn)
    }
}

#[js_class]
impl RedisClient {
    #[js_method(constructor)]
    fn constructor(url: Optional<String>) -> JSResult<Self> {
        let url = url.0.ok_or_else(|| {
            HostError::new(
                "E_INVALID_ARG",
                "RedisClient(url) requires an explicit Redis URL",
            )
            .with_name("TypeError")
        })?;
        Ok(Self::new(url, None))
    }

    /// Explicitly connect to the Redis server.
    /// Optional — commands auto-connect on first use.
    #[js_method]
    async fn connect(&self) -> JSResult<()> {
        self.ensure_conn().await?;
        Ok(())
    }

    /// Close the connection and all subscriptions.
    #[js_method]
    fn close(&self) {
        let subs = self.subs.borrow_mut().drain().collect::<Vec<_>>();
        for (_, close_tx) in subs {
            if let Ok(mut guard) = close_tx.lock() {
                guard.take();
            }
        }
        *self.conn.borrow_mut() = None;
    }

    /// Whether the client currently holds an open connection.
    #[js_method(getter)]
    fn connected(&self) -> bool {
        self.conn.borrow().is_some()
    }

    // ── String operations ────────────────────────────────────────────

    #[js_method]
    async fn set(&self, key: String, value: String) -> JSResult<String> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let _: () = conn.set(&key, &value).await.map_err(redis_err)?;
        Ok("OK".to_string())
    }

    #[js_method]
    async fn get(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: Option<String> = conn.get(&key).await.map_err(redis_err)?;
        match result {
            Some(s) => Ok(JSValue::from_rust(&ctx, s)),
            None => Ok(JSValue::null(&ctx)),
        }
    }

    #[js_method]
    async fn del(&self, key: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let count: i32 = conn.del(&key).await.map_err(redis_err)?;
        Ok(count)
    }

    #[js_method]
    async fn exists(&self, key: String) -> JSResult<bool> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let exists: bool = conn.exists(&key).await.map_err(redis_err)?;
        Ok(exists)
    }

    #[js_method]
    async fn expire(&self, key: String, seconds: i64) -> JSResult<bool> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: bool = conn.expire(&key, seconds).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn ttl(&self, key: String) -> JSResult<i64> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let ttl: i64 = conn.ttl(&key).await.map_err(redis_err)?;
        Ok(ttl)
    }

    // ── Numeric operations ───────────────────────────────────────────

    #[js_method]
    async fn incr(&self, key: String) -> JSResult<i64> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let val: i64 = conn.incr(&key, 1i64).await.map_err(redis_err)?;
        Ok(val)
    }

    #[js_method]
    async fn decr(&self, key: String) -> JSResult<i64> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let val: i64 = conn.decr(&key, 1i64).await.map_err(redis_err)?;
        Ok(val)
    }

    // ── Hash operations ──────────────────────────────────────────────

    #[js_method]
    async fn hset(&self, key: String, field: String, value: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: i32 = conn.hset(&key, &field, &value).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn hget(&self, ctx: JSContext, key: String, field: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: Option<String> = conn.hget(&key, &field).await.map_err(redis_err)?;
        match result {
            Some(s) => Ok(JSValue::from_rust(&ctx, s)),
            None => Ok(JSValue::null(&ctx)),
        }
    }

    #[js_method]
    async fn hmset(&self, key: String, fields: Vec<String>) -> JSResult<String> {
        if !fields.len().is_multiple_of(2) {
            return Err(HostError::new(
                "E_INVALID_ARG",
                "Fields must be [field, value, ...] pairs (even length)",
            )
            .with_name("TypeError")
            .into());
        }
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let pairs: Vec<(&str, &str)> = fields
            .chunks(2)
            .map(|c| (c[0].as_str(), c[1].as_str()))
            .collect();
        let _: () = conn.hset_multiple(&key, &pairs).await.map_err(redis_err)?;
        Ok("OK".to_string())
    }

    #[js_method]
    async fn hmget(&self, ctx: JSContext, key: String, fields: Vec<String>) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let mut cmd = redis::cmd("HMGET");
        cmd.arg(&key);
        for field in &fields {
            cmd.arg(field);
        }
        let results: Vec<Option<String>> = cmd.query_async(&mut conn).await.map_err(redis_err)?;
        let arr = JSArray::new(&ctx)?;
        for r in &results {
            match r {
                Some(s) => {
                    arr.push(JSValue::from_rust(&ctx, s.as_str()))?;
                }
                None => {
                    arr.push(JSValue::null(&ctx))?;
                }
            }
        }
        Ok(arr.into_js_value(&ctx))
    }

    #[js_method]
    async fn hincrby(&self, key: String, field: String, increment: i64) -> JSResult<i64> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let val: i64 = conn
            .hincr(&key, &field, increment)
            .await
            .map_err(redis_err)?;
        Ok(val)
    }

    #[js_method]
    async fn hincrbyfloat(&self, key: String, field: String, increment: f64) -> JSResult<f64> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let val: f64 = conn
            .hincr(&key, &field, increment)
            .await
            .map_err(redis_err)?;
        Ok(val)
    }

    // ── Set operations ───────────────────────────────────────────────

    #[js_method]
    async fn sadd(&self, key: String, member: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: i32 = conn.sadd(&key, &member).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn srem(&self, key: String, member: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: i32 = conn.srem(&key, &member).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn sismember(&self, key: String, member: String) -> JSResult<bool> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: bool = conn.sismember(&key, &member).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn smembers(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let results: Vec<String> = conn.smembers(&key).await.map_err(redis_err)?;
        let arr = JSArray::new(&ctx)?;
        for s in &results {
            arr.push(JSValue::from_rust(&ctx, s.as_str()))?;
        }
        Ok(arr.into_js_value(&ctx))
    }

    #[js_method]
    async fn srandmember(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: Option<String> = conn.srandmember(&key).await.map_err(redis_err)?;
        match result {
            Some(s) => Ok(JSValue::from_rust(&ctx, s)),
            None => Ok(JSValue::null(&ctx)),
        }
    }

    #[js_method]
    async fn spop(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: Option<String> = conn.spop(&key).await.map_err(redis_err)?;
        match result {
            Some(s) => Ok(JSValue::from_rust(&ctx, s)),
            None => Ok(JSValue::null(&ctx)),
        }
    }

    // ── List operations ──────────────────────────────────────────────

    #[js_method]
    async fn lpush(&self, key: String, value: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: i32 = conn.lpush(&key, &value).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn rpush(&self, key: String, value: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: i32 = conn.rpush(&key, &value).await.map_err(redis_err)?;
        Ok(result)
    }

    #[js_method]
    async fn lpop(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: Option<String> = conn.lpop(&key, None).await.map_err(redis_err)?;
        match result {
            Some(s) => Ok(JSValue::from_rust(&ctx, s)),
            None => Ok(JSValue::null(&ctx)),
        }
    }

    #[js_method]
    async fn rpop(&self, ctx: JSContext, key: String) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let result: Option<String> = conn.rpop(&key, None).await.map_err(redis_err)?;
        match result {
            Some(s) => Ok(JSValue::from_rust(&ctx, s)),
            None => Ok(JSValue::null(&ctx)),
        }
    }

    #[js_method]
    async fn lrange(
        &self,
        ctx: JSContext,
        key: String,
        start: i64,
        stop: i64,
    ) -> JSResult<JSValue> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let results: Vec<String> = conn
            .lrange(&key, start as isize, stop as isize)
            .await
            .map_err(redis_err)?;
        let arr = JSArray::new(&ctx)?;
        for s in &results {
            arr.push(JSValue::from_rust(&ctx, s.as_str()))?;
        }
        Ok(arr.into_js_value(&ctx))
    }

    #[js_method]
    async fn llen(&self, key: String) -> JSResult<i64> {
        let mut conn = self.ensure_conn().await?;
        let key = self.prefixed_name(&key);
        let len: i64 = conn.llen(&key).await.map_err(redis_err)?;
        Ok(len)
    }

    // ── Pub/Sub ──────────────────────────────────────────────────────

    #[js_method]
    async fn publish(&self, channel: String, message: String) -> JSResult<i32> {
        let mut conn = self.ensure_conn().await?;
        let channel = self.prefixed_name(&channel);
        let result: i32 = conn.publish(&channel, &message).await.map_err(redis_err)?;
        Ok(result)
    }

    /// Subscribe to a channel and return an async-iterable subscription object.
    #[js_method]
    async fn subscribe(
        &self,
        ctx: JSContext,
        channel: String,
        options: Optional<JSObject>,
    ) -> JSResult<JSObject> {
        let mut abort_rx = subscribe_abort_receiver_from_options(&options)?;
        let logical_channel = channel;
        let channel = self.prefixed_name(&logical_channel);

        let client = Client::open(self.url.as_str()).map_err(|e| {
            HostError::new("E_INVALID_ARG", format!("Invalid Redis URL: {}", e))
                .with_name("TypeError")
        })?;

        let mut pubsub = client.get_async_pubsub().await.map_err(|e| {
            HostError::new("E_IO", format!("Failed to open PubSub connection: {}", e))
        })?;

        pubsub
            .subscribe(&channel)
            .await
            .map_err(|e| HostError::new("E_IO", format!("Failed to subscribe: {}", e)))?;

        let (close_tx, mut close_rx) = oneshot::channel::<()>();
        let close_tx = Arc::new(Mutex::new(Some(close_tx)));
        let (msg_tx, msg_rx) = mpsc::channel::<Result<RedisSubscriptionMessage, String>>(64);
        let event_channel = logical_channel.clone();

        rong::spawn_local(async move {
            let mut stream = pubsub.on_message();
            loop {
                if let Some(abort_rx) = &mut abort_rx {
                    tokio::select! {
                        msg = stream.next() => {
                            match msg {
                                Some(msg) => {
                                    let payload = match msg.get_payload::<String>() {
                                        Ok(payload) => payload,
                                        Err(e) => {
                                            let _ = msg_tx.send(Err(format!("Failed to decode pub/sub payload: {}", e))).await;
                                            break;
                                        }
                                    };
                                    let event = RedisSubscriptionMessage {
                                        channel: event_channel.clone(),
                                        message: payload,
                                    };
                                    if msg_tx.send(Ok(event)).await.is_err() {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                        _ = &mut close_rx => break,
                        _ = abort_rx.recv() => break,
                    }
                } else {
                    tokio::select! {
                        msg = stream.next() => {
                            match msg {
                                Some(msg) => {
                                    let payload = match msg.get_payload::<String>() {
                                        Ok(payload) => payload,
                                        Err(e) => {
                                            let _ = msg_tx.send(Err(format!("Failed to decode pub/sub payload: {}", e))).await;
                                            break;
                                        }
                                    };
                                    let event = RedisSubscriptionMessage {
                                        channel: event_channel.clone(),
                                        message: payload,
                                    };
                                    if msg_tx.send(Ok(event)).await.is_err() {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                        _ = &mut close_rx => break,
                    }
                }
            }
        });

        let id = self.next_sub_id.get().saturating_add(1);
        self.next_sub_id.set(id);
        self.subs.borrow_mut().insert(id, close_tx.clone());

        let subscription = RedisSubscription {
            id,
            channel: logical_channel,
            close_tx,
            rx_slot: Arc::new(Mutex::new(Some(msg_rx))),
            owner_subs: self.subs.clone(),
        };
        let obj = rong::Class::lookup::<RedisSubscription>(&ctx)?.instance(subscription);
        if let Err(e) = rong::install_async_iterator_symbol(&ctx, &obj) {
            self.subs.borrow_mut().remove(&id);
            return Err(e);
        }
        Ok(obj)
    }

    // ── Raw command ──────────────────────────────────────────────────

    #[js_method]
    async fn send(&self, ctx: JSContext, command: String, args: Vec<String>) -> JSResult<JSValue> {
        self.ensure_raw_command_allowed()?;
        let mut conn = self.ensure_conn().await?;
        let mut redis_cmd = redis::cmd(&command);
        for arg in &args {
            redis_cmd.arg(arg);
        }
        let result: RedisValue = redis_cmd.query_async(&mut conn).await.map_err(redis_err)?;
        redis_value_to_js(&ctx, result)
    }

    #[js_method(gc_mark)]
    fn gc_mark_with<F>(&self, _mark_fn: F)
    where
        F: FnMut(&JSValue),
    {
    }
}

fn subscribe_abort_receiver_from_options(
    options: &Optional<JSObject>,
) -> JSResult<Option<rong_abort::AbortReceiver>> {
    let Some(options) = options.0.as_ref() else {
        return Ok(None);
    };
    if !options.has_property("signal")? {
        return Ok(None);
    }

    let signal = options.get::<_, JSValue>("signal")?;
    if signal.is_undefined() || signal.is_null() {
        return Ok(None);
    }

    let signal_obj = signal.into_object().ok_or_else(|| {
        HostError::new("E_INVALID_ARG", "options.signal must be an AbortSignal")
            .with_name("TypeError")
    })?;
    let signal = signal_obj.borrow::<AbortSignal>().map_err(|_| {
        HostError::new("E_INVALID_ARG", "options.signal must be an AbortSignal")
            .with_name("TypeError")
    })?;
    if signal.aborted() {
        return Err(rong::RongJSError::from_thrown_value(signal.get_reason()));
    }
    Ok(Some(signal.subscribe()))
}

fn redis_err(e: redis::RedisError) -> rong::RongJSError {
    HostError::new("E_IO", e.to_string()).into()
}

fn redis_value_to_js(ctx: &JSContext, value: RedisValue) -> JSResult<JSValue> {
    match value {
        RedisValue::Nil => Ok(JSValue::null(ctx)),
        RedisValue::Int(i) => Ok(JSValue::from_rust(ctx, i)),
        RedisValue::BulkString(bytes) => match String::from_utf8(bytes) {
            Ok(s) => Ok(JSValue::from_rust(ctx, s)),
            Err(e) => Ok(JSValue::from_rust(
                ctx,
                String::from_utf8_lossy(e.as_bytes()).to_string(),
            )),
        },
        RedisValue::SimpleString(s) => Ok(JSValue::from_rust(ctx, s)),
        RedisValue::Okay => Ok(JSValue::from_rust(ctx, "OK")),
        RedisValue::Array(arr) | RedisValue::Set(arr) => {
            let js_arr = JSArray::new(ctx)?;
            for v in arr {
                js_arr.push(redis_value_to_js(ctx, v)?)?;
            }
            Ok(js_arr.into_js_value(ctx))
        }
        RedisValue::Double(f) => Ok(JSValue::from_rust(ctx, f)),
        RedisValue::Boolean(b) => Ok(JSValue::from_rust(ctx, b)),
        RedisValue::Map(pairs) => {
            let obj = rong::CoreJSObject::new(ctx);
            for (k, v) in pairs {
                if let RedisValue::BulkString(key_bytes) = k {
                    if let Ok(key) = String::from_utf8(key_bytes) {
                        obj.set(key.as_str(), redis_value_to_js(ctx, v)?)?;
                    }
                } else if let RedisValue::SimpleString(key) = k {
                    obj.set(key.as_str(), redis_value_to_js(ctx, v)?)?;
                }
            }
            Ok(obj.into_js_value())
        }
        RedisValue::VerbatimString { text, .. } => Ok(JSValue::from_rust(ctx, text)),
        _ => Ok(JSValue::null(ctx)),
    }
}
