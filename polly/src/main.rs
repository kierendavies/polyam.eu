#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

mod auto_delete;
mod commands;
mod config;
mod context;
mod error;
mod error_reporting;
mod onboarding;
mod task;

use std::{fs, sync::Arc, time::Duration};

use anyhow::Context as _;
use poise::{
    futures_util::join,
    serenity_prelude::{GatewayIntents, Ready},
};
use shuttle_poise::ShuttlePoise;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;

use crate::{
    auto_delete::auto_delete,
    commands::bubblewrap::bubblewrap,
    config::Config,
    error::Error,
    error_reporting::report_error,
};

pub struct DataInner {
    pub config: Config,
    pub db: PgPool,
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
) -> crate::error::Result<Data> {
    for guild in &ready.guilds {
        poise::builtins::register_in_guild(
            serenity_context,
            &framework.options().commands,
            guild.id,
        )
        .await?;
    }

    let data = Arc::new(DataInner { config, db });

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
    event: &poise::Event<'_>,
    framework_context: PoiseFrameworkContext<'_>,
    _user_data: &Data,
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
                        let framework_error = poise::FrameworkError::EventHandler {
                            error,
                            ctx: serenity_context,
                            event,
                            framework: framework_context,
                        };
                        on_error(framework_error).await;
                    }
                })+
            )
        };
    }

    forward_to!(onboarding::handle_event);

    Ok(())
}

async fn framework(
    token: String,
    config: Config,
    db: PgPool,
) -> Result<Arc<PoiseFramework>, serenity::Error> {
    poise::Framework::builder()
        .token(token)
        .intents(
            GatewayIntents::non_privileged()
                | GatewayIntents::GUILD_MEMBERS
                | GatewayIntents::MESSAGE_CONTENT,
        )
        .setup(|serenity_context, ready, framework| {
            Box::pin(setup(serenity_context, ready, framework, config, db))
        })
        .options(poise::FrameworkOptions {
            commands: commands(),
            on_error: |error| Box::pin(on_error(error)),
            event_handler: |serenity_context, event, framework_context, user_data| {
                Box::pin(handle_event(
                    serenity_context,
                    event,
                    framework_context,
                    user_data,
                ))
            },
            ..Default::default()
        })
        .build()
        .await
}

#[shuttle_runtime::main]
async fn shuttle_main(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] db: PgPool,
) -> ShuttlePoise<Data, Error> {
    let token = secret_store
        .get("DISCORD_TOKEN")
        .context("Getting DISCORD_TOKEN")?;

    let config: Config =
        toml::from_str(&fs::read_to_string("polly.toml")?).context("Parsing config")?;

    sqlx::migrate!()
        .run(&db)
        .await
        .context("Migrating database")?;

    let framework = framework(token, config, db)
        .await
        .context("Creating framework")?;

    Ok(framework.into())
}
