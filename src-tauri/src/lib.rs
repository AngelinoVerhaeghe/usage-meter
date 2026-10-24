mod claude;
mod codex;
mod engine;
mod i18n;
mod logs;
mod model;
mod settings;
mod tray;
mod util;

use std::sync::Arc;
use tauri::{Manager, WindowEvent};

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// `usage-meter --print`: read every service once and print the numbers as JSON.
/// For troubleshooting a colleague's machine; no window, no tray.
pub fn print_report() {
    tauri::async_runtime::block_on(async {
        let client = util::http_client().expect("HTTP client");
        let now = util::now();
        let mut out = serde_json::Map::new();
        for (name, result) in [("claude", claude::fetch(&client).await), ("chatgpt", codex::fetch(&client).await)] {
            let value = match result {
                Ok(reading) => {
                    let usage = if name == "claude" {
                        logs::scan_claude(&reading.limits, now)
                    } else {
                        logs::scan_codex(&reading.limits, now)
                    };
                    serde_json::json!({ "status": util::status(&reading), "reading": reading, "usage": usage })
                }
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            };
            out.insert(name.into(), value);
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    });
}

pub fn run() {
    tauri::Builder::default()
        // A second launch (e.g. from Finder or the Start menu) opens the running app's popover.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Menu bar app: no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let engine = Arc::new(engine::Engine::load(app.handle())?);
            app.manage(engine.clone());
            tray::create(app.handle())?;
            engine::start(app.handle().clone(), engine);
            // USAGE_METER_SHOW=1 opens the popover at startup (for development).
            if std::env::var_os("USAGE_METER_SHOW").is_some() {
                tray::show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Behave like a popover: clicking elsewhere closes it.
            match event {
                WindowEvent::Focused(true) => tray::focused(),
                WindowEvent::Focused(false) => tray::blurred(window.app_handle()),
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            engine::get_snapshot,
            engine::refresh_now,
            engine::set_language,
            settings::get_settings,
            settings::set_settings,
            tray::hide_window,
            tray::fit_window,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running Usage Meter");
}
