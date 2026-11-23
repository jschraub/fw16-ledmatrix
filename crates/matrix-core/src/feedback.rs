//! One transient gauge per logical panel. Time and source readings are inputs.
use crate::{Frame, HEIGHT, WIDTH, brightness::Brightness};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, deny_unknown_fields)]
pub struct Feedback {
    // Zero disables an overlay. Enabled durations are whole seconds.
    pub volume: u8,
    pub brightness: u8,
    pub ac: u8,
    pub charging: bool,
}
impl Feedback {
    pub fn valid(&self) -> bool {
        [self.volume, self.brightness, self.ac]
            .iter()
            .all(|&s| s == 0 || (3..=15).contains(&s))
    }
}

#[derive(Clone, Copy, Default)]
pub struct Signals {
    pub volume: Option<(u8, bool)>,
    pub brightness: Option<u8>,
    pub ac: Option<bool>,
    pub battery: Option<u8>,
    pub automatic_dim: bool,
}
#[derive(Default)]
pub struct Slot {
    previous: Signals,
    gauge: Option<(u8, Duration)>,
}
fn changed<T: Eq>(previous: Option<T>, next: Option<T>) -> Option<T> {
    next.filter(|v| previous.as_ref().is_some_and(|old| old != v))
}
impl Slot {
    pub fn tick(&mut self, now: Duration, settings: &Feedback, next: Signals) -> Option<u8> {
        let old = std::mem::replace(&mut self.previous, next);
        // Stable tie-break for changes first observed in the same render tick.
        for (seconds, value) in [
            (settings.ac, changed(old.ac, next.ac).and(next.battery)),
            (
                settings.brightness,
                changed(old.brightness, next.brightness).filter(|_| !next.automatic_dim),
            ),
            (
                settings.volume,
                changed(old.volume, next.volume).map(|(v, mute)| if mute { 0 } else { v }),
            ),
        ] {
            if let Some(level) = value.filter(|_| seconds > 0) {
                self.gauge = Some((level, now + Duration::from_secs(seconds.into())));
            }
        }
        self.gauge.filter(|(_, end)| now < *end).map(|(v, _)| v)
    }
}

pub fn gauge(level: u8) -> Frame {
    let rows = (usize::from(level.min(100)) * HEIGHT + 50) / 100;
    let mut pixels = vec![0; WIDTH * HEIGHT];
    pixels[(HEIGHT - rows) * WIDTH..].fill(crate::DATA);
    Frame {
        width: WIDTH as u8,
        height: HEIGHT as u8,
        pixels,
    }
}

pub fn charging_level(policy: &Brightness, now: Duration) -> Option<u8> {
    let base = policy.level()?;
    let span = (base / 4).max(1).min(policy.maximum - policy.minimum);
    let low = base.min(policy.maximum - span);
    let phase = (now.as_millis() % 8000) as u32;
    let ramp = phase.min(8000 - phase);
    Some(low + (u32::from(span) * ramp / 4000) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edges_replace_and_extend_without_startup_or_recovery_events() {
        let settings = Feedback {
            volume: 3,
            ac: 4,
            brightness: 5,
            charging: false,
        };
        let mut slot = Slot::default();
        let mut s = Signals {
            volume: Some((20, false)),
            brightness: Some(30),
            ac: Some(false),
            battery: Some(82),
            automatic_dim: false,
        };
        let at = Duration::from_secs;
        assert_eq!(slot.tick(at(0), &settings, s), None);
        s.ac = Some(true);
        assert_eq!(slot.tick(at(1), &settings, s), Some(82));
        s.volume = Some((44, false));
        assert_eq!(slot.tick(at(4), &settings, s), Some(44));
        s.automatic_dim = true;
        s.brightness = Some(10);
        assert_eq!(slot.tick(at(6), &settings, s), Some(44));
        assert_eq!(slot.tick(at(7), &settings, s), None);
        s.volume = None;
        assert_eq!(slot.tick(at(8), &settings, s), None);
        s.volume = Some((90, false));
        assert_eq!(slot.tick(at(9), &settings, s), None);
    }
    #[test]
    fn pulse_respects_floor_ceiling_and_explicit_sleep() {
        for base in [3, 32, 100] {
            let mut p = Brightness {
                minimum: 3,
                maximum: 100,
                manual: base,
                off: false,
            };
            let low = charging_level(&p, Duration::ZERO).unwrap();
            let high = charging_level(&p, Duration::from_secs(4)).unwrap();
            assert!(low >= 3 && high <= 100 && high > low);
            p.off = true;
            assert_eq!(charging_level(&p, Duration::ZERO), None);
        }
    }
}
