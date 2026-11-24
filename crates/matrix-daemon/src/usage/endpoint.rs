use super::login::Login;
use matrix_core::{
    assistant::Provider,
    usage::{Usage, UsageStatus, Window},
};
use serde_json::Value;

pub(super) fn parse(provider: Provider, raw: &[u8], now: u32) -> Result<Usage, UsageStatus> {
    let json: Value = serde_json::from_slice(raw).map_err(|_| UsageStatus::SchemaError)?;
    let mut data = Usage::empty(provider, UsageStatus::Fresh);
    let ttl = match provider {
        Provider::Claude => {
            if !["five_hour", "seven_day"]
                .iter()
                .any(|k| json.get(k).is_some())
            {
                return Err(UsageStatus::SchemaError);
            }
            for (key, slot) in [
                ("five_hour", &mut data.five_hour),
                ("seven_day", &mut data.weekly),
            ] {
                let w = &json[key];
                if w.is_null() {
                    continue;
                }
                *slot = Some(Window {
                    used_percent: percent(&w["utilization"])?,
                    resets_at: match w["resets_at"].as_str() {
                        Some(s) => Some(
                            chrono::DateTime::parse_from_rfc3339(s)
                                .ok()
                                .and_then(|t| t.timestamp().try_into().ok())
                                .ok_or(UsageStatus::SchemaError)?,
                        ),
                        None if w["resets_at"].is_null() => None,
                        _ => return Err(UsageStatus::SchemaError),
                    },
                });
            }
            12 * 3600
        }
        Provider::OpencodeOpenai => {
            let limits = json["rate_limit"]
                .as_object()
                .ok_or(UsageStatus::SchemaError)?;
            if !["primary_window", "secondary_window"]
                .iter()
                .any(|k| limits.contains_key(*k))
            {
                return Err(UsageStatus::SchemaError);
            }
            for w in ["primary_window", "secondary_window"]
                .iter()
                .filter_map(|k| limits.get(*k))
                .filter(|v| !v.is_null())
            {
                let slot = match w["limit_window_seconds"]
                    .as_u64()
                    .ok_or(UsageStatus::SchemaError)?
                {
                    18000 => &mut data.five_hour,
                    604800 => &mut data.weekly,
                    _ => continue,
                };
                *slot = Some(Window {
                    used_percent: percent(&w["used_percent"])?,
                    resets_at: if w["reset_at"].is_null() {
                        None
                    } else {
                        Some(
                            w["reset_at"]
                                .as_u64()
                                .and_then(|n| n.try_into().ok())
                                .ok_or(UsageStatus::SchemaError)?,
                        )
                    },
                });
            }
            300
        }
    };
    data.expires_at = Some(now.saturating_add(ttl));
    Ok(data.at(now))
}

fn percent(v: &Value) -> Result<u8, UsageStatus> {
    v.as_f64()
        .filter(|n| (0.0..=100.0).contains(n))
        .map(|n| n.round() as u8)
        .ok_or(UsageStatus::SchemaError)
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    provider: Provider,
    login: &Login,
) -> Result<Usage, UsageStatus> {
    let request = match provider {
        Provider::Claude => client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("anthropic-beta", "oauth-2025-04-20"),
        Provider::OpencodeOpenai => {
            let req = client.get("https://chatgpt.com/backend-api/wham/usage");
            if let Some(id) = &login.account {
                req.header("ChatGPT-Account-Id", id)
            } else {
                req
            }
        }
    };
    let mut response = request
        .bearer_auth(&login.token)
        .send()
        .await
        .map_err(|_| UsageStatus::RequestFailed)?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageStatus::Unauthorized);
    }
    if !response.status().is_success() {
        return Err(UsageStatus::RequestFailed);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UsageStatus::RequestFailed)?
    {
        if body.len() + chunk.len() > 65536 {
            return Err(UsageStatus::SchemaError);
        }
        body.extend_from_slice(&chunk);
    }
    parse(provider, &body, super::now())
}
