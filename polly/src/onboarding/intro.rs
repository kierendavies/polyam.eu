use anyhow::Context as _;
use http::StatusCode;
use poise::serenity_prelude::{
    ActionRowComponent,
    GuildId,
    InputTextStyle,
    InteractionResponseType,
    Member,
    Message,
    ModalSubmitInteraction,
    User,
    UserId,
};
use serenity::builder::{
    CreateButton,
    CreateEmbed,
    CreateInteractionResponse,
    CreateMessage,
    EditMessage,
};
use tracing::warn;

use super::{persist, quarantine::unquarantine};
use crate::{context::Context, error::Result};

pub const MODAL_ID: &str = "onboarding_intro";
const ID_ABOUT_ME: &str = "about_me";
const ID_POLYAMORY_EXPERIENCE: &str = "polyamory_experience";

const LABEL_INTRODUCE_YOURSELF: &str = "Introduce yourself";
const LABEL_ABOUT_ME: &str = "About me";
const LABEL_POLYAMORY_EXPERIENCE: &str = "Polyamory experience";

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

pub fn create_button() -> CreateButton {
    let mut button = CreateButton::default();

    button
        .custom_id(MODAL_ID)
        .label(LABEL_INTRODUCE_YOURSELF)
        .emoji('👋');

    button
}

pub fn create_modal<'a>(prefill: Option<&Intro>) -> CreateInteractionResponse<'a> {
    let mut response = CreateInteractionResponse::default();

    response
        .kind(InteractionResponseType::Modal)
        .interaction_response_data(|data| {
            data.custom_id(MODAL_ID)
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
                                if let Some(intro) = prefill {
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
                                if let Some(intro) = prefill {
                                    text.value(intro.polyamory_experience);
                                }
                                text
                            })
                        })
                })
        });

    response
}

fn create_embed(user: &User, intro: &Intro) -> CreateEmbed {
    let mut embed = CreateEmbed::default();

    embed
        .description(format!("{user}"))
        .field(LABEL_ABOUT_ME, intro.about_me, false)
        .field(
            LABEL_POLYAMORY_EXPERIENCE,
            intro.polyamory_experience,
            false,
        );

    if let Some(avatar_url) = user.static_avatar_url() {
        embed.thumbnail(avatar_url);
    }

    embed
}

fn create_message<'a>(user: &User, intro: &Intro) -> CreateMessage<'a> {
    let mut message = CreateMessage::default();

    message
        .content(format!("Introduction: {user}"))
        .embed(|embed| {
            *embed = create_embed(user, intro);
            embed
        });

    message
}

fn edit_message<'a>(user: &User, intro: &Intro) -> EditMessage<'a> {
    let mut message = EditMessage::default();

    message.embed(|embed| {
        *embed = create_embed(user, intro);
        embed
    });

    message
}

#[tracing::instrument(skip_all)]
async fn publish(ctx: &impl Context, member: &Member, intro: &Intro<'_>) -> Result<Message> {
    let config = ctx.config().guild(member.guild_id)?;

    let mut tx = ctx.db().begin().await?;

    let message = if let Some((channel_id, message_id)) =
        persist::intro_message::get(&mut tx, member.guild_id, member.user.id).await?
    {
        channel_id
            .edit_message(ctx.serenity(), message_id, |message| {
                *message = edit_message(&member.user, intro);
                message
            })
            .await?
    } else {
        let message = config
            .intros_channel
            .send_message(ctx.serenity(), |message| {
                *message = create_message(&member.user, intro);
                message
            })
            .await?;

        persist::intro_message::set(
            &mut tx,
            member.guild_id,
            member.user.id,
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
pub async fn submit(ctx: &impl Context, interaction: &ModalSubmitInteraction) -> Result<()> {
    let mut member = interaction
        .member
        .clone()
        .context("Interaction has no member")?;
    let config = ctx.config().guild(member.guild_id)?;
    let intro = Intro::from_modal_submit_interaction(interaction)?;

    let is_from_quarantine = interaction
        .message
        .as_ref()
        .is_some_and(|message| message.channel_id == config.quarantine_channel);

    if is_from_quarantine {
        let ack_content = "Thanks for submitting your introduction. In the next few seconds, you'll get access to the rest of the server.";
        interaction
            .create_interaction_response(ctx.serenity(), |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|data| data.content(ack_content).ephemeral(true))
            })
            .await?;

        publish(ctx, &member, &intro).await?;
        unquarantine(ctx, &mut member).await?;

        interaction
            .delete_original_interaction_response(ctx.serenity())
            .await?;
    } else {
        let message = publish(ctx, &member, &intro).await?;
        let message_url = message.link_ensured(ctx.serenity()).await;

        interaction
            .create_interaction_response(ctx.serenity(), |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|data| {
                        data.content(format!("Introduction updated {message_url}"))
                    })
            })
            .await?;
    }

    Ok(())
}
