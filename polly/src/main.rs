#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

mod commands;
pub mod config;
mod error;
mod onboarding;

use crate::commands::bubblewrap::bubblewrap;
use crate::config::Config;
use anyhow::Context as _;
use poise::serenity_prelude::Context;
use poise::serenity_prelude::GatewayIntents;
use poise::serenity_prelude::Interaction;
use shuttle_poise::ShuttlePoise;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::error;
use tracing::warn;

pub struct UserData {
    config: Config,
    db: PgPool,
}

type Framework = poise::Framework<UserData, crate::error::Error>;
type FrameworkContext<'a> = poise::FrameworkContext<'a, UserData, crate::error::Error>;

async fn handle_event(
    ctx: &Context,
    framework: FrameworkContext<'_>,
    event: &poise::Event<'_>,
) -> crate::error::Result<()> {
    match event {
        poise::Event::GuildMemberAddition { new_member } => {
            onboarding::guild_member_addition(ctx, framework, new_member).await?;
        }

        poise::Event::GuildMemberRemoval {
            guild_id,
            user,
            member_data_if_available: _,
        } => {
            onboarding::guild_member_removal(ctx, framework, guild_id, user).await?;
        }

        poise::Event::InteractionCreate {
            interaction: Interaction::MessageComponent(interaction),
        } if interaction
            .data
            .custom_id
            .starts_with(onboarding::ID_PREFIX) =>
        {
            onboarding::message_component_interaction(ctx, framework, interaction).await?;
        }
        poise::Event::InteractionCreate {
            interaction: Interaction::ModalSubmit(interaction),
        } if interaction
            .data
            .custom_id
            .starts_with(onboarding::ID_PREFIX) =>
        {
            onboarding::modal_submit_interaction(ctx, framework, interaction).await?;
        }

        _ => {}
    }

    Ok(())
}

pub async fn bot_framework(
    token: String,
    config: Config,
    db: PgPool,
) -> crate::error::Result<Arc<Framework>> {
    let framework = poise::Framework::builder()
        .token(token)
        .intents(GatewayIntents::GUILD_MEMBERS)
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                for guild in &ready.guilds {
                    poise::builtins::register_in_guild(
                        ctx,
                        &framework.options().commands,
                        guild.id,
                    )
                    .await?;
                }
                Ok(UserData { config, db })
            })
        })
        .options(poise::FrameworkOptions {
            commands: vec![bubblewrap(), onboarding::intro()],
            on_error: |error| {
                Box::pin(async move {
                    let handled = match error {
                        poise::FrameworkError::Command { error, ctx } => {
                            error!(?error);
                            poise::builtins::on_error(poise::FrameworkError::Command { error, ctx })
                                .await
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
                })
            },
            event_handler: |ctx, event, framework, _| {
                Box::pin(async move { handle_event(ctx, framework, event).await })
            },
            ..Default::default()
        })
        .build()
        .await?;

    Ok(framework)
}

#[shuttle_runtime::main]
async fn shuttle_main(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
    #[shuttle_static_folder::StaticFolder] static_folder: PathBuf,
    #[shuttle_shared_db::Postgres] db: PgPool,
) -> ShuttlePoise<UserData, crate::error::Error> {
    let token = secret_store
        .get("DISCORD_TOKEN")
        .context("Getting DISCORD_TOKEN")?;

    let config: Config = toml::from_str(&fs::read_to_string(static_folder.join("polly.toml"))?)
        .context("Parsing config")?;

    sqlx::migrate!()
        .run(&db)
        .await
        .context("Migrating database")?;

    let framework = bot_framework(token, config, db)
        .await
        .context("Creating framework")?;

    Ok(framework.into())
}
