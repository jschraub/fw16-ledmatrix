use matrix_core::assistant::Provider;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Read, path::Path};

// Deliberately not Debug/Serialize. Only the fingerprint outlives a poll.
pub(super) struct Login {
    pub token: String,
    pub account: Option<String>,
    pub key: [u8; 32],
}

fn header(value: &Value) -> Option<String> {
    let s = value.as_str()?;
    (!s.is_empty() && s.len() <= 16384 && s.bytes().all(|b| b.is_ascii_graphic()))
        .then(|| s.to_owned())
}

pub(super) fn claude(data: &[u8]) -> Option<Login> {
    let json: Value = serde_json::from_slice(data).ok()?;
    Some(identity(
        header(&json["claudeAiOauth"]["accessToken"])?,
        None,
        None,
    ))
}

fn identity(token: String, account: Option<String>, row: Option<String>) -> Login {
    let mut hash = Sha256::new();
    hash.update(row.unwrap_or_default());
    hash.update([0]);
    hash.update([u8::from(account.is_some())]);
    hash.update(account.as_deref().unwrap_or(&token));
    Login {
        token,
        account,
        key: hash.finalize().into(),
    }
}

// Select only fields required by this endpoint, never refresh tokens or labels.
pub(super) fn database(path: &Path) -> Option<Login> {
    use rusqlite::{Connection, OpenFlags};
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    c.busy_timeout(std::time::Duration::from_millis(100)).ok()?;
    let mut statement = c.prepare("SELECT id, json_extract(value,'$.type'), json_extract(value,'$.access'), coalesce(json_extract(value,'$.metadata.accountID'),json_extract(value,'$.accountId'))
        FROM credential WHERE integration_id='openai' AND length(value)<=262144 AND
        (active=1 OR (active IS NULL AND (SELECT count(*) FROM credential WHERE integration_id='openai')=1)) LIMIT 2").ok()?;
    let mut rows = statement.query([]).ok()?;
    let row = rows.next().ok()??;
    if row.get::<_, String>(1).ok()? != "oauth" {
        return None;
    }
    let id = row.get(0).ok()?;
    let token = header(&Value::String(row.get(2).ok()?))?;
    let account = match row.get::<_, Option<String>>(3).ok()? {
        Some(s) => Some(header(&Value::String(s))?),
        None => None,
    };
    rows.next()
        .ok()?
        .is_none()
        .then(|| identity(token, account, Some(id)))
}

const LIMIT: usize = 256 * 1024;
fn file(path: &Path) -> Option<Login> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut data = Vec::new();
    file.take((LIMIT + 1) as u64).read_to_end(&mut data).ok()?;
    (data.len() <= LIMIT).then(|| claude(&data)).flatten()
}

pub(super) fn discover(provider: Provider) -> Option<Login> {
    let path = match provider {
        Provider::Claude => std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(Into::into)
            .or_else(|| Some(dirs::home_dir()?.join(".claude")))?
            .join(".credentials.json"),
        Provider::OpencodeOpenai => {
            let path = std::env::var_os("OPENCODE_DB")
                .map(std::path::PathBuf::from)
                .or_else(|| Some(dirs::data_local_dir()?.join("opencode/opencode.db")))?;
            return path.is_absolute().then(|| database(&path)).flatten();
        }
    };
    if !path.is_absolute() {
        return None;
    }
    file(&path)
}
