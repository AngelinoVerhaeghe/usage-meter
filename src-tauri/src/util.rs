//! Small helpers: time, formatting, model names, HTTP.

use crate::model::{Detail, FetchError, Reading};
use chrono::{DateTime, Local, TimeZone};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn iso_ts(value: &str) -> Option<f64> {
    DateTime::parse_from_rfc3339(value).ok().map(|d| d.timestamp_millis() as f64 / 1000.0)
}

/// A JSON field that is either an ISO date or Unix seconds.
pub fn ts_field(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(iso_ts))
}

fn local(ts: f64) -> Option<DateTime<Local>> {
    // Round to the minute: some resets land at xx:59:59.9 next to siblings at xx:00.
    Local.timestamp_opt(((ts / 60.0).round() * 60.0) as i64, 0).single()
}

/// "15:00" today, "Mon 20:00" this week, "31 Oct" further out.
pub fn fmt_reset(ts: f64) -> String {
    let (Some(when), now) = (local(ts), Local::now()) else { return String::new() };
    if when.date_naive() == now.date_naive() {
        when.format("%H:%M").to_string()
    } else if (when - now).num_days().abs() < 6 {
        when.format("%a %H:%M").to_string()
    } else {
        when.format("%d %b").to_string()
    }
}

pub fn fmt_hm(ts: f64) -> String {
    local(ts).map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// claude-opus-4-8 -> Opus 4.8, gpt-5.6-luna -> GPT-5.6 Luna.
pub fn pretty_model(model: &str) -> String {
    let parts: Vec<&str> = model.split('-').collect();
    let is_num = |p: &&str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit());
    if parts[0] == "claude" {
        let words: Vec<String> = parts[1..].iter().filter(|p| !is_num(p)).map(|p| capitalize(p)).collect();
        // skip date suffixes like 20251001
        let version: Vec<&str> = parts[1..].iter().filter(|p| is_num(p) && p.len() < 8).copied().collect();
        let mut out = words.join(" ");
        if !version.is_empty() {
            out = format!("{out} {}", version.join("."));
        }
        return out.trim().to_string();
    }
    if parts[0] == "gpt" && parts.len() > 1 {
        let mut out = format!("GPT-{}", parts[1]);
        for p in &parts[2..] {
            out = format!("{out} {}", capitalize(p));
        }
        return out;
    }
    model.to_string()
}

/// `{"display_name": "Fable"}`, `{"id": ...}` or a plain string.
pub fn display_name(v: &Value) -> Option<String> {
    match v {
        Value::Object(o) => o
            .get("display_name")
            .or_else(|| o.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".claude"))
}

pub fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".codex"))
}

/// HTTP client for the usage endpoints. Redirects are refused, so a login token
/// is only ever sent to the host it was meant for.
pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

pub async fn get_json(client: &reqwest::Client, url: &str, headers: &[(&str, String)]) -> Result<Value, FetchError> {
    let mut map = HeaderMap::new();
    map.insert(USER_AGENT, HeaderValue::from_static("usage-meter/0.1"));
    for (k, v) in headers {
        if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
            map.insert(name, value);
        }
    }
    let resp = client
        .get(url)
        .headers(map)
        .send()
        .await
        .map_err(|_| FetchError::Usage("network unreachable".into()))?;
    let status = resp.status().as_u16();
    if status == 429 {
        let retry = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        return Err(FetchError::RateLimited(retry));
    }
    if !(200..300).contains(&status) {
        let hint = if status == 401 || status == 403 { " (login rejected)" } else { "" };
        return Err(FetchError::Usage(format!("HTTP {status}{hint}")));
    }
    resp.json().await.map_err(|_| FetchError::Usage("unexpected response".into()))
}

/// Reset time of a used-up plan-wide limit (0 if unknown), or None when nothing is blocked.
pub fn blocked_until(reading: &Reading) -> Option<f64> {
    reading
        .limits
        .iter()
        .filter(|l| l.model.is_none() && l.percent >= 100.0)
        .map(|l| l.resets_at.unwrap_or(0.0))
        .reduce(f64::max)
}

/// One plain-language line: what can I use right now?
pub fn status(reading: &Reading) -> Option<Detail> {
    if reading.limits.is_empty() {
        return None;
    }
    if let Some(until) = blocked_until(reading) {
        let mut text = "All models blocked".to_string();
        if until > 0.0 {
            text += &format!(" until {}", fmt_reset(until));
        }
        if let Some(reason) = reading.block_reason.as_deref().filter(|r| !r.is_empty()) {
            text += &format!(" · {reason}");
        }
        return Some(Detail::new("Status", text, "bad"));
    }
    let capped: Vec<_> = reading.limits.iter().filter(|l| l.model.is_some() && l.percent >= 100.0).collect();
    if let Some(first) = capped.first() {
        let names: Vec<&str> = capped.iter().filter_map(|l| l.model.as_deref()).collect();
        let until = first.resets_at.map(fmt_reset).unwrap_or_default();
        return Some(Detail::new(
            "Status",
            format!("{} blocked until {until} · other models OK", names.join(", ")),
            "warn",
        ));
    }
    Some(Detail::new("Status", "All models available", "ok"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Limit;

    fn limit(percent: f64, model: Option<&str>) -> Limit {
        Limit {
            label: "x".into(),
            window: Some("week".into()),
            model: model.map(str::to_string),
            percent,
            resets_at: Some(now() + 3600.0),
            critical: false,
        }
    }

    #[test]
    fn model_names() {
        assert_eq!(pretty_model("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(pretty_model("claude-haiku-4-5-20251001"), "Haiku 4.5");
        assert_eq!(pretty_model("gpt-6-astra"), "GPT-6 Astra");
        assert_eq!(pretty_model("something-else"), "something-else");
    }

    #[test]
    fn status_lines() {
        let ok = Reading { limits: vec![limit(50.0, None)], ..Default::default() };
        assert_eq!(status(&ok).unwrap().tone, "ok");
        let one_model = Reading { limits: vec![limit(50.0, None), limit(100.0, Some("Fable"))], ..Default::default() };
        let s = status(&one_model).unwrap();
        assert_eq!(s.tone, "warn");
        assert!(s.text.starts_with("Fable blocked until"));
        let all = Reading { limits: vec![limit(100.0, None)], ..Default::default() };
        assert_eq!(status(&all).unwrap().tone, "bad");
        assert!(status(&Reading::default()).is_none());
    }

    #[test]
    fn timestamps() {
        assert_eq!(iso_ts("2026-10-24T15:00:00Z"), Some(1792854000.0));
        assert_eq!(ts_field(&serde_json::json!(1792854000)), Some(1792854000.0));
        assert_eq!(ts_field(&serde_json::Value::Null), None);
    }
}
