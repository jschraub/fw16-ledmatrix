use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Brightness {
    pub minimum: u8,
    pub maximum: u8,
    pub manual: u8,
    pub off: bool,
}
impl Default for Brightness {
    fn default() -> Self {
        Self {
            minimum: 3,
            maximum: 255,
            manual: 32,
            off: false,
        }
    }
}
impl Brightness {
    pub fn valid(&self) -> bool {
        self.minimum > 0
            && self.minimum <= self.maximum
            && self.manual >= self.minimum
            && self.manual <= self.maximum
    }
    pub fn level(&self) -> Option<u8> {
        (self.valid() && !self.off).then_some(self.manual)
    }
}
pub fn effective<'a>(
    global: &'a Brightness,
    panel: &'a crate::layout::PanelLayout,
) -> &'a Brightness {
    panel.brightness_override.as_ref().unwrap_or(global)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits_inheritance_and_intentional_off_are_distinct() {
        let global = Brightness::default();
        let mut panel = crate::layout::defaults().remove(0);
        assert_eq!(effective(&global, &panel).level(), Some(32));
        panel.brightness_override = Some(Brightness {
            minimum: 5,
            maximum: 100,
            manual: 50,
            off: true,
        });
        assert!(effective(&global, &panel).valid());
        assert_eq!(effective(&global, &panel).level(), None);
        panel.brightness_override.as_mut().unwrap().off = false;
        assert_eq!(effective(&global, &panel).level(), Some(50));
        assert!(
            !Brightness {
                minimum: 100,
                maximum: 50,
                manual: 70,
                off: false
            }
            .valid()
        );
    }
}
