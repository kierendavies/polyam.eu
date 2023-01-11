#![warn(clippy::pedantic)]

mod commands;

use crate::commands::bubblewrap::Bubblewrap;
use crate::commands::enabled_commands_impl;
use serenity::async_trait;
use serenity::model::prelude::interaction::Interaction;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::env;
use tracing::error;
use tracing::info;
use tracing::info_span;
use tracing::Instrument;

struct EnabledCommands;

impl EnabledCommands {
    enabled_commands_impl!(Bubblewrap);
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    #[tracing::instrument(skip_all)]
    // #[tracing::instrument(skip(self, ctx))]
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!(?ready);

        for guild in ready.guilds {
            let span = info_span!("set_application_commands", %guild.id);
            async {
                let commands = guild
                    .id
                    .set_application_commands(&ctx.http, EnabledCommands::set_application_commands)
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
        info!(?interaction, "interaction_create");

        match &interaction {
            Interaction::Ping(_) => error!("Unhandled ping"),

            Interaction::ApplicationCommand(command_interaction) => {
                info!("");
                if let Err(error) =
                    EnabledCommands::handle_command_interaction(ctx, command_interaction).await
                {
                    error!(%error);
                }
            }

            Interaction::MessageComponent(_) => todo!(),
            Interaction::Autocomplete(_) => todo!(),
            Interaction::ModalSubmit(_) => todo!(),
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
