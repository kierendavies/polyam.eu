pub mod bubblewrap;

use crate::UserData;
use anyhow::Error;

pub type Context<'a> = poise::Context<'a, UserData, Error>;
