use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplaySelection {
    pub connector: String,
    pub backlight: String,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.:@".contains(&c))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DisplaySources {
    pub connected: Vec<String>,
    // None means present but unreadable. Zero is a valid reading.
    pub backlights: BTreeMap<String, Option<u8>>,
}

impl DisplaySources {
    /// Transform an output copy only; manual remains the persisted fallback.
    pub fn resolve(&self, mut config: Config) -> Config {
        if let Some(source) = &config.follow_display
            && self.connected.contains(&source.connector)
            && let Some(Some(percent @ 0..=100)) = self.backlights.get(&source.backlight)
        {
            let policy = &mut config.brightness;
            let span = u16::from(policy.maximum - policy.minimum);
            policy.manual = policy.minimum + ((span * u16::from(*percent) + 50) / 100) as u8;
        }
        config
    }
}
