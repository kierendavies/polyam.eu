use anyhow::Context as _;
use http::StatusCode;
use poise::serenity_prelude::{ChannelId, GuildId, Message, User, UserId};
use serenity::{
    builder::{CreateEmbed, CreateMessage, EditMessage},
    model::{guild::Member, Permissions},
    prelude::Context,
};
use sqlx::PgPool;

use super::{
    cache,
    IntroFields,
    ID_INTRO_QUARANTINE,
    LABEL_ABOUT_ME,
    LABEL_INTRODUCE_YOURSELF,
    LABEL_POLYAMORY_EXPERIENCE,
};
use crate::error::{bail, Result};

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
pub(super) async fn send_welcome_message(
    ctx: &Context,
    db: &PgPool,
    channel_id: ChannelId,
    member: &Member,
) -> Result<Message> {
    let channel = channel_id
        .to_channel(ctx)
        .await?
        .guild()
        .context("Not a guild channel")?;

    assert!(channel.guild_id == member.guild_id);

    let guild = member.guild_id.to_partial_guild(ctx).await?;

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

    let message = channel_id
        .send_message(ctx, |message| {
            create_welcome_message(&guild.name, member, message)
        })
        .await?;

    cache::welcome_message::set(db, member.guild_id, member.user.id, channel_id, message.id)
        .await?;

    Ok(message)
}

#[tracing::instrument(skip_all)]
pub(super) async fn delete_welcome_message(
    ctx: &Context,
    db: &PgPool,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<()> {
    let mut tx = db.begin().await?;

    if let Some((channel_id, message_id)) =
        cache::welcome_message::get(&mut tx, guild_id, user_id).await?
    {
        channel_id.delete_message(ctx, message_id).await?;
        cache::welcome_message::delete(&mut tx, guild_id, user_id).await?;
    }

    tx.commit().await?;

    Ok(())
}

fn create_intro_embed<'a>(
    user: &User,
    intro_fields: &IntroFields,
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
    intro_fields: &IntroFields,
    message: &'b mut CreateMessage<'a>,
) -> &'b mut CreateMessage<'a> {
    message
        .content(format!("Introduction: {user}"))
        .embed(|embed| create_intro_embed(user, intro_fields, embed))
}

fn edit_intro_message<'a, 'b>(
    user: &User,
    intro_fields: &IntroFields,
    message: &'b mut EditMessage<'a>,
) -> &'b mut EditMessage<'a> {
    message.embed(|embed| create_intro_embed(user, intro_fields, embed))
}

#[tracing::instrument(skip_all)]
pub(super) async fn edit_or_send_intro_message(
    ctx: &Context,
    db: &PgPool,
    guild_id: GuildId,
    channel_id: ChannelId,
    user: &User,
    intro_fields: &IntroFields<'_>,
) -> Result<Message> {
    let mut tx = db.begin().await?;

    let message = if let Some((channel_id, message_id)) =
        cache::intro_message::get(&mut tx, guild_id, user.id).await?
    {
        channel_id
            .edit_message(ctx, message_id, |message| {
                edit_intro_message(user, intro_fields, message)
            })
            .await?
    } else {
        let message = channel_id
            .send_message(ctx, |message| {
                create_intro_message(user, intro_fields, message)
            })
            .await?;
        cache::intro_message::set(db, guild_id, user.id, channel_id, message.id).await?;
        message
    };

    tx.commit().await?;

    Ok(message)
}

pub(super) async fn get_intro_message(
    ctx: &Context,
    db: &PgPool,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<Option<Message>> {
    let mut tx = db.begin().await?;

    let message = if let Some((channel_id, message_id)) =
        cache::intro_message::get(&mut tx, guild_id, user_id).await?
    {
        match channel_id.message(ctx, message_id).await {
            Ok(message) => Some(message),
            Err(serenity::Error::Http(err)) if err.status_code() == Some(StatusCode::NOT_FOUND) => {
                cache::intro_message::delete(&mut tx, guild_id, user_id).await?;
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
