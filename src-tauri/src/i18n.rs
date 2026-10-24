//! The few texts the Rust side shows itself: notifications, the tray menu and
//! tooltip. Everything in the window is translated in src/i18n.ts.
//! The UI tells us the resolved language ("en" or "nl"), so "System" works the
//! same way on both sides.

use crate::model::Limit;
use chrono::{Datelike, Local, TimeZone};

pub fn is_nl(lang: &str) -> bool {
    lang == "nl"
}

pub fn text(lang: &str, key: &str) -> &'static str {
    let nl = is_nl(lang);
    match key {
        "open" => if nl { "Usage Meter openen" } else { "Open Usage Meter" },
        "refresh" => if nl { "Nu vernieuwen" } else { "Refresh now" },
        "quit" => if nl { "Stoppen" } else { "Quit" },
        "not_connected" => if nl { "niet verbonden" } else { "not connected" },
        "reset_body" => if nl { "Je limiet is gereset." } else { "Your limit has reset." },
        _ => "",
    }
}

/// "Weekly · Fable" / "Wekelijks · Fable", built from the limit's window and model.
pub fn limit_label(lim: &Limit, lang: &str) -> String {
    let nl = is_nl(lang);
    let base = match lim.window.as_deref() {
        Some("5h") => if nl { "5u-sessie" } else { "5h session" },
        Some("week") => if nl { "Wekelijks" } else { "Weekly" },
        _ => return lim.label.clone(),
    };
    match &lim.model {
        Some(m) => format!("{base} · {m}"),
        None => base.to_string(),
    }
}

/// Like util::fmt_reset, with Dutch day and month names when asked.
pub fn fmt_reset(ts: f64, lang: &str) -> String {
    if !is_nl(lang) {
        return crate::util::fmt_reset(ts);
    }
    let Some(when) = Local.timestamp_opt(((ts / 60.0).round() * 60.0) as i64, 0).single() else { return String::new() };
    let now = Local::now();
    const DAYS: [&str; 7] = ["ma", "di", "wo", "do", "vr", "za", "zo"];
    const MONTHS: [&str; 12] = ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"];
    if when.date_naive() == now.date_naive() {
        when.format("%H:%M").to_string()
    } else if (when - now).num_days().abs() < 6 {
        format!("{} {}", DAYS[when.weekday().num_days_from_monday() as usize], when.format("%H:%M"))
    } else {
        format!("{} {}", when.day(), MONTHS[when.month0() as usize])
    }
}
