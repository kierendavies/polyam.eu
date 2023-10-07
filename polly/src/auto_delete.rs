use crate::{context::Context, error::Result};

pub async fn auto_delete(ctx: &impl Context) -> Result<()> {
    let now = chrono::Utc::now();

    for cfg in &ctx.config().auto_delete {
        let cutoff = now - cfg.after;

        loop {
            // Repeatedly get the oldest messages.
            let messages = cfg
                .channel
                .messages(ctx.serenity(), |b| b.after(0).limit(100))
                .await?;

            let n_messages = messages.len();

            let delete_ids = messages
                .into_iter()
                .filter(|msg| *msg.timestamp < cutoff)
                .map(|msg| msg.id)
                .collect::<Vec<_>>();

            let n_delete = delete_ids.len();

            // Stop if there's nothing to delete.
            if n_delete == 0 {
                break;
            }

            cfg.channel
                .delete_messages(ctx.serenity(), &delete_ids)
                .await?;

            // If we already found a message that is too new, then all subsequent messages will also be too new.
            if n_delete < n_messages {
                break;
            }
        }
    }

    Ok(())
}
