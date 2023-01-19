pub mod bubblewrap;

use crate::Error;
use crate::UserData;

pub type Context<'a> = poise::Context<'a, UserData, Error>;
