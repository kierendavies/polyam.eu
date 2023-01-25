pub mod bubblewrap;

use crate::error::Error;
use crate::UserData;

pub type CommandContext<'a> = poise::Context<'a, UserData, Error>;
