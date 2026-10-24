// External product analytics disabled for the client build.

use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AnalyticsSession(pub String);

pub fn track_app_started(_locale: String, _session_id: String, _queue_dir: PathBuf) {}

pub fn track_event(
    _event_name: String,
    _locale: String,
    _session_id: String,
    _queue_dir: PathBuf,
    _props: Option<Value>,
) {}
