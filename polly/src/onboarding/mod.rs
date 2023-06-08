mod messages;
mod persist;

use std::{collections::HashSet, future};

use anyhow::Context as _;
use poise::{
    serenity_prelude::{
        ActionRowComponent,
        CreateInteractionResponse,
        GuildId,
        InputTextStyle,
        InteractionResponseType,
        Member,
        Message,
        MessageComponentInteraction,
        ModalSubmitInteraction,
        User,
    },
    ApplicationCommandOrAutocompleteInteraction,
};
use serenity::futures::TryStreamExt;
use tracing::{info, warn};

use self::messages::{delete_welcome_message, edit_or_send_intro_message, send_welcome_message};
use crate::{
    context::Context,
    error::{bail, Error, Result},
    onboarding::messages::get_intro_message,
    UserData,
};

pub const ID_PREFIX: &str = "onboarding_";

const ID_INTRO_QUARANTINE: &str = "onboarding_intro_quarantine";
const ID_INTRO_SLASH: &str = "onboarding_intro_slash";
const ID_ABOUT_ME: &str = "about_me";
const ID_POLYAMORY_EXPERIENCE: &str = "polyamory_experience";

const LABEL_INTRODUCE_YOURSELF: &str = "Introduce yourself";
const LABEL_ABOUT_ME: &str = "About me";
const LABEL_POLYAMORY_EXPERIENCE: &str = "Polyamory experience";

fn label_to_id(label: &str) -> Option<&'static str> {
    match label {
        LABEL_ABOUT_ME => Some(ID_ABOUT_ME),
        LABEL_POLYAMORY_EXPERIENCE => Some(ID_POLYAMORY_EXPERIENCE),
        _ => None,
    }
}

struct IntroFields<'a> {
    about_me: &'a str,
    polyamory_experience: &'a str,
}

impl<'a> IntroFields<'a> {
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

        Ok(IntroFields {
            about_me: about_me.with_context(|| format!("Missing field: {ID_ABOUT_ME}"))?,
            polyamory_experience: polyamory_experience
                .with_context(|| format!("Missing field: {ID_POLYAMORY_EXPERIENCE}"))?,
        })
    }

    fn from_modal_submit_interaction(msi: &'a ModalSubmitInteraction) -> Result<Self> {
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

    fn from_message_embeds(message: &'a Message) -> Result<Self> {
        Self::from_fields(
            message
                .embeds
                .iter()
                .flat_map(|embed| embed.fields.iter())
                .filter_map(|field| label_to_id(&field.name).map(|id| (id, field.value.as_str()))),
        )
    }
}

#[tracing::instrument(skip_all)]
async fn quarantine(ctx: &impl Context, member: &mut Member) -> Result<()> {
    let config = ctx.config().guild(member.guild_id)?;

    member
        .add_role(ctx.serenity(), config.quarantine_role)
        .await?;
    info!(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
        "Quarantined member"
    );

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn unquarantine(ctx: &impl Context, member: &mut Member) -> Result<()> {
    let config = ctx.config().guild(member.guild_id)?;

    member
        .remove_role(ctx.serenity(), config.quarantine_role)
        .await?;
    info!(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
        "Unquarantined member"
    );

    Ok(())
}

fn create_intro_modal<'a, 'b>(
    id: &str,
    intro_fields: Option<&IntroFields>,
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
                                    .min_length(50);
                                if let Some(intro_fields) = intro_fields {
                                    text.value(intro_fields.about_me);
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
                                    .required(true);
                                if let Some(intro_fields) = intro_fields {
                                    text.value(intro_fields.polyamory_experience);
                                }
                                text
                            })
                        })
                })
        })
}

#[tracing::instrument(skip_all)]
async fn submit_intro_quarantined(
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
    let intro_fields = IntroFields::from_modal_submit_interaction(interaction)?;

    unquarantine(ctx, &mut member).await?;
    edit_or_send_intro_message(ctx, guild_id, &interaction.user, &intro_fields).await?;
    interaction
        .delete_original_interaction_response(ctx.serenity())
        .await?;
    delete_welcome_message(ctx, guild_id, interaction.user.id).await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn submit_intro_slash(
    ctx: &impl Context,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    let guild_id = interaction
        .guild_id
        .context("Interaction has no guild_id")?;
    let intro_fields = IntroFields::from_modal_submit_interaction(interaction)?;
    let message =
        edit_or_send_intro_message(ctx, guild_id, &interaction.user, &intro_fields).await?;
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

#[tracing::instrument(
    fields(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
    ),
    skip_all,
)]
pub async fn guild_member_addition(ctx: &impl Context, member: &Member) -> Result<()> {
    let mut member = member.clone();
    quarantine(ctx, &mut member).await?;
    send_welcome_message(ctx, &member).await?;

    Ok(())
}

pub async fn guild_member_removal(
    ctx: &impl Context,
    guild_id: &GuildId,
    user: &User,
) -> Result<()> {
    delete_welcome_message(ctx, *guild_id, user.id).await?;

    Ok(())
}

#[tracing::instrument(
    fields(
        %interaction.id,
        interaction.guild_id = %interaction.guild_id.unwrap_or_default(),
        %interaction.user.id,
        interaction.user.tag = interaction.user.tag(),
        interaction.data.custom_id,
    ),
    skip_all,
)]
pub async fn message_component_interaction(
    ctx: &impl Context,
    interaction: &MessageComponentInteraction,
) -> Result<()> {
    match interaction.data.custom_id.as_str() {
        ID_INTRO_QUARANTINE => {
            interaction
                .create_interaction_response(ctx.serenity(), |response| {
                    create_intro_modal(ID_INTRO_QUARANTINE, None, response)
                })
                .await?;
        }

        _ => bail!("Unhandled custom_id: {:?}", interaction.data.custom_id),
    }

    Ok(())
}

#[tracing::instrument(
    fields(
        %interaction.id,
        interaction.guild_id = %interaction.guild_id.unwrap_or_default(),
        %interaction.user.id,
        interaction.user.tag = interaction.user.tag(),
        interaction.data.custom_id,
    ),
    skip_all,
)]
pub async fn modal_submit_interaction(
    ctx: &impl Context,
    interaction: &ModalSubmitInteraction,
) -> Result<()> {
    match interaction.data.custom_id.as_str() {
        ID_INTRO_QUARANTINE => {
            submit_intro_quarantined(ctx, interaction).await?;
        }

        ID_INTRO_SLASH => {
            submit_intro_slash(ctx, interaction).await?;
        }

        _ => bail!("Unhandled custom_id: {:?}", interaction.data.custom_id),
    }

    Ok(())
}

#[poise::command(
    default_member_permissions = "ADMINISTRATOR",
    guild_only,
    owners_only,
    required_permissions = "ADMINISTRATOR",
    slash_command
)]
pub async fn onboarding_sync_db(ctx: poise::ApplicationContext<'_, UserData, Error>) -> Result<()> {
    ctx.defer().await?;

    let bot_id = ctx.framework().bot_id;
    let guild_id = ctx.guild_id().context("Context has no guild_id")?;
    let config = ctx.config().guild(guild_id)?;

    let found_intros: Vec<Message> = config
        .intros_channel
        .messages_iter(ctx.serenity_context())
        .try_filter(|message| {
            future::ready(
                message.author.id == bot_id
                    && !message.embeds.is_empty()
                    && !message.mentions.is_empty(),
            )
        })
        .try_collect()
        .await?;
    let found_intro_message_ids: HashSet<_> =
        found_intros.iter().map(|message| message.id).collect();

    let mut n_added = 0;
    let mut n_deleted = 0;

    let mut tx = ctx.data().db.begin().await?;

    let persisted_intros = persist::intro_message::get_all(&mut tx, guild_id).await?;
    let persisted_intro_message_ids: HashSet<_> = persisted_intros
        .iter()
        .map(|(_, _, message_id)| *message_id)
        .collect();

    for message in &found_intros {
        if !persisted_intro_message_ids.contains(&message.id) {
            let user_id = message
                .mentions
                .first()
                .context("Message has no mentions")?
                .id;
            persist::intro_message::set(&mut tx, guild_id, user_id, message.channel_id, message.id)
                .await?;
            n_added += 1;
        }
    }

    for (user_id, _, message_id) in &persisted_intros {
        if !found_intro_message_ids.contains(message_id) {
            persist::intro_message::delete(&mut tx, guild_id, *user_id).await?;
            n_deleted += 1;
        }
    }

    tx.commit().await?;

    ctx.say(format!("Intros: added {n_added}, deleted {n_deleted}"))
        .await?;
    Ok(())
}

/// Edit your introduction
#[poise::command(guild_only, slash_command)]
#[tracing::instrument(
    fields(
        ctx.id = ctx.id(),
        ctx.guild_id = %ctx.guild_id().unwrap_or_default(),
        ctx.author.id = %ctx.author().id,
        ctx.author.tag = ?ctx.author().tag(),
    ),
    skip(ctx),
)]
pub async fn intro(ctx: poise::ApplicationContext<'_, UserData, Error>) -> Result<()> {
    let ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction) = ctx.interaction else {
        bail!("Expected ApplicationCommandInteraction");
    };

    let guild_id = ctx.guild_id().context("Context has no guild_id")?;

    let intro_message = get_intro_message(&ctx, guild_id, ctx.author().id).await?;

    let intro_fields = intro_message
        .as_ref()
        .map(IntroFields::from_message_embeds)
        .transpose()
        .unwrap_or_else(|error| {
            warn!(?error, "Error getting intro fields");
            None
        });

    interaction
        .create_interaction_response(ctx.serenity_context, |response| {
            create_intro_modal(ID_INTRO_SLASH, intro_fields.as_ref(), response)
        })
        .await?;

    Ok(())
}
