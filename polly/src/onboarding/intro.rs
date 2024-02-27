use anyhow::Context as _;
use serenity::all::{
    ActionRowComponent,
    CreateActionRow,
    CreateButton,
    CreateEmbed,
    CreateInputText,
    CreateInteractionResponse,
    CreateInteractionResponseMessage,
    CreateMessage,
    CreateModal,
    EditMessage,
    GuildId,
    InputTextStyle,
    Member,
    Message,
    ModalInteraction,
    User,
    UserId,
};
use tracing::warn;

use super::{persist, quarantine::unquarantine};
use crate::{
    context::Context,
    error::{is_http_not_found, Result},
};

pub const MODAL_ID: &str = "onboarding_intro";
const ID_ABOUT_ME: &str = "about_me";
const ID_POLYAMORY_EXPERIENCE: &str = "polyamory_experience";

const LABEL_INTRODUCE_YOURSELF: &str = "Introduce yourself";
const LABEL_ABOUT_ME: &str = "About me";
const LABEL_POLYAMORY_EXPERIENCE: &str = "Polyamory experience";

pub struct Intro {
    pub about_me: String,
    pub polyamory_experience: String,
}

impl Intro {
    fn from_fields<'a>(fields: impl Iterator<Item = (&'a str, impl Into<String>)>) -> Result<Self> {
        let mut about_me: Option<String> = None;
        let mut polyamory_experience: Option<String> = None;

        for (id, value) in fields {
            match id {
                ID_ABOUT_ME => about_me = Some(value.into()),
                ID_POLYAMORY_EXPERIENCE => polyamory_experience = Some(value.into()),
                _ => warn!(id, "Unhandled field ID"),
            }
        }

        Ok(Intro {
            about_me: about_me.with_context(|| format!("Missing field: {ID_ABOUT_ME}"))?,
            polyamory_experience: polyamory_experience
                .with_context(|| format!("Missing field: {ID_POLYAMORY_EXPERIENCE}"))?,
        })
    }

    fn from_modal_interaction(interaction: &ModalInteraction) -> Result<Self> {
        Self::from_fields(
            interaction
                .data
                .components
                .iter()
                .flat_map(|row| row.components.iter())
                .filter_map(|component| match component {
                    ActionRowComponent::InputText(input_text) => {
                        Some((input_text.custom_id.as_str(), input_text.value.as_ref()?))
                    }
                    _ => None,
                }),
        )
    }

    fn from_message_embeds(message: Message) -> Result<Self> {
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
                .into_iter()
                .flat_map(|embed| embed.fields.into_iter())
                .filter_map(|field| label_to_id(&field.name).map(|id| (id, field.value))),
        )
    }
}

pub fn create_button() -> CreateButton {
    CreateButton::new(MODAL_ID)
        .label(LABEL_INTRODUCE_YOURSELF)
        .emoji('👋')
}

pub async fn get(ctx: &impl Context, guild_id: GuildId, user_id: UserId) -> Result<Option<Intro>> {
    let mut tx = ctx.db().begin().await?;

    let message = if let Some((channel_id, message_id)) =
        persist::intro_message::get(&mut *tx, guild_id, user_id).await?
    {
        channel_id
            .message(ctx.serenity(), message_id)
            .await
            .map(Some)
            .or_else(|err| {
                if is_http_not_found(&err) {
                    Ok(None)
                } else {
                    Err(err)
                }
            })?
    } else {
        None
    };

    tx.commit().await?;

    let intro = message.map(Intro::from_message_embeds).transpose()?;

    Ok(intro)
}

fn create_modal(existing_intro: Option<&Intro>) -> CreateModal {
    let mut about_me = CreateInputText::new(InputTextStyle::Paragraph, LABEL_ABOUT_ME, ID_ABOUT_ME)
        .placeholder("I like long walks on the beach... 🏖")
        .required(true)
        .min_length(50)
        .max_length(1000);
    if let Some(intro) = existing_intro {
        about_me = about_me.value(&intro.about_me);
    }

    let mut polyamory_experience = CreateInputText::new(
        InputTextStyle::Paragraph,
        LABEL_POLYAMORY_EXPERIENCE,
        ID_POLYAMORY_EXPERIENCE,
    )
    .placeholder("It's okay if you have none 💕")
    .required(true)
    .max_length(1000);
    if let Some(intro) = existing_intro {
        polyamory_experience = polyamory_experience.value(&intro.polyamory_experience);
    }

    CreateModal::new(MODAL_ID, LABEL_INTRODUCE_YOURSELF).components(vec![
        CreateActionRow::InputText(about_me),
        CreateActionRow::InputText(polyamory_experience),
    ])
}

pub async fn create_modal_for_member(ctx: &impl Context, member: &Member) -> Result<CreateModal> {
    let existing_intro = get(ctx, member.guild_id, member.user.id).await?;

    Ok(create_modal(existing_intro.as_ref()))
}

fn create_embed(user: &User, intro: &Intro) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .description(format!("{user}"))
        .field(LABEL_ABOUT_ME, &intro.about_me, false)
        .field(
            LABEL_POLYAMORY_EXPERIENCE,
            &intro.polyamory_experience,
            false,
        );

    if let Some(avatar_url) = user.static_avatar_url() {
        embed = embed.thumbnail(avatar_url);
    }

    embed
}

fn create_message(user: &User, intro: &Intro) -> CreateMessage {
    CreateMessage::new()
        .content(format!("Introduction: {user}"))
        .embed(create_embed(user, intro))
}

fn edit_message(user: &User, intro: &Intro) -> EditMessage {
    EditMessage::new().embed(create_embed(user, intro))
}

#[tracing::instrument(skip_all)]
async fn publish(ctx: &impl Context, member: &Member, intro: &Intro) -> Result<Message> {
    let config = ctx.config().guild(member.guild_id)?;

    let mut tx = ctx.db().begin().await?;

    let message = if let Some((channel_id, message_id)) =
        persist::intro_message::get(&mut *tx, member.guild_id, member.user.id).await?
    {
        channel_id
            .edit_message(
                ctx.serenity(),
                message_id,
                edit_message(&member.user, intro),
            )
            .await?
    } else {
        let message = config
            .intros_channel
            .send_message(ctx.serenity(), create_message(&member.user, intro))
            .await?;

        persist::intro_message::set(
            &mut *tx,
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

#[tracing::instrument(skip_all)]
pub async fn submit(ctx: &impl Context, interaction: &ModalInteraction) -> Result<()> {
    let mut member = interaction
        .member
        .clone()
        .context("Interaction has no member")?;
    let config = ctx.config().guild(member.guild_id)?;
    let intro = Intro::from_modal_interaction(interaction)?;

    // TODO: When Shuttle has updated to Rust 1.70, switch this to is_some_and
    let is_from_quarantine = match &interaction.message {
        Some(message) => message.channel_id == config.quarantine_channel,
        None => false,
    };

    if is_from_quarantine {
        let ack_content = "Thanks for submitting your introduction. In the next few seconds, you'll get access to the rest of the server.";
        interaction
            .create_response(
                ctx.serenity(),
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(ack_content)
                        .ephemeral(true),
                ),
            )
            .await?;

        publish(ctx, &member, &intro).await?;
        unquarantine(ctx, &mut member).await?;
    } else {
        let message = publish(ctx, &member, &intro).await?;
        let message_url = message.link_ensured(ctx.serenity()).await;

        interaction
            .create_response(
                ctx.serenity(),
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content(format!("Introduction updated {message_url}")),
                ),
            )
            .await?;
    }

    Ok(())
}

pub async fn update_avatar(ctx: &impl Context, member: &Member) -> Result<()> {
    let Some((channel_id, message_id)) =
        persist::intro_message::get(ctx.db(), member.guild_id, member.user.id).await?
    else {
        return Ok(());
    };

    let message = channel_id.message(ctx.serenity(), message_id).await?;
    let intro = Intro::from_message_embeds(message)?;

    channel_id
        .edit_message(
            ctx.serenity(),
            message_id,
            edit_message(&member.user, &intro),
        )
        .await?;

    Ok(())
}
