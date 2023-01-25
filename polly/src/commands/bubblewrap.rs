use crate::commands::CommandContext;
use crate::error::Result;
use once_cell::sync::Lazy;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;

const BUBBLES: [(&str, u32); 3] = [("🔵", 240), ("💥", 10), ("🐱", 1)];

static DISTRIBUTION: Lazy<WeightedIndex<u32>> =
    Lazy::new(|| WeightedIndex::new(BUBBLES.map(|b| b.1)).unwrap());

const SIZE: u32 = 5;

/// Get some bubble wrap to pop
#[poise::command(slash_command)]
#[tracing::instrument(skip(ctx))]
pub async fn bubblewrap(ctx: CommandContext<'_>) -> Result<()> {
    let mut text = String::new();
    {
        let mut rng = rand::thread_rng();
        for _ in 0..SIZE {
            for _ in 0..SIZE {
                let bubble = BUBBLES[DISTRIBUTION.sample(&mut rng)].0;
                text.push_str(&format!("||{bubble}||"));
            }
            text.push('\n');
        }
    }

    ctx.say(text).await?;
    Ok(())
}
