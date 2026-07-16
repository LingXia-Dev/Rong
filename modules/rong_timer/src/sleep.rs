use rong::{HostError, JSContext, JSDate, JSFunc, JSResult, JSValue, function::Optional};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

use super::{TimerCancellation, TimerRegistration};

fn type_error(message: &str) -> HostError {
    HostError::new(rong::error::E_TYPE, message).with_name("TypeError")
}

fn duration_from_number(ms: f64, message: &str) -> JSResult<u64> {
    if !ms.is_finite() {
        return Err(type_error(message).into());
    }
    Ok(ms.max(0.0) as u64)
}

fn current_time_ms() -> JSResult<f64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| HostError::new(rong::error::E_INTERNAL, err.to_string()))?
        .as_millis() as f64)
}

fn parse_sleep_delay(value: Optional<JSValue>) -> JSResult<u64> {
    let Some(value) = value.0 else {
        return Ok(0);
    };

    if value.is_date() {
        let date: JSDate = value.to_rust()?;
        let target_time = date.get_time()?;
        if !target_time.is_finite() {
            return Err(type_error("Rong.sleep target Date must be valid").into());
        }
        return duration_from_number(
            target_time - current_time_ms()?,
            "Rong.sleep target Date must be valid",
        );
    }

    let delay_ms = value
        .to_rust::<f64>()
        .map_err(|_| type_error("Rong.sleep expects a number of milliseconds or a Date"))?;
    duration_from_number(
        delay_ms,
        "Rong.sleep expects a finite number of milliseconds or a valid Date",
    )
}

fn parse_sleep_sync_delay(value: Optional<JSValue>) -> JSResult<u64> {
    let Some(value) = value.0 else {
        return Ok(0);
    };

    let delay_ms = value
        .to_rust::<f64>()
        .map_err(|_| type_error("Rong.sleepSync expects a number of milliseconds"))?;
    duration_from_number(
        delay_ms,
        "Rong.sleepSync expects a finite number of milliseconds",
    )
}

async fn sleep_async(ctx: JSContext, delay: Optional<JSValue>) -> JSResult<()> {
    let delay = parse_sleep_delay(delay)?;

    let registry = super::TimerRegistry::ensure(&ctx);
    let timer_id = registry.next_id();
    let (cancel, mut cancel_rx) = TimerCancellation::new();
    registry.register_timer(timer_id, cancel);
    let _registration = TimerRegistration {
        registry,
        id: timer_id,
    };

    tokio::select! {
        _ = sleep(Duration::from_millis(delay)) => Ok(()),
        _ = TimerCancellation::cancelled(&mut cancel_rx) => Ok(()),
    }
}

fn sleep_sync(delay: Optional<JSValue>) -> JSResult<()> {
    let delay = parse_sleep_sync_delay(delay)?;
    if delay > 0 {
        std::thread::sleep(Duration::from_millis(delay));
    }
    Ok(())
}

pub(crate) fn init(ctx: &JSContext) -> JSResult<()> {
    let rong = ctx.host_namespace();

    rong.set("sleep", JSFunc::new(ctx, sleep_async)?.name("sleep")?)?;
    rong.set(
        "sleepSync",
        JSFunc::new(ctx, sleep_sync)?.name("sleepSync")?,
    )?;

    Ok(())
}
