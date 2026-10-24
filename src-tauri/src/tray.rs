//! Menu bar (macOS) / tray (Windows) icon and the popover window next to it.

use crate::engine::{Engine, STALE_AFTER};
use crate::i18n;
use crate::model::Snapshot;
use crate::util::now;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, WebviewWindow};

const TRAY_ID: &str = "main";
const WIDTH: f64 = 380.0;
const GAP: f64 = 6.0; // logical px between the icon and the popover

static LAST_HIDDEN_MS: AtomicU64 = AtomicU64::new(0);
/// Set once the popover has had focus since it was shown; only then does losing it close the popover.
static WAS_FOCUSED: AtomicBool = AtomicBool::new(false);
/// Where the tray icon is on screen (physical px: x, y, w, h), learned from its events.
static TRAY_RECT: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

fn now_ms() -> u64 {
    (now() * 1000.0) as u64
}

fn remember(app: &AppHandle, rect: &Rect) {
    let scale = app.get_webview_window("main").and_then(|w| w.scale_factor().ok()).unwrap_or(1.0);
    let p = rect.position.to_physical::<f64>(scale);
    let s = rect.size.to_physical::<f64>(scale);
    *TRAY_RECT.lock().unwrap() = Some((p.x, p.y, s.width, s.height));
}

fn lang(app: &AppHandle) -> String {
    app.state::<Arc<Engine>>().lang.lock().unwrap().clone()
}

fn build_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", i18n::text(lang, "open"), true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", i18n::text(lang, "refresh"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::text(lang, "quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &refresh, &quit])
}

/// Rebuild the right-click menu in another language.
pub fn set_language(app: &AppHandle, lang: &str) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app, lang)) {
        let _ = tray.set_menu(Some(menu));
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app, &lang(app))?;

    // macOS: a black "template" icon the system tints for light/dark menu bars.
    // Windows: a coloured icon that reads on both light and dark taskbars.
    #[cfg(target_os = "macos")]
    let icon = Image::from_bytes(include_bytes!("../icons/tray-mac.png"))?;
    #[cfg(not(target_os = "macos"))]
    let icon = Image::from_bytes(include_bytes!("../icons/tray-win.png"))?;

    let builder = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Usage Meter")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show(app),
            "refresh" => {
                let app = app.clone();
                let engine = app.state::<Arc<Engine>>().inner().clone();
                tauri::async_runtime::spawn(async move { engine.refresh(&app, true).await });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { rect, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => {
                remember(tray.app_handle(), &rect);
                toggle(tray.app_handle());
            }
            TrayIconEvent::Enter { rect, .. } | TrayIconEvent::Move { rect, .. } => remember(tray.app_handle(), &rect),
            _ => {}
        });
    #[cfg(target_os = "macos")]
    let builder = builder.icon_as_template(true);
    builder.build(app)?;
    Ok(())
}

/// Below the menu bar icon on macOS, above the tray icon on Windows (or below it
/// when the taskbar is at the top), kept on screen. Without a known icon
/// position: the top-right (macOS) or bottom-right (Windows) corner.
fn place(window: &WebviewWindow) {
    let (Ok(scale), Ok(size)) = (window.scale_factor(), window.outer_size()) else { return };
    let (w, h) = (size.width as f64, size.height as f64);
    let gap = GAP * scale;
    let tray = *TRAY_RECT.lock().unwrap();
    let monitor = match tray {
        Some((x, y, _, _)) => window.monitor_from_point(x, y).ok().flatten(),
        None => None,
    }
    .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let (mx, my) = (monitor.position().x as f64, monitor.position().y as f64);
    let (mw, mh) = (monitor.size().width as f64, monitor.size().height as f64);

    let (mut x, mut y) = match tray {
        Some((tx, ty, tw, th)) => {
            let x = tx + tw / 2.0 - w / 2.0;
            let below = ty + th + gap;
            let above = ty - h - gap;
            // Icon in the top half of the screen (macOS, or a top taskbar): open downwards.
            (x, if ty < my + mh / 2.0 { below } else { above })
        }
        None if cfg!(target_os = "macos") => (mx + mw - w - 12.0 * scale, my + 34.0 * scale),
        None => (mx + mw - w - 12.0 * scale, my + mh - h - 56.0 * scale),
    };
    x = x.clamp(mx + 8.0 * scale, mx + mw - w - 8.0 * scale);
    y = y.clamp(my, my + mh - h);
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

pub fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    if window.is_visible().unwrap_or(false) {
        hide(app);
    } else if now_ms().saturating_sub(LAST_HIDDEN_MS.load(Ordering::Relaxed)) > 300 {
        // Clicking the icon blurs the open popover first, which already hid it.
        show(app);
    }
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    WAS_FOCUSED.store(false, Ordering::Relaxed);
    place(&window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("shown", ());
}

pub fn focused() {
    WAS_FOCUSED.store(true, Ordering::Relaxed);
}

/// Behave like a popover: close when the user clicks somewhere else. macOS also
/// reports a brief focus loss while clicking inside or resizing the window, so
/// wait a moment and only close if focus is really gone.
pub fn blurred(app: &AppHandle) {
    if !WAS_FOCUSED.load(Ordering::Relaxed) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let Some(window) = app.get_webview_window("main") else { return };
        if window.is_visible().unwrap_or(false) && !window.is_focused().unwrap_or(true) {
            hide(&app);
        }
    });
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        LAST_HIDDEN_MS.store(now_ms(), Ordering::Relaxed);
    }
}

/// Menu bar text: the highest plan-wide limit. Tooltip: one line per service.
pub fn update(app: &AppHandle, snapshot: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let now = now();
    let mut worst: Option<f64> = None;
    let mut lines = vec!["Usage Meter".to_string()];
    for acc in &snapshot.accounts {
        let fresh = acc.fetched_at.is_some_and(|t| now - t <= STALE_AFTER);
        let general: Vec<f64> = acc.reading.limits.iter().filter(|l| l.model.is_none()).map(|l| l.percent).collect();
        match general.iter().cloned().reduce(f64::max) {
            Some(p) => {
                if fresh {
                    worst = Some(worst.map_or(p, |w: f64| w.max(p)));
                }
                lines.push(format!("{}: {p:.0}%", acc.name));
            }
            None if acc.connected => lines.push(format!("{}: –", acc.name)),
            None => lines.push(format!("{}: {}", acc.name, i18n::text(&lang(app), "not_connected"))),
        }
    }
    let title = worst.map(|w| format!("{w:.0}%")).unwrap_or_else(|| "–".into());
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(Some(title));
    #[cfg(not(target_os = "macos"))]
    let _ = title;
    let _ = tray.set_tooltip(Some(lines.join("\n")));
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    hide(&app);
}

/// The UI asks for the height its content needs; resize and stay next to the icon.
#[tauri::command]
pub fn fit_window(app: AppHandle, height: f64) {
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.set_size(LogicalSize::new(WIDTH, height.clamp(300.0, 720.0)));
    if window.is_visible().unwrap_or(false) {
        place(&window);
    }
}
