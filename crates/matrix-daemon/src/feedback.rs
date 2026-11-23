use matrix_client::Snapshot;
use matrix_core::{
    config::Config,
    feedback::{Feedback, Signals, Slot, charging_level, gauge},
};
use std::{collections::BTreeMap, time::Duration};

#[derive(Default)]
pub struct Runtime {
    slots: BTreeMap<
        String,
        (
            Feedback,
            Option<matrix_core::display::DisplaySelection>,
            Slot,
        ),
    >,
    epoch: usize,
}
impl Runtime {
    pub fn apply(
        &mut self,
        frame: &mut Snapshot,
        config: &Config,
        volume: crate::audio::Volume,
        now: Duration,
    ) {
        if frame.preview
            || frame.session_locked == Some(true)
            || self.epoch != frame.suspend_epoch
            || !frame.suspend_epoch.is_multiple_of(2)
        {
            self.slots.clear();
            self.epoch = frame.suspend_epoch;
            return;
        }
        self.slots
            .retain(|name, _| config.panels.iter().any(|p| &p.name == name));
        for (panel, out) in config.panels.iter().zip(&mut frame.panels) {
            let Some(settings) = panel.feedback.as_ref().filter(|_| out.brightness.is_some())
            else {
                self.slots.remove(&panel.name);
                continue;
            };
            let follow = if panel.brightness_override.is_none() {
                config.follow_display.clone()
            } else {
                None
            };
            let entry = self.slots.entry(panel.name.clone()).or_default();
            if entry.0 != *settings || entry.1 != follow {
                *entry = (settings.clone(), follow.clone(), Slot::default());
            }
            let power = frame.power.as_ref();
            let brightness = if let Some(src) = follow {
                frame
                    .display_sources
                    .as_ref()
                    .filter(|d| d.connected.contains(&src.connector))
                    .and_then(|d| d.backlights.get(&src.backlight))
                    .copied()
                    .flatten()
            } else {
                out.brightness
                    .map(|v| ((u16::from(v) * 100 + 127) / 255) as u8)
            };
            let signals = Signals {
                volume,
                brightness,
                automatic_dim: panel.brightness_override.is_none()
                    && config.follow_display.is_some()
                    && frame
                        .display_sources
                        .as_ref()
                        .is_some_and(|d| d.automatic_dim == Some(true)),
                ac: power.and_then(|p| p.ac),
                battery: power.and_then(|p| p.percent),
            };
            if let Some(level) = entry.2.tick(now, settings, signals) {
                out.frame = gauge(level);
            }
            if settings.charging && power.and_then(|p| p.charging) == Some(true) {
                out.brightness = charging_level(
                    matrix_core::brightness::effective(&config.brightness, panel),
                    now,
                );
            }
        }
        if let Some(first) = frame.panels.first() {
            frame.frame = first.frame.clone();
        }
    }
}
