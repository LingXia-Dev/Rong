use rong::{JSResult, Rong, RongJS, Source, TaskMessage};
use std::error::Error;
use std::time::Duration;
use tracing::{error, info, warn};

fn expression_dispatch_plan(worker_count: usize) -> Vec<(usize, usize, usize)> {
    (1..worker_count)
        .zip(0..worker_count.saturating_sub(1))
        .map(|(worker_index, task_index)| (worker_index, worker_index, task_index))
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let num_workers = 4;
    let rong = Rong::<RongJS>::builder()
        .shared()
        .workers(num_workers)
        .build()?;
    info!(num_workers, "Rong worker pool created");

    let workers = rong.workers();
    if workers.len() != num_workers {
        error!(
            expected = num_workers,
            actual = workers.len(),
            "Worker pool size mismatch"
        );
        return Ok(());
    }
    info!(count = workers.len(), "Acquired all workers");

    // Designate Worker 0 for setInterval, others for expressions
    let interval_worker = workers[0].clone();
    let interval_worker_id = interval_worker.id();
    info!(
        worker_id = interval_worker_id,
        "Designating worker for interval task"
    );

    let _interval_task = interval_worker.spawn(async move |runtime, _receiver| -> JSResult<()> {
        info!(worker_id = interval_worker_id, "Interval task started");
        let ctx = runtime.context();
        // Optional: Initialize modules if needed
        let _ = rong_modules::init_all(&ctx).map_err(|e| {
            warn!(worker_id = interval_worker_id, error = ?e, "Failed to init rong_modules (ignoring)");
        });

        let js_code = format!(
            r#"
                console.log("[Worker {} JS] Setting up interval...");
                setInterval(() => {{ console.log("[Worker {} JS] Interval Tick!"); }}, 500);
                console.log("[Worker {} JS] Interval setup complete.");
            "#,
            interval_worker_id, interval_worker_id, interval_worker_id
        );

        ctx.eval::<()>(Source::from_bytes(js_code.as_bytes()))?;

        info!(worker_id = interval_worker_id, "JS interval evaluated, sleeping");
        tokio::time::sleep(Duration::from_secs(60)).await;
        info!(worker_id = interval_worker_id, "Interval task finished sleep (unexpected)");
        Ok(())
    }).await?;
    info!(worker_id = interval_worker_id, "Interval task spawned");

    // Spawn expression evaluation tasks on other workers
    let mut expr_tasks = Vec::with_capacity(workers.len().saturating_sub(1));
    for worker in workers.iter().skip(1) {
        let worker = worker.clone();
        let worker_id = worker.id();
        let task = worker.spawn(async move |runtime, mut receiver| -> JSResult<()> {
            info!(worker_id, "Expr task waiting for expression");
            let ctx = runtime.context();

            if let Some(TaskMessage::String(expr_str)) = receiver.recv().await {
                info!(worker_id, expr = %expr_str, "Received expression");
                match ctx.eval::<i32>(Source::from_bytes(expr_str.as_bytes())) {
                    Ok(result) => {
                        info!(worker_id, expr = %expr_str, result, "Evaluated expression");
                    }
                    Err(e) => {
                        error!(worker_id, expr = %expr_str, error = ?e, "Failed to evaluate expression");
                        return Err(e);
                    }
                }
            } else {
                warn!(worker_id, "Channel closed unexpectedly");
            }
            info!(worker_id, "Expr task finished");
            Ok(())
        }).await?;
        expr_tasks.push(task);
        info!(worker_id = worker.id(), "Expression task spawned");
    }

    // Give tasks time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send expressions ONLY to workers 1, 2, 3
    info!(count = workers.len() - 1, "Sending expressions to workers");
    for (i, worker_index, task_index) in expression_dispatch_plan(workers.len()) {
        let worker = &workers[worker_index];
        let task = &expr_tasks[task_index];
        let expr = format!("{}+{}", i * 2 + 1, i * 2 + 2);
        info!(worker_id = worker.id(), expr = %expr, "Sending expression");
        if let Err(e) = task.send(TaskMessage::String(expr)).await {
            error!(worker_id = worker.id(), error = ?e, "Failed to send message");
        }
    }

    info!(
        worker_id = interval_worker_id,
        "Waiting to observe interval logs"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;

    info!(
        worker_id = interval_worker_id,
        "Terminating interval worker"
    );
    interval_worker.terminate()?;

    info!("Waiting for all workers termination/idle");
    rong.join().await?;
    info!("All workers idle. Example complete.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_one_expression_to_each_non_interval_worker() {
        assert_eq!(
            expression_dispatch_plan(4),
            vec![(1, 1, 0), (2, 2, 1), (3, 3, 2)]
        );
    }
}
