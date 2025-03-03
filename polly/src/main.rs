#![feature(hash_extract_if)]
#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

mod auto_delete;
mod commands;
mod config;
mod context;
mod cracker;
mod error;
mod error_reporting;
mod onboarding;
mod task;

use std::{fs, sync::Arc, time::Duration};

use anyhow::Context as _;
use futures::join;
use once_cell::sync::Lazy;
use serenity::all::{FullEvent, GatewayIntents, Ready};
use shuttle_runtime::SecretStore;
use shuttle_serenity::SerenityService;
use sqlx::PgPool;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    auto_delete::auto_delete,
    commands::bubblewrap::bubblewrap,
    config::Config,
    error::Error,
    error_reporting::{report_error, report_event_handler_error},
};

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

pub struct DataInner {
    pub config: Config,
    pub db: PgPool,
    pub tenor_api_key: String,
}

type Data = Arc<DataInner>;

type PoiseApplicationContext<'a> = poise::ApplicationContext<'a, Data, Error>;
type PoiseCommand = poise::Command<Data, Error>;
type PoiseContext<'a> = poise::Context<'a, Data, Error>;
type PoiseFramework = poise::Framework<Data, Error>;
type PoiseFrameworkContext<'a> = poise::FrameworkContext<'a, Data, Error>;
type PoiseFrameworkError<'a> = poise::FrameworkError<'a, Data, Error>;

fn commands() -> Vec<PoiseCommand> {
    vec![
        bubblewrap(),
        onboarding::intro(),
        onboarding::onboarding_sync_db(),
    ]
}

#[tracing::instrument(skip_all)]
async fn setup(
    serenity_context: &serenity::client::Context,
    ready: &Ready,
    framework: &PoiseFramework,
    config: Config,
    db: PgPool,
    tenor_api_key: String,
) -> crate::error::Result<Data> {
    for guild in &ready.guilds {
        poise::builtins::register_in_guild(
            serenity_context,
            &framework.options().commands,
            guild.id,
        )
        .await?;
    }

    let data = Arc::new(DataInner {
        config,
        db,
        tenor_api_key,
    });

    macro_rules! spawn_periodic {
        ($task:path, $secs:expr) => {{
            let ctx = context::Owned {
                serenity: serenity_context.clone(),
                data: Arc::clone(&data),
            };

            tokio::spawn(async move {
                task::periodic(stringify!($task), Duration::from_secs($secs), &ctx, $task).await;
            });
        }};

        ($task:path, $mins:literal m) => {
            spawn_periodic!($task, $mins * 60)
        };

        ($task:path, $hours:literal h) => {
            spawn_periodic!($task, $hours * 60 * 60)
        };
    }

    spawn_periodic!(auto_delete, 1 m);
    spawn_periodic!(onboarding::check_quarantine, 10 m);
    spawn_periodic!(onboarding::kick_inactive, 1 h);

    Ok(data)
}

async fn on_error(err: PoiseFrameworkError<'_>) {
    const ERROR_REPLY_TEXT: &str = "😵‍💫 Something went wrong. I'll let my admins know about it.";

    async fn inner(err: PoiseFrameworkError<'_>) -> crate::error::Result<()> {
        match err {
            poise::FrameworkError::Command { ctx, .. }
            | poise::FrameworkError::CommandPanic { ctx, .. } => {
                ctx.say(ERROR_REPLY_TEXT).await?;
                report_error(err).await?;
            }

            poise::FrameworkError::EventHandler { .. } => {
                report_error(err).await?;
            }

            _ => {
                poise::builtins::on_error(err).await?;
            }
        }

        Ok(())
    }

    if let Err(handling_err) = inner(err).await {
        tracing::error!(error = ?handling_err, "Error while handling error");
    }
}

#[tracing::instrument(skip_all)]
async fn handle_event(
    serenity_context: &serenity::client::Context,
    event: &FullEvent,
    framework_context: PoiseFrameworkContext<'_>,
) -> crate::error::Result<()> {
    let ctx = context::Event {
        serenity: serenity_context,
        framework: framework_context,
    };

    macro_rules! forward_to {
        ($($fn:expr),+ $(,)?) => {
            join!(
                $(async {
                    if let Err(error) = $fn(&ctx, event).await {
                        _ = report_event_handler_error(error, &serenity_context, event, framework_context).await;
                    }
                }),+
            )
        };
    }

    forward_to!(cracker::handle_event, onboarding::handle_event);

    Ok(())
}

async fn serenity_client(
    token: String,
    config: Config,
    db: PgPool,
    tenor_api_key: String,
) -> Result<serenity::Client, serenity::Error> {
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .setup(|serenity_context, ready, framework| {
            Box::pin(setup(
                serenity_context,
                ready,
                framework,
                config,
                db,
                tenor_api_key,
            ))
        })
        .options(poise::FrameworkOptions {
            commands: commands(),
            on_error: |error| Box::pin(on_error(error)),
            event_handler: |serenity_context, event, framework_context, _| {
                Box::pin(handle_event(serenity_context, event, framework_context))
            },
            ..Default::default()
        })
        .build();

    let client = serenity::Client::builder(token, intents)
        .framework(framework)
        .await?;

    Ok(client)
}

#[shuttle_runtime::main]
async fn shuttle_main(
    #[shuttle_runtime::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] db: PgPool,
) -> Result<SerenityService, shuttle_runtime::Error> {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("polly=trace".parse().unwrap()),
        )
        .finish()
        .with(ErrorLayer::default())
        .init();

    let token = secret_store
        .get("DISCORD_TOKEN")
        .context("Getting DISCORD_TOKEN")?;

    let tenor_api_key = secret_store
        .get("TENOR_API_KEY")
        .context("Getting TENOR_API_KEY")?;

    let config: Config =
        toml::from_str(&fs::read_to_string("polly.toml").context("Reading polly.toml")?)
            .context("Parsing config")?;

    sqlx::migrate!()
        .run(&db)
        .await
        .context("Migrating database")?;

    let client = serenity_client(token, config, db, tenor_api_key)
        .await
        .context("Creating framework")?;

    // https://killavus.github.io/posts/thread-pool-graceful-shutdown/
    tokio::spawn({
        let shard_manager = Arc::clone(&client.shard_manager);

        async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to register Ctrl-C handler");

            shard_manager.shutdown_all().await;
        }
    });

    Ok(client.into())
}
