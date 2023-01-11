pub mod bubblewrap;

use anyhow::Result;
use serenity::async_trait;
use serenity::builder::CreateApplicationCommand;
use serenity::json::Value;
use serenity::model::prelude::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::Context;
use serenity::prelude::SerenityError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("Interaction not handled")]
    InteractionNotHandled,
    #[error("Invalid option {0:?}: {1:?}")]
    OptionInvalid(&'static str, Option<Value>),
    #[error("Missing option {0:?}")]
    OptionMissing(&'static str),
}

#[async_trait]
pub trait Command {
    const NAME: &'static str;

    fn create_application_command<'a>(
        &self,
        command: &'a mut CreateApplicationCommand,
    ) -> &'a mut CreateApplicationCommand;

    async fn handle_command_interaction(
        &self,
        ctx: Context,
        interaction: &ApplicationCommandInteraction,
    ) -> Result<()>;
}

macro_rules! enabled_commands_impl {
    ($($command:path),*) => {
        fn set_application_commands(
            commands: &mut serenity::builder::CreateApplicationCommands,
        ) -> &mut serenity::builder::CreateApplicationCommands {
            use crate::commands::Command;
            commands
                $(.create_application_command(|c| $command.create_application_command(c)))*
        }

        async fn handle_command_interaction(
            ctx: serenity::prelude::Context,
            interaction: &serenity::model::prelude::interaction::application_command::ApplicationCommandInteraction,
        ) -> anyhow::Result<()> {
            use crate::commands::Command;
            match interaction.data.name.as_str() {
                $(
                    <$command>::NAME => $command
                        .handle_command_interaction(ctx, interaction)
                        .await,
                )*
                _ => Err(crate::commands::CommandError::InteractionNotHandled)?,
            }
        }
    };
}
pub(crate) use enabled_commands_impl;

macro_rules! option_value {
    ($options:expr, $name:expr, $type:path) => {
        match $options.iter().find(|opt| opt.name == $name) {
            Some(opt) => match opt.resolved {
                Some($type(value)) => Ok(value),
                _ => Err(crate::commands::CommandError::OptionInvalid(
                    $name,
                    opt.value.clone(),
                )),
            },
            None => Err(crate::commands::CommandError::OptionMissing($name)),
        }
    };
    ($options:expr, $name:expr, $type:path, $default:expr) => {
        match $options.iter().find(|opt| opt.name == $name) {
            Some(opt) => match opt.resolved {
                Some($type(value)) => Ok(value),
                _ => Err(crate::commands::CommandError::OptionInvalid(
                    $name,
                    opt.value.clone(),
                )),
            },
            None => Ok($default),
        }
    };
}
pub(self) use option_value;

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
