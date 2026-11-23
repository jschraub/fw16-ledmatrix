use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DisplaySelection {
    pub connector: String,
    pub backlight: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub curve: Option<Curve>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Linear,
    Quadratic,
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
    // Only connectors with an explicit non-On power reading. Missing is not off.
    #[serde(default)]
    pub powered_off: Vec<String>,
    #[serde(default)]
    pub power_sources: Vec<String>,
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
            let span = u32::from(policy.maximum - policy.minimum);
            let p = u32::from(*percent);
            let (weight, scale) = match source.curve.unwrap_or(Curve::Linear) {
                Curve::Linear => (p, 100),
                Curve::Quadratic => (p * p, 10000),
            };
            policy.manual = policy.minimum + ((span * weight + scale / 2) / scale) as u8;
        }
        if config
            .sleep_with_display
            .as_ref()
            .is_some_and(|id| self.powered_off.contains(id))
        {
            config.brightness.off = true;
            for policy in config
                .panels
                .iter_mut()
                .filter_map(|p| p.brightness_override.as_mut())
            {
                policy.off = true;
            }
        }
        config
    }
}
