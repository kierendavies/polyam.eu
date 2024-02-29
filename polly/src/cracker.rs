use anyhow::Context as _;
use once_cell::sync::Lazy;
use rand::seq::SliceRandom;
use regex::Regex;
use serde::Deserialize;
use serenity::all::FullEvent;

use crate::{context::Context, error::Result, HTTP_CLIENT};

static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(want a|wanna) cracker\b").unwrap());

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
        .choose(&mut rand::thread_rng())
        .context("No Tenor search results")?
        .url
        .as_str();

    message.reply(ctx.serenity(), tenor_gif_url).await?;

    Ok(())
}
