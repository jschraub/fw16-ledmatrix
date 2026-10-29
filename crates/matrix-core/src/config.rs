use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u16,
    pub clock_intensity: u8,
    #[serde(default)]
    pub brightness: crate::brightness::Brightness,
    #[serde(default = "crate::layout::defaults")]
    pub panels: Vec<crate::layout::PanelLayout>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 2,
            clock_intensity: 180,
            brightness: Default::default(),
            panels: crate::layout::defaults(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}
impl Config {
    pub fn validate(&self) -> Vec<FieldError> {
        let mut errors = vec![];
        if !self.brightness.valid() {
            errors.push(FieldError { field: "brightness".into(), message: "Require 1 <= minimum <= manual <= maximum <= 255. Use explicit off, not zero brightness.".into() });
        }
        if self.schema_version != 2 {
            errors.push(FieldError { field: "schema_version".into(), message: "Unsupported configuration schema; preserve the file and use a matching version.".into() });
        }
        if self.clock_intensity == 0 {
            errors.push(FieldError {
                field: "clock_intensity".into(),
                message: "Clock intensity must be between 1 and 255.".into(),
            });
        }
        use crate::layout::{Presentation, Source};
        if !(1..=2).contains(&self.panels.len()) {
            errors.push(FieldError {
                field: "panels".into(),
                message: "Configure one or two panels.".into(),
            });
        }
        let mut names = std::collections::BTreeSet::new();
        let mut bindings = std::collections::BTreeSet::new();
        for (p, panel) in self.panels.iter().enumerate() {
            if panel
                .brightness_override
                .as_ref()
                .is_some_and(|brightness| !brightness.valid())
            {
                errors.push(FieldError {
                    field: format!("panels.{p}.brightness_override"),
                    message: "Invalid panel brightness limits or manual level.".into(),
                });
            }
            if let Some(binding) = &panel.binding
                && (binding.is_empty() || binding.len() > 1024 || !bindings.insert(binding.clone()))
            {
                errors.push(FieldError {
                    field: format!("panels.{p}.binding"),
                    message: "Each physical location can belong to only one logical panel.".into(),
                });
            }
            if panel.name.is_empty() || panel.name.len() > 32 || !names.insert(panel.name.clone()) {
                errors.push(FieldError {
                    field: format!("panels.{p}.name"),
                    message: "Panel names must be unique and 1–32 bytes.".into(),
                });
            }
            if panel.slots.len() != panel.template.presentations().len() {
                errors.push(FieldError {
                    field: format!("panels.{p}.slots"),
                    message: "Slot count must match the guided template.".into(),
                });
            }
            for (s, widget) in panel.slots.iter().enumerate() {
                if let Some(widget) = widget
                    && (panel.template.presentations().get(s) != Some(&widget.presentation)
                        || widget.intensity == Some(0)
                        || widget
                            .session_id
                            .as_ref()
                            .is_some_and(|id| !crate::assistant::safe_id(id))
                        || widget
                            .warning_threshold
                            .is_some_and(|threshold| threshold > 100)
                        || (widget.source == Source::Clock)
                            != (widget.presentation == Presentation::Clock))
                {
                    errors.push(FieldError {
                        field: format!("panels.{p}.slots.{s}"),
                        message: "Incompatible widget/slot, source or intensity.".into(),
                    });
                }
            }
        }
        errors
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
pub struct SavedConfig {
    pub revision: u32,
    pub config: Config,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ConfigStatus {
    Read,
    Valid,
    Applied,
    Invalid,
    Conflict,
    SaveFailed,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ConfigReply {
    pub status: ConfigStatus,
    pub saved: SavedConfig,
    pub errors: Vec<FieldError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_panel_count_names_geometry_sources_and_intensities() {
        let mut config = Config::default();
        assert!(config.validate().is_empty());
        config.panels[1].name = config.panels[0].name.clone();
        assert!(!config.validate().is_empty());
        config = Config::default();
        config.panels[0].binding = Some("same-location".into());
        config.panels[1].binding = Some("same-location".into());
        assert!(!config.validate().is_empty());
        config = Config::default();
        config.panels[0].slots.push(None);
        assert!(!config.validate().is_empty());
        config = Config::default();
        config.panels[0].slots[0].as_mut().unwrap().source = crate::layout::Source::Battery;
        assert!(!config.validate().is_empty());
        config = Config::default();
        config.panels[1].slots[1] = config.panels[1].slots[2].clone();
        assert!(config.validate().is_empty());
        config.panels[1].slots[1].as_mut().unwrap().intensity = Some(0);
        assert!(!config.validate().is_empty());
    }
}
