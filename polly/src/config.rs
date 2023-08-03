use std::collections::BTreeMap;

use anyhow::anyhow;
use poise::serenity_prelude::{ChannelId, GuildId, RoleId};
use serde::{de, Deserialize as _};
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub errors_channel: ChannelId,
    #[serde(deserialize_with = "deserialize_snowflake_map", flatten)]
    pub guilds: BTreeMap<GuildId, GuildConfig>,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct GuildConfig {
    pub quarantine_role: RoleId,
    pub quarantine_channel: ChannelId,
    pub intros_channel: ChannelId,
}

impl Config {
    pub fn guild(&self, id: GuildId) -> crate::error::Result<&GuildConfig> {
        self.guilds
            .get(&id)
            .ok_or(anyhow!("No config for guild").into())
    }
}

// https://users.rust-lang.org/t/how-to-use-serde-to-deserialize-toml-key-as-u32/33231/3
fn deserialize_snowflake_map<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: de::Deserializer<'de>,
    K: From<u64> + Ord,
    V: de::Deserialize<'de>,
{
    let str_map = BTreeMap::<String, V>::deserialize(deserializer)?;

    let parsed_map = str_map
        .into_iter()
        .map(|(str_key, value)| match str_key.parse() {
            Ok(int_key) => Ok((K::from(int_key), value)),
            Err(_) => Err(de::Error::invalid_value(
                de::Unexpected::Str(&str_key),
                &"snowflake",
            )),
        })
        .collect::<Result<_, _>>()?;

    Ok(parsed_map)
}
