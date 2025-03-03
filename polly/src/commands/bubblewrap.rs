use once_cell::sync::Lazy;
use rand::{distr::weighted::WeightedIndex, prelude::Distribution};

use crate::{error::Result, PoiseApplicationContext};

const BUBBLES: [(&str, u32); 3] = [("🔵", 240), ("💥", 10), ("🐱", 1)];

static DISTRIBUTION: Lazy<WeightedIndex<u32>> =
    Lazy::new(|| WeightedIndex::new(BUBBLES.map(|b| b.1)).unwrap());

const SIZE: u32 = 5;

/// Get some bubble wrap to pop
#[poise::command(slash_command)]
#[tracing::instrument(skip(ctx))]
pub async fn bubblewrap(ctx: PoiseApplicationContext<'_>) -> Result<()> {
    let mut text = String::new();
    {
        let mut rng = rand::rng();
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
