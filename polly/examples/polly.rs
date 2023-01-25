#![warn(clippy::pedantic)]

use polly::bot_framework;
use polly::config::Config;
use std::fs;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::metadata::LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("polly=trace".parse().unwrap())
                .add_directive("serenity::gateway::shard=warn".parse().unwrap()),
        )
        .finish()
        .with(tracing_error::ErrorLayer::default())
        .init();

    let token = std::env::var("DISCORD_TOKEN").expect("Error reading DISCORD_TOKEN");

    let config: Config =
        toml::from_str(&fs::read_to_string("static/polly.toml").expect("Error reading config"))
            .expect("Error parsing config");

    let framework = bot_framework(token, config)
        .await
        .expect("Error creating framework");

    framework.start().await.unwrap();
}
