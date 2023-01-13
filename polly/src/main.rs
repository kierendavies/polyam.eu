#![warn(clippy::pedantic)]

mod commands;

use crate::commands::bubblewrap::Bubblewrap;
use crate::commands::Command;
use serenity::async_trait;
use serenity::model::prelude::interaction::Interaction;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::env;
use tracing::error;
use tracing::info;
use tracing::info_span;
use tracing::Instrument;

macro_rules! enable_commands {
    ($($command:path),*) => {
        macro_rules! register_commands {
            ($command_builder:expr) => {
                $command_builder
                    $(.create_application_command(|c| $command.create_application_command(c)))*
            }
        }

        macro_rules! handle_command {
            ($ctx:expr, $interaction:expr, $command_name:expr) => {{
                match $command_name {
                    $(<$command>::NAME => Some($command.handle_interaction($ctx, $interaction).await),)*
                    _ => None,
                }
            }}
        }
    };
}

enable_commands!(Bubblewrap);

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    #[tracing::instrument(skip_all)]
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!(?ready);

        for guild in ready.guilds {
            let span = info_span!("set_application_commands", %guild.id);
            async {
                let commands = guild
                    .id
                    .set_application_commands(&ctx, |commands| register_commands!(commands))
                    .await
                    .unwrap();

                let command_names: Vec<_> = commands.iter().map(|c| c.name.clone()).collect();
                info!(?command_names);
            }
            .instrument(span)
            .await;
        }
    }

    #[tracing::instrument(skip_all, fields(interaction.id = %interaction.id()))]
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let command_name: &str = match &interaction {
            Interaction::ApplicationCommand(i) => &i.data.name,
            Interaction::MessageComponent(i) => {
                let Some(message_interaction) = &i.message.interaction else {
                    error!(?interaction, "Message interaction is missing");
                    return;
                };
                &message_interaction.name
            }
            _ => {
                error!(?interaction, "Interaction type not handled");
                return;
            }
        };

        match handle_command!(ctx, &interaction, command_name) {
            Some(Ok(_)) => info!(command_name, "Handled command"),
            Some(Err(error)) => error!(%error, ?interaction),
            None => error!(command_name, ?interaction, "Unknown command name"),
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN").expect("Error reading DISCORD_TOKEN");

    let intents = GatewayIntents::empty();

    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    client.start().await.expect("Client error");
}
