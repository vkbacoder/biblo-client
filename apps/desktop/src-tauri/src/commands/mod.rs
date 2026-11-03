//! Tauri commands — the bridge the web UI calls into.
//!
//! Queries over stored data (activity, screenshots, browser visits), permission
//! status, settings, pause/resume, and export. Filled in across later tasks.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::platform::{self, CapabilityRow, Permission};
use crate::policy::{
    EffectiveScreenshotPolicy, ManualCaptureRequest, ManualCaptureResult, ScreenshotPolicyManager,
    ServerScreenshotPolicy,
};
use crate::settings::{Settings, SettingsState};
use crate::storage::Db;
use crate::sync::auth::{AuthState, Session};
use crate::sync::ScreenshotPolicyAdapter;
use crate::trackers::TrackerControl;

/// Temporary smoke-test command kept from the scaffold; remove once real
/// commands exist (task 9+).
#[tauri::command]
pub fn ping() -> String {
    "ctracking: ok".to_string()
}

/// Pause or resume all tracking. Routed through the tray helper so the menu bar
/// indicator and the dashboard pill stay in sync.
#[tauri::command]
pub fn set_paused(paused: bool, app: tauri::AppHandle) {
    crate::tray::set_paused(&app, paused);
}

/// UI reports whether the user is still on the setup surfaces (welcome/login/
/// onboarding). Pauses tracking there and gates the tray Start item.
#[tauri::command]
pub fn set_in_setup(in_setup: bool, app: tauri::AppHandle) {
    crate::tray::set_in_setup(&app, in_setup);
}

#[tauri::command]
pub fn is_paused(control: State<Arc<TrackerControl>>) -> bool {
    control.paused.load(Ordering::Relaxed)
}

/// Current tracking state for the UI: "tracking" | "idle" | "paused".
#[tauri::command]
pub fn tracking_state(control: State<Arc<TrackerControl>>) -> String {
    if control.paused.load(Ordering::Relaxed) {
        "paused"
    } else if platform::idle_seconds() >= control.idle_threshold_s.load(Ordering::Relaxed) as f64 {
        "idle"
    } else {
        "tracking"
    }
    .to_string()
}

// ---------- permissions ----------

/// The setup/consent rows for the current OS (see docs/12 §2). macOS returns the 3
/// TCC rows with live grant/deny state; Windows returns capture/consent rows derived
/// from the user's opt-out settings. The React screen renders whatever it's given.
/// Cheap; the UI polls it.
#[tauri::command]
pub fn permissions_status(
    settings: State<Arc<crate::settings::SettingsState>>,
) -> Vec<CapabilityRow> {
    let s = settings.current.lock().unwrap().clone();
    platform::capability_rows(&s)
}

/// Open the System Settings pane for the given permission key
/// (`accessibility` | `input_monitoring` | `screen_recording`).
#[tauri::command]
pub fn open_permission_settings(which: String) -> Result<(), String> {
    let p = Permission::from_key(&which).ok_or_else(|| format!("unknown permission: {which}"))?;
    platform::open_settings(p);
    Ok(())
}

/// Trigger the first-time Screen Recording prompt (the only one requestable in-app).
#[tauri::command]
pub fn request_screen_recording() -> bool {
    platform::request_screen_recording()
}

/// Request Input Monitoring — registers the app in the list and prompts.
#[tauri::command]
pub fn request_input_monitoring() -> bool {
    platform::request_input_monitoring()
}

/// Request Accessibility — registers the app in the list and prompts.
#[tauri::command]
pub fn request_accessibility() -> bool {
    platform::request_accessibility()
}

// ---------- settings ----------

#[tauri::command]
pub fn get_settings(
    state: State<Arc<crate::settings::SettingsState>>,
) -> crate::settings::Settings {
    state.current.lock().unwrap().clone()
}

/// Persist the chosen UI locale and re-translate the native tray to match. Called
/// by the in-app language switcher so the menu-bar item follows the app's language.
#[tauri::command]
pub fn set_locale(
    locale: String,
    app: tauri::AppHandle,
    state: State<Arc<crate::settings::SettingsState>>,
) -> Result<(), String> {
    {
        let mut cur = state.current.lock().unwrap();
        cur.locale = locale;
        crate::settings::save(&state.path, &cur).map_err(err)?;
    }
    crate::tray::relabel(&app);
    Ok(())
}

/// Persist new settings and apply them live to the trackers + ingest server.
#[tauri::command]
pub fn set_settings(
    mut value: crate::settings::Settings,
    app: tauri::AppHandle,
    state: State<Arc<crate::settings::SettingsState>>,
    control: State<Arc<TrackerControl>>,
) -> Result<(), String> {
    // The UI's settings payload doesn't carry `locale` (it's owned by `set_locale`),
    // so preserve the persisted value instead of letting serde's default reset it.
    value.locale = state.current.lock().unwrap().locale.clone();
    // When the org controls capture settings, ignore changes to those fields —
    // the rest (theme, dock, etc.) still apply.
    if state.managed.lock().unwrap().locked() {
        let cur = state.current.lock().unwrap().clone();
        value.screenshot_interval_s = cur.screenshot_interval_s;
        value.idle_threshold_s = cur.idle_threshold_s;
        value.screenshot_retention_days = cur.screenshot_retention_days;
        value.screenshot_mode = cur.screenshot_mode;
        value.screenshot_skip_apps = cur.screenshot_skip_apps;
    }
    crate::settings::apply(&value, &control);
    crate::apply_dock_policy(&app, value.hide_dock);
    crate::settings::save(&state.path, &value).map_err(err)?;
    *state.current.lock().unwrap() = value;
    Ok(())
}

/// Fetch the org capture policy and, if it's locked (managed and override not
/// allowed), apply it to the live settings + trackers. Returns the managed status so
/// the UI can lock the corresponding controls. Standalone users keep local defaults.
#[tauri::command]
pub async fn apply_org_policy(
    settings: State<'_, Arc<crate::settings::SettingsState>>,
    auth: State<'_, Arc<AuthState>>,
    control: State<'_, Arc<TrackerControl>>,
) -> Result<crate::settings::CaptureManaged, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    let policy = client.fetch_policy().await?;

    let status = crate::settings::CaptureManaged {
        managed: policy.managed,
        allow_employee_override: policy.allow_employee_override,
        family: policy.kind.as_deref() == Some("family"),
    };
    *settings.managed.lock().unwrap() = status;

    if status.locked() {
        let mut s = settings.current.lock().unwrap().clone();
        if let Some(v) = policy.screenshot_interval_s {
            s.screenshot_interval_s = v;
        }
        if let Some(v) = policy.idle_threshold_s {
            s.idle_threshold_s = v;
        }
        // None retention = "keep forever" on the backend; leave the local value.
        if let Some(v) = policy.screenshot_retention_days {
            s.screenshot_retention_days = v;
        }
        if let Some(v) = policy.screenshot_mode {
            s.screenshot_mode = v;
        }
        if let Some(v) = policy.screenshot_skip_apps {
            s.screenshot_skip_apps = v;
        }
        crate::settings::apply(&s, &control);
        let _ = crate::settings::save(&settings.path, &s);
        *settings.current.lock().unwrap() = s;
    }
    Ok(status)
}

/// Current org capture-policy status for the UI (to lock/unlock the controls).
#[tauri::command]
pub fn capture_policy(
    settings: State<Arc<crate::settings::SettingsState>>,
) -> crate::settings::CaptureManaged {
    *settings.managed.lock().unwrap()
}

/// The curated sensitive-app list for the Settings UI (grouped by category):
/// skip-list suggestions and the privacy-mode prefill. Fetched from the backend
/// so the rules stay server-controlled; falls back to the baked-in copy offline.
#[tauri::command]
pub async fn privacy_apps() -> Result<Vec<crate::sync::client::PrivacyAppCategory>, String> {
    Ok(crate::fetch_privacy_apps_or_baked().await)
}

/// Browser ingest link info for Settings (active port + whether a token exists).
#[derive(Serialize)]
pub struct BrowserLinkInfo {
    pub port: Option<u16>,
    pub token_active: bool,
}

#[tauri::command]
pub fn browser_link(link: State<crate::server::BrowserLink>) -> BrowserLinkInfo {
    BrowserLinkInfo {
        port: link.port,
        token_active: !link.token.is_empty(),
    }
}

/// Determine if the current session and settings authorize full screenshot access.
///
/// Authorized internal monitoring roles:
/// - Users with role or account_type: `owner`, `manager`, `admin`
/// - Standalone local users (unless `client_mode` is enabled)
///
/// Restricted client / unauthorized roles:
/// - Any user with `client_mode` enabled in settings
/// - Users with role or account_type: `client`, `viewer`
/// - Standard employees in client-restricted context
pub fn is_screenshot_authorized(auth: &AuthState, settings: &Settings) -> bool {
    // If client mode is enabled in settings, force client-restricted view
    if settings.client_mode {
        return false;
    }

    if let Some(session) = auth.session() {
        let role = session.role.to_lowercase();
        let account_type = session.account_type.to_lowercase();

        // Explicit client or viewer roles are unauthorized
        if role == "client"
            || account_type == "client"
            || role == "viewer"
            || account_type == "viewer"
        {
            return false;
        }

        // Explicitly authorized internal monitoring roles (owner, manager, admin, team_lead)
        if role == "owner"
            || role == "manager"
            || role == "admin"
            || role == "team_lead"
            || role == "teamlead"
            || account_type == "owner"
            || account_type == "manager"
            || account_type == "admin"
            || account_type == "team_lead"
            || account_type == "teamlead"
        {
            return true;
        }

        // Standard employees are tracked subjects, not internal monitors
        false
    } else {
        // Standalone local user (no account): authorized unless client_mode is toggled on
        !settings.client_mode
    }
}

/// Capture a screenshot of every display right now. Returns how many were saved.
/// Restricted to authorized internal monitoring roles and gated by server policy.
#[tauri::command]
pub fn capture_now(
    app: tauri::AppHandle,
    db: State<Arc<Db>>,
    control: State<Arc<TrackerControl>>,
    auth: State<Arc<AuthState>>,
    settings: State<Arc<SettingsState>>,
    policy_manager: State<Arc<ScreenshotPolicyManager>>,
) -> Result<usize, String> {
    let s = settings.current.lock().unwrap().clone();
    if !is_screenshot_authorized(&auth, &s) {
        return Err("Access denied: Screenshot capture can only be commanded by authorized internal monitoring roles".into());
    }
    let eff = policy_manager.effective_policy();
    if !eff.capture_enabled {
        return Err("Screenshot capture is disabled by server policy".into());
    }
    if !eff.manual_capture_enabled {
        return Err("Manual screenshot capture is not permitted by server policy".into());
    }
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(err)?.join("screenshots");
    Ok(crate::trackers::capture_once(&db, &dir, &control))
}

// ---------- dashboard ----------

#[derive(Serialize)]
pub struct AppTotal {
    pub app_name: String,
    pub total_s: i64,
}

#[derive(Serialize)]
pub struct Seg {
    pub ts: i64,
    pub app_name: String,
    pub duration_s: i64,
}

#[derive(Serialize)]
pub struct DashboardData {
    pub total_active_s: i64,
    pub top_app: Option<String>,
    pub by_app: Vec<AppTotal>,
    pub timeline: Vec<Seg>,
    pub keypresses: i64,
    pub screenshots: i64,
}

/// Aggregated activity for the half-open window `[from_ts, to_ts)`.
#[tauri::command]
pub fn dashboard_data(
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
) -> Result<DashboardData, String> {
    let samples = db.activity_between(from_ts, to_ts).map_err(err)?;

    let mut by_app_map: HashMap<String, i64> = HashMap::new();
    let mut total_active_s = 0i64;
    let mut timeline = Vec::with_capacity(samples.len());
    for s in &samples {
        *by_app_map.entry(s.app_name.clone()).or_insert(0) += s.duration_s;
        total_active_s += s.duration_s;
        timeline.push(Seg {
            ts: s.ts,
            app_name: s.app_name.clone(),
            duration_s: s.duration_s,
        });
    }

    let mut by_app: Vec<AppTotal> = by_app_map
        .into_iter()
        .map(|(app_name, total_s)| AppTotal { app_name, total_s })
        .collect();
    by_app.sort_by(|a, b| b.total_s.cmp(&a.total_s));
    let top_app = by_app.first().map(|a| a.app_name.clone());

    let keypresses = db
        .keystrokes_between(from_ts, to_ts)
        .map_err(err)?
        .iter()
        .map(|(_, c)| *c)
        .sum();
    let screenshots = db.screenshots_between(from_ts, to_ts).map_err(err)?.len() as i64;

    Ok(DashboardData {
        total_active_s,
        top_app,
        by_app,
        timeline,
        keypresses,
        screenshots,
    })
}

/// Aggregated summary of screenshots for client-facing or unauthorized views.
/// Exposes total count and hourly buckets without revealing individual files or exact timestamps.
#[derive(Serialize)]
pub struct ScreenshotSummary {
    pub authorized: bool,
    pub total_count: i64,
    pub period_start: i64,
    pub period_end: i64,
    pub hourly_counts: Vec<(i64, i64)>,
}

/// Aggregated screenshot summary command. Always safe to call for both authorized
/// internal monitoring and client-facing views.
#[tauri::command]
pub fn screenshot_summary(
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
    auth: State<Arc<AuthState>>,
    settings: State<Arc<SettingsState>>,
) -> Result<ScreenshotSummary, String> {
    let s = settings.current.lock().unwrap().clone();
    let authorized = is_screenshot_authorized(&auth, &s);
    let rows = db.screenshots_between(from_ts, to_ts).map_err(err)?;
    let total_count = rows.len() as i64;

    let mut buckets: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for r in rows {
        let hour_bucket = (r.ts / 3600) * 3600;
        *buckets.entry(hour_bucket).or_insert(0) += 1;
    }
    let hourly_counts = buckets.into_iter().collect();

    Ok(ScreenshotSummary {
        authorized,
        total_count,
        period_start: from_ts,
        period_end: to_ts,
        hourly_counts,
    })
}

/// Screenshots captured in `[from_ts, to_ts)` (newest first), for the gallery.
/// Restricted to authorized internal monitoring roles. For clients or unauthorized
/// callers, returns an empty list, preventing access to image files and exact timestamps.
#[tauri::command]
pub fn screenshot_list(
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
    auth: State<Arc<AuthState>>,
    settings: State<Arc<SettingsState>>,
) -> Result<Vec<crate::storage::Screenshot>, String> {
    let s = settings.current.lock().unwrap().clone();
    if !is_screenshot_authorized(&auth, &s) {
        return Ok(Vec::new());
    }
    let mut rows = db.screenshots_between(from_ts, to_ts).map_err(err)?;
    rows.reverse(); // newest first
    Ok(rows)
}

/// Browser visits in `[from_ts, to_ts)` (newest first).
#[tauri::command]
pub fn browser_visits(
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
) -> Result<Vec<crate::storage::BrowserVisit>, String> {
    let mut rows = db.browser_visits_between(from_ts, to_ts).map_err(err)?;
    rows.reverse();
    Ok(rows)
}

/// Per-minute keystroke buckets `[ts_bucket, count]` in `[from_ts, to_ts)`.
#[tauri::command]
pub fn keystroke_buckets(
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
) -> Result<Vec<(i64, i64)>, String> {
    db.keystrokes_between(from_ts, to_ts).map_err(err)
}

#[derive(Serialize)]
struct KeystrokeRow {
    ts_bucket: i64,
    count: i64,
}

/// Export all tables to a single JSON document under `dir`, limited to
/// `[from_ts, to_ts)`. User-triggered only.
#[tauri::command]
pub fn export_json(
    dir: String,
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
    auth: State<Arc<AuthState>>,
    settings: State<Arc<SettingsState>>,
) -> Result<ExportSummary, String> {
    let s = settings.current.lock().unwrap().clone();
    let authorized = is_screenshot_authorized(&auth, &s);
    export_json_to_dir_filtered(&db, &dir, from_ts, to_ts, authorized)
}

/// Testable core of [`export_json`].
pub fn export_json_to_dir(
    db: &Db,
    dir: &str,
    from_ts: i64,
    to_ts: i64,
) -> Result<ExportSummary, String> {
    export_json_to_dir_filtered(db, dir, from_ts, to_ts, true)
}

pub fn export_json_to_dir_filtered(
    db: &Db,
    dir: &str,
    from_ts: i64,
    to_ts: i64,
    include_screenshots: bool,
) -> Result<ExportSummary, String> {
    use std::path::Path;
    let activity = db.activity_between(from_ts, to_ts).map_err(err)?;
    let keystrokes: Vec<KeystrokeRow> = db
        .keystrokes_between(from_ts, to_ts)
        .map_err(err)?
        .into_iter()
        .map(|(ts_bucket, count)| KeystrokeRow { ts_bucket, count })
        .collect();
    let screenshots = if include_screenshots {
        db.screenshots_between(from_ts, to_ts).map_err(err)?
    } else {
        Vec::new()
    };
    let visits = db.browser_visits_between(from_ts, to_ts).map_err(err)?;

    let rows = activity.len() + keystrokes.len() + screenshots.len() + visits.len();
    let doc = serde_json::json!({
        "activity_sample": serde_json::to_value(&activity).map_err(err)?,
        "keystroke_bucket": serde_json::to_value(&keystrokes).map_err(err)?,
        "screenshot": serde_json::to_value(&screenshots).map_err(err)?,
        "browser_visit": serde_json::to_value(&visits).map_err(err)?,
    });

    let path = Path::new(dir).join("employeetrack_export.json");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(err)?).map_err(err)?;

    Ok(ExportSummary {
        dir: dir.to_string(),
        files: vec![FileResult {
            name: "employeetrack_export.json".into(),
            rows,
        }],
    })
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------- auth / session (task 51) ----------

use crate::sync::client::BackendClient;

/// The backend base URL (compile-time default; env override for dev).
fn backend_url() -> String {
    crate::settings::backend_base_url()
}

/// The web signup wizard URL, opened in the system browser from the desktop
/// welcome/login screens. The web admin is served under `/admin` on the backend.
#[tauri::command]
pub fn signup_url() -> String {
    format!("{}/admin/signup", backend_url().trim_end_matches('/'))
}

/// The web admin dashboard URL. Opened in the system browser from the native
/// Admin section (workspace creation, employee detail). Served under `/admin`.
#[tauri::command]
pub fn admin_url() -> String {
    format!("{}/admin/", backend_url().trim_end_matches('/'))
}

/// Workspaces owned by the signed-in user (`GET /v1/businesses/mine`). Uses the
/// tracker's own session token; an empty list means the user isn't an owner, so
/// the Admin section shows the "no access" state.
#[tauri::command]
pub async fn admin_businesses(
    auth: State<'_, Arc<AuthState>>,
) -> Result<Vec<crate::sync::client::OwnerBusiness>, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.owner_businesses().await
}

/// Today's employee roster for an owned workspace (`GET /v1/reports/employees`).
/// `day_start` is the viewer's local midnight (unix seconds), so "today" matches
/// the per-employee detail views.
#[tauri::command]
pub async fn admin_roster(
    business_id: String,
    day_start: i64,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Vec<crate::sync::client::RosterEntry>, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.owner_roster(&business_id, day_start).await
}

// ---------- admin: per-employee detail reports ----------
// Owner-only drill-down for the native EmployeeDetail screen. All scoped
// server-side to businesses the caller owns; token comes from AuthState.

/// Timeline + per-app breakdown for one employee in `[from, to)` (unix seconds).
#[tauri::command]
pub async fn admin_employee_activity(
    employee_id: String,
    from: i64,
    to: i64,
    auth: State<'_, Arc<AuthState>>,
) -> Result<crate::sync::client::EmployeeActivity, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.owner_employee_activity(&employee_id, from, to).await
}

/// Keystroke count buckets for one employee in `[from, to)` (counts only).
#[tauri::command]
pub async fn admin_employee_keystrokes(
    employee_id: String,
    from: i64,
    to: i64,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Vec<crate::sync::client::KeystrokeBucketRow>, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client
        .owner_employee_keystrokes(&employee_id, from, to)
        .await
}

/// Browser page visits for one employee in `[from, to)`.
#[tauri::command]
pub async fn admin_employee_browser(
    employee_id: String,
    from: i64,
    to: i64,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Vec<crate::sync::client::BrowserVisitRow>, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.owner_employee_browser(&employee_id, from, to).await
}

/// Paginated screenshot metadata for one employee in `[from, to)` (unix seconds),
/// newest first from the backend.
#[tauri::command]
pub async fn admin_employee_screenshots(
    employee_id: String,
    from: i64,
    to: i64,
    limit: u32,
    offset: u32,
    auth: State<'_, Arc<AuthState>>,
) -> Result<crate::sync::client::ScreenshotPage, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client
        .owner_employee_screenshots(&employee_id, from, to, limit, offset)
        .await
}

/// One screenshot as a base64 `data:` URL, fetched with the owner's bearer token
/// (the image endpoint needs auth, so a plain `<img src>` can't reach it).
#[tauri::command]
pub async fn admin_screenshot_data(
    client_uuid: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<String, String> {
    use base64::Engine;
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    let (bytes, content_type) = client.owner_screenshot_bytes(&client_uuid).await?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{content_type};base64,{b64}"))
}

/// Create a new workspace (team/family) owned by the caller. Kind is decided
/// server-side from the owner's account type.
#[tauri::command]
pub async fn admin_create_business(
    name: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<crate::sync::client::OwnerBusiness, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.owner_create_business(&name).await
}

/// Create (pre-provision) a member in an owned workspace. Exactly one of
/// `email` / `username` is expected; the UI picks based on the entered login.
#[tauri::command]
pub async fn admin_create_employee(
    business_id: String,
    display_name: String,
    email: Option<String>,
    username: Option<String>,
    password: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<crate::sync::client::CreatedEmployee, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client
        .owner_create_employee(
            &business_id,
            &display_name,
            email.as_deref(),
            username.as_deref(),
            &password,
        )
        .await
}

/// In-app messages (ticket 145) for the signed-in user, resolved to `locale`.
/// Logged out / personal mode ⇒ none (only the bundled "What's new" applies then).
#[tauri::command]
pub async fn messages_fetch(
    platform: String,
    version: String,
    locale: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<serde_json::Value, String> {
    if auth.session().is_none() {
        return Ok(serde_json::json!({ "messages": [] }));
    }
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.fetch_messages(&platform, &version, &locale).await
}

/// Submit a survey answer for in-app message `id`.
#[tauri::command]
pub async fn message_respond(
    id: String,
    answers: serde_json::Value,
    platform: String,
    version: String,
    locale: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<(), String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    let body = serde_json::json!({
        "answers": answers, "platform": platform, "version": version, "locale": locale,
    });
    client.respond_message(&id, &body).await
}

/// Report a popup analytics event (shown / dismissed / later / cta / submitted).
/// No-op when logged out; best effort otherwise.
#[tauri::command]
pub async fn message_event(
    id: String,
    event: String,
    platform: String,
    version: String,
    locale: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<(), String> {
    if auth.session().is_none() {
        return Ok(());
    }
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    let body = serde_json::json!({
        "event": event, "platform": platform, "version": version, "locale": locale,
    });
    client.message_event(&id, &body).await
}

/// Log in and persist the session to disk. Wrong credentials surface a clear error
/// and store nothing. Synchronizes server screenshot policy upon successful login.
#[tauri::command]
pub async fn login(
    email: String,
    password: String,
    business_id: Option<String>,
    auth: State<'_, Arc<AuthState>>,
    _policy_manager: State<'_, Arc<ScreenshotPolicyManager>>,
    adapter: State<'_, Arc<ScreenshotPolicyAdapter>>,
) -> Result<Session, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    let session = client
        .login(&email, &password, business_id.as_deref())
        .await?;
    auth.store(session.clone())?;

    // Refresh policy for newly logged-in employee (transitions to fail-closed if endpoint unavailable)
    let emp_id = session.resolved_employee_id();
    let _ = adapter.sync_policy(&emp_id).await;

    Ok(session)
}

/// Clear the stored session (Keychain + memory) and invalidate server policy.
#[tauri::command]
pub fn logout(
    auth: State<Arc<AuthState>>,
    policy_manager: State<Arc<ScreenshotPolicyManager>>,
    control: State<Arc<TrackerControl>>,
) -> Result<(), String> {
    policy_manager.invalidate_on_logout(Some(&control));
    auth.clear()
}

/// The current session, or `None` when logged out. Drives the login UI.
#[tauri::command]
pub fn current_session(auth: State<Arc<AuthState>>) -> Option<Session> {
    auth.session()
}

// ---------- sync status (task 53) ----------

#[derive(Serialize)]
pub struct SyncStatusView {
    pub last_sync_ts: i64,
    pub pending: u64,
    pub last_error: String,
}

/// Last sync time, pending count, and last error for the UI / menu bar.
#[tauri::command]
pub fn sync_status(status: State<Arc<crate::sync::worker::SyncStatus>>) -> SyncStatusView {
    use std::sync::atomic::Ordering;
    SyncStatusView {
        last_sync_ts: status.last_sync_ts.load(Ordering::Relaxed),
        pending: status.pending.load(Ordering::Relaxed),
        last_error: status.last_error.lock().unwrap().clone(),
    }
}

// ---------- screenshot policy (server-controlled) ----------

/// Get the effective server screenshot policy status for UI and controllers.
#[tauri::command]
pub fn screenshot_policy_status(
    policy_manager: State<Arc<ScreenshotPolicyManager>>,
) -> EffectiveScreenshotPolicy {
    policy_manager.effective_policy()
}

/// Refresh the screenshot policy with the server.
#[tauri::command]
pub async fn screenshot_policy_refresh(
    adapter: State<'_, Arc<ScreenshotPolicyAdapter>>,
    auth: State<'_, Arc<AuthState>>,
) -> Result<EffectiveScreenshotPolicy, String> {
    let emp_id = auth
        .session()
        .map(|s| s.resolved_employee_id())
        .unwrap_or_default();
    Ok(adapter.sync_policy(&emp_id).await)
}

/// Process an authenticated manual capture request from the server.
#[tauri::command]
pub fn screenshot_manual_request(
    request: ManualCaptureRequest,
    app: tauri::AppHandle,
    db: State<Arc<Db>>,
    control: State<Arc<TrackerControl>>,
    auth: State<Arc<AuthState>>,
    policy_manager: State<Arc<ScreenshotPolicyManager>>,
) -> Result<ManualCaptureResult, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(err)?.join("screenshots");
    let current_emp = auth
        .session()
        .map(|s| s.resolved_employee_id())
        .unwrap_or_default();
    let now = crate::now_unix();
    let res = crate::policy::process_manual_capture(
        request,
        &current_emp,
        &db,
        &dir,
        &control,
        &policy_manager,
        now,
    );
    Ok(res)
}

/// Simulation command for testing server policy updates.
#[tauri::command]
pub fn screenshot_policy_simulate_update(
    policy: ServerScreenshotPolicy,
    control: State<Arc<TrackerControl>>,
    policy_manager: State<Arc<ScreenshotPolicyManager>>,
) -> Result<EffectiveScreenshotPolicy, String> {
    policy_manager.apply_server_policy(policy, Some(&control))
}

// ---------- export ----------

#[derive(Serialize)]
pub struct FileResult {
    pub name: String,
    pub rows: usize,
}

#[derive(Serialize)]
pub struct ExportSummary {
    pub dir: String,
    pub files: Vec<FileResult>,
}

fn opt_i(v: Option<i64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

/// Export every table to one CSV each under `dir`, limited to `[from_ts, to_ts)`.
/// User-triggered only — the sole path by which data leaves the machine.
#[tauri::command]
pub fn export_csv(
    dir: String,
    from_ts: i64,
    to_ts: i64,
    db: State<Arc<Db>>,
    auth: State<Arc<AuthState>>,
    settings: State<Arc<SettingsState>>,
) -> Result<ExportSummary, String> {
    let s = settings.current.lock().unwrap().clone();
    let authorized = is_screenshot_authorized(&auth, &s);
    export_to_dir_filtered(&db, &dir, from_ts, to_ts, authorized)
}

/// Testable core of [`export_csv`] — no Tauri state, just a DB + destination.
pub fn export_to_dir(
    db: &Db,
    dir: &str,
    from_ts: i64,
    to_ts: i64,
) -> Result<ExportSummary, String> {
    export_to_dir_filtered(db, dir, from_ts, to_ts, true)
}

pub fn export_to_dir_filtered(
    db: &Db,
    dir: &str,
    from_ts: i64,
    to_ts: i64,
    include_screenshots: bool,
) -> Result<ExportSummary, String> {
    use std::path::Path;
    let base = Path::new(dir);
    let mut files = Vec::new();

    // activity_sample
    {
        let rows = db.activity_between(from_ts, to_ts).map_err(err)?;
        let mut w = csv::Writer::from_path(base.join("activity_sample.csv")).map_err(err)?;
        w.write_record(["ts", "app_name", "window_title", "pid", "duration_s"])
            .map_err(err)?;
        for r in &rows {
            w.write_record([
                r.ts.to_string(),
                r.app_name.clone(),
                r.window_title.clone().unwrap_or_default(),
                opt_i(r.pid),
                r.duration_s.to_string(),
            ])
            .map_err(err)?;
        }
        w.flush().map_err(err)?;
        files.push(FileResult {
            name: "activity_sample.csv".into(),
            rows: rows.len(),
        });
    }

    // keystroke_bucket
    {
        let rows = db.keystrokes_between(from_ts, to_ts).map_err(err)?;
        let mut w = csv::Writer::from_path(base.join("keystroke_bucket.csv")).map_err(err)?;
        w.write_record(["ts_bucket", "count"]).map_err(err)?;
        for (ts_bucket, count) in &rows {
            w.write_record([ts_bucket.to_string(), count.to_string()])
                .map_err(err)?;
        }
        w.flush().map_err(err)?;
        files.push(FileResult {
            name: "keystroke_bucket.csv".into(),
            rows: rows.len(),
        });
    }

    // screenshot
    if include_screenshots {
        let rows = db.screenshots_between(from_ts, to_ts).map_err(err)?;
        let mut w = csv::Writer::from_path(base.join("screenshot.csv")).map_err(err)?;
        w.write_record(["ts", "file_path", "display_id", "width", "height"])
            .map_err(err)?;
        for r in &rows {
            w.write_record([
                r.ts.to_string(),
                r.file_path.clone(),
                opt_i(r.display_id),
                opt_i(r.width),
                opt_i(r.height),
            ])
            .map_err(err)?;
        }
        w.flush().map_err(err)?;
        files.push(FileResult {
            name: "screenshot.csv".into(),
            rows: rows.len(),
        });
    }

    // browser_visit
    {
        let rows = db.browser_visits_between(from_ts, to_ts).map_err(err)?;
        let mut w = csv::Writer::from_path(base.join("browser_visit.csv")).map_err(err)?;
        w.write_record(["ts", "url", "page_title", "browser", "duration_s"])
            .map_err(err)?;
        for r in &rows {
            w.write_record([
                r.ts.to_string(),
                r.url.clone(),
                r.page_title.clone().unwrap_or_default(),
                r.browser.clone().unwrap_or_default(),
                r.duration_s.to_string(),
            ])
            .map_err(err)?;
        }
        w.flush().map_err(err)?;
        files.push(FileResult {
            name: "browser_visit.csv".into(),
            rows: rows.len(),
        });
    }

    Ok(ExportSummary {
        dir: dir.to_string(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::ActivitySample;

    #[test]
    fn export_quotes_tricky_fields() {
        let db = Db::open_in_memory().unwrap();
        // A window title with comma, quote, newline, and emoji.
        let nasty = "a, \"b\"\nc 🚀";
        db.insert_activity_sample(&ActivitySample {
            ts: 100,
            app_name: "Code".into(),
            window_title: Some(nasty.into()),
            pid: Some(7),
            duration_s: 12,
        })
        .unwrap();

        let dir =
            std::env::temp_dir().join(format!("ctracking_export_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let summary = export_to_dir(&db, dir.to_str().unwrap(), 0, i64::MAX).unwrap();
        assert_eq!(summary.files.len(), 4);

        // Read activity_sample.csv back with a CSV parser — escaping must round-trip.
        let mut rdr = csv::Reader::from_path(dir.join("activity_sample.csv")).unwrap();
        let rec = rdr.records().next().unwrap().unwrap();
        assert_eq!(&rec[1], "Code");
        assert_eq!(&rec[2], nasty); // exact field preserved through quoting
        assert_eq!(&rec[4], "12");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn json_export_is_valid_and_complete() {
        let db = Db::open_in_memory().unwrap();
        db.insert_activity_sample(&ActivitySample {
            ts: 100,
            app_name: "Code".into(),
            window_title: None,
            pid: None,
            duration_s: 5,
        })
        .unwrap();
        db.add_keystrokes(60, 9).unwrap();

        let dir = std::env::temp_dir().join(format!("ctracking_json_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        export_json_to_dir(&db, dir.to_str().unwrap(), 0, i64::MAX).unwrap();

        let text = std::fs::read_to_string(dir.join("employeetrack_export.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["activity_sample"][0]["app_name"], "Code");
        assert_eq!(v["keystroke_bucket"][0]["count"], 9);
        assert!(v["screenshot"].is_array() && v["browser_visit"].is_array());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn screenshot_authorization_roles() {
        let dir = std::env::temp_dir().join(format!("auth_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let auth = AuthState::load(dir.join("session.json"));
        let mut settings = Settings::default();

        // 1. Standalone user, client_mode = false -> authorized
        settings.client_mode = false;
        assert!(is_screenshot_authorized(&auth, &settings));

        // 2. Standalone user, client_mode = true -> unauthorized
        settings.client_mode = true;
        assert!(!is_screenshot_authorized(&auth, &settings));

        settings.client_mode = false;

        // 3. Client role -> unauthorized
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "client@example.com".into(),
            business_id: None,
            display_name: "Client".into(),
            username: "client".into(),
            account_type: "client".into(),
            role: "client".into(),
            employee_id: None,
        });
        assert!(!is_screenshot_authorized(&auth, &settings));

        // 4. Employee role -> unauthorized
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "emp@example.com".into(),
            business_id: None,
            display_name: "Employee".into(),
            username: "emp".into(),
            account_type: "employee".into(),
            role: "employee".into(),
            employee_id: None,
        });
        assert!(!is_screenshot_authorized(&auth, &settings));

        // 5. Owner role -> authorized
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "owner@example.com".into(),
            business_id: None,
            display_name: "Owner".into(),
            username: "owner".into(),
            account_type: "manager".into(),
            role: "owner".into(),
            employee_id: None,
        });
        assert!(is_screenshot_authorized(&auth, &settings));

        // 6. Team Lead role -> authorized
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "teamlead@example.com".into(),
            business_id: None,
            display_name: "Team Lead".into(),
            username: "teamlead".into(),
            account_type: "team_lead".into(),
            role: "team_lead".into(),
            employee_id: Some("emp_identity_01".into()),
        });
        settings.client_mode = false;
        assert!(is_screenshot_authorized(&auth, &settings));

        // 7. Promotion and Demotion preserves identity while updating role authorization:
        // Start as employee -> unauthorized
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "user@example.com".into(),
            business_id: Some("biz_123".into()),
            display_name: "User One".into(),
            username: "userone".into(),
            account_type: "employee".into(),
            role: "employee".into(),
            employee_id: Some("emp_identity_01".into()),
        });
        assert!(!is_screenshot_authorized(&auth, &settings));
        let sess1 = auth.session().unwrap();
        assert_eq!(sess1.resolved_employee_id(), "emp_identity_01");

        // Promote to team_lead -> authorized, identity preserved
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "user@example.com".into(),
            business_id: Some("biz_123".into()),
            display_name: "User One".into(),
            username: "userone".into(),
            account_type: "team_lead".into(),
            role: "team_lead".into(),
            employee_id: Some("emp_identity_01".into()),
        });
        assert!(is_screenshot_authorized(&auth, &settings));
        let sess2 = auth.session().unwrap();
        assert_eq!(sess2.resolved_employee_id(), "emp_identity_01");

        // Demote back to employee -> unauthorized, identity preserved
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "user@example.com".into(),
            business_id: Some("biz_123".into()),
            display_name: "User One".into(),
            username: "userone".into(),
            account_type: "employee".into(),
            role: "employee".into(),
            employee_id: Some("emp_identity_01".into()),
        });
        assert!(!is_screenshot_authorized(&auth, &settings));
        let sess3 = auth.session().unwrap();
        assert_eq!(sess3.resolved_employee_id(), "emp_identity_01");

        // 8. Owner with client_mode turned on -> unauthorized (restricted view active)
        let _ = auth.store(Session {
            access_token: "tok".into(),
            refresh_token: "ref".into(),
            email: "owner@example.com".into(),
            business_id: None,
            display_name: "Owner".into(),
            username: "owner".into(),
            account_type: "manager".into(),
            role: "owner".into(),
            employee_id: None,
        });
        settings.client_mode = true;
        assert!(!is_screenshot_authorized(&auth, &settings));

        let _ = auth.clear();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_filters_screenshots_when_unauthorized() {
        let db = Db::open_in_memory().unwrap();
        db.insert_screenshot(&crate::storage::Screenshot {
            ts: 1000,
            file_path: "/path/to/shot.webp".into(),
            display_id: Some(1),
            width: Some(1920),
            height: Some(1080),
        })
        .unwrap();

        let dir = std::env::temp_dir().join(format!("filter_export_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Filtered JSON export (unauthorized: include_screenshots = false)
        export_json_to_dir_filtered(&db, dir.to_str().unwrap(), 0, i64::MAX, false).unwrap();
        let text = std::fs::read_to_string(dir.join("employeetrack_export.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["screenshot"].as_array().unwrap().len(), 0);

        // Filtered CSV export (unauthorized: include_screenshots = false)
        let summary =
            export_to_dir_filtered(&db, dir.to_str().unwrap(), 0, i64::MAX, false).unwrap();
        assert!(!summary.files.iter().any(|f| f.name == "screenshot.csv"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
