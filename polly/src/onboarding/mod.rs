mod intro;
mod persist;
mod quarantine;

use std::{collections::HashSet, future};

use anyhow::Context as _;
use futures::TryStreamExt;
use poise::CommandInteractionType;
use serenity::all::{
    ComponentInteraction,
    CreateInteractionResponse,
    FullEvent,
    GuildId,
    Interaction,
    Member,
    Message,
    ModalInteraction,
    User,
};

use self::quarantine::{delete_welcome_message, quarantine};
use crate::{
    config::GuildConfig,
    context::Context,
    error::{Error, Result},
    PoiseApplicationContext,
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
    if member.user.bot {
        return Ok(());
    }

    if intro::get(ctx, member.guild_id, member.user.id)
        .await?
        .is_none()
    {
        quarantine(ctx, member).await?;
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
    if user.bot {
        return Ok(());
    }

    delete_welcome_message(ctx, *guild_id, user.id).await?;

    Ok(())
}

async fn guild_member_update(ctx: &impl Context, new: Option<&Member>) -> Result<()> {
    let member = new.context("Member update has no new member")?;

    if member.user.bot {
        return Ok(());
    }

    intro::update_avatar(ctx, member).await?;

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
pub async fn component_interaction(
    ctx: &impl Context,
    interaction: &ComponentInteraction,
) -> Result<()> {
    match interaction.data.custom_id.as_str() {
        intro::MODAL_ID => {
            let member = interaction
                .member
                .as_ref()
                .context("Interaction has no member")?;
            let modal = intro::create_modal_for_member(ctx, member).await?;

            interaction
                .create_response(ctx.serenity(), CreateInteractionResponse::Modal(modal))
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
pub async fn modal_interaction(ctx: &impl Context, interaction: &ModalInteraction) -> Result<()> {
    match interaction.data.custom_id.as_str() {
        intro::MODAL_ID => intro::submit(ctx, interaction).await,

        _ => Ok(()),
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_event(ctx: &impl Context, event: &FullEvent) -> Result<()> {
    match event {
        FullEvent::GuildMemberAddition { new_member } => {
            guild_member_addition(ctx, new_member).await
        }

        FullEvent::GuildMemberRemoval {
            guild_id,
            user,
            member_data_if_available: _,
        } => guild_member_removal(ctx, guild_id, user).await,

        FullEvent::GuildMemberUpdate {
            old_if_available: _,
            new,
            event: _,
        } => guild_member_update(ctx, new.as_ref()).await,

        FullEvent::InteractionCreate {
            interaction: Interaction::Component(interaction),
        } => component_interaction(ctx, interaction).await,

        FullEvent::InteractionCreate {
            interaction: Interaction::Modal(interaction),
        } => modal_interaction(ctx, interaction).await,

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
pub async fn onboarding_sync_db(ctx: PoiseApplicationContext<'_>) -> Result<()> {
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
pub async fn intro(ctx: PoiseApplicationContext<'_>) -> Result<()> {
    if ctx.interaction_type != CommandInteractionType::Command {
        return Ok(());
    }

    let member = ctx.author_member().await.context("Context has no member")?;

    let modal = intro::create_modal_for_member(&ctx, &member).await?;
    ctx.interaction
        .create_response(
            ctx.serenity_context,
            CreateInteractionResponse::Modal(modal),
        )
        .await?;

    Ok(())
}

fn connected_configured_guilds(
    ctx: &impl Context,
) -> impl Iterator<Item = (GuildId, &'_ GuildConfig)> {
    ctx.serenity()
        .cache
        .guilds()
        .into_iter()
        .filter_map(|guild_id| {
            ctx.config()
                .guilds
                .get(&guild_id)
                .map(|config| (guild_id, config))
        })
}

pub async fn check_quarantine(ctx: &impl Context) -> Result<()> {
    for (guild_id, config) in connected_configured_guilds(ctx) {
        guild_id
            .members_iter(ctx.serenity())
            .err_into::<Error>()
            .try_filter(|member| future::ready(!member.user.bot))
            .try_for_each(|member| async move {
                let introduced = persist::intro_message::get(ctx.db(), guild_id, member.user.id)
                    .await?
                    .is_some();
                let quarantined = member.roles.contains(&config.quarantine_role);

                if !introduced && !quarantined {
                    tracing::warn!(
                        %member.guild_id,
                        %member.user.id,
                        member.user.tag = member.user.tag(),
                        "Member has no intro and is not quarantined",
                    );
                    quarantine(ctx, &member).await?;
                }

                Ok(())
            })
            .await?;
    }

    Ok(())
}

pub async fn kick_inactive(ctx: &impl Context) -> Result<()> {
    const INACTIVE_DAYS: i64 = 7;
    const REASON: &str = "Onboarding not completed"; // Shows in the audit log.

    let cutoff = chrono::Utc::now() - chrono::Duration::days(INACTIVE_DAYS);

    for (guild_id, config) in connected_configured_guilds(ctx) {
        let guild_name = guild_id
            .name(ctx.serenity())
            .context("Guild not available in cache")?;

        let message = format!(
            "You were kicked from {guild_name} because you did not submit an introduction. \
            You can join again using an invite link."
        );

        guild_id
            .members_iter(ctx.serenity())
            .err_into::<Error>()
            .try_filter(|member| future::ready(!member.user.bot))
            .try_for_each(|member| {
                // Needed to satisfy the borrow checker.
                let message = message.as_str();

                async move {
                    let quarantined = member.roles.contains(&config.quarantine_role);
                    let old = member
                        .joined_at
                        .is_some_and(|joined_at| *joined_at < cutoff);

                    if quarantined && old {
                        let dm_channel = member.user.create_dm_channel(ctx.serenity()).await?;
                        dm_channel.say(ctx.serenity(), message).await?;

                        member.kick_with_reason(ctx.serenity(), REASON).await?;
                    }

                    Ok(())
                }
            })
            .await?;
    }

    Ok(())
}
