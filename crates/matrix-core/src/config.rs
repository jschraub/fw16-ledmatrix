use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u16,
    pub clock_intensity: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            clock_intensity: 180,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}
impl Config {
    pub fn validate(&self) -> Vec<FieldError> {
        let mut errors = vec![];
        if self.schema_version != 1 {
            errors.push(FieldError { field: "schema_version".into(), message: "Unsupported configuration schema; preserve the file and use a matching version.".into() });
        }
        if self.clock_intensity == 0 {
            errors.push(FieldError {
                field: "clock_intensity".into(),
                message: "Clock intensity must be between 1 and 255.".into(),
            });
        }
        errors
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
pub struct SavedConfig {
    pub revision: u32,
    pub config: Config,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConfigStatus {
    Read,
    Valid,
    Applied,
    Invalid,
    Conflict,
    SaveFailed,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConfigReply {
    pub status: ConfigStatus,
    pub saved: SavedConfig,
    pub errors: Vec<FieldError>,
}
