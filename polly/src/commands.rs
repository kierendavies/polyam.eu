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
