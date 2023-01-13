#![warn(clippy::pedantic)]

mod commands;

use crate::commands::bubblewrap::bubblewrap;
use serenity::model::gateway::GatewayIntents;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![bubblewrap()],
            ..Default::default()
        })
        .token(std::env::var("DISCORD_TOKEN").expect("Error reading DISCORD_TOKEN"))
        .intents(GatewayIntents::empty())
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                for guild in &ready.guilds {
                    poise::builtins::register_in_guild(
                        ctx,
                        &framework.options().commands,
                        guild.id,
                    )
                    .await?;
                }
                Ok(())
            })
        });

    framework.run().await.unwrap();
}
