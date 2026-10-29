use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PowerStatus {
    Available,
    Unavailable,
    Error,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PowerReading {
    pub status: PowerStatus,
    pub percent: Option<u8>,
    pub charging: Option<bool>,
    pub ac: Option<bool>,
    pub updated_at: String,
}
