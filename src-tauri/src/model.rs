//! Shapes shared by the readers, the engine and the UI (sent as JSON).

use serde::{Deserialize, Serialize};
use std::fmt;

/// One limit bar: a 5h session, a weekly window, or a per-model weekly cap.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Limit {
    pub label: String,
    /// "5h", "week" or None for other windows.
    pub window: Option<String>,
    /// Set when the limit only covers one model or feature (e.g. "Fable").
    pub model: Option<String>,
    pub percent: f64,
    /// Unix seconds.
    pub resets_at: Option<f64>,
    #[serde(default)]
    pub critical: bool,
}

/// A label/value line in the details view. tone: ok | warn | bad | dim | text.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Detail {
    pub label: String,
    pub text: String,
    pub tone: String,
    /// A time that belongs with the text ("unavailable" until when), Unix seconds.
    /// Sent raw so the UI formats it in the user's language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

impl Detail {
    pub fn new(label: &str, text: impl Into<String>, tone: &str) -> Self {
        Self { label: label.into(), text: text.into(), tone: tone.into(), at: None }
    }
}

/// Messages (or turns) and tokens per model, from the local CLI logs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub h5: [u64; 2],
    pub week: [u64; 2],
    pub blocked_at: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageTable {
    pub source: String,
    pub unit: String,
    pub token_unit: String,
    pub models: Vec<ModelUsage>,
}

/// What one reader returns.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reading {
    pub plan: String,
    pub limits: Vec<Limit>,
    pub details: Vec<Detail>,
    pub block_reason: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// "claude" or "codex".
    pub kind: String,
    pub name: String,
    #[serde(flatten)]
    pub reading: Reading,
    pub usage: Option<UsageTable>,
    pub error: Option<String>,
    /// False when no CLI login was found on this computer.
    #[serde(default = "yes")]
    pub connected: bool,
    pub fetched_at: Option<f64>,
    #[serde(default)]
    pub tried_at: f64,
    #[serde(default)]
    pub retry_at: f64,
    #[serde(default)]
    pub scanned_at: f64,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub accounts: Vec<Account>,
    pub updated_at: f64,
}

#[derive(Debug)]
pub enum FetchError {
    /// Readable failure: expired login, HTTP error, odd response.
    Usage(String),
    /// HTTP 429; seconds from Retry-After (0 if missing).
    RateLimited(u64),
    /// No login for this service on this computer.
    NotConnected(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Usage(m) | FetchError::NotConnected(m) => write!(f, "{m}"),
            FetchError::RateLimited(_) => write!(f, "rate limited"),
        }
    }
}
