#![warn(clippy::pedantic)]

mod commands;
mod config;
mod onboarding;

use crate::commands::bubblewrap::bubblewrap;
use crate::config::Config;
use anyhow::Error;
use anyhow::Result;
use poise::serenity_prelude::Context;
use poise::serenity_prelude::GatewayIntents;
use poise::serenity_prelude::Interaction;
use std::fs;
use tracing::error;

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
    tracing_subscriber::fmt::init();

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
                    match error {
                        poise::FrameworkError::EventHandler { error, event, .. } => {
                            error!(%error, ?event);
                        }
                        _ => {
                            if let Err(e) = poise::builtins::on_error(error).await {
                                error!("Error while handling error: {}", e);
                            }
                        }
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
