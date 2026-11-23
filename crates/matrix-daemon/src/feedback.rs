use matrix_client::Snapshot;
use matrix_core::{
    config::Config,
    feedback::{Feedback, Signals, Slot, charging_level, gauge},
};
use std::{collections::BTreeMap, time::Duration};

#[derive(Default)]
pub struct Runtime {
    slots: BTreeMap<String, (Feedback, Option<String>, Slot)>,
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
            // Gauge content follows the screen; the LED drive level remains
            // governed independently by the panel brightness policy.
            let reading = frame.display_sources.as_ref().and_then(|sources| {
                let key = match &config.follow_display {
                    Some(src) if sources.connected.contains(&src.connector) => &src.backlight,
                    Some(_) => return None,
                    None if sources.backlights.len() == 1 => sources.backlights.keys().next()?,
                    None => return None,
                };
                Some((key, sources.backlights.get(key).copied().flatten()?))
            });
            let source = reading.map(|(key, _)| key.clone());
            let brightness = reading.map(|(_, value)| value);
            let entry = self.slots.entry(panel.name.clone()).or_default();
            if entry.0 != *settings {
                *entry = (settings.clone(), None, Slot::default());
            }
            if entry.1 != source {
                entry.2.rebaseline_brightness(brightness);
                entry.1 = source;
            }
            let power = frame.power.as_ref();
            let signals = Signals {
                volume,
                brightness,
                automatic_dim: frame
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

#[cfg(test)]
mod tests {
    use super::*;
    use matrix_client::PanelFrame;
    use matrix_core::{display::DisplaySources, render_clock};

    #[test]
    fn manual_panel_shows_screen_changes_without_changing_its_output_level() {
        let mut config = Config::default();
        let panel = &mut config.panels[1];
        panel.feedback = Some(Feedback {
            brightness: 3,
            ..Default::default()
        });
        let mut runtime = Runtime::default();
        let ambient = render_clock(12, 34).unwrap();
        let mut tick = |seconds, source: &str, percent, automatic| {
            let mut s = crate::snapshot(seconds, 12, 34);
            s.display_sources = Some(DisplaySources {
                backlights: [(source.to_owned(), Some(percent))].into(),
                automatic_dim: Some(automatic),
                ..Default::default()
            });
            s.panels = config
                .panels
                .iter()
                .map(|p| PanelFrame {
                    name: p.name.clone(),
                    binding: None,
                    brightness: Some(32),
                    frame: ambient.clone(),
                })
                .collect();
            runtime.apply(&mut s, &config, None, Duration::from_secs(seconds.into()));
            assert_eq!(s.panels[0].frame, ambient);
            assert_eq!(s.panels[1].brightness, Some(32));
            s.panels.pop().unwrap().frame
        };
        assert_eq!(tick(0, "screen", 55, false), ambient);
        assert_eq!(tick(1, "screen", 75, false), gauge(75));
        assert_eq!(tick(3, "screen", 75, false), gauge(75));
        assert_eq!(tick(4, "screen", 75, false), ambient);
        assert_eq!(tick(5, "screen", 35, true), ambient);
        assert_eq!(tick(6, "replacement", 60, false), ambient);
    }
}
