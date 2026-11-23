use crate::brightness::Brightness;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Pulse {
    Subtle,
    #[default]
    Default,
    Extreme,
    Custom {
        spread: u8,
        cycle_seconds: u8,
    },
}

impl Pulse {
    pub fn valid(self) -> bool {
        match self {
            Self::Custom {
                spread,
                cycle_seconds,
            } => (1..=254).contains(&spread) && (4..=60).contains(&cycle_seconds),
            _ => true,
        }
    }

    pub fn level(self, policy: &Brightness, now: Duration) -> Option<u8> {
        let base = policy.level()?;
        let bounds = |multiplier: u8| {
            let high = base.saturating_mul(multiplier).min(policy.maximum);
            (high.div_ceil(multiplier).max(policy.minimum), high, 8)
        };
        let spread_bounds = |spread: u8, seconds: u8| {
            let span = spread.min(policy.maximum - policy.minimum);
            let low = base.min(policy.maximum - span);
            (low, low + span, seconds)
        };
        let (low, high, seconds) = match self {
            Self::Subtle => spread_bounds((base / 4).max(1), 8),
            Self::Default => bounds(2),
            Self::Extreme => bounds(4),
            Self::Custom {
                spread,
                cycle_seconds,
            } if self.valid() => spread_bounds(spread, cycle_seconds),
            Self::Custom { .. } => return Some(base),
        };
        let period = u32::from(seconds) * 1000;
        let phase = (now.as_millis() % u128::from(period)) as u32;
        Some(low + (u32::from(high - low) * phase.min(period - phase) / (period / 2)) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets_and_custom_are_bounded_and_use_full_cycle_seconds() {
        let mut p = Brightness::default();
        let custom = Pulse::Custom {
            spread: 48,
            cycle_seconds: 12,
        };
        for (mode, seconds, peak) in [
            (Pulse::Subtle, 8, 40),
            (Pulse::Default, 8, 64),
            (Pulse::Extreme, 8, 128),
            (custom, 12, 80),
        ] {
            assert_eq!(
                [0, seconds / 2, seconds].map(|s| mode.level(&p, Duration::from_secs(s))),
                [Some(32), Some(peak), Some(32)]
            );
        }
        p.maximum = 51;
        p.minimum = 31;
        assert_eq!(custom.level(&p, Duration::ZERO), Some(31));
        assert_eq!(custom.level(&p, Duration::from_secs(6)), Some(51));
        p.off = true;
        assert_eq!(custom.level(&p, Duration::from_secs(6)), None);
    }
    #[test]
    fn missing_profile_uses_default_and_invalid_custom_is_rejected() {
        use crate::feedback::Feedback;
        let old: Feedback = serde_json::from_str(r#"{"charging":true}"#).unwrap();
        assert_eq!(old.pulse.unwrap_or_default(), Pulse::Default);
        for (spread, cycle_seconds) in [(0, 8), (255, 8), (32, 3), (32, 61)] {
            assert!(
                !Feedback {
                    pulse: Some(Pulse::Custom {
                        spread,
                        cycle_seconds
                    }),
                    ..old.clone()
                }
                .valid()
            );
        }
    }
}
