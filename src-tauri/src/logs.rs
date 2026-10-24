//! Per-model activity inside the current 5h and weekly windows, read from the
//! Claude Code / Codex logs on this computer. Only covers the CLIs on this
//! machine (not claude.ai or chatgpt.com), but it shows which model is eating
//! the shared limits.

use crate::model::{Limit, ModelUsage, UsageTable};
use crate::util::{claude_dir, codex_home, iso_ts, pretty_model};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Start of the current 5h and weekly windows, from their reset times.
fn window_starts(limits: &[Limit], now: f64) -> (f64, f64) {
    let start = |window: &str, span: f64| {
        limits
            .iter()
            .filter(|l| l.window.as_deref() == Some(window) && l.model.is_none())
            .filter_map(|l| l.resets_at)
            .reduce(f64::min)
            .map(|r| r - span)
            .unwrap_or(now - span)
    };
    (start("5h", 5.0 * 3600.0), start("week", 7.0 * 86400.0))
}

fn modified(path: &Path) -> f64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn jsonl_files(dir: PathBuf, depth: Option<usize>, since: f64) -> Vec<PathBuf> {
    let mut walk = WalkDir::new(dir);
    if let Some(d) = depth {
        walk = walk.min_depth(d).max_depth(d);
    }
    let mut files: Vec<PathBuf> = walk
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl") && modified(p) >= since)
        .collect();
    files.sort(); // Codex file names sort chronologically
    files
}

fn lines(path: &Path) -> impl Iterator<Item = String> {
    std::fs::File::open(path)
        .ok()
        .map(|f| BufReader::new(f).lines().map_while(Result::ok))
        .into_iter()
        .flatten()
}

fn table(source: &str, unit: &str, token_unit: &str, stats: HashMap<String, ModelUsage>) -> UsageTable {
    let mut models: Vec<ModelUsage> = stats
        .into_iter()
        .map(|(model, mut s)| {
            s.model = pretty_model(&model);
            s
        })
        .collect();
    models.sort_by(|a, b| b.week[1].cmp(&a.week[1]).then(b.week[0].cmp(&a.week[0])));
    models.truncate(6);
    UsageTable { source: source.into(), unit: unit.into(), token_unit: token_unit.into(), models }
}

/// Messages and output tokens per model from Claude Code's transcripts.
pub fn scan_claude(limits: &[Limit], now: f64) -> UsageTable {
    let (start_5h, start_week) = window_starts(limits, now);
    let mut stats: HashMap<String, ModelUsage> = HashMap::new();
    let mut seen = HashSet::new();
    for path in jsonl_files(claude_dir().join("projects"), None, start_week) {
        for line in lines(&path) {
            if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
                continue;
            }
            let Ok(d) = serde_json::from_str::<Value>(&line) else { continue };
            if d["type"].as_str() != Some("assistant") {
                continue;
            }
            let msg = &d["message"];
            let model = msg["model"].as_str().unwrap_or("");
            let Some(ts) = d["timestamp"].as_str().and_then(iso_ts) else { continue };
            if model.is_empty() || model.starts_with('<') || ts < start_week {
                continue;
            }
            // One API response is logged once per content block.
            let key = (msg["id"].as_str().unwrap_or("").to_string(), d["requestId"].as_str().unwrap_or("").to_string());
            if (!key.0.is_empty() || !key.1.is_empty()) && !seen.insert(key) {
                continue;
            }
            let out = msg["usage"]["output_tokens"].as_u64().unwrap_or(0);
            let s = stats.entry(model.to_string()).or_default();
            s.week[0] += 1;
            s.week[1] += out;
            if ts >= start_5h {
                s.h5[0] += 1;
                s.h5[1] += out;
            }
        }
    }
    table("Claude Code", "msgs", "out", stats)
}

/// Turns and tokens per model from Codex session logs, plus when a model last hit a limit.
pub fn scan_codex(limits: &[Limit], now: f64) -> UsageTable {
    let (start_5h, start_week) = window_starts(limits, now);
    let mut stats: HashMap<String, ModelUsage> = HashMap::new();
    for path in jsonl_files(codex_home().join("sessions"), Some(4), start_week) {
        let (mut model, mut total) = (None::<String>, 0u64);
        for line in lines(&path) {
            if !line.contains("\"turn_context\"") && !line.contains("\"token_count\"") && !line.contains("\"task_complete\"") {
                continue;
            }
            let Ok(d) = serde_json::from_str::<Value>(&line) else { continue };
            let payload = &d["payload"];
            let ts = d["timestamp"].as_str().and_then(iso_ts).unwrap_or(0.0);
            if d["type"].as_str() == Some("turn_context") {
                if let Some(m) = payload["model"].as_str() {
                    model = Some(m.to_string());
                }
                continue;
            }
            let Some(m) = model.clone() else { continue };
            let s = stats.entry(m).or_default();
            let (in_week, in_5h) = (ts >= start_week, ts >= start_5h);
            match payload["type"].as_str() {
                Some("token_count") => {
                    // total_token_usage is cumulative per session; the same reading is often logged twice.
                    let now_total = payload["info"]["total_token_usage"]["total_tokens"].as_u64().unwrap_or(total);
                    let delta = now_total.saturating_sub(total);
                    total = total.max(now_total);
                    if in_week {
                        s.week[1] += delta;
                    }
                    if in_5h {
                        s.h5[1] += delta;
                    }
                }
                Some("task_complete") => {
                    let error = &payload["error"];
                    if error["codex_error_info"].as_str() == Some("usage_limit_exceeded") {
                        s.blocked_at = Some(ts);
                    } else if error.is_null() {
                        s.blocked_at = None; // it worked again afterwards
                        if in_week {
                            s.week[0] += 1;
                        }
                        if in_5h {
                            s.h5[0] += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    stats.retain(|_, s| s.week != [0, 0] || s.blocked_at.is_some());
    table("Codex", "turns", "tok", stats)
}
