//! Polls each service on a timer, keeps the last good numbers, backs off after
//! HTTP 429, scans the local logs, sends alerts and feeds the UI and the tray.

use crate::model::{Account, FetchError, Reading, Snapshot};
use crate::settings::{self, Settings};
use crate::util::{fmt_hm, now};
use crate::{claude, codex, i18n, logs, tray};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

const CLICK_FLOOR: f64 = 60.0; // a manual refresh hits each service at most once a minute
const RATE_LIMIT_WAIT: f64 = 600.0; // after HTTP 429 wait this long (or longer, if Retry-After says so)
pub const STALE_AFTER: f64 = 30.0 * 60.0; // older numbers are shown dimmed and don't alert

#[derive(Clone, Default, Serialize, Deserialize)]
struct Alerts {
    /// "account|limit|reset bucket|threshold" -> when it fired
    fired: HashMap<String, f64>,
    /// "account|limit" currently at 100%, to say when it's available again
    full: HashSet<String>,
    /// The first run only records what's already over, so installing doesn't spam.
    seeded: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Persisted {
    accounts: Vec<Account>,
    alerts: Alerts,
}

pub struct Engine {
    pub snapshot: Mutex<Snapshot>,
    pub settings: Mutex<Settings>,
    /// Resolved language, "en" or "nl" (the UI resolves "system" and tells us).
    pub lang: Mutex<String>,
    alerts: Mutex<Alerts>,
    client: reqwest::Client,
    running: tokio::sync::Mutex<()>,
    state_path: Option<PathBuf>,
    first_run: bool,
}

impl Engine {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let (settings, first_run) = settings::load(app);
        if first_run {
            let _ = app.autolaunch().enable(); // "Open at login" starts on
            settings::save(app, &settings);
        }
        let state_path = app.path().app_cache_dir().ok().map(|d| {
            let _ = std::fs::create_dir_all(&d);
            d.join("state.json") // numbers only, never logins
        });
        let persisted: Persisted = state_path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();

        let accounts = [("claude", "claude", "Claude"), ("chatgpt", "codex", "ChatGPT")]
            .into_iter()
            .map(|(id, kind, name)| {
                persisted.accounts.iter().find(|a| a.id == id).cloned().unwrap_or_else(|| Account {
                    id: id.into(),
                    kind: kind.into(),
                    name: name.into(),
                    connected: true,
                    ..Default::default()
                })
            })
            .collect();

        let client = crate::util::http_client()?;
        Ok(Self {
            snapshot: Mutex::new(Snapshot { accounts, updated_at: 0.0 }),
            lang: Mutex::new(if settings.language == "nl" { "nl".into() } else { "en".into() }),
            settings: Mutex::new(settings),
            alerts: Mutex::new(persisted.alerts),
            client,
            running: tokio::sync::Mutex::new(()),
            state_path,
            first_run,
        })
    }

    /// Fetch whatever is due (everything not tried in the last minute when forced).
    pub async fn refresh(&self, app: &AppHandle, force: bool) {
        let Ok(_guard) = self.running.try_lock() else { return }; // a refresh is already running
        let now = now();
        let minutes = self.settings.lock().unwrap().refresh_minutes.max(3);
        let interval = if force { CLICK_FLOOR } else { (minutes * 60) as f64 - 15.0 };
        let accounts = self.snapshot.lock().unwrap().accounts.clone();

        let fetches = accounts.into_iter().map(|acc| {
            let client = self.client.clone();
            async move {
                // Backing off after a rate limit wins, even on a manual refresh.
                // Looking for a new login is local, so a click always does that.
                let interval = if force && !acc.connected { 0.0 } else { interval };
                if now < acc.retry_at || now - acc.tried_at < interval {
                    return acc;
                }
                let result = match acc.kind.as_str() {
                    "claude" => claude::fetch(&client).await,
                    _ => codex::fetch(&client).await,
                };
                apply(acc, result, now)
            }
        });
        let mut accounts = futures::future::join_all(fetches).await;

        // Local logs don't depend on the endpoints, so they keep updating through a 429.
        let to_scan: Vec<(usize, String, Vec<_>)> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| a.connected && now - a.scanned_at >= interval)
            .map(|(i, a)| (i, a.kind.clone(), a.reading.limits.clone()))
            .collect();
        if !to_scan.is_empty() {
            let scanned = tauri::async_runtime::spawn_blocking(move || {
                to_scan
                    .into_iter()
                    .map(|(i, kind, limits)| {
                        let table = if kind == "claude" { logs::scan_claude(&limits, now) } else { logs::scan_codex(&limits, now) };
                        (i, table)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            for (i, table) in scanned {
                accounts[i].usage = Some(table).filter(|t| !t.models.is_empty());
                accounts[i].scanned_at = now;
            }
        }

        let snapshot = Snapshot { accounts, updated_at: now };
        *self.snapshot.lock().unwrap() = snapshot.clone();
        self.alert(app, &snapshot);
        self.persist(&snapshot);
        tray::update(app, &snapshot);
        let _ = app.emit("snapshot", &snapshot);
    }

    fn alert(&self, app: &AppHandle, snapshot: &Snapshot) {
        let settings = self.settings.lock().unwrap().clone();
        let lang = self.lang.lock().unwrap().clone();
        let nl = i18n::is_nl(&lang);
        let mut alerts = self.alerts.lock().unwrap();
        let now = now();
        let mut thresholds = vec![];
        if settings.warn_at > 0.0 && settings.warn_at < 100.0 {
            thresholds.push(settings.warn_at);
        }
        if settings.notify_full {
            thresholds.push(100.0);
        }
        let mut full_now = HashSet::new();
        let mut messages = vec![];

        for acc in &snapshot.accounts {
            let fresh = acc.fetched_at.is_some_and(|t| now - t <= STALE_AFTER);
            if !fresh {
                continue;
            }
            for lim in &acc.reading.limits {
                let id = format!("{}|{}", acc.id, lim.label);
                if lim.percent >= 100.0 {
                    full_now.insert(id.clone());
                }
                for &th in &thresholds {
                    // One alert per limit window; resets_at jitters by milliseconds, so round it.
                    let key = format!("{id}|{}|{th}", (lim.resets_at.unwrap_or(0.0) / 600.0).round());
                    if lim.percent < th || alerts.fired.contains_key(&key) {
                        continue;
                    }
                    alerts.fired.insert(key, now);
                    let label = i18n::limit_label(lim, &lang);
                    let when = lim.resets_at.map(|r| i18n::fmt_reset(r, &lang));
                    let (title, body) = match (nl, th >= 100.0) {
                        (false, true) => (format!("{}: {label} limit reached", acc.name), when.map(|w| format!("Resets {w}"))),
                        (true, true) => (format!("{}: limiet {label} bereikt", acc.name), when.map(|w| format!("Reset {w}"))),
                        (false, false) => (format!("{}: {label} at {:.0}%", acc.name, lim.percent), when.map(|w| format!("Resets {w}"))),
                        (true, false) => (format!("{}: {label} op {:.0}%", acc.name, lim.percent), when.map(|w| format!("Reset {w}"))),
                    };
                    messages.push((title, body.unwrap_or_default()));
                }
            }
            if fresh && settings.notify_reset {
                for id in alerts.full.iter().filter(|id| id.starts_with(&format!("{}|", acc.id))) {
                    if !full_now.contains(id) {
                        let english = id.split_once('|').map(|x| x.1).unwrap_or("");
                        let label = acc
                            .reading
                            .limits
                            .iter()
                            .find(|l| l.label == english)
                            .map(|l| i18n::limit_label(l, &lang))
                            .unwrap_or_else(|| english.to_string());
                        let title = if nl {
                            format!("{}: {label} weer beschikbaar", acc.name)
                        } else {
                            format!("{}: {label} available again", acc.name)
                        };
                        messages.push((title, i18n::text(&lang, "reset_body").into()));
                    }
                }
            }
        }

        if alerts.seeded {
            for (title, body) in messages {
                let _ = app.notification().builder().title(title).body(body).show();
            }
        }
        // Only forget "full" limits for accounts we have fresh numbers for.
        let fresh_ids: HashSet<&str> = snapshot
            .accounts
            .iter()
            .filter(|a| a.fetched_at.is_some_and(|t| now - t <= STALE_AFTER))
            .map(|a| a.id.as_str())
            .collect();
        alerts.full.retain(|id| !fresh_ids.contains(id.split('|').next().unwrap_or("")));
        alerts.full.extend(full_now);
        alerts.fired.retain(|_, t| now - *t < 8.0 * 86400.0);
        alerts.seeded = true;
    }

    fn persist(&self, snapshot: &Snapshot) {
        let Some(path) = &self.state_path else { return };
        let data = Persisted { accounts: snapshot.accounts.clone(), alerts: self.alerts.lock().unwrap().clone() };
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_string(&data).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

fn apply(mut acc: Account, result: Result<Reading, FetchError>, now: f64) -> Account {
    acc.tried_at = now;
    match result {
        Ok(reading) => {
            acc.reading = reading;
            acc.error = None;
            acc.connected = true;
            acc.fetched_at = Some(now);
            acc.retry_at = 0.0;
        }
        Err(FetchError::RateLimited(secs)) => {
            let wait = (secs as f64).max(RATE_LIMIT_WAIT);
            acc.retry_at = now + wait;
            acc.error = Some(format!("Rate limited, next try {}", fmt_hm(now + wait)));
        }
        Err(FetchError::NotConnected(msg)) => {
            acc.connected = false;
            acc.error = Some(msg);
            acc.reading = Reading::default();
            acc.usage = None;
            acc.fetched_at = None;
        }
        // Keep the last good numbers; they dim once stale.
        Err(FetchError::Usage(msg)) => acc.error = Some(msg),
    }
    acc
}

/// Refresh every minute; each service is only called when it's due.
pub fn start(app: AppHandle, engine: Arc<Engine>) {
    tauri::async_runtime::spawn(async move {
        engine.refresh(&app, false).await;
        // Installed before Claude Code or Codex: open the welcome screen once.
        if engine.first_run && engine.snapshot.lock().unwrap().accounts.iter().all(|a| !a.connected) {
            tray::show(&app);
        }
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            engine.refresh(&app, false).await;
        }
    });
}

#[tauri::command]
pub fn get_snapshot(engine: State<'_, Arc<Engine>>) -> Snapshot {
    engine.snapshot.lock().unwrap().clone()
}

/// The UI's resolved language ("en" / "nl"), for notifications and the tray.
#[tauri::command]
pub fn set_language(app: AppHandle, engine: State<'_, Arc<Engine>>, lang: String) {
    let lang = if lang == "nl" { "nl".to_string() } else { "en".to_string() };
    *engine.lang.lock().unwrap() = lang.clone();
    tray::set_language(&app, &lang);
    tray::update(&app, &engine.snapshot.lock().unwrap().clone());
}

#[tauri::command]
pub async fn refresh_now(app: AppHandle, engine: State<'_, Arc<Engine>>) -> Result<Snapshot, String> {
    let engine = engine.inner().clone();
    engine.refresh(&app, true).await;
    let snapshot = engine.snapshot.lock().unwrap().clone();
    Ok(snapshot)
}
