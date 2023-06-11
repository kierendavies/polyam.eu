use sqlx::PgPool;

use crate::{config::Config, error::Error};

pub struct UserData {
    pub config: Config,
    pub db: PgPool,
}

pub trait Context {
    fn serenity(&self) -> &serenity::client::Context;
    fn data(&self) -> &UserData;

    fn config(&self) -> &Config {
        &self.data().config
    }

    fn db(&self) -> &PgPool {
        &self.data().db
    }
}

impl<E> Context for poise::ApplicationContext<'_, UserData, E> {
    fn serenity(&self) -> &serenity::client::Context {
        self.serenity_context
    }

    fn data(&self) -> &UserData {
        self.data
    }
}

#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Copy)]
pub struct EventContext<'a> {
    pub serenity: &'a serenity::client::Context,
    pub framework: poise::FrameworkContext<'a, UserData, Error>,
}

impl Context for EventContext<'_> {
    fn serenity(&self) -> &serenity::client::Context {
        self.serenity
    }

    fn data(&self) -> &UserData {
        self.framework.user_data
    }
}
