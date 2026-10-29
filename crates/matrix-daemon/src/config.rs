use matrix_core::config::{Config, ConfigReply, ConfigStatus, SavedConfig};
use std::{
    io,
    io::Write,
    path::{Path, PathBuf},
};

pub struct Store {
    saved: SavedConfig,
    path: Option<PathBuf>,
    preview: Option<Lease>,
    next_lease: u32,
}
struct Lease {
    owner: String,
    generation: u32,
    config: Config,
    expires: std::time::Instant,
}
impl Store {
    pub fn memory() -> Self {
        Self {
            saved: SavedConfig::default(),
            path: None,
            preview: None,
            next_lease: 0,
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
            preview: None,
            next_lease: 0,
        })
    }
    pub fn read(&self) -> SavedConfig {
        self.saved.clone()
    }
    pub fn preview_active(&self, now: std::time::Instant) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|lease| now < lease.expires)
    }
    pub fn output(&mut self) -> Config {
        self.output_at(std::time::Instant::now())
    }
    fn output_at(&mut self, now: std::time::Instant) -> Config {
        if !self.preview_active(now) {
            self.preview = None;
        }
        self.preview
            .as_ref()
            .map(|lease| lease.config.clone())
            .unwrap_or_else(|| self.saved.config.clone())
    }
    pub fn begin_preview(
        &mut self,
        owner: &str,
        config: Config,
        seconds: u8,
        now: std::time::Instant,
    ) -> Result<u32, matrix_core::preview::PreviewStatus> {
        use matrix_core::preview::PreviewStatus;
        if owner.is_empty()
            || owner.len() > 64
            || !owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || !(1..=15).contains(&seconds)
            || !config.validate().is_empty()
        {
            return Err(PreviewStatus::Invalid);
        }
        if self.preview_active(now) {
            return Err(PreviewStatus::Busy);
        }
        self.next_lease = self
            .next_lease
            .checked_add(1)
            .ok_or(PreviewStatus::Invalid)?;
        self.preview = Some(Lease {
            owner: owner.into(),
            generation: self.next_lease,
            config,
            expires: now + std::time::Duration::from_secs(u64::from(seconds)),
        });
        Ok(self.next_lease)
    }
    pub fn owns_preview(&self, owner: &str, generation: u32, now: std::time::Instant) -> bool {
        self.preview.as_ref().is_some_and(|lease| {
            lease.owner == owner && lease.generation == generation && now < lease.expires
        })
    }
    pub fn cancel_preview(&mut self, owner: &str, generation: Option<u32>) -> bool {
        if self.preview.as_ref().is_some_and(|lease| {
            lease.owner == owner && generation.is_none_or(|id| id == lease.generation)
        }) {
            self.preview = None;
            true
        } else {
            false
        }
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
    #[test]
    fn preview_is_exclusive_ephemeral_and_restores_latest_applied_state() {
        use matrix_core::preview::PreviewStatus;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let mut store = Store::load(path.clone()).unwrap();
        let now = std::time::Instant::now();
        let draft = Config {
            clock_intensity: 200,
            ..Config::default()
        };
        let first = store
            .begin_preview("owner", draft.clone(), 10, now)
            .unwrap();
        assert_eq!(store.output_at(now).clock_intensity, 200);
        assert_eq!(store.read().config.clock_intensity, 180);
        assert!(!path.exists());
        assert_eq!(
            store.begin_preview("other", draft.clone(), 10, now),
            Err(PreviewStatus::Busy)
        );
        assert!(!store.cancel_preview("other", None));
        let applied = Config {
            clock_intensity: 220,
            ..Config::default()
        };
        store.apply(0, applied);
        assert_eq!(store.output_at(now).clock_intensity, 200);
        assert_eq!(
            store
                .output_at(now + std::time::Duration::from_secs(10))
                .clock_intensity,
            220
        );
        let next = store
            .begin_preview("owner", draft, 10, now + std::time::Duration::from_secs(11))
            .unwrap();
        assert_ne!(first, next);
        assert!(!store.cancel_preview("owner", Some(first)));
        assert!(store.cancel_preview("owner", Some(next)));
        assert_eq!(store.output().clock_intensity, 220);
        assert_eq!(
            Store::load(path).unwrap().read().config.clock_intensity,
            220
        );
    }
}
