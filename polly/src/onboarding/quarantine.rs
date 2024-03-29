use anyhow::Context as _;
use serenity::all::{CreateActionRow, CreateMessage, GuildId, Member, Message, UserId};
use tracing::info;

use super::{intro, persist};
use crate::{
    context::Context,
    error::{is_http_not_found, Result},
};

fn create_welcome_message(guild_name: &str, member: &Member) -> CreateMessage {
    let content = format!(
        "Welcome to {guild_name}, {member}! Please introduce yourself before you can start chatting.\n\
        \n\
        **Rules**\n\
        1. **DM = BAN**. This server is not for dating or hookups.\n\
        2. You must be at least 18 years old.\n\
        3. Always follow the Code of Conduct, available at https://polyam.eu/coc.html.\n\
        4. Speak English in the common channels."
    );

    CreateMessage::new()
        .content(content)
        .components(vec![CreateActionRow::Buttons(vec![intro::create_button()])])
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

    let message = channel
        .send_message(ctx.serenity(), create_welcome_message(&guild.name, member))
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
        persist::welcome_message::get(&mut *tx, guild_id, user_id).await?
    {
        channel_id
            .delete_message(ctx.serenity(), message_id)
            .await
            .or_else(|err| {
                // If the message was already deleted, continue with deleting the database row.
                if is_http_not_found(&err) {
                    Ok(())
                } else {
                    Err(err)
                }
            })?;

        persist::welcome_message::delete(&mut *tx, guild_id, user_id).await?;
    }

    tx.commit().await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn quarantine(ctx: &impl Context, member: &Member) -> Result<()> {
    let config = ctx.config().guild(member.guild_id)?;

    member
        .add_role(ctx.serenity(), config.quarantine_role)
        .await?;

    send_welcome_message(ctx, member).await?;

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

    delete_welcome_message(ctx, member.guild_id, member.user.id).await?;

    info!(
        %member.guild_id,
        %member.user.id,
        member.user.tag = member.user.tag(),
        "Unquarantined member"
    );

    Ok(())
}
