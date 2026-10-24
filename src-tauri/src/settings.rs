//! User settings, saved as JSON in the app's config folder.

use crate::engine::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// "system" | "en" | "nl"
    pub language: String,
    /// The limit endpoints answer HTTP 429 if polled harder than every few minutes.
    pub refresh_minutes: u64,
    /// Warn when a limit reaches this %. 0 = off.
    pub warn_at: f64,
    pub notify_full: bool,
    pub notify_reset: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { theme: "system".into(), language: "system".into(), refresh_minutes: 5, warn_at: 80.0, notify_full: true, notify_reset: true }
    }
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

/// The saved settings, and whether this is the first run.
pub fn load(app: &AppHandle) -> (Settings, bool) {
    match path(app).and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => (serde_json::from_str(&text).unwrap_or_default(), false),
        None => (Settings::default(), true),
    }
}

pub fn save(app: &AppHandle, settings: &Settings) {
    if let Some(p) = path(app) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, serde_json::to_string_pretty(settings).unwrap_or_default());
    }
}

#[tauri::command]
pub fn get_settings(engine: State<'_, Arc<Engine>>) -> Settings {
    engine.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, engine: State<'_, Arc<Engine>>, mut settings: Settings) {
    if !["system", "light", "dark"].contains(&settings.theme.as_str()) {
        settings.theme = "system".into();
    }
    if !["system", "en", "nl"].contains(&settings.language.as_str()) {
        settings.language = "system".into();
    }
    settings.refresh_minutes = settings.refresh_minutes.clamp(3, 60);
    settings.warn_at = if settings.warn_at.is_finite() { settings.warn_at.clamp(0.0, 95.0) } else { 80.0 };
    save(&app, &settings);
    *engine.settings.lock().unwrap() = settings.clone();
    let _ = app.emit("settings", settings);
}
