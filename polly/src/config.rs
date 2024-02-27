use std::{collections::BTreeMap, time::Duration};

use anyhow::anyhow;
use serde::{de, Deserialize as _};
use serde_derive::Deserialize;
use serenity::all::{ChannelId, GuildId, RoleId};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub errors_channel: ChannelId,
    #[serde(deserialize_with = "deserialize_snowflake_map", flatten)]
    pub guilds: BTreeMap<GuildId, GuildConfig>,
    pub auto_delete: Vec<AutoDeleteConfig>,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct GuildConfig {
    pub quarantine_role: RoleId,
    pub quarantine_channel: ChannelId,
    pub intros_channel: ChannelId,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Deserialize)]
pub struct AutoDeleteConfig {
    pub channel: ChannelId,
    #[serde(deserialize_with = "deserialize_duration")]
    pub after: Duration,
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

fn deserialize_duration<'de, D>(deserializer: D) -> Result<std::time::Duration, D::Error>
where
    D: de::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    let iso_duration = iso8601::duration(&s).map_err(de::Error::custom)?;

    Ok(iso_duration.into())
}
