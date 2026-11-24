use crate::assistant::Provider;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The only account-usage data allowed into IPC and diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Usage {
    pub provider: Provider,
    pub status: UsageStatus,
    pub five_hour: Option<Window>,
    pub weekly: Option<Window>,
    pub expires_at: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus {
    Disabled,
    UnsupportedLogin,
    Unauthorized,
    AccountChanged,
    Fresh,
    NoWindows,
    SchemaError,
    RequestFailed,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Window {
    pub used_percent: u8,
    pub resets_at: Option<u32>,
}

impl Usage {
    pub fn empty(provider: Provider, status: UsageStatus) -> Self {
        Self {
            provider,
            status,
            five_hour: None,
            weekly: None,
            expires_at: None,
        }
    }

    pub fn at(mut self, now: u32) -> Self {
        if self.expires_at.is_some_and(|end| now >= end) {
            return Self::empty(self.provider, UsageStatus::Expired);
        }
        for slot in [&mut self.five_hour, &mut self.weekly] {
            if slot.is_some_and(|w| w.resets_at.is_some_and(|end| now >= end)) {
                *slot = None;
            }
        }
        if self.five_hour.is_none() && self.weekly.is_none() && self.status == UsageStatus::Fresh {
            self.status = UsageStatus::NoWindows;
        }
        self
    }
}
