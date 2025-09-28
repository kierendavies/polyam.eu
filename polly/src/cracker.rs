use std::sync::LazyLock;

use anyhow::Context as _;
use rand::seq::IndexedRandom;
use regex::Regex;
use serde::Deserialize;
use serenity::all::FullEvent;

use crate::{HTTP_CLIENT, context::Context, error::Result};

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(want a|wanna) cracker\b").unwrap());

#[derive(Debug, Deserialize)]
struct TenorSearchResults {
    results: Vec<TenorSearchResult>,
}

#[derive(Debug, Deserialize)]
struct TenorSearchResult {
    url: String,
}

#[tracing::instrument(skip_all)]
pub async fn handle_event(ctx: &impl Context, event: &FullEvent) -> Result<()> {
    let FullEvent::Message {
        new_message: message,
    } = event
    else {
        return Ok(());
    };

    if !message.mentions_me(ctx.serenity()).await? {
        return Ok(());
    }

    if !RE.is_match(&message.content) {
        return Ok(());
    }

    let tenor_search_results = HTTP_CLIENT
        .get("https://tenor.googleapis.com/v2/search")
        .query(&[
            ("key", ctx.data().tenor_api_key.as_str()),
            ("q", "polly want a cracker"),
            ("limit", "5"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TenorSearchResults>()
        .await?;

    let tenor_gif_url = tenor_search_results
        .results
        .choose(&mut rand::rng())
        .context("No Tenor search results")?
        .url
        .as_str();

    message.reply(ctx.serenity(), tenor_gif_url).await?;

    Ok(())
}
