use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Device {
    pub port: String,
    pub location: Option<String>,
    pub firmware: Option<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DeviceReply {
    pub devices: Vec<Device>,
    pub message: String,
}
