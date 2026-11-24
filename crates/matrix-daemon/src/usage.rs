mod endpoint;
mod login;
#[cfg(test)]
mod tests;

use matrix_core::{
    assistant::Provider,
    usage::{Usage, UsageStatus},
};
use std::time::{Duration, Instant};
use tokio::sync::watch;

fn now() -> u32 {
    chrono::Utc::now()
        .timestamp()
        .try_into()
        .unwrap_or(u32::MAX)
}

fn client() -> Result<reqwest::Client, UsageStatus> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| UsageStatus::RequestFailed)
}

async fn checked(
    client: &reqwest::Client,
    provider: Provider,
    login: &login::Login,
) -> Result<Usage, UsageStatus> {
    let result = endpoint::fetch(client, provider, login).await;
    if login::discover(provider).is_none_or(|latest| latest.key != login.key) {
        return Err(UsageStatus::AccountChanged);
    }
    result
}

/// Standalone, single authorized request; never binds IPC or opens a panel.
pub async fn check(provider: Provider) -> Usage {
    let result = async {
        let login = login::discover(provider).ok_or(UsageStatus::UnsupportedLogin)?;
        checked(&client()?, provider, &login).await
    }
    .await;
    result.unwrap_or_else(|status| Usage::empty(provider, status))
}

// Cache only transient failures, and only until the original expiry/reset.
fn after(previous: Usage, result: Result<Usage, UsageStatus>, time: u32) -> Usage {
    match result {
        Ok(data) => data,
        Err(status @ (UsageStatus::RequestFailed | UsageStatus::SchemaError))
            if previous.expires_at.is_some() =>
        {
            Usage { status, ..previous }.at(time)
        }
        Err(status) => Usage::empty(previous.provider, status),
    }
}

pub fn start(provider: Provider, opt_in: &str) -> watch::Receiver<Usage> {
    let (tx, rx) = watch::channel(Usage::empty(provider, UsageStatus::Disabled));
    if cfg!(target_os = "linux") && std::env::var(opt_in).as_deref() == Ok("1") {
        tokio::spawn(async move {
            let mut key = None;
            let mut next = Instant::now();
            loop {
                let login = login::discover(provider);
                let identity = login.as_ref().map(|l| l.key);
                if key != identity {
                    tx.send_replace(Usage::empty(provider, UsageStatus::AccountChanged));
                    key = identity;
                    next = Instant::now();
                }
                if let Some(login) = login {
                    if Instant::now() >= next {
                        let result = match client() {
                            Ok(c) => checked(&c, provider, &login).await,
                            Err(e) => Err(e),
                        };
                        let previous = tx.borrow().clone();
                        tx.send_replace(after(previous, result, now()));
                        next = Instant::now() + Duration::from_secs(60);
                    }
                } else {
                    tx.send_replace(Usage::empty(provider, UsageStatus::UnsupportedLogin));
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }
    rx
}
