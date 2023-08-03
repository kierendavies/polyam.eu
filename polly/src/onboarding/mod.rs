mod intro;
mod persist;
mod quarantine;

use std::{collections::HashSet, future};

use anyhow::Context as _;
use poise::{
    serenity_prelude::{
        GuildId,
        Interaction,
        Member,
        Message,
        MessageComponentInteraction,
        ModalSubmitInteraction,
        User,
    },
    ApplicationCommandOrAutocompleteInteraction,
};
use serenity::futures::TryStreamExt;

use self::quarantine::{delete_welcome_message, quarantine};
use crate::{
    context::{Context, UserData},
    error::{bail, Error, Result},
};

#[tracing::instrument(
    fields(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
    ),
    skip_all,
)]
async fn guild_member_addition(ctx: &impl Context, member: &Member) -> Result<()> {
    let mut member = member.clone();

    if intro::get(ctx, member.guild_id, member.user.id)
        .await?
        .is_none()
    {
        quarantine(ctx, &mut member).await?;
    }

    Ok(())
}

#[tracing::instrument(
    fields(
        %guild_id,
        %user.id,
        user.tag = user.tag(),
    ),
    skip_all,
)]
async fn guild_member_removal(ctx: &impl Context, guild_id: &GuildId, user: &User) -> Result<()> {
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
        intro::MODAL_ID => {
            let member = interaction
                .member
                .as_ref()
                .context("Interaction has no member")?;
            let modal = intro::create_modal_for_member(ctx, member).await?;

            interaction
                .create_interaction_response(ctx.serenity(), |response| {
                    *response = modal;
                    response
                })
                .await?;

            Ok(())
        }

        _ => Ok(()),
    }
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
        intro::MODAL_ID => intro::submit(ctx, interaction).await,

        _ => Ok(()),
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_event(ctx: &impl Context, event: &poise::Event<'_>) -> Result<()> {
    match event {
        poise::Event::GuildMemberAddition { new_member } => {
            guild_member_addition(ctx, new_member).await
        }

        poise::Event::GuildMemberRemoval {
            guild_id,
            user,
            member_data_if_available: _,
        } => guild_member_removal(ctx, guild_id, user).await,

        poise::Event::InteractionCreate {
            interaction: Interaction::MessageComponent(interaction),
        } => message_component_interaction(ctx, interaction).await,

        poise::Event::InteractionCreate {
            interaction: Interaction::ModalSubmit(interaction),
        } => modal_submit_interaction(ctx, interaction).await,

        _ => Ok(()),
    }
}

#[poise::command(
    default_member_permissions = "ADMINISTRATOR",
    guild_only,
    owners_only,
    required_permissions = "ADMINISTRATOR",
    slash_command
)]
#[tracing::instrument(
    fields(
        ctx.id = ctx.id(),
        ctx.guild_id = %ctx.guild_id().unwrap_or_default(),
    ),
    skip(ctx),
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

    let persisted_intros = persist::intro_message::get_all(&mut *tx, guild_id).await?;
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
            persist::intro_message::set(
                &mut *tx,
                guild_id,
                user_id,
                message.channel_id,
                message.id,
            )
            .await?;
            n_added += 1;
        }
    }

    for (user_id, _, message_id) in &persisted_intros {
        if !found_intro_message_ids.contains(message_id) {
            persist::intro_message::delete(&mut *tx, guild_id, *user_id).await?;
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
    let ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction) =
        ctx.interaction
    else {
        bail!("Expected ApplicationCommandInteraction");
    };

    let member = ctx.author_member().await.context("Context has no member")?;

    let modal = intro::create_modal_for_member(&ctx, &member).await?;

    interaction
        .create_interaction_response(ctx.serenity_context, |response| {
            *response = modal;
            response
        })
        .await?;

    Ok(())
}
