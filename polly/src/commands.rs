pub mod bubblewrap;

use anyhow::Result;
use serenity::async_trait;
use serenity::builder::CreateApplicationCommand;
use serenity::model::prelude::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::Interaction;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::Context;
use serenity::prelude::SerenityError;

#[async_trait]
pub trait Command {
    const NAME: &'static str;

    fn create_application_command<'a>(
        &self,
        command: &'a mut CreateApplicationCommand,
    ) -> &'a mut CreateApplicationCommand;

    async fn handle_interaction(&self, ctx: Context, interaction: &Interaction) -> Result<()>;
}

macro_rules! enabled_commands_impl {
    ($($command:path),*) => {
        fn set_application_commands(
            commands: &mut ::serenity::builder::CreateApplicationCommands,
        ) -> &mut ::serenity::builder::CreateApplicationCommands {
            use $crate::commands::Command;
            commands
                $(.create_application_command(|c| $command.create_application_command(c)))*
        }

        async fn handle_interaction(
            ctx: ::serenity::prelude::Context,
            interaction: &::serenity::model::prelude::interaction::Interaction,
        ) -> ::anyhow::Result<()> {
            use $crate::commands::Command;

            let command_name: &str = match &interaction {
                ::serenity::model::prelude::interaction::Interaction::ApplicationCommand(i) => &i.data.name,
                ::serenity::model::prelude::interaction::Interaction::MessageComponent(i) => {
                    let Some(message_interaction) = &i.message.interaction else {
                        ::anyhow::bail!("Message interaction is missing");
                    };
                    &message_interaction.name
                },
                _ => ::anyhow::bail!("Interaction type not handled"),
            };
            ::tracing::info!(command_name);

            match command_name {
                $(
                    <$command>::NAME => $command.handle_interaction(ctx, interaction).await,
                )*
                _ => ::anyhow::bail!("No command matches name {command_name:?}"),
            }
        }
    };
}
pub(crate) use enabled_commands_impl;

async fn respond_with_content<S: ToString>(
    ctx: Context,
    interaction: &ApplicationCommandInteraction,
    content: S,
) -> Result<(), SerenityError> {
    interaction
        .create_interaction_response(ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|response| response.content(content))
        })
        .await
}
