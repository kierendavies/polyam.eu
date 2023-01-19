#![warn(clippy::pedantic)]

mod commands;
mod config;
mod onboarding;

use crate::commands::bubblewrap::bubblewrap;
use crate::config::Config;
use poise::serenity_prelude::Context;
use poise::serenity_prelude::GatewayIntents;
use poise::serenity_prelude::Interaction;
use std::fmt;
use std::fs;
use tracing::error;
use tracing::warn;
use tracing_error::SpanTrace;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(thiserror::Error)]
pub struct Error {
    source: anyhow::Error,
    span_trace: SpanTrace,
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(f)?;
        write!(f, "\n\nSpan trace:\n")?;
        fmt::Display::fmt(&self.span_trace, f)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(f)
    }
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Error {
            source: error,
            span_trace: SpanTrace::capture(),
        }
    }
}

impl From<serenity::Error> for Error {
    fn from(error: serenity::Error) -> Self {
        Error {
            source: error.into(),
            span_trace: SpanTrace::capture(),
        }
    }
}

macro_rules! bail {
    ($($args:tt)*) => {
        return Err(anyhow::anyhow!($($args)*).into())
    };
}
pub(crate) use bail;

type Result<T> = core::result::Result<T, Error>;

pub struct UserData {
    config: Config,
}

type FrameworkContext<'a> = poise::FrameworkContext<'a, UserData, Error>;

async fn handle_event(
    ctx: &Context,
    framework: FrameworkContext<'_>,
    event: &poise::Event<'_>,
) -> Result<()> {
    match event {
        poise::Event::GuildMemberAddition { new_member } => {
            onboarding::guild_member_addition(ctx, framework, new_member).await?;
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("polly=trace".parse().unwrap()),
        )
        .finish()
        .with(tracing_error::ErrorLayer::default())
        .init();

    let config: Config = toml::from_str(&fs::read_to_string("polly.toml").unwrap()).unwrap();

    let framework = poise::Framework::builder()
        .token(std::env::var("DISCORD_TOKEN").expect("Error reading DISCORD_TOKEN"))
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
                Ok(UserData { config })
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
        });

    framework.run().await.unwrap();
}
