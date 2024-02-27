use std::{convert::identity, time::Duration};

use serenity::all::{ChannelId, GetMessages, Message};

use crate::{context::Context, error::Result};

async fn delete_messages(
    ctx: &impl Context,
    now: chrono::DateTime<chrono::Utc>,
    channel_id: ChannelId,
    messages: &[Message],
) -> Result<()> {
    // "You can only bulk delete messages that are under 14 days old."
    // Pad the cutoff by one minute.
    const BULK_DELETE_MAX_AGE: Duration = Duration::from_secs((14 * 24 * 60 * 60) - 60);

    let bulk_cutoff = now - BULK_DELETE_MAX_AGE;
    let bulk_cutoff_index = messages
        .binary_search_by_key(&bulk_cutoff, |msg| *msg.timestamp)
        .unwrap_or_else(identity);
    let (individually, bulk) = messages.split_at(bulk_cutoff_index);

    for msg in individually {
        channel_id.delete_message(ctx.serenity(), msg.id).await?;
    }

    if !bulk.is_empty() {
        channel_id.delete_messages(ctx.serenity(), bulk).await?;
    }

    Ok(())
}

pub async fn auto_delete(ctx: &impl Context) -> Result<()> {
    let now = chrono::Utc::now();

    for cfg in &ctx.config().auto_delete {
        let cutoff = now - cfg.after;

        loop {
            // Repeatedly get the oldest messages that have not been deleted.
            let mut batch = cfg
                .channel
                .messages(ctx.serenity(), GetMessages::new().after(0).limit(100))
                .await?;

            batch.sort_by_key(|msg| msg.timestamp);

            let cutoff_index = batch
                .binary_search_by_key(&cutoff, |msg| *msg.timestamp)
                .unwrap_or_else(identity);
            let to_delete = &batch[..cutoff_index];

            // Stop if we have run out of messages to delete.
            if to_delete.is_empty() {
                break;
            }

            let from_timestamp = *to_delete.first().unwrap().timestamp;
            let to_timestamp = *to_delete.last().unwrap().timestamp;
            tracing::info!(n = to_delete.len(), %from_timestamp, %to_timestamp, "Deleting messages");
            delete_messages(ctx, now, cfg.channel, to_delete).await?;

            // If we already received any message that is too new, then all subsequent messages will also be too new.
            if to_delete.len() < batch.len() {
                break;
            }
        }
    }

    Ok(())
}
