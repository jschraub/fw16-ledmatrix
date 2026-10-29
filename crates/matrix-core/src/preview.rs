use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PreviewStatus {
    Ended,
    Cancelled,
    Busy,
    Invalid,
    NotOwner,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct PreviewReply {
    pub status: PreviewStatus,
}
