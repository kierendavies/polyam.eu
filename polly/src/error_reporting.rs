use std::fmt::Write;

use poise::serenity_prelude::Message;
use serenity::{constants::MESSAGE_CODE_LIMIT, prelude::Mentionable};

use crate::{
    context::Context,
    error::{bail, is_http_not_found, Error, Result},
    PoiseApplicationContext,
    PoiseContext,
    PoiseFrameworkContext,
    PoiseFrameworkError,
};

async fn get_response(ctx: &PoiseContext<'_>) -> Result<Option<Message>> {
    let resp = match ctx {
        poise::Context::Application(PoiseApplicationContext {
            interaction:
                poise::ApplicationCommandOrAutocompleteInteraction::ApplicationCommand(interaction),
            ..
        }) => interaction
            .get_interaction_response(ctx.serenity_context())
            .await
            .map(Some)
            .or_else(|err| {
                if is_http_not_found(&err) {
                    Ok(None)
                } else {
                    Err(err)
                }
            })?,

        _ => None,
    };

    Ok(resp)
}

async fn write_command_info(
    w: &mut impl Write,
    ctx: &PoiseContext<'_>,
) -> crate::error::Result<()> {
    write!(w, "{} in ", ctx.author().mention())?;

    if let Some(guild) = ctx.partial_guild().await {
        write!(w, "`{}` ({})", guild.name, guild.id,)?;
    } else {
        write!(w, "DM")?;
    }

    write!(w, " {}", ctx.channel_id().mention())?;

    if let Some(resp) = get_response(ctx).await? {
        write!(w, " {}", resp.link_ensured(ctx).await)?;
    }

    writeln!(w)?;

    writeln!(w, "`{}`", ctx.invocation_string())?;

    Ok(())
}

fn write_code_block_truncated(w: &mut impl Write, limit: usize, text: &str) -> Result<()> {
    // Discord measures length in Unicode codepoints.
    // The padding has no multi-byte chars, so `.len()` is the same as `.chars().count()`.
    const PADDING_LEN: usize = "```\n\n```\n(999999 bytes truncated)\n".len();

    let (shown, hidden) = text.char_indices().nth(limit - PADDING_LEN).map_or(
        // If the limit is past the end, show all text.
        (text, ""),
        // Otherwise try to split just before a line break.
        |(limit_index, first_hidden)| {
            let split_index = if first_hidden == '\n' {
                // We're already at a line break.
                limit_index
            } else {
                // Search for the last line break.
                // If there isn't one, we have to split somewhere in the line.
                text[..limit_index].rfind('\n').unwrap_or(limit_index)
            };

            text.split_at(split_index)
        },
    );

    writeln!(w, "```\n{}\n```", shown.trim_end())?;

    // Report how much was truncated, unless it was only whitespace.
    if !hidden.trim().is_empty() {
        writeln!(w, "({} bytes truncated)", hidden.len())?;
    }

    Ok(())
}

async fn report_event_handler_error(
    error: Error,
    serenity_context: &serenity::client::Context,
    event: &poise::Event<'_>,
    framework_context: PoiseFrameworkContext<'_>,
) -> Result<()> {
    tracing::error!(?event, ?error, "Event handler error");

    let mut text = String::new();
    writeln!(text, "**Event handler error**")?;

    let event_text_limit = (MESSAGE_CODE_LIMIT - text.chars().count()) / 2;
    let event_text = format!("{event:?}");
    write_code_block_truncated(&mut text, event_text_limit, &event_text)?;

    let error_text_limit = MESSAGE_CODE_LIMIT - text.chars().count();
    let error_text = format!("{error:?}");
    write_code_block_truncated(&mut text, error_text_limit, &error_text)?;

    let channel = framework_context.user_data.config.errors_channel;
    channel.say(serenity_context, text).await?;

    Ok(())
}

async fn report_command_error(error: Error, ctx: PoiseContext<'_>) -> Result<()> {
    tracing::error!(?error, "Command error");

    let mut text = String::new();
    writeln!(text, "**Command error**")?;

    write_command_info(&mut text, &ctx).await?;

    let error_text_limit = MESSAGE_CODE_LIMIT - text.chars().count();
    write_code_block_truncated(&mut text, error_text_limit, &format!("{error:?}"))?;

    let channel = ctx.config().errors_channel;
    channel.say(ctx, text).await?;

    Ok(())
}

async fn report_command_panic(payload: Option<String>, ctx: PoiseContext<'_>) -> Result<()> {
    tracing::error!(payload, "Command panic");

    let mut text = String::new();
    writeln!(text, "**Command panic**")?;

    write_command_info(&mut text, &ctx).await?;

    if let Some(payload) = payload {
        let payload_limit = MESSAGE_CODE_LIMIT - text.chars().count();
        write_code_block_truncated(&mut text, payload_limit, &payload)?;
    } else {
        writeln!(text, "No payload")?;
    }

    let channel = ctx.config().errors_channel;
    channel.say(ctx, text).await?;

    Ok(())
}

pub async fn report_error(err: PoiseFrameworkError<'_>) -> Result<()> {
    match err {
        poise::FrameworkError::EventHandler {
            error,
            ctx,
            event,
            framework,
        } => report_event_handler_error(error, ctx, event, framework).await,

        poise::FrameworkError::Command { error, ctx } => report_command_error(error, ctx).await,

        poise::FrameworkError::CommandPanic { payload, ctx } => {
            report_command_panic(payload, ctx).await
        }

        _ => bail!("Reporting not supported for this error variant"),
    }
}

pub async fn report_background_task_error<C: Context>(
    task_name: &str,
    ctx: &C,
    error: Error,
) -> crate::error::Result<()> {
    tracing::error!(?error, task_name, "Background task error");

    let mut text = String::new();
    writeln!(text, "**Background task error**")?;
    writeln!(text, "`{task_name}`")?;

    let error_text_limit = MESSAGE_CODE_LIMIT - text.chars().count();
    let error_text = format!("{error:?}");
    write_code_block_truncated(&mut text, error_text_limit, &error_text)?;

    let errors_channel = ctx.config().errors_channel;
    errors_channel.say(ctx.serenity(), text).await?;

    Ok(())
}
