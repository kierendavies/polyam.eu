use anyhow::Context as _;
use http::StatusCode;
use poise::serenity_prelude::{GuildId, Member, Message, UserId};
use serenity::{builder::CreateMessage, model::Permissions};
use tracing::info;

use super::{persist, ID_INTRO_QUARANTINE, LABEL_INTRODUCE_YOURSELF};
use crate::{
    context::Context,
    error::{bail, Result},
};

#[tracing::instrument(skip_all)]
pub async fn quarantine(ctx: &impl Context, member: &mut Member) -> Result<()> {
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
pub async fn unquarantine(ctx: &impl Context, member: &mut Member) -> Result<()> {
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

fn create_welcome_message<'a, 'b>(
    guild_name: &str,
    member: &Member,
    message: &'b mut CreateMessage<'a>,
) -> &'b mut CreateMessage<'a> {
    let content = format!(
        "Welcome to {guild_name}, {member}! Please introduce yourself before you can start chatting.\n\
        \n\
        **Rules**\n\
        1. **DM = BAN**. This server is not for dating or hookups.\n\
        2. You must be at least 18 years old.\n\
        3. Always follow the Code of Conduct, available at https://polyam.eu/coc.html.\n\
        4. Speak English in the common channels."
    );

    message.content(content).components(|components| {
        components.create_action_row(|row| {
            row.create_button(|button| {
                button
                    .custom_id(ID_INTRO_QUARANTINE)
                    .label(LABEL_INTRODUCE_YOURSELF)
                    .emoji('👋')
            })
        })
    })
}

#[tracing::instrument(skip_all)]
pub async fn send_welcome_message(ctx: &impl Context, member: &Member) -> Result<Message> {
    let config = ctx.config().guild(member.guild_id)?;

    let channel = config
        .quarantine_channel
        .to_channel(ctx.serenity())
        .await?
        .guild()
        .context("Not a guild channel")?;

    assert!(channel.guild_id == member.guild_id);

    let guild = member.guild_id.to_partial_guild(ctx.serenity()).await?;

    let perms = guild.user_permissions_in(&channel, member)?;
    if !perms.contains(Permissions::VIEW_CHANNEL) {
        bail!(
            "Missing VIEW_CHANNEL permission: guild.id={}, guild.name={:?}, channel.id={}, channel.name={:?}, member.user.id={}, member.user.tag={:?}",
            guild.id,
            guild.name,
            channel.id,
            channel.name,
            member.user.id,
            member.user.tag(),
        );
    }

    let message = channel
        .send_message(ctx.serenity(), |message| {
            create_welcome_message(&guild.name, member, message)
        })
        .await?;

    persist::welcome_message::set(
        ctx.db(),
        member.guild_id,
        member.user.id,
        channel.id,
        message.id,
    )
    .await?;

    Ok(message)
}

pub async fn delete_welcome_message(
    ctx: &impl Context,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<()> {
    let mut tx = ctx.db().begin().await?;

    if let Some((channel_id, message_id)) =
        persist::welcome_message::get(&mut tx, guild_id, user_id).await?
    {
        channel_id
            .delete_message(ctx.serenity(), message_id)
            .await
            .or_else(|err| match err {
                // If the message was already deleted, continue with deleting the database row.
                serenity::Error::Http(inner)
                    if inner.status_code() == Some(StatusCode::NOT_FOUND) =>
                {
                    Ok(())
                }
                _ => Err(err),
            })?;

        persist::welcome_message::delete(&mut tx, guild_id, user_id).await?;
    }

    tx.commit().await?;

    Ok(())
}
