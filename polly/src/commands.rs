pub mod bubblewrap;

use anyhow::Error;

type Context<'a> = poise::Context<'a, (), Error>;
