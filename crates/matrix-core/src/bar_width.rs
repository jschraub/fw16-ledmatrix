use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Width in LED columns. Odd widths have a unique center on a nine-column panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(try_from = "u8", into = "u8")]
pub struct BarWidth(u8);

impl Default for BarWidth {
    fn default() -> Self {
        Self(5)
    }
}

impl TryFrom<u8> for BarWidth {
    type Error = &'static str;
    fn try_from(n: u8) -> Result<Self, Self::Error> {
        match n {
            1 | 3 | 5 | 7 | 9 => Ok(Self(n)),
            _ => Err("bar width must be 1, 3, 5, 7, or 9 columns"),
        }
    }
}

impl From<BarWidth> for u8 {
    fn from(width: BarWidth) -> u8 {
        width.0
    }
}
