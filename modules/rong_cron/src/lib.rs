//! In-process cron support.
//!
//! This module implements `cron.parse(expression, relativeDate?)` and
//! `cron(schedule, handler)`. It intentionally does not implement OS-level
//! cron registration.

use chrono::{TimeZone, Utc};
use croner::parser::{CronParser, Seconds, Year};
use rong::function::{Optional, This};
use rong::{
    HostError, JSContext, JSContextService, JSDate, JSFunc, JSObject, JSResult, JSRuntimeService,
    JSValue, RongExecutor,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{mpsc, oneshot};
use tokio_cron_scheduler::{Job, JobScheduler, job::JobId};

#[cfg(test)]
const CRON_TEST_ID_PROPERTY: &str = "__rongCronTestId";

fn lock_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn type_error(message: impl Into<String>) -> HostError {
    HostError::new(rong::error::E_TYPE, message.into()).with_name("TypeError")
}

fn cron_error(message: impl Into<String>) -> rong::RongJSError {
    type_error(message).into()
}

fn normalize_expression(expression: &str) -> JSResult<String> {
    let mut normalized = expression.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(cron_error("Cron expression must not be empty"));
    }

    normalized = match normalized.as_str() {
        "@YEARLY" | "@ANNUALLY" => "0 0 1 1 *".to_string(),
        "@MONTHLY" => "0 0 1 * *".to_string(),
        "@WEEKLY" => "0 0 * * 0".to_string(),
        "@DAILY" | "@MIDNIGHT" => "0 0 * * *".to_string(),
        "@HOURLY" => "0 * * * *".to_string(),
        _ if normalized.starts_with('@') => {
            return Err(cron_error(format!(
                "Unsupported cron nickname: {}",
                expression.trim()
            )));
        }
        _ => normalized,
    };

    for (full, short) in [
        ("SUNDAY", "SUN"),
        ("MONDAY", "MON"),
        ("TUESDAY", "TUE"),
        ("WEDNESDAY", "WED"),
        ("THURSDAY", "THU"),
        ("FRIDAY", "FRI"),
        ("SATURDAY", "SAT"),
        ("JANUARY", "JAN"),
        ("FEBRUARY", "FEB"),
        ("MARCH", "MAR"),
        ("APRIL", "APR"),
        ("JUNE", "JUN"),
        ("JULY", "JUL"),
        ("AUGUST", "AUG"),
        ("SEPTEMBER", "SEP"),
        ("OCTOBER", "OCT"),
        ("NOVEMBER", "NOV"),
        ("DECEMBER", "DEC"),
    ] {
        normalized = normalized.replace(full, short);
    }

    let field_count = normalized.split_whitespace().count();
    if field_count != 5 {
        return Err(cron_error(
            "Cron expression must be a 5-field expression or a supported nickname",
        ));
    }

    Ok(normalized)
}

fn parse_cron(expression: &str) -> JSResult<croner::Cron> {
    let normalized = normalize_expression(expression)?;
    CronParser::builder()
        .seconds(Seconds::Disallowed)
        .year(Year::Disallowed)
        .dom_and_dow(false)
        .build()
        .parse(&normalized)
        .map_err(|_| cron_error(format!("Invalid cron expression: {expression}")))
}

fn scheduler_expressions(expression: &str) -> JSResult<Vec<String>> {
    let normalized = normalize_expression(expression)?;
    let fields = normalized.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(cron_error(
            "Cron expression must be a 5-field expression or a supported nickname",
        ));
    }

    let minute = fields[0];
    let hour = fields[1];
    let dom = fields[2];
    let month = fields[3];
    let dow = fields[4];

    if dom != "*" && dow != "*" {
        Ok(vec![
            format!("0 {minute} {hour} {dom} {month} *"),
            format!("0 {minute} {hour} * {month} {dow}"),
        ])
    } else {
        Ok(vec![format!("0 {normalized}")])
    }
}

fn date_time_from_epoch_ms(ms: f64) -> JSResult<chrono::DateTime<Utc>> {
    if !ms.is_finite() {
        return Err(cron_error(
            "Cron relativeDate must be a finite number or Date",
        ));
    }

    Utc.timestamp_millis_opt(ms as i64)
        .single()
        .ok_or_else(|| cron_error("Cron relativeDate is outside the supported Date range"))
}

fn relative_date_ms(value: Optional<JSValue>) -> JSResult<f64> {
    let Some(value) = value.0 else {
        return Ok(Utc::now().timestamp_millis() as f64);
    };

    if value.is_undefined() || value.is_null() {
        return Ok(Utc::now().timestamp_millis() as f64);
    }

    if value.is_date() {
        let date: JSDate = value.to_rust()?;
        let time = date.get_time()?;
        if !time.is_finite() {
            return Err(cron_error("Cron relativeDate must be a valid Date"));
        }
        return Ok(time);
    }

    value
        .to_rust::<f64>()
        .map_err(|_| cron_error("Cron relativeDate must be a finite number or Date"))
        .and_then(|time| {
            if time.is_finite() {
                Ok(time)
            } else {
                Err(cron_error(
                    "Cron relativeDate must be a finite number or Date",
                ))
            }
        })
}

fn cron_parse(
    ctx: JSContext,
    expression: String,
    relative_date: Optional<JSValue>,
) -> JSResult<JSValue> {
    let cron = parse_cron(&expression)?;
    let start = date_time_from_epoch_ms(relative_date_ms(relative_date)?)?;

    match cron.find_next_occurrence(&start, false) {
        Ok(next) => Ok(JSDate::new(&ctx, next.timestamp_millis() as f64).into()),
        Err(_) => Ok(JSValue::null(&ctx)),
    }
}

struct CronInvocation {
    id: u32,
    done: oneshot::Sender<()>,
}

#[derive(Clone, Default)]
struct CronSchedulerState {
    scheduler: Arc<Mutex<Option<JobScheduler>>>,
}

impl JSRuntimeService for CronSchedulerState {
    fn on_shutdown(&self) {
        if let Some(mut scheduler) = self.take() {
            RongExecutor::global().spawn(async move {
                let _ = scheduler.shutdown().await;
            });
        }
    }
}

struct CronEntry {
    scheduler_ids: Vec<JobId>,
    callback: JSFunc,
    handle: JSObject,
    active: Arc<AtomicBool>,
    refed: AtomicBool,
}

#[derive(Clone)]
struct CronContextRegistry {
    inner: Rc<CronContextRegistryInner>,
}

struct CronContextRegistryInner {
    next_id: AtomicU32,
    entries: RefCell<HashMap<u32, CronEntry>>,
    tx: mpsc::UnboundedSender<CronInvocation>,
    rx: RefCell<Option<mpsc::UnboundedReceiver<CronInvocation>>>,
    scheduler: CronSchedulerState,
}

impl CronContextRegistry {
    fn ensure(ctx: &JSContext) -> Self {
        if let Some(registry) = ctx.get_service::<Self>() {
            return registry.clone();
        }

        let scheduler = ctx
            .runtime()
            .get_or_init_service::<CronSchedulerState>()
            .clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let registry = Self {
            inner: Rc::new(CronContextRegistryInner {
                next_id: AtomicU32::new(1),
                entries: RefCell::new(HashMap::new()),
                tx,
                rx: RefCell::new(Some(rx)),
                scheduler,
            }),
        };
        ctx.set_service(registry.clone());
        registry.start(ctx);
        registry
    }

    fn next_id(&self) -> u32 {
        self.inner.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn tx(&self) -> mpsc::UnboundedSender<CronInvocation> {
        self.inner.tx.clone()
    }

    fn start(&self, ctx: &JSContext) {
        let Some(mut rx) = self.inner.rx.borrow_mut().take() else {
            return;
        };
        let registry = self.clone();

        ctx.spawn_task(async move {
            while let Some(invocation) = rx.recv().await {
                let callback = registry.callback(invocation.id);
                if let Some((callback, handle)) = callback {
                    let _ = callback.call_async::<_, JSValue>(Some(handle), ()).await;
                }
                let _ = invocation.done.send(());
            }
        });
    }

    fn callback(&self, id: u32) -> Option<(JSFunc, JSObject)> {
        let entries = self.inner.entries.borrow();
        let entry = entries.get(&id)?;
        if !entry.active.load(Ordering::SeqCst) {
            return None;
        }
        Some((entry.callback.clone(), entry.handle.clone()))
    }

    fn insert(&self, id: u32, entry: CronEntry) {
        self.inner.entries.borrow_mut().insert(id, entry);
    }

    fn stop_job(&self, id: u32) {
        let removed = self.inner.entries.borrow_mut().remove(&id);
        if let Some(entry) = removed {
            entry.active.store(false, Ordering::SeqCst);
            if let Some(scheduler) = self.inner.scheduler.current() {
                RongExecutor::global().spawn(async move {
                    for scheduler_id in entry.scheduler_ids {
                        let _ = scheduler.remove(&scheduler_id).await;
                    }
                });
            }
        }
    }

    fn set_refed(&self, id: u32, refed: bool) {
        if let Some(entry) = self.inner.entries.borrow().get(&id) {
            entry.refed.store(refed, Ordering::SeqCst);
        }
    }

    fn scheduler_state(&self) -> CronSchedulerState {
        self.inner.scheduler.clone()
    }

    fn shutdown(&self) {
        let entries = {
            let mut entries = self.inner.entries.borrow_mut();
            entries
                .drain()
                .flat_map(|(_, entry)| {
                    entry.active.store(false, Ordering::SeqCst);
                    entry.scheduler_ids
                })
                .collect::<Vec<_>>()
        };

        if let Some(scheduler) = self.inner.scheduler.current() {
            RongExecutor::global().spawn(async move {
                for id in entries {
                    let _ = scheduler.remove(&id).await;
                }
            });
        }

        let _ = self.inner.rx.borrow_mut().take();
    }
}

impl JSContextService for CronContextRegistry {
    fn on_shutdown(&self) {
        self.shutdown();
    }
}

impl CronSchedulerState {
    fn current(&self) -> Option<JobScheduler> {
        lock_poison(&self.scheduler).clone()
    }

    fn take(&self) -> Option<JobScheduler> {
        lock_poison(&self.scheduler).take()
    }

    async fn get_or_start(&self) -> Result<JobScheduler, rong::RongJSError> {
        if let Some(scheduler) = self.current() {
            return Ok(scheduler);
        }

        let scheduler = JobScheduler::new()
            .await
            .map_err(|err| HostError::new(rong::error::E_INTERNAL, err.to_string()))?;
        scheduler
            .start()
            .await
            .map_err(|err| HostError::new(rong::error::E_INTERNAL, err.to_string()))?;

        let mut slot = lock_poison(&self.scheduler);
        if let Some(existing) = slot.clone() {
            return Ok(existing);
        }
        *slot = Some(scheduler.clone());
        Ok(scheduler)
    }
}

fn make_handle(
    ctx: &JSContext,
    registry: CronContextRegistry,
    id: u32,
    cron: &str,
) -> JSResult<JSObject> {
    let handle = JSObject::new(ctx);
    #[cfg(test)]
    handle.define_property(
        CRON_TEST_ID_PROPERTY,
        rong::PropertyDescriptor::from_rust(ctx, id)
            .readonly()
            .hidden()
            .non_configurable(),
    )?;

    handle.define_property(
        "cron",
        rong::PropertyDescriptor::from_rust(ctx, cron.to_string())
            .readonly()
            .enumerable()
            .non_configurable(),
    )?;

    let stop_registry = registry.clone();
    handle.set(
        "stop",
        JSFunc::new(ctx, move |this: This<JSObject>| -> JSResult<JSObject> {
            stop_registry.stop_job(id);
            Ok(this.0.clone())
        })?
        .name("stop")?,
    )?;

    let unref_registry = registry.clone();
    handle.set(
        "unref",
        JSFunc::new(ctx, move |this: This<JSObject>| -> JSResult<JSObject> {
            unref_registry.set_refed(id, false);
            Ok(this.0.clone())
        })?
        .name("unref")?,
    )?;

    handle.set(
        "ref",
        JSFunc::new(ctx, move |this: This<JSObject>| -> JSResult<JSObject> {
            registry.set_refed(id, true);
            Ok(this.0.clone())
        })?
        .name("ref")?,
    )?;

    Ok(handle)
}

fn create_cron_job(
    ctx: JSContext,
    registry: CronContextRegistry,
    schedule: String,
    handler: JSFunc,
) -> JSResult<JSObject> {
    let normalized = normalize_expression(&schedule)?;
    let schedules_for_scheduler = scheduler_expressions(&schedule)?;

    if cron_parse(
        ctx.clone(),
        schedule.clone(),
        Optional(Some(JSValue::undefined(&ctx))),
    )?
    .is_null()
    {
        return Err(cron_error(format!(
            "Cron expression has no future occurrences: {schedule}"
        )));
    }

    let id = registry.next_id();
    let handle = make_handle(&ctx, registry.clone(), id, &normalized)?;
    let active = Arc::new(AtomicBool::new(true));
    let running = Arc::new(AtomicBool::new(false));
    let callback_tx = registry.tx();

    let mut jobs = Vec::with_capacity(schedules_for_scheduler.len());
    let mut scheduler_ids = Vec::with_capacity(schedules_for_scheduler.len());
    for schedule_for_scheduler in schedules_for_scheduler {
        let active_for_job = active.clone();
        let running_for_job = running.clone();
        let callback_tx_for_job = callback_tx.clone();
        let job = Job::new_async(schedule_for_scheduler, move |_uuid, _jobs| {
            let active = active_for_job.clone();
            let running = running_for_job.clone();
            let tx = callback_tx_for_job.clone();

            Box::pin(async move {
                if !active.load(Ordering::SeqCst) {
                    return;
                }
                if running.swap(true, Ordering::SeqCst) {
                    return;
                }

                let (done_tx, done_rx) = oneshot::channel();
                if tx.send(CronInvocation { id, done: done_tx }).is_ok() {
                    let _ = done_rx.await;
                }
                running.store(false, Ordering::SeqCst);
            })
        })
        .map_err(|_| cron_error(format!("Invalid cron expression: {schedule}")))?;

        scheduler_ids.push(job.guid());
        jobs.push(job);
    }

    let active_for_add = active.clone();
    registry.insert(
        id,
        CronEntry {
            scheduler_ids,
            callback: handler,
            handle: handle.clone(),
            active,
            refed: AtomicBool::new(true),
        },
    );

    let scheduler_state = registry.scheduler_state();
    RongExecutor::global().spawn(async move {
        let Ok(scheduler) = scheduler_state.get_or_start().await else {
            active_for_add.store(false, Ordering::SeqCst);
            return;
        };
        for job in jobs {
            if scheduler.add(job).await.is_err() {
                active_for_add.store(false, Ordering::SeqCst);
                return;
            }
        }
    });

    Ok(handle)
}

fn cron_call(
    ctx: JSContext,
    first: JSValue,
    second: Optional<JSValue>,
    third: Optional<JSValue>,
) -> JSResult<JSObject> {
    let schedule = first
        .to_rust::<String>()
        .map_err(|_| cron_error("Rong.cron expects a schedule string"))?;

    if third.0.is_some() {
        return Err(cron_error("OS-level cron registration is not supported"));
    }

    let Some(second) = second.0 else {
        return Err(cron_error("Rong.cron expects a handler function"));
    };
    let handler = second
        .to_rust::<JSFunc>()
        .map_err(|_| cron_error("Rong.cron expects a handler function"))?;

    let registry = CronContextRegistry::ensure(&ctx);

    create_cron_job(ctx, registry, schedule, handler)
}

/// Initialize the Cron module.
pub fn init(ctx: &JSContext) -> JSResult<()> {
    let _ = ctx.runtime().get_or_init_service::<CronSchedulerState>();
    let _ = CronContextRegistry::ensure(ctx);

    let cron = JSFunc::new(ctx, cron_call)?.name("cron")?;
    cron.set("parse", JSFunc::new(ctx, cron_parse)?.name("parse")?)?;

    let rong = ctx.host_namespace();
    rong.set("cron", cron.clone())?;

    let global = ctx.global();
    let bun = match global.get::<_, JSObject>("Bun") {
        Ok(obj) => obj,
        Err(_) => {
            let obj = JSObject::new(ctx);
            global.set("Bun", obj.clone())?;
            obj
        }
    };
    bun.set("cron", cron)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rong_test::*;

    async fn trigger_cron_job_for_test(ctx: JSContext, job: JSObject) -> JSResult<()> {
        let id: u32 = job.get(CRON_TEST_ID_PROPERTY)?;
        let registry = CronContextRegistry::ensure(&ctx);

        let (done_tx, done_rx) = oneshot::channel();
        registry
            .tx()
            .send(CronInvocation { id, done: done_tx })
            .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Cron test trigger failed"))?;
        let _ = done_rx.await;
        Ok(())
    }

    #[test]
    fn test_cron_unit() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;
            rong_console::init(&ctx)?;
            rong_assert::init(&ctx)?;
            ctx.global().set(
                "__triggerCronJob",
                JSFunc::new(&ctx, trigger_cron_job_for_test)?.name("__triggerCronJob")?,
            )?;

            let passed = UnitJSRunner::load_script(&ctx, "cron.js")
                .await?
                .run()
                .await?;
            assert!(passed);

            Ok(())
        });
    }

    #[test]
    fn cron_registers_in_process_handle() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;

            let cron: String = ctx.eval(rong::Source::from_bytes(
                r#"
                const job = Rong.cron("* * * * *", function () {});
                const cron = job.cron;
                job.stop();
                cron;
                "#,
            ))?;
            assert_eq!(cron, "* * * * *");

            Ok(())
        });
    }

    #[test]
    fn context_shutdown_cancels_pending_cron_callback() {
        async_run!(|ctx: JSContext| async move {
            init(&ctx)?;

            let job: JSObject = ctx.eval(rong::Source::from_bytes(
                r#"
                Rong.cron("* * * * *", async function () {
                    await new Promise(() => {});
                });
                "#,
            ))?;
            let id: u32 = job.get(CRON_TEST_ID_PROPERTY)?;
            let registry = CronContextRegistry::ensure(&ctx);
            let (done_tx, done_rx) = oneshot::channel();
            registry
                .tx()
                .send(CronInvocation { id, done: done_tx })
                .map_err(|_| HostError::new(rong::error::E_INTERNAL, "Cron test trigger failed"))?;

            tokio::task::yield_now().await;
            ctx.shutdown_tasks().await;

            assert!(
                done_rx.await.is_err(),
                "context shutdown must cancel an in-flight cron callback"
            );
            Ok(())
        });
    }
}
