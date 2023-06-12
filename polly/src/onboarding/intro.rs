use anyhow::Context as _;
use http::StatusCode;
use poise::serenity_prelude::{
    ActionRowComponent,
    GuildId,
    InputTextStyle,
    InteractionResponseType,
    Message,
    ModalSubmitInteraction,
    User,
    UserId,
};
use serenity::builder::{CreateEmbed, CreateInteractionResponse, CreateMessage, EditMessage};
use tracing::warn;

use super::{
    persist,
    quarantine::{delete_welcome_message, unquarantine},
    ID_ABOUT_ME,
    ID_POLYAMORY_EXPERIENCE,
    LABEL_ABOUT_ME,
    LABEL_INTRODUCE_YOURSELF,
    LABEL_POLYAMORY_EXPERIENCE,
};
use crate::{context::Context, error::Result};

pub struct Intro<'a> {
    pub about_me: &'a str,
    pub polyamory_experience: &'a str,
}

impl<'a> Intro<'a> {
    fn from_fields(fields: impl Iterator<Item = (&'a str, &'a str)>) -> Result<Self> {
        let mut about_me: Option<&str> = None;
        let mut polyamory_experience: Option<&str> = None;

        for (id, value) in fields {
            match id {
                ID_ABOUT_ME => about_me = Some(value),
                ID_POLYAMORY_EXPERIENCE => polyamory_experience = Some(value),
                _ => warn!(id, "Unhandled field ID"),
            }
        }

        Ok(Intro {
            about_me: about_me.with_context(|| format!("Missing field: {ID_ABOUT_ME}"))?,
            polyamory_experience: polyamory_experience
                .with_context(|| format!("Missing field: {ID_POLYAMORY_EXPERIENCE}"))?,
        })
    }

    pub fn from_modal_submit_interaction(msi: &'a ModalSubmitInteraction) -> Result<Self> {
        Self::from_fields(
            msi.data
                .components
                .iter()
                .flat_map(|row| row.components.iter())
                .filter_map(|component| match component {
                    ActionRowComponent::InputText(input_text) => {
                        Some((input_text.custom_id.as_str(), input_text.value.as_str()))
                    }
                    _ => None,
                }),
        )
    }

    pub fn from_message_embeds(message: &'a Message) -> Result<Self> {
        fn label_to_id(label: &str) -> Option<&'static str> {
            match label {
                LABEL_ABOUT_ME => Some(ID_ABOUT_ME),
                LABEL_POLYAMORY_EXPERIENCE => Some(ID_POLYAMORY_EXPERIENCE),
                _ => None,
            }
        }

        Self::from_fields(
            message
                .embeds
                .iter()
                .flat_map(|embed| embed.fields.iter())
                .filter_map(|field| label_to_id(&field.name).map(|id| (id, field.value.as_str()))),
        )
    }
}

pub fn create_intro_modal<'a, 'b>(
    id: &str,
    intro: Option<&Intro>,
    response: &'b mut CreateInteractionResponse<'a>,
) -> &'b mut CreateInteractionResponse<'a> {
    response
        .kind(InteractionResponseType::Modal)
        .interaction_response_data(|data| {
            data.custom_id(id)
                .title(LABEL_INTRODUCE_YOURSELF)
                .components(|components| {
                    components
                        .create_action_row(|row| {
                            row.create_input_text(|text| {
                                text.custom_id(ID_ABOUT_ME)
                                    .label(LABEL_ABOUT_ME)
                                    .style(InputTextStyle::Paragraph)
                                    .placeholder("I like long walks on the beach... 🏖")
                                    .required(true)
                                    .min_length(50)
                                    .max_length(1000);
                                if let Some(intro) = intro {
                                    text.value(intro.about_me);
                                }
                                text
                            })
                        })
                        .create_action_row(|row| {
                            row.create_input_text(|text| {
                                text.custom_id(ID_POLYAMORY_EXPERIENCE)
                                    .label(LABEL_POLYAMORY_EXPERIENCE)
                                    .style(InputTextStyle::Paragraph)
                                    .placeholder("It's okay if you have none 💕")
                                    .required(true)
                                    .max_length(1000);
                                if let Some(intro) = intro {
                                    text.value(intro.polyamory_experience);
                                }
                                text
                            })
                        })
                })
        })
}

fn create_intro_embed<'a>(
    user: &User,
    intro_fields: &Intro,
    embed: &'a mut CreateEmbed,
) -> &'a mut CreateEmbed {
    embed
        .description(format!("{user}"))
        .field(LABEL_ABOUT_ME, intro_fields.about_me, false)
        .field(
            LABEL_POLYAMORY_EXPERIENCE,
            intro_fields.polyamory_experience,
            false,
        );
    if let Some(avatar_url) = user.static_avatar_url() {
        embed.thumbnail(avatar_url);
    }
    embed
}

fn create_intro_message<'a, 'b>(
    user: &User,
    intro_fields: &Intro,
    message: &'b mut CreateMessage<'a>,
) -> &'b mut CreateMessage<'a> {
    message
        .content(format!("Introduction: {user}"))
        .embed(|embed| create_intro_embed(user, intro_fields, embed))
}

fn edit_intro_message<'a, 'b>(
    user: &User,
    intro_fields: &Intro,
    message: &'b mut EditMessage<'a>,
) -> &'b mut EditMessage<'a> {
    message.embed(|embed| create_intro_embed(user, intro_fields, embed))
}

#[tracing::instrument(skip_all)]
pub async fn edit_or_send_intro_message(
    ctx: &impl Context,
    guild_id: GuildId,
    user: &User,
    intro_fields: &Intro<'_>,
) -> Result<Message> {
    let config = ctx.config().guild(guild_id)?;

    let mut tx = ctx.db().begin().await?;

    let message = if let Some((channel_id, message_id)) =
        persist::intro_message::get(&mut tx, guild_id, user.id).await?
    {
        channel_id
            .edit_message(ctx.serenity(), message_id, |message| {
                edit_intro_message(user, intro_fields, message)
            })
            .await?
    } else {
        let message = config
            .intros_channel
            .send_message(ctx.serenity(), |message| {
                create_intro_message(user, intro_fields, message)
            })
            .await?;
        persist::intro_message::set(
            &mut tx,
            guild_id,
            user.id,
            config.intros_channel,
            message.id,
        )
        .await?;
        message
    };

    tx.commit().await?;

    Ok(message)
}

pub async fn get_intro_message(
    ctx: &impl Context,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<Option<Message>> {
    let mut tx = ctx.db().begin().await?;

    let message = if let Some((channel_id, message_id)) =
        persist::intro_message::get(&mut tx, guild_id, user_id).await?
    {
        match channel_id.message(ctx.serenity(), message_id).await {
            Ok(message) => Some(message),
            Err(serenity::Error::Http(err)) if err.status_code() == Some(StatusCode::NOT_FOUND) => {
                persist::intro_message::delete(&mut tx, guild_id, user_id).await?;
                None
            }
            Err(err) => return Err(err.into()),
        }
    } else {
        None
    };

    tx.commit().await?;

    Ok(message)
}

#[tracing::instrument(skip_all)]
pub async fn submit_intro_quarantined(
    ctx: &impl Context,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let ack_content = "Thanks for submitting your introduction. In the next few seconds, you'll get access to the rest of the server.";
    interaction
        .create_interaction_response(ctx.serenity(), |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|data| data.content(ack_content).ephemeral(true))
        })
        .await?;

    let guild_id = interaction
        .guild_id
        .context("Interaction has no guild_id")?;
    let mut member = interaction
        .member
        .clone()
        .context("Interaction has no member")?;
    let intro = Intro::from_modal_submit_interaction(interaction)?;

    unquarantine(ctx, &mut member).await?;
    edit_or_send_intro_message(ctx, guild_id, &interaction.user, &intro).await?;
    interaction
        .delete_original_interaction_response(ctx.serenity())
        .await?;
    delete_welcome_message(ctx, guild_id, interaction.user.id).await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn submit_intro_slash(
    ctx: &impl Context,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let guild_id = interaction
        .guild_id
        .context("Interaction has no guild_id")?;
    let intro = Intro::from_modal_submit_interaction(interaction)?;
    let message = edit_or_send_intro_message(ctx, guild_id, &interaction.user, &intro).await?;
    let message_url = message.link_ensured(ctx.serenity()).await;

    interaction
        .create_interaction_response(ctx.serenity(), |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|data| {
                    data.content(format!("Introduction updated {message_url}"))
                    // Putting the URL in a button causes an error. Discord bug?
                    // Invalid Form Body (data.components.0.components.0.custom_id: A custom id is required)
                    // .components(|components| {
                    //     components.create_action_row(|row| {
                    //         row.create_button(|button| {
                    //             button.label("See your introduction").url(message_url)
                    //         })
                    //     })
                    // })
                })
        })
        .await?;

    Ok(())
}
