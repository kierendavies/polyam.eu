#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

mod commands;
pub mod config;
mod context;
mod error;
mod onboarding;

use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Context as _;
use poise::{
    futures_util::join,
    serenity_prelude::{GatewayIntents, Ready},
};
use shuttle_poise::ShuttlePoise;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use tracing::{error, warn};

use crate::{
    commands::bubblewrap::bubblewrap,
    config::Config,
    context::{EventContext, UserData},
    error::Error,
};

fn commands() -> Vec<poise::Command<UserData, Error>> {
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
    framework: &poise::Framework<UserData, Error>,
    config: Config,
    db: PgPool,
) -> crate::error::Result<UserData> {
    for guild in &ready.guilds {
        poise::builtins::register_in_guild(
            serenity_context,
            &framework.options().commands,
            guild.id,
        )
        .await?;
    }

    Ok(UserData { config, db })
}

async fn on_error(error: poise::FrameworkError<'_, UserData, Error>) {
    let handled = match error {
        poise::FrameworkError::Command { error, ctx } => {
            error!(?error);
            poise::builtins::on_error(poise::FrameworkError::Command { error, ctx }).await
        }
        poise::FrameworkError::EventHandler { error, event, .. } => {
            error!(?event, ?error);
            Ok(())
        }
        error => poise::builtins::on_error(error).await,
    };
    if let Err(error) = handled {
        error!(?error, "Error while handling error");
    }
}

#[tracing::instrument(skip_all)]
async fn handle_event(
    serenity_context: &serenity::client::Context,
    event: &poise::Event<'_>,
    framework_context: poise::FrameworkContext<'_, UserData, Error>,
    _user_data: &UserData,
) -> crate::error::Result<()> {
    let ctx = EventContext {
        serenity: serenity_context,
        framework: framework_context,
    };

    macro_rules! forward_to {
        ($($fn:expr),+ $(,)?) => {
            join!(
                $(async {
                    if let Err(error) = $fn(ctx, event).await {
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

pub async fn framework(
    token: String,
    config: Config,
    db: PgPool,
) -> Result<Arc<poise::Framework<UserData, Error>>, serenity::Error> {
    poise::Framework::builder()
        .token(token)
        .intents(GatewayIntents::GUILD_MEMBERS)
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
    #[shuttle_static_folder::StaticFolder] static_folder: PathBuf,
    #[shuttle_shared_db::Postgres] db: PgPool,
) -> ShuttlePoise<UserData, Error> {
    let token = secret_store
        .get("DISCORD_TOKEN")
        .context("Getting DISCORD_TOKEN")?;

    let config: Config = toml::from_str(&fs::read_to_string(static_folder.join("polly.toml"))?)
        .context("Parsing config")?;

    sqlx::migrate!()
        .run(&db)
        .await
        .context("Migrating database")?;

    let framework = framework(token, config, db)
        .await
        .context("Creating framework")?;

    Ok(framework.into())
}
