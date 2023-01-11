pub mod bubblewrap;

use serenity::async_trait;
use serenity::builder::CreateApplicationCommand;
use serenity::json::Value;
use serenity::model::prelude::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::Context;
use serenity::prelude::SerenityError;

#[derive(Debug)]
pub enum CommandError {
    InteractionNotHandled,
    OptionInvalid(&'static str, Option<Value>),
    OptionMissing(&'static str),
    Serenity(SerenityError),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::InteractionNotHandled => write!(f, "Interaction not handled"),
            CommandError::OptionInvalid(name, value) => {
                write!(f, "Option invalid: {name} => {value:?}")
            }
            CommandError::OptionMissing(name) => write!(f, "Option missing: {name}"),
            CommandError::Serenity(inner) => inner.fmt(f),
        }
    }
}

impl std::error::Error for CommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CommandError::Serenity(inner) => Some(inner),
            _ => None,
        }
    }
}

impl From<SerenityError> for CommandError {
    fn from(value: SerenityError) -> CommandError {
        CommandError::Serenity(value)
    }
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
    ) -> Result<(), CommandError>;
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
        ) -> Result<(), crate::commands::CommandError> {
            use crate::commands::Command;
            match interaction.data.name.as_str() {
                $(
                    <$command>::NAME => $command
                        .handle_command_interaction(ctx, interaction)
                        .await,
                )*
                _ => Err(crate::commands::CommandError::InteractionNotHandled),
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
                _ => Err(Error::OptionInvalid($name, opt.value.clone())),
            },
            None => Err(Error::OptionMissing($name)),
        }
    };
    ($options:expr, $name:expr, $type:path, $default:expr) => {
        match $options.iter().find(|opt| opt.name == $name) {
            Some(opt) => match opt.resolved {
                Some($type(value)) => Ok(value),
                _ => Err(CommandError::OptionInvalid($name, opt.value.clone())),
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
