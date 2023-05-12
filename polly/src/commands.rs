pub mod bubblewrap;

use crate::{error::Error, UserData};

pub type CommandContext<'a> = poise::Context<'a, UserData, Error>;
