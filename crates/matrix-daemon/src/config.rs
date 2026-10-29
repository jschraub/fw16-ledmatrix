use matrix_core::config::{Config, ConfigReply, ConfigStatus, SavedConfig};
use std::{
    io,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Store {
    saved: SavedConfig,
    path: Option<PathBuf>,
}
impl Store {
    pub fn memory() -> Self {
        Self {
            saved: SavedConfig::default(),
            path: None,
        }
    }
    pub fn default_path() -> io::Result<PathBuf> {
        std::env::var_os("MATRIX_CONFIG")
            .map(PathBuf::from)
            .or_else(|| dirs::config_dir().map(|p| p.join("fw16-matrix/config.json")))
            .ok_or_else(|| io::Error::other("user configuration directory unavailable"))
    }
    pub fn load(path: PathBuf) -> io::Result<Self> {
        let saved = match std::fs::read(&path) {
            Ok(bytes) => {
                if bytes.len() > 32768 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "configuration too large; preserved",
                    ));
                }
                let mut saved: SavedConfig = serde_json::from_slice(&bytes).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "configuration invalid; preserved",
                    )
                })?;
                if saved.config.schema_version == 1 {
                    saved.config.schema_version = 2;
                }
                if !saved.config.validate().is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "configuration unsupported; preserved",
                    ));
                }
                saved
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => SavedConfig::default(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            saved,
            path: Some(path),
        })
    }
    pub fn read(&self) -> SavedConfig {
        self.saved.clone()
    }
    pub fn validate(&self, config: &Config) -> ConfigReply {
        let errors = config.validate();
        ConfigReply {
            status: if errors.is_empty() {
                ConfigStatus::Valid
            } else {
                ConfigStatus::Invalid
            },
            saved: self.read(),
            errors,
        }
    }
    pub fn apply(&mut self, expected_revision: u32, config: Config) -> ConfigReply {
        let mut reply = self.validate(&config);
        if reply.status == ConfigStatus::Invalid {
            return reply;
        }
        if expected_revision != self.saved.revision {
            reply.status = ConfigStatus::Conflict;
            return reply;
        }
        let Some(revision) = self.saved.revision.checked_add(1) else {
            reply.status = ConfigStatus::SaveFailed;
            return reply;
        };
        let next = SavedConfig { revision, config };
        if let Some(path) = &self.path
            && persist(path, &next).is_err()
        {
            reply.status = ConfigStatus::SaveFailed;
            return reply;
        }
        self.saved = next;
        ConfigReply {
            status: ConfigStatus::Applied,
            saved: self.read(),
            errors: vec![],
        }
    }
}
fn persist(path: &Path, saved: &SavedConfig) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("configuration needs parent directory"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
    }
    #[cfg(windows)]
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec_pretty(saved)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durable_apply_validates_and_reloads_without_partial_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let mut store = Store::load(path.clone()).unwrap();
        let config = Config {
            clock_intensity: 220,
            ..Config::default()
        };
        assert_eq!(store.apply(0, config.clone()).status, ConfigStatus::Applied);
        assert_eq!(Store::load(path).unwrap().read().config, config);
        assert_eq!(
            store
                .apply(
                    1,
                    Config {
                        clock_intensity: 0,
                        ..config
                    }
                )
                .status,
            ConfigStatus::Invalid
        );
        assert_eq!(store.read().revision, 1);
    }
    #[test]
    fn corrupt_and_future_files_are_preserved_and_save_failures_do_not_publish() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        std::fs::write(&path, "corrupt-private-data").unwrap();
        assert!(Store::load(path.clone()).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "corrupt-private-data"
        );
        let saved = SavedConfig {
            config: Config {
                schema_version: 999,
                ..Config::default()
            },
            ..SavedConfig::default()
        };
        std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(Store::load(path.clone()).is_err());
        std::fs::remove_file(&path).unwrap();
        let mut store = Store::load(path.clone()).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            store.apply(0, Config::default()).status,
            ConfigStatus::SaveFailed
        );
        assert_eq!(store.read().revision, 0);
    }
    #[test]
    fn schema_one_migrates_in_memory_without_silently_rewriting_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.json");
        let legacy = r#"{"revision":3,"config":{"schema_version":1,"clock_intensity":220}}"#;
        std::fs::write(&path, legacy).unwrap();
        let store = Store::load(path.clone()).unwrap();
        assert_eq!(store.read().config.schema_version, 2);
        assert_eq!(store.read().config.panels.len(), 2);
        assert_eq!(store.read().revision, 3);
        assert_eq!(std::fs::read_to_string(path).unwrap(), legacy);
    }
}
