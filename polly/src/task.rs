use std::{future::Future, time::Duration};

use rand::prelude::Distribution;
use tokio::time::Instant;

use crate::{context::Context, error::Result, error_reporting::report_background_task_error};

async fn run<'ctx, Ctx, Fut, F>(task_name: &str, ctx: &'ctx Ctx, f: F)
where
    Ctx: Context,
    Fut: Future<Output = Result<()>>,
    F: Fn(&'ctx Ctx) -> Fut,
{
    tracing::info!(task_name, "Running task");
    if let Err(err) = f(ctx).await {
        if let Err(handling_err) = report_background_task_error(task_name, ctx, err).await {
            tracing::error!(error = ?handling_err, "Error while handling error");
        }
    }
}

pub async fn periodic<'ctx, Ctx, Fut, F>(task_name: &str, period: Duration, ctx: &'ctx Ctx, f: F)
where
    Ctx: Context,
    Fut: Future<Output = Result<()>>,
    F: Fn(&'ctx Ctx) -> Fut,
{
    const MAX_DELAY: Duration = Duration::from_secs(60);

    let init_delay = rand::distr::Uniform::new(Duration::ZERO, period.min(MAX_DELAY))
        .unwrap()
        .sample(&mut rand::rng());

    let mut timer = tokio::time::interval_at(Instant::now() + init_delay, period);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        timer.tick().await;
        run(task_name, ctx, &f).await;
    }
}
