//! ChatGPT plan limits as Codex sees them (same numbers as Codex /status),
//! read with the login the Codex CLI saves in ~/.codex/auth.json.
//! ChatGPT's chat message caps aren't exposed anywhere, so this is the closest
//! reading for a ChatGPT plan. Unofficial endpoint.

use crate::model::{Detail, FetchError, Limit, Reading};
use crate::util::{capitalize, codex_home, get_json, now, pretty_model, ts_field};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::Path;
use walkdir::WalkDir;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn window_name(seconds: f64) -> Option<&'static str> {
    match seconds as i64 {
        18000 => Some("5h"),
        604800 => Some("week"),
        _ => None,
    }
}

fn label_for(window: Option<&str>, seconds: f64) -> String {
    match window {
        Some("5h") => "5h session".into(),
        Some("week") => "Weekly".into(),
        _ => format!("{}h window", (seconds / 3600.0).round()),
    }
}

/// Limit rows for one Codex pool: its 5h and weekly windows.
fn windows(pool: &Value, scope: Option<&str>) -> Vec<Limit> {
    let mut rows = Vec::new();
    for key in ["primary_window", "secondary_window"] {
        let w = &pool[key];
        if !w.is_object() {
            continue;
        }
        let secs = w["limit_window_seconds"].as_f64().unwrap_or(0.0);
        let window = window_name(secs);
        let base = label_for(window, secs);
        rows.push(Limit {
            label: scope.map(|s| format!("{base} · {s}")).unwrap_or(base),
            window: window.map(str::to_string),
            model: scope.map(str::to_string),
            percent: w["used_percent"].as_f64().unwrap_or(0.0),
            resets_at: ts_field(&w["reset_at"]),
            critical: false,
        });
    }
    rows
}

pub async fn fetch(client: &reqwest::Client) -> Result<Reading, FetchError> {
    let home = codex_home();
    let auth: Value = std::fs::read_to_string(home.join("auth.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| FetchError::NotConnected("No Codex login found. Install Codex and run `codex login`.".into()))?;
    let tokens = &auth["tokens"];
    let Some(token) = tokens["access_token"].as_str() else {
        return Err(FetchError::NotConnected("Codex isn't signed in with ChatGPT. Run `codex login`.".into()));
    };

    let data = match get_json(
        client,
        USAGE_URL,
        &[
            ("Authorization", format!("Bearer {token}")),
            ("ChatGPT-Account-Id", tokens["account_id"].as_str().unwrap_or("").into()),
            ("User-Agent", "codex_cli_rs/0.1".into()),
            ("originator", "codex_cli_rs".into()),
        ],
    )
    .await
    {
        Ok(d) => d,
        Err(FetchError::RateLimited(s)) => return Err(FetchError::RateLimited(s)),
        Err(live) => {
            if let Some(mut snap) = snapshot(&home) {
                snap.note = Some(format!("From the last Codex session (live: {live})"));
                return Ok(snap);
            }
            return Err(live);
        }
    };
    Ok(parse(&data))
}

/// Turn the wham/usage response into a Reading.
pub fn parse(data: &Value) -> Reading {
    // The main pool is shared by every model; extra pools cover one model or feature.
    let mut limits = windows(&data["rate_limit"], None);
    for extra in data["additional_rate_limits"].as_array().into_iter().flatten() {
        let name = extra["limit_name"].as_str().or_else(|| extra["metered_feature"].as_str());
        limits.extend(windows(&extra["rate_limit"], name));
    }
    limits.extend(windows(&data["code_review_rate_limit"], Some("Code review")));

    let mut details = Vec::new();
    if let Some(models) = data["model_usage"].as_object() {
        for (model, info) in models {
            if info["available"].as_bool().unwrap_or(true) {
                details.push(Detail::new(&pretty_model(model), "available", "ok"));
            } else {
                let mut text = "unavailable".to_string();
                if info["credits_would_enable"].as_bool() == Some(true) {
                    text += " · credits would unlock it";
                }
                let mut detail = Detail::new(&pretty_model(model), text, "bad");
                detail.at = ts_field(&info["available_at"]);
                details.push(detail);
            }
        }
    }
    let credits = &data["credits"];
    if credits["unlimited"].as_bool() == Some(true) {
        details.push(Detail::new("Credits", "unlimited", "ok"));
    } else if credits["has_credits"].as_bool() == Some(true) {
        let balance = match &credits["balance"] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let mut text = format!("balance {balance}");
        if let Some(approx) = credits["approx_local_messages"].as_u64() {
            text += &format!(" · ~{approx} messages");
        }
        details.push(Detail::new("Credits", text, "text"));
    } else {
        details.push(Detail::new("Credits", "none", "dim"));
    }
    if let Some(count) = data["rate_limit_reset_credits"]["available_count"].as_u64().filter(|c| *c > 0) {
        details.push(Detail::new("Limit resets", format!("{count} available"), "text"));
    }
    if data["spend_control"]["reached"].as_bool() == Some(true) {
        details.push(Detail::new("Spend control", "limit reached", "bad"));
    }

    let reached = data["rate_limit_reached_type"]["type"].as_str().unwrap_or("");
    let reason = match reached {
        "workspace_owner_credits_depleted" => "workspace out of credits".to_string(),
        other => other.replace('_', " "),
    };
    Reading {
        plan: capitalize(data["plan_type"].as_str().unwrap_or("")),
        limits,
        details,
        block_reason: Some(reason).filter(|r| !r.is_empty()),
        note: None,
    }
}

/// Newest main-pool reading Codex logged locally, for when the live call fails.
fn snapshot(home: &Path) -> Option<Reading> {
    let mut files: Vec<_> = WalkDir::new(home.join("sessions"))
        .min_depth(4)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    for path in files.iter().rev().take(5) {
        let Ok(file) = std::fs::File::open(path) else { continue };
        let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
        for line in lines.iter().rev() {
            if !line.contains("\"rate_limits\"") {
                continue;
            }
            let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
            let rl = &d["payload"]["rate_limits"];
            if !rl["primary"].is_object() && !rl["secondary"].is_object() {
                continue; // e.g. the credits pool, which has no windows
            }
            let mut limits = Vec::new();
            for key in ["primary", "secondary"] {
                let w = &rl[key];
                if !w.is_object() {
                    continue;
                }
                let reset = ts_field(&w["resets_at"]);
                let expired = reset.is_some_and(|r| r < now());
                let secs = w["window_minutes"].as_f64().unwrap_or(0.0) * 60.0;
                let window = window_name(secs);
                limits.push(Limit {
                    label: label_for(window, secs),
                    window: window.map(str::to_string),
                    model: None,
                    percent: if expired { 0.0 } else { w["used_percent"].as_f64().unwrap_or(0.0) },
                    resets_at: if expired { None } else { reset },
                    critical: false,
                });
            }
            return Some(Reading {
                plan: capitalize(rl["plan_type"].as_str().unwrap_or("")),
                limits,
                ..Default::default()
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_pools_credits_and_models() {
        let data = json!({
            "plan_type": "team",
            "rate_limit": {
                "primary_window": { "used_percent": 12.0, "limit_window_seconds": 18000, "reset_at": 1_800_000_000 },
                "secondary_window": { "used_percent": 45.0, "limit_window_seconds": 604800, "reset_at": 1_800_400_000 }
            },
            "additional_rate_limits": [
                { "limit_name": "GPT-6 Astra", "rate_limit": { "primary_window": { "used_percent": 100.0, "limit_window_seconds": 604800 } } }
            ],
            "model_usage": { "gpt-6-astra": { "available": false, "available_at": 1_800_400_000, "credits_would_enable": true } },
            "credits": { "has_credits": true, "balance": "12.5", "approx_local_messages": 40 },
            "rate_limit_reset_credits": { "available_count": 2 },
            "rate_limit_reached_type": { "type": "workspace_owner_credits_depleted" }
        });
        let r = parse(&data);
        assert_eq!(r.plan, "Team");
        assert_eq!(r.limits.len(), 3);
        assert_eq!(r.limits[0].window.as_deref(), Some("5h"));
        assert_eq!(r.limits[1].window.as_deref(), Some("week"));
        assert_eq!(r.limits[2].model.as_deref(), Some("GPT-6 Astra"));
        assert_eq!(r.limits[2].label, "Weekly · GPT-6 Astra");
        assert_eq!(r.details[0].label, "GPT-6 Astra");
        assert_eq!(r.details[0].text, "unavailable · credits would unlock it");
        assert_eq!(r.details[0].at, Some(1_800_400_000.0));
        assert_eq!(r.details[1].text, "balance 12.5 · ~40 messages");
        assert_eq!(r.details[2].text, "2 available");
        assert_eq!(r.block_reason.as_deref(), Some("workspace out of credits"));
    }

    #[test]
    fn empty_response_gives_no_limits() {
        let r = parse(&json!({}));
        assert!(r.limits.is_empty());
        assert_eq!(r.details.len(), 1); // "Credits: none"
        assert!(r.block_reason.is_none());
    }
}
