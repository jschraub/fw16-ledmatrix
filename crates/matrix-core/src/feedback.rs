//! One transient gauge per logical panel. Time and source readings are inputs.
use crate::{Frame, HEIGHT, WIDTH, bar_width::BarWidth, pulse::Pulse};
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
    pub volume_width: BarWidth,
    pub brightness_width: BarWidth,
    pub ac_width: BarWidth,
    pub charging: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pulse: Option<Pulse>,
}
impl Feedback {
    pub fn valid(&self) -> bool {
        [self.volume, self.brightness, self.ac]
            .iter()
            .all(|&s| s == 0 || (3..=15).contains(&s))
            && self.pulse.is_none_or(Pulse::valid)
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
    gauge: Option<((u8, BarWidth), Duration)>,
}
fn changed<T: Eq>(previous: Option<T>, next: Option<T>) -> Option<T> {
    next.filter(|v| previous.as_ref().is_some_and(|old| old != v))
}
impl Slot {
    pub fn rebaseline_brightness(&mut self, level: Option<u8>) {
        self.previous.brightness = level;
    }
    pub fn tick(
        &mut self,
        now: Duration,
        settings: &Feedback,
        next: Signals,
    ) -> Option<(u8, BarWidth)> {
        let old = std::mem::replace(&mut self.previous, next);
        // Stable tie-break for changes first observed in the same render tick.
        for (seconds, width, value) in [
            (
                settings.ac,
                settings.ac_width,
                changed(old.ac, next.ac).and(next.battery),
            ),
            (
                settings.brightness,
                settings.brightness_width,
                changed(old.brightness, next.brightness).filter(|_| !next.automatic_dim),
            ),
            (
                settings.volume,
                settings.volume_width,
                changed(old.volume, next.volume).map(|(v, mute)| if mute { 0 } else { v }),
            ),
        ] {
            if let Some(level) = value.filter(|_| seconds > 0) {
                self.gauge = Some(((level, width), now + Duration::from_secs(seconds.into())));
            }
        }
        self.gauge.filter(|(_, end)| now < *end).map(|(v, _)| v)
    }
}

pub fn gauge(level: u8, width: BarWidth) -> Frame {
    let rows = (usize::from(level.min(100)) * HEIGHT + 50) / 100;
    let mut pixels = vec![0; WIDTH * HEIGHT];
    let width = usize::from(u8::from(width));
    let left = (WIDTH - width) / 2;
    for row in pixels[(HEIGHT - rows) * WIDTH..].chunks_exact_mut(WIDTH) {
        row[left..left + width].fill(crate::DATA);
    }
    Frame {
        width: WIDTH as u8,
        height: HEIGHT as u8,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brightness::Brightness;
    #[test]
    fn bar_widths_are_centered_and_keep_the_level_from_the_bottom() {
        for width in [1u8, 3, 5, 7, 9] {
            let frame = gauge(50, width.try_into().unwrap());
            for (i, &pixel) in frame.pixels.iter().enumerate() {
                let lit = i / WIDTH >= 17 && (i % WIDTH).abs_diff(4) <= usize::from(width / 2);
                assert_eq!(pixel, if lit { crate::DATA } else { 0 });
            }
        }
        let old: Feedback = serde_json::from_str(r#"{"volume":3}"#).unwrap();
        assert_eq!(
            [old.volume_width, old.brightness_width, old.ac_width].map(u8::from),
            [5; 3]
        );
        for n in [0, 2, 4, 6, 8, 10] {
            assert!(serde_json::from_value::<Feedback>(serde_json::json!({"ac_width":n})).is_err());
        }
    }
    #[test]
    fn edges_replace_and_extend_without_startup_or_recovery_events() {
        let settings = Feedback {
            volume: 3,
            ac: 4,
            brightness: 5,
            charging: false,
            pulse: None,
            volume_width: 3.try_into().unwrap(),
            ac_width: 7.try_into().unwrap(),
            ..Default::default()
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
        assert_eq!(
            slot.tick(at(1), &settings, s),
            Some((82, settings.ac_width))
        );
        s.volume = Some((44, false));
        assert_eq!(
            slot.tick(at(4), &settings, s),
            Some((44, settings.volume_width))
        );
        s.automatic_dim = true;
        s.brightness = Some(10);
        assert_eq!(
            slot.tick(at(6), &settings, s),
            Some((44, settings.volume_width))
        );
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
            let low = Pulse::Default.level(&p, Duration::ZERO).unwrap();
            let high = Pulse::Default.level(&p, Duration::from_secs(4)).unwrap();
            assert!(low >= 3 && high <= 100 && high > low);
            p.off = true;
            assert_eq!(Pulse::Default.level(&p, Duration::ZERO), None);
        }
    }

    #[test]
    fn pulse_doubles_normal_brightness_and_returns_to_base() {
        let policy = Brightness::default();
        let levels = [0, 2, 4, 6, 8].map(|s| Pulse::Default.level(&policy, Duration::from_secs(s)));
        assert_eq!(levels, [Some(32), Some(48), Some(64), Some(48), Some(32)]);
    }
}
