//! Claude Pro/Max plan limits, read with the login Claude Code already stores.
//! Same numbers as claude.ai > Settings > Usage. Unofficial endpoint: if the
//! shape changes, this is the file to fix.

use crate::model::{Detail, FetchError, Limit, Reading};
use crate::util::{capitalize, claude_dir, display_name, get_json, now, ts_field};
use serde_json::Value;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const NOT_CONNECTED: &str = "No Claude Code login found. Install Claude Code and sign in once.";

fn read_credentials() -> Result<Value, FetchError> {
    // macOS: Claude Code keeps its login in the Keychain.
    #[cfg(target_os = "macos")]
    if let Ok(out) = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
    {
        if out.status.success() {
            if let Ok(v) = serde_json::from_slice(&out.stdout) {
                return Ok(v);
            }
        }
    }
    // Windows and Linux: a file next to Claude Code's settings.
    let path = claude_dir().join(".credentials.json");
    let text = std::fs::read_to_string(path).map_err(|_| FetchError::NotConnected(NOT_CONNECTED.into()))?;
    serde_json::from_str(&text).map_err(|_| FetchError::Usage("can't read the Claude Code login".into()))
}

fn plan_name(oauth: &Value) -> String {
    // rateLimitTier looks like "default_claude_max_20x"
    let tier = oauth["rateLimitTier"].as_str().unwrap_or("");
    if let Some(i) = tier.find("max_") {
        let rest = &tier[i + 4..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && rest[digits.len()..].starts_with('x') {
            return format!("Max {digits}x");
        }
    }
    capitalize(oauth["subscriptionType"].as_str().unwrap_or(""))
}

fn window_label(window: Option<&str>) -> Option<&'static str> {
    match window {
        Some("5h") => Some("5h session"),
        Some("week") => Some("Weekly"),
        _ => None,
    }
}

pub async fn fetch(client: &reqwest::Client) -> Result<Reading, FetchError> {
    let creds = read_credentials()?;
    let oauth = creds.get("claudeAiOauth").ok_or_else(|| FetchError::NotConnected(NOT_CONNECTED.into()))?;
    let token = oauth["accessToken"].as_str().ok_or_else(|| FetchError::NotConnected(NOT_CONNECTED.into()))?;
    let expires = oauth["expiresAt"].as_f64().unwrap_or(0.0) / 1000.0;
    if expires > 0.0 && expires < now() {
        // Refreshing it ourselves would rotate the token out from under Claude Code.
        return Err(FetchError::Usage("Login expired. Open Claude Code once to renew it.".into()));
    }

    let data = get_json(
        client,
        USAGE_URL,
        &[("Authorization", format!("Bearer {token}")), ("anthropic-beta", "oauth-2025-04-20".into())],
    )
    .await?;
    Ok(parse(&data, oauth))
}

/// Turn the usage response (and the plan info stored with the login) into a Reading.
pub fn parse(data: &Value, oauth: &Value) -> Reading {
    let mut limits = Vec::new();
    for lim in data["limits"].as_array().into_iter().flatten() {
        let kind = lim["kind"].as_str().unwrap_or("limit");
        let tag = display_name(&lim["scope"]["model"]).or_else(|| display_name(&lim["scope"]["surface"]));
        let window = if kind == "session" {
            Some("5h")
        } else if kind.starts_with("weekly") {
            Some("week")
        } else {
            None
        };
        let base = window_label(window).map(str::to_string).unwrap_or_else(|| capitalize(&kind.replace('_', " ")));
        limits.push(Limit {
            label: tag.as_ref().map(|t| format!("{base} · {t}")).unwrap_or(base),
            window: window.map(str::to_string),
            model: tag,
            percent: lim["percent"].as_f64().unwrap_or(0.0),
            resets_at: ts_field(&lim["resets_at"]),
            critical: lim["severity"].as_str() == Some("critical"),
        });
    }
    if limits.is_empty() {
        // Older response shape without the "limits" list.
        for (key, window, tag) in [
            ("five_hour", "5h", None),
            ("seven_day", "week", None),
            ("seven_day_opus", "week", Some("Opus")),
            ("seven_day_sonnet", "week", Some("Sonnet")),
        ] {
            let Some(pct) = data[key]["utilization"].as_f64() else { continue };
            let base = window_label(Some(window)).unwrap_or_default();
            limits.push(Limit {
                label: tag.map(|t| format!("{base} · {t}")).unwrap_or_else(|| base.to_string()),
                window: Some(window.into()),
                model: tag.map(str::to_string),
                percent: pct,
                resets_at: ts_field(&data[key]["resets_at"]),
                critical: false,
            });
        }
    }

    let extra = &data["extra_usage"];
    let details = if extra["is_enabled"].as_bool() == Some(true) {
        let scale = 10f64.powi(extra["decimal_places"].as_i64().unwrap_or(0) as i32);
        let used = extra["used_credits"].as_f64().unwrap_or(0.0) / scale;
        let mut text = format!("{used:.2}");
        if let Some(cap) = extra["monthly_limit"].as_f64() {
            text += &format!(" of {:.2}", cap / scale);
        }
        let currency = extra["currency"].as_str().unwrap_or("");
        let tone = if extra["spend_limit_reached"].as_bool() == Some(true) { "bad" } else { "text" };
        vec![Detail::new("Extra usage", format!("{text} {currency} used").trim().to_string(), tone)]
    } else {
        vec![Detail::new("Extra usage", "off", "dim")]
    };

    Reading { plan: plan_name(oauth), limits, details, block_reason: None, note: None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_limits_list_with_model_scope() {
        let data = json!({
            "limits": [
                { "kind": "session", "percent": 3, "resets_at": "2026-10-24T15:00:00+00:00" },
                { "kind": "weekly", "percent": 66, "resets_at": "2026-10-26T20:00:00+00:00" },
                { "kind": "weekly_model", "percent": 100, "severity": "critical",
                  "scope": { "model": { "display_name": "Fable" } }, "resets_at": "2026-10-26T20:00:00+00:00" }
            ],
            "extra_usage": { "is_enabled": false }
        });
        let r = parse(&data, &json!({ "rateLimitTier": "default_claude_max_20x" }));
        assert_eq!(r.plan, "Max 20x");
        assert_eq!(r.limits.len(), 3);
        assert_eq!(r.limits[0].label, "5h session");
        assert_eq!(r.limits[1].label, "Weekly");
        assert_eq!(r.limits[2].label, "Weekly · Fable");
        assert_eq!(r.limits[2].model.as_deref(), Some("Fable"));
        assert!(r.limits[2].critical);
        assert!(r.limits[0].resets_at.is_some());
        assert_eq!(r.details[0].text, "off");
    }

    #[test]
    fn falls_back_to_older_response_shape() {
        let data = json!({
            "five_hour": { "utilization": 10.0, "resets_at": "2026-10-24T15:00:00Z" },
            "seven_day": { "utilization": 50.0 },
            "seven_day_opus": { "utilization": 80.0 },
            "extra_usage": { "is_enabled": true, "used_credits": 150, "monthly_limit": 2000, "decimal_places": 2, "currency": "EUR" }
        });
        let r = parse(&data, &json!({ "subscriptionType": "pro" }));
        assert_eq!(r.plan, "Pro");
        assert_eq!(r.limits.iter().map(|l| l.label.as_str()).collect::<Vec<_>>(), ["5h session", "Weekly", "Weekly · Opus"]);
        assert_eq!(r.details[0].text, "1.50 of 20.00 EUR used");
    }
}
