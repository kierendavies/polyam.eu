use std::{collections::HashSet, fmt::Write as _};

use anyhow::Context as _;

use super::{intro, persist};
use crate::{
    context::{Context, UserData},
    error::{Error, Result},
};

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
pub async fn onboarding_migrate_welcome(
    ctx: poise::ApplicationContext<'_, UserData, Error>,
) -> Result<()> {
    let content = "Ready to write your new introduction? Hit this button!";
    let guild_id = ctx.guild_id().context("Context has no guild_id")?;
    let config = ctx.config().guild(guild_id)?;

    let channel_id = config.old_members_quarantine_channel;

    let message = channel_id
        .send_message(ctx.serenity(), |message| {
            message.content(content).components(|components| {
                components.create_action_row(|row| {
                    row.create_button(|button| {
                        *button = intro::create_button();
                        button
                    })
                })
            })
        })
        .await?;

    let message_link = message.link_ensured(ctx.serenity()).await;
    ctx.say(message_link).await?;

    Ok(())
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
pub async fn onboarding_migrate(
    ctx: poise::ApplicationContext<'_, UserData, Error>,
    limit: u32,
) -> Result<()> {
    ctx.defer().await?;

    let guild = ctx.guild().context("Context has no guild")?;
    let config = ctx.config().guild(guild.id)?;
    let role_id = config.old_members_quarantine_role;

    // No need to worry about pagination, because we have <1000 members.
    let members = guild.members(ctx.serenity(), None, None).await?;
    let n_members = members.len();

    let intro_user_ids = persist::intro_message::get_all(ctx.db(), guild.id)
        .await?
        .iter()
        .map(|(user_id, _, _)| *user_id)
        .collect::<HashSet<_>>();

    let mut to_quarantine = members;
    to_quarantine.retain(|member| {
        !member.user.bot
            && !intro_user_ids.contains(&member.user.id)
            && !member.roles.contains(&role_id)
    });
    let n_elegible = to_quarantine.len();
    to_quarantine.truncate(limit as usize);
    let n_to_quarantine = to_quarantine.len();

    ctx.say(format!(
        "Total members: {n_members}\n\
            Elegible for quarantine: {n_elegible}\n\
            To be quarantined now: {n_to_quarantine}",
    ))
    .await?
    .into_message()
    .await?;

    for chunk in to_quarantine.chunks_mut(10) {
        let mut text = String::from("Quarantining:\n");
        for member in &*chunk {
            writeln!(text, "{member}")?;
        }
        ctx.say(text).await?;

        for member in chunk {
            member.add_role(ctx.serenity(), role_id).await?;
        }
    }

    ctx.say("Done").await?;

    Ok(())
}
