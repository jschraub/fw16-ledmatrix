use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Available,
    Unavailable,
    Error,
    NotVerified,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Capability {
    pub name: String,
    pub state: CapabilityState,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CapabilityReport {
    pub os: String,
    pub architecture: String,
    pub desktop: String,
    pub panel_count: u32,
    pub capabilities: Vec<Capability>,
}
