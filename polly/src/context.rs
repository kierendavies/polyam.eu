use sqlx::PgPool;

use crate::{config::Config, Data, PoiseApplicationContext, PoiseContext, PoiseFrameworkContext};

pub trait Context {
    fn serenity(&self) -> &serenity::client::Context;
    fn data(&self) -> &Data;

    fn config(&self) -> &Config {
        &self.data().config
    }

    fn db(&self) -> &PgPool {
        &self.data().db
    }
}

impl Context for PoiseContext<'_> {
    fn serenity(&self) -> &serenity::client::Context {
        match self {
            poise::Context::Application(ctx) => ctx.serenity_context,
            poise::Context::Prefix(ctx) => ctx.serenity_context,
        }
    }

    fn data(&self) -> &Data {
        match self {
            poise::Context::Application(ctx) => ctx.data,
            poise::Context::Prefix(ctx) => ctx.data,
        }
    }
}

impl Context for PoiseApplicationContext<'_> {
    fn serenity(&self) -> &serenity::client::Context {
        self.serenity_context
    }

    fn data(&self) -> &Data {
        self.data
    }
}

#[derive(Clone, Copy)]
pub struct Event<'a> {
    pub serenity: &'a serenity::client::Context,
    pub framework: PoiseFrameworkContext<'a>,
}

impl Context for Event<'_> {
    fn serenity(&self) -> &serenity::client::Context {
        self.serenity
    }

    fn data(&self) -> &Data {
        self.framework.user_data
    }
}

#[derive(Clone)]
pub struct Owned {
    pub serenity: serenity::client::Context,
    pub data: Data,
}

impl Context for Owned {
    fn serenity(&self) -> &serenity::client::Context {
        &self.serenity
    }

    fn data(&self) -> &Data {
        &self.data
    }
}
