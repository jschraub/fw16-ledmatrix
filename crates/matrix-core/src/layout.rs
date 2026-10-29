use crate::{DATA, FONT, Frame, HEIGHT, WIDTH};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    ClockGauge,
    NumberTwoGauges,
    FullGauge,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    Clock,
    Number,
    Gauge,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Clock,
    Battery,
    Context,
    Activity,
    FiveHour,
    Weekly,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Widget {
    pub source: Source,
    pub presentation: Presentation,
    pub intensity: Option<u8>,
    pub reversed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PanelLayout {
    pub name: String,
    pub template: Template,
    pub slots: Vec<Option<Widget>>,
}

impl Template {
    pub fn presentations(self) -> &'static [Presentation] {
        match self {
            Self::ClockGauge => &[Presentation::Clock, Presentation::Gauge],
            Self::NumberTwoGauges => &[
                Presentation::Number,
                Presentation::Gauge,
                Presentation::Gauge,
            ],
            Self::FullGauge => &[Presentation::Gauge],
        }
    }
}
pub fn default_slots(template: Template) -> Vec<Option<Widget>> {
    template
        .presentations()
        .iter()
        .enumerate()
        .map(|(index, p)| {
            Some(Widget {
                source: match (template, index) {
                    (Template::ClockGauge, 0) => Source::Clock,
                    (Template::ClockGauge | Template::FullGauge, _) => Source::Battery,
                    (Template::NumberTwoGauges, 0) => Source::Context,
                    (Template::NumberTwoGauges, 1) => Source::FiveHour,
                    _ => Source::Weekly,
                },
                presentation: *p,
                intensity: None,
                reversed: false,
            })
        })
        .collect()
}
pub fn defaults() -> Vec<PanelLayout> {
    vec![
        ("left", Template::ClockGauge),
        ("right", Template::NumberTwoGauges),
    ]
    .into_iter()
    .map(|(name, template)| PanelLayout {
        name: name.into(),
        template,
        slots: default_slots(template),
    })
    .collect()
}

#[derive(Clone)]
pub struct Readings {
    pub default_clock_intensity: u8,
    pub hour: u8,
    pub minute: u8,
    pub battery: Option<f64>,
    pub context: Option<f64>,
    pub activity: Option<f64>,
    pub five_hour: Option<f64>,
    pub weekly: Option<f64>,
}
impl Default for Readings {
    fn default() -> Self {
        Self {
            default_clock_intensity: DATA,
            hour: 0,
            minute: 0,
            battery: None,
            context: None,
            activity: None,
            five_hour: None,
            weekly: None,
        }
    }
}
impl Readings {
    pub fn sample(hour: u8, minute: u8) -> Self {
        Self {
            hour,
            minute,
            battery: Some(74.0),
            context: Some(41.0),
            activity: Some(100.0),
            five_hour: Some(62.0),
            weekly: Some(18.0),
            ..Self::default()
        }
    }
    fn value(&self, source: Source) -> Option<f64> {
        match source {
            Source::Clock => None,
            Source::Battery => self.battery,
            Source::Context => self.context,
            Source::Activity => self.activity,
            Source::FiveHour => self.five_hour,
            Source::Weekly => self.weekly,
        }
        .filter(|v| v.is_finite())
    }
}
fn number(frame: &mut Frame, y: usize, value: i32, intensity: u8) {
    for (x, digit) in [(1, value / 10), (5, value % 10)] {
        let glyph = if (0..=99).contains(&value) {
            FONT[digit as usize]
        } else {
            ["#.#", "#.#", ".#.", "#.#", "#.#"]
        };
        for (dy, row) in glyph.iter().enumerate() {
            for (dx, cell) in row.bytes().enumerate() {
                if cell == b'#' {
                    frame.pixels[(y + dy) * WIDTH + x + dx] = intensity;
                }
            }
        }
    }
}
pub fn render(panel: &PanelLayout, readings: &Readings) -> Frame {
    let mut frame = Frame {
        width: WIDTH as u8,
        height: HEIGHT as u8,
        pixels: vec![0; WIDTH * HEIGHT],
    };
    for (index, slot) in panel.slots.iter().enumerate() {
        let Some(widget) = slot else {
            continue;
        };
        let intensity = widget
            .intensity
            .unwrap_or(if widget.source == Source::Clock {
                readings.default_clock_intensity
            } else {
                DATA
            });
        if panel.template.presentations().get(index) != Some(&widget.presentation) {
            continue;
        }
        if widget.presentation == Presentation::Clock {
            number(&mut frame, 1, i32::from(readings.hour), intensity);
            number(&mut frame, 7, i32::from(readings.minute), intensity);
            continue;
        }
        let Some(value) = readings.value(widget.source) else {
            continue;
        };
        if widget.presentation == Presentation::Number {
            number(&mut frame, 1, value as i32, intensity);
            continue;
        }
        let (y0, y1, x0, x1) = match panel.template {
            Template::ClockGauge => (13, 33, 3, 5),
            Template::NumberTwoGauges if index == 1 => (7, 33, 1, 2),
            Template::NumberTwoGauges => (7, 33, 6, 7),
            Template::FullGauge => (0, 33, 3, 5),
        };
        let exact = (value / 100.0).clamp(0.0, 1.0) * (y1 - y0 + 1) as f64;
        for offset in 0..=(y1 - y0) {
            let intensity = (f64::from(intensity) * (exact - offset as f64).clamp(0.0, 1.0)) as u8;
            let y = if widget.reversed {
                y0 + offset
            } else {
                y1 - offset
            };
            for x in x0..=x1 {
                frame.pixels[y * WIDTH + x] = intensity;
            }
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_geometry_matches_poc_and_missing_values_never_reflow() {
        let panels = defaults();
        let live = Readings {
            hour: 12,
            minute: 34,
            ..Readings::default()
        };
        assert_eq!(
            render(&panels[0], &live),
            crate::render_clock(12, 34).unwrap()
        );
        let sample = render(&panels[0], &Readings::sample(12, 34));
        assert_eq!(
            &sample.pixels[..13 * WIDTH],
            &render(&panels[0], &live).pixels[..13 * WIDTH]
        );
        for panel in &panels {
            assert!(render(panel, &live).is_valid());
            assert!(render(panel, &Readings::sample(23, 59)).is_valid());
        }
    }
    #[test]
    fn full_gauge_direction_empty_slots_and_truthful_overflow() {
        let mut panel = PanelLayout {
            name: "test".into(),
            template: Template::FullGauge,
            slots: default_slots(Template::FullGauge),
        };
        let readings = Readings {
            battery: Some(10.0),
            ..Readings::default()
        };
        let normal = render(&panel, &readings);
        assert!(normal.pixels[33 * WIDTH + 3] > 0);
        assert_eq!(normal.pixels[3], 0);
        panel.slots[0].as_mut().unwrap().reversed = true;
        assert!(render(&panel, &readings).pixels[3] > 0);
        panel.slots[0] = None;
        assert!(render(&panel, &readings).pixels.iter().all(|p| *p == 0));
        let panel = defaults().remove(1);
        let full = render(
            &panel,
            &Readings {
                context: Some(100.0),
                ..Readings::default()
            },
        );
        assert_eq!(
            &full.pixels[WIDTH..2 * WIDTH],
            &[0, 180, 0, 180, 0, 180, 0, 180, 0]
        );
    }
}
