#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

mod commands;
pub mod config;
mod context;
mod error;
mod onboarding;

use std::{
    fmt::{self, Write as _},
    fs,
    path::PathBuf,
    sync::Arc,
};

use anyhow::Context as _;
use poise::{
    futures_util::join,
    serenity_prelude::{GatewayIntents, Ready},
    ReplyHandle,
};
use serenity::{constants::MESSAGE_CODE_LIMIT, prelude::Mentionable};
use shuttle_poise::ShuttlePoise;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use tracing::{error, warn};

use crate::{
    commands::bubblewrap::bubblewrap,
    config::Config,
    context::{Context, EventContext, UserData},
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

const ERROR_REPLY_TEXT: &str = "😵‍💫 Something went wrong. I'll let my admins know about it.";

async fn write_command_info(
    w: &mut impl fmt::Write,
    ctx: &poise::Context<'_, UserData, Error>,
    reply: &ReplyHandle<'_>,
) -> crate::error::Result<()> {
    let reply_link = reply.message().await?.link_ensured(ctx).await;

    write!(w, "{} in ", ctx.author().mention())?;
    if let Some(guild) = ctx.partial_guild().await {
        write!(w, "`{}` ({})", guild.name, guild.id,)?;
    } else {
        write!(w, "DM")?;
    }
    writeln!(w, " {} {}", ctx.channel_id().mention(), reply_link)?;

    writeln!(w, "`{}`", ctx.invocation_string())?;

    Ok(())
}

fn write_code_block_truncated(
    w: &mut impl fmt::Write,
    limit: usize,
    text: &str,
) -> crate::error::Result<()> {
    // Discord measures length in Unicode codepoints.
    // The padding has no multi-byte chars, so `.len()` is the same as `.chars().count()`.
    const PADDING_LEN: usize = "```\n\n```\n(999999 bytes truncated)\n".len();

    let limit_index = text
        .char_indices()
        .nth(limit - PADDING_LEN)
        .map_or(text.len(), |(i, _)| i);

    // Try to split at a line break.
    let split_index = text[..limit_index].rfind('\n').unwrap_or(limit_index);

    let (shown, hidden) = text.split_at(split_index);

    writeln!(w, "```\n{}\n```", shown.trim_end())?;
    if !hidden.is_empty() {
        writeln!(w, "({} bytes truncated)", hidden.len())?;
    }

    Ok(())
}

async fn on_event_handler_error(
    error: Error,
    serenity_context: &serenity::client::Context,
    event: &poise::Event<'_>,
    framework_context: poise::FrameworkContext<'_, UserData, Error>,
) -> crate::error::Result<()> {
    error!(?event, ?error, "Event handler error");

    let mut text = String::new();

    writeln!(text, "**Event handler error**")?;

    let event_text_limit = (MESSAGE_CODE_LIMIT - text.chars().count()) / 2;
    let event_text = format!("{event:?}");
    write_code_block_truncated(&mut text, event_text_limit, &event_text)?;

    let error_text_limit = MESSAGE_CODE_LIMIT - text.chars().count();
    let error_text = format!("{error:?}");
    write_code_block_truncated(&mut text, error_text_limit, &error_text)?;

    let errors_channel = framework_context.user_data.config.errors_channel;
    errors_channel.say(serenity_context, text).await?;

    Ok(())
}

async fn on_command_error(
    error: Error,
    ctx: poise::Context<'_, UserData, Error>,
) -> crate::error::Result<()> {
    error!(?error, "Command error");

    let reply = ctx.say(ERROR_REPLY_TEXT).await?;

    let mut text = String::new();
    writeln!(text, "**Command error**")?;

    write_command_info(&mut text, &ctx, &reply).await?;

    let error_text_limit = MESSAGE_CODE_LIMIT - text.chars().count();
    let error_text = format!("{error:?}");
    write_code_block_truncated(&mut text, error_text_limit, &error_text)?;

    let errors_channel = ctx.config().errors_channel;
    errors_channel.say(ctx, text).await?;

    Ok(())
}

async fn on_command_panic(
    payload: Option<String>,
    ctx: poise::Context<'_, UserData, Error>,
) -> crate::error::Result<()> {
    error!(payload, "Command panic");

    let reply = ctx.say(ERROR_REPLY_TEXT).await?;

    let mut text = String::new();
    writeln!(text, "**Command panic**")?;

    write_command_info(&mut text, &ctx, &reply).await?;

    if let Some(payload) = payload {
        let payload_limit = MESSAGE_CODE_LIMIT - text.chars().count();
        write_code_block_truncated(&mut text, payload_limit, &payload)?;
    } else {
        writeln!(text, "No payload")?;
    }

    let errors_channel = ctx.config().errors_channel;
    errors_channel.say(ctx, text).await?;

    Ok(())
}

async fn on_error(error: poise::FrameworkError<'_, UserData, Error>) {
    let handled = match error {
        poise::FrameworkError::EventHandler {
            error,
            ctx,
            event,
            framework,
        } => on_event_handler_error(error, ctx, event, framework).await,

        poise::FrameworkError::Command { error, ctx } => on_command_error(error, ctx).await,

        poise::FrameworkError::CommandPanic { payload, ctx } => {
            on_command_panic(payload, ctx).await
        }

        error => poise::builtins::on_error(error).await.map_err(Into::into),
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

pub async fn framework(
    token: String,
    config: Config,
    db: PgPool,
) -> Result<Arc<poise::Framework<UserData, Error>>, serenity::Error> {
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
