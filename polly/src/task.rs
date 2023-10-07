use std::{future::Future, time::Duration};

use crate::{context::Context, error::Result, error_reporting::report_background_task_error};

pub async fn periodic<'ctx, Ctx, Fut, F>(task_name: &str, period: Duration, ctx: &'ctx Ctx, f: F)
where
    Ctx: Context,
    Fut: Future<Output = Result<()>>,
    F: Fn(&'ctx Ctx) -> Fut,
{
    let mut timer = tokio::time::interval(period);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        timer.tick().await;

        tracing::info!(task_name, "Running scheduled task");
        if let Err(err) = f(ctx).await {
            if let Err(handling_err) = report_background_task_error(task_name, ctx, err).await {
                tracing::error!(error = ?handling_err, "Error while handling error");
            }
        }
    }
}
