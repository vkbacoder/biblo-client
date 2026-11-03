//! Background sync worker (task 53 & screenshot request integration).
//!
//! Pushes pending local rows (`synced = 0`) to the backend on a timer (~5 min)
//! and once shortly after app start. One pass per table, batched. On a 2xx the
//! backend echoes the accepted `client_uuid`s and we flip exactly those to
//! `synced = 1` — never deleting local rows (docs/11). Failures back off
//! exponentially so a down backend isn't hammered.
//!
//! Recurringly polls pending server-requested manual screenshot captures (~15s)
//! when authenticated, revalidates server policy, executes capture while
//! preserving privacy exclusions, and acknowledges completion/rejection status.
//!
//! Offline / logged-out / no backend → the pass is a no-op and rows stay pending,
//! which is the whole point of local-first.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::auth::AuthState;
use super::client::BackendClient;
use super::screenshot_policy::ScreenshotPolicyAdapter;
use super::BATCH_LIMIT;
use crate::policy::{ManualCaptureStatus, PolicyState, ScreenshotPolicyManager};
use crate::storage::{Db, SyncTable};
use crate::trackers::TrackerControl;

/// Base interval between sync passes (5 min). Backoff multiplies this on failure.
pub const BASE_INTERVAL: Duration = Duration::from_secs(300);
/// Cap the backoff so we still retry roughly every 16 min when the backend is down.
pub const MAX_INTERVAL: Duration = Duration::from_secs(16 * 60);
/// Short delay before the first pass so startup isn't blocked.
pub const STARTUP_DELAY: Duration = Duration::from_secs(10);
/// Base interval between pending screenshot capture request checks (15 seconds).
pub const REQUEST_POLL_INTERVAL: Duration = Duration::from_secs(15);
/// Cap the backoff for request polling when backend is unreachable (60 seconds).
pub const MAX_REQUEST_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Sync status surfaced to the UI / menu bar (task 53).
#[derive(Default)]
pub struct SyncStatus {
    /// Unix seconds of the last *successful* pass (0 = never).
    pub last_sync_ts: AtomicI64,
    /// Rows still pending after the last pass.
    pub pending: AtomicU64,
    /// Last error message, if the last pass failed (empty = ok).
    pub last_error: Mutex<String>,
}

impl SyncStatus {
    fn record_success(&self, pending: i64) {
        self.last_sync_ts
            .store(crate::now_unix(), Ordering::Relaxed);
        self.pending.store(pending.max(0) as u64, Ordering::Relaxed);
        *self.last_error.lock().unwrap() = String::new();
    }

    fn record_error(&self, msg: String, pending: i64) {
        self.pending.store(pending.max(0) as u64, Ordering::Relaxed);
        *self.last_error.lock().unwrap() = msg;
    }
}

/// Everything a sync pass needs. Cloneable handles shared with the worker thread.
#[derive(Clone)]
pub struct SyncContext {
    pub db: Arc<Db>,
    pub auth: Arc<AuthState>,
    pub status: Arc<SyncStatus>,
    /// Backend base URL + device_id are read fresh each pass so settings changes
    /// (and the first-run device_id) take effect without a restart.
    pub settings: Arc<crate::settings::SettingsState>,
    pub policy_manager: Arc<ScreenshotPolicyManager>,
    pub adapter: Arc<ScreenshotPolicyAdapter>,
    pub control: Arc<TrackerControl>,
    pub screenshots_dir: PathBuf,
    pub stop_signal: Arc<AtomicBool>,
}

impl SyncContext {
    pub fn new(
        db: Arc<Db>,
        auth: Arc<AuthState>,
        status: Arc<SyncStatus>,
        settings: Arc<crate::settings::SettingsState>,
        policy_manager: Arc<ScreenshotPolicyManager>,
        adapter: Arc<ScreenshotPolicyAdapter>,
        control: Arc<TrackerControl>,
        screenshots_dir: PathBuf,
    ) -> Self {
        Self {
            db,
            auth,
            status,
            settings,
            policy_manager,
            adapter,
            control,
            screenshots_dir,
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
    }
}

/// Spawn the worker on its own thread with a dedicated single-thread tokio runtime
/// (mirrors `server::start`). Returns immediately.
pub fn start(ctx: SyncContext) {
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                crate::log_warn!("sync", "runtime build failed: {e}");
                return;
            }
        };
        rt.block_on(run(ctx));
    });
}

async fn run(ctx: SyncContext) {
    tokio::time::sleep(STARTUP_DELAY).await;
    let mut sync_backoff = BASE_INTERVAL;
    let mut request_backoff = REQUEST_POLL_INTERVAL;

    // Trigger initial checks on first tick
    let mut last_sync = tokio::time::Instant::now() - BASE_INTERVAL;
    let mut last_request_poll = tokio::time::Instant::now() - REQUEST_POLL_INTERVAL;

    loop {
        if ctx.stop_signal.load(Ordering::Relaxed) {
            crate::log_info!("sync", "worker stop signal received; terminating run loop");
            break;
        }

        let now = tokio::time::Instant::now();

        // 1. Periodically fetch and process pending screenshot capture requests (~15s)
        if now.duration_since(last_request_poll) >= request_backoff {
            last_request_poll = now;
            match process_pending_capture_requests(&ctx).await {
                PassOutcome::Ok => {
                    request_backoff = REQUEST_POLL_INTERVAL;
                }
                PassOutcome::Skipped => {
                    request_backoff = REQUEST_POLL_INTERVAL;
                }
                PassOutcome::Failed => {
                    // Back off request polling on network / server down, up to MAX_REQUEST_POLL_INTERVAL
                    request_backoff = (request_backoff * 2).min(MAX_REQUEST_POLL_INTERVAL);
                }
            }
        }

        // 2. Periodically execute data sync batch pass (~5 min)
        let now = tokio::time::Instant::now();
        if now.duration_since(last_sync) >= sync_backoff {
            last_sync = now;
            match run_once(&ctx).await {
                PassOutcome::Ok => {
                    sync_backoff = BASE_INTERVAL;
                }
                PassOutcome::Skipped => {
                    sync_backoff = BASE_INTERVAL;
                }
                PassOutcome::Failed => {
                    sync_backoff = (sync_backoff * 2).min(MAX_INTERVAL);
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PassOutcome {
    Ok,
    Skipped,
    Failed,
}

/// Fetch and execute pending manual screenshot requests from the server.
/// Follows safe fail-closed policy validation, deduplication, and acknowledgment.
pub async fn process_pending_capture_requests(ctx: &SyncContext) -> PassOutcome {
    if !ctx.auth.is_logged_in() {
        return PassOutcome::Skipped;
    }

    let session = match ctx.auth.session() {
        Some(s) => s,
        None => return PassOutcome::Skipped,
    };
    let employee_id = session.resolved_employee_id();
    let base_url = crate::settings::backend_base_url();
    let device_id = ctx.settings.current.lock().unwrap().device_id.clone();
    if base_url.is_empty() || device_id.is_empty() || employee_id.is_empty() {
        return PassOutcome::Skipped;
    }

    let requests = match ctx
        .adapter
        .fetch_pending_capture_requests(&employee_id)
        .await
    {
        Ok(reqs) => reqs,
        Err(e) => {
            crate::log_warn!("sync", "failed to fetch pending screenshot requests: {e}");
            return PassOutcome::Failed;
        }
    };

    if requests.is_empty() {
        return PassOutcome::Ok;
    }

    let mut executed_any = false;

    for req in requests {
        // Prevent accidental capture after logout
        if !ctx.auth.is_logged_in() {
            crate::log_info!(
                "sync",
                "aborted pending capture request {}: logged out",
                req.request_id
            );
            break;
        }

        // Atomically prevent concurrent duplicate captures or repeated processing
        if !ctx.policy_manager.try_start_request(&req.request_id) {
            crate::log_info!(
                "sync",
                "request {} already processed or in-flight; skipping duplicate",
                req.request_id
            );
            continue;
        }

        let now = crate::now_unix();
        let current_emp = ctx
            .auth
            .session()
            .map(|s| s.resolved_employee_id())
            .unwrap_or_default();

        // Server policy refresh when required before a manual capture:
        // Do not trust a stale local policy when the server cannot confirm authorization.
        let eff = ctx.policy_manager.effective_policy();
        let needs_refresh = eff.state != PolicyState::Active
            || req.policy_version > eff.policy_version
            || !eff.capture_enabled
            || !eff.manual_capture_enabled;

        if needs_refresh {
            crate::log_info!(
                "sync",
                "refreshing server policy for request {} (local state: {:?}, v{}, req_v{})",
                req.request_id,
                eff.state,
                eff.policy_version,
                req.policy_version
            );
            let _ = ctx.adapter.sync_policy(&current_emp).await;
        }

        // Process capture: verifies expiry, employee identity, policy authorization,
        // enforces privacy exclusions, and generates real screenshot client_uuid.
        let result = crate::policy::process_manual_capture(
            req.clone(),
            &current_emp,
            &ctx.db,
            &ctx.screenshots_dir,
            &ctx.control,
            &ctx.policy_manager,
            now,
        );

        // Acknowledge the capture request status (both completed and failed/rejected) to the server
        if let Err(e) = ctx.adapter.ack_capture_request(&result).await {
            crate::log_warn!(
                "sync",
                "failed to acknowledge capture request {}: {e}",
                req.request_id
            );
        }

        if result.status == ManualCaptureStatus::Completed {
            executed_any = true;
        }
    }

    // Immediately upload pending screenshot multipart files so the backend receives the image
    if executed_any {
        let _ = sync_pending_screenshots(ctx).await;
    }

    PassOutcome::Ok
}

/// Upload pending screenshots to the backend via multipart upload.
pub async fn sync_pending_screenshots(ctx: &SyncContext) -> Result<(), String> {
    let base_url = ctx.adapter.base_url();
    let device_id = ctx.settings.current.lock().unwrap().device_id.clone();
    let business_id = ctx.auth.session().and_then(|s| s.business_id);
    if base_url.is_empty() || device_id.is_empty() || !ctx.auth.is_logged_in() {
        return Ok(());
    }

    let client = BackendClient::new(base_url.to_string(), ctx.auth.clone());
    match ctx.db.pending_screenshots(BATCH_LIMIT) {
        Ok(shots) => {
            for shot in shots {
                match client
                    .sync_screenshot(&device_id, business_id.as_deref(), &shot)
                    .await
                {
                    Ok(accepted) => {
                        let _ = ctx.db.mark_synced(SyncTable::Screenshot, &accepted);
                    }
                    Err(e) => {
                        crate::log_warn!(
                            "sync",
                            "screenshot upload halted on {}: {e}",
                            shot.client_uuid
                        );
                        return Err(e);
                    }
                }
            }
            Ok(())
        }
        Err(e) => Err(format!("db: {e}")),
    }
}

/// One full sync pass over all tables. Public so a command can trigger a manual
/// "sync now" later. Returns the outcome that drives the backoff.
pub async fn run_once(ctx: &SyncContext) -> PassOutcome {
    // Logged out → nothing to authenticate the push; stay pending.
    if !ctx.auth.is_logged_in() {
        return PassOutcome::Skipped;
    }

    let base_url = ctx.adapter.base_url();
    let device_id = ctx.settings.current.lock().unwrap().device_id.clone();
    // business_id is resolved at login and lives on the session.
    let business_id = ctx.auth.session().and_then(|s| s.business_id);
    if base_url.is_empty() || device_id.is_empty() {
        return PassOutcome::Skipped;
    }

    let client = BackendClient::new(base_url.to_string(), ctx.auth.clone());
    let mut failed = false;

    // --- JSON batch: activity + keystrokes + browser ---
    loop {
        let activity = match ctx.db.pending_activity(BATCH_LIMIT) {
            Ok(v) => v,
            Err(e) => {
                ctx.status
                    .record_error(format!("db: {e}"), pending_total(ctx));
                return PassOutcome::Failed;
            }
        };
        let keystrokes = ctx.db.pending_keystrokes(BATCH_LIMIT).unwrap_or_default();
        let browser = ctx.db.pending_browser(BATCH_LIMIT).unwrap_or_default();

        if activity.is_empty() && keystrokes.is_empty() && browser.is_empty() {
            break;
        }

        match client
            .sync_batch(
                &device_id,
                business_id.as_deref(),
                &activity,
                &keystrokes,
                &browser,
            )
            .await
        {
            Ok(accepted) => {
                let _ = ctx.db.mark_synced(SyncTable::Activity, &accepted.activity);
                let _ = ctx
                    .db
                    .mark_synced(SyncTable::Keystroke, &accepted.keystrokes);
                let _ = ctx.db.mark_synced(SyncTable::Browser, &accepted.browser);

                // If the backend accepted nothing, break to avoid an infinite loop
                // on a poison row; it'll be retried next pass.
                if accepted.activity.is_empty()
                    && accepted.keystrokes.is_empty()
                    && accepted.browser.is_empty()
                {
                    break;
                }
                // Full batches likely mean more pending — loop again. Partial → done.
                if activity.len() < BATCH_LIMIT as usize
                    && keystrokes.len() < BATCH_LIMIT as usize
                    && browser.len() < BATCH_LIMIT as usize
                {
                    break;
                }
            }
            Err(e) => {
                ctx.status.record_error(e, pending_total(ctx));
                failed = true;
                break;
            }
        }
    }

    // --- Screenshots: one multipart upload each ---
    if !failed {
        if let Err(e) = sync_pending_screenshots(ctx).await {
            ctx.status.record_error(e, pending_total(ctx));
            failed = true;
        }
    }

    // --- Check and execute any pending server capture requests ---
    if !failed {
        match process_pending_capture_requests(ctx).await {
            PassOutcome::Failed => {
                failed = true;
            }
            _ => {}
        }
    }

    let pending = pending_total(ctx);
    if failed {
        PassOutcome::Failed
    } else {
        ctx.status.record_success(pending);
        PassOutcome::Ok
    }
}

fn pending_total(ctx: &SyncContext) -> i64 {
    ctx.db.pending_count().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{ManualCaptureRequest, ManualCaptureResult, ServerScreenshotPolicy};
    use crate::settings::{CaptureManaged, Settings, SettingsState};
    use crate::sync::auth::Session;
    use axum::extract::{Path as AxumPath, Query};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;

    #[derive(Clone, Default)]
    struct MockState {
        pending_requests: Arc<Mutex<Vec<ManualCaptureRequest>>>,
        acks_received: Arc<Mutex<Vec<ManualCaptureResult>>>,
        server_policy: Arc<Mutex<Option<ServerScreenshotPolicy>>>,
        policy_fail: Arc<AtomicBool>,
        suspended: Arc<AtomicBool>,
        ack_count: Arc<AtomicUsize>,
    }

    async fn start_mock_backend() -> (String, MockState) {
        let state = MockState::default();
        let state_for_router = state.clone();

        let app = Router::new()
            .route(
                "/v1/screenshot-requests/pending",
                get({
                    let state = state_for_router.clone();
                    move |Query(_q): Query<HashMap<String, String>>| {
                        let state = state.clone();
                        async move {
                            let reqs = state.pending_requests.lock().unwrap().clone();
                            Json(json!({ "requests": reqs }))
                        }
                    }
                }),
            )
            .route(
                "/v1/screenshot-requests/:id/ack",
                post({
                    let state = state_for_router.clone();
                    move |AxumPath(_id): AxumPath<String>, Json(body): Json<ManualCaptureResult>| {
                        let state = state.clone();
                        async move {
                            state.acks_received.lock().unwrap().push(body);
                            state.ack_count.fetch_add(1, Ordering::SeqCst);
                            Json(json!({ "status": "acknowledged" }))
                        }
                    }
                }),
            )
            .route(
                "/v1/screenshot-policy",
                get({
                    let state = state_for_router.clone();
                    move |Query(_q): Query<HashMap<String, String>>| {
                        let state = state.clone();
                        async move {
                            if state.policy_fail.load(Ordering::SeqCst) {
                                return (
                                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(json!({ "error": "internal error" })),
                                );
                            }
                            let p = state.server_policy.lock().unwrap().clone().unwrap_or(
                                ServerScreenshotPolicy {
                                    capture_enabled: true,
                                    continuous_capture_enabled: true,
                                    capture_interval_minutes: 5,
                                    manual_capture_enabled: true,
                                    policy_version: 1,
                                    policy_updated_at: 1000,
                                    employee_id: "emp_alice".into(),
                                },
                            );
                            (axum::http::StatusCode::OK, Json(json!(p)))
                        }
                    }
                }),
            )
            .route(
                "/v1/screenshot-policy/ack",
                post(|| async { Json(json!({ "status": "acknowledged" })) }),
            )
            .route(
                "/v1/sync/screenshots",
                post({
                    let state = state_for_router.clone();
                    move |body: axum::body::Bytes| {
                        let state = state.clone();
                        async move {
                            if state.suspended.load(Ordering::SeqCst) {
                                (
                                    axum::http::StatusCode::FORBIDDEN,
                                    Json(json!({ "error": "membership_suspended" })),
                                )
                            } else {
                                let body_str = String::from_utf8_lossy(&body);
                                let uuid = if let Some(idx) = body_str.find("name=\"client_uuid\"") {
                                    let rest = &body_str[idx..];
                                    if let Some(val_start) = rest.find("\r\n\r\n") {
                                        let val = &rest[val_start + 4..];
                                        val.lines().next().unwrap_or("").trim().to_string()
                                    } else {
                                        "mock_uuid".to_string()
                                    }
                                } else {
                                    "mock_uuid".to_string()
                                };
                                (
                                    axum::http::StatusCode::OK,
                                    Json(json!({ "accepted": [uuid] })),
                                )
                            }
                        }
                    }
                }),
            )
            .route(
                "/v1/sync/batch",
                post({
                    let state = state_for_router.clone();
                    move || {
                        let state = state.clone();
                        async move {
                            if state.suspended.load(Ordering::SeqCst) {
                                (
                                    axum::http::StatusCode::FORBIDDEN,
                                    Json(json!({ "error": "membership_suspended" })),
                                )
                            } else {
                                (
                                    axum::http::StatusCode::OK,
                                    Json(json!({ "accepted": { "activity": [], "keystrokes": [], "browser": [] } })),
                                )
                            }
                        }
                    }
                }),
            )
            .route(
                "/v1/auth/refresh",
                post(|| async {
                    Json(json!({
                        "access_token": "mock_refreshed_tok",
                        "refresh_token": "mock_refreshed_ref",
                    }))
                }),
            );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{addr}"), state)
    }

    fn test_context(base_url: &str, employee_id: &str) -> (SyncContext, PathBuf) {
        let test_dir = std::env::temp_dir().join(format!(
            "worker_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&test_dir).unwrap();
        let shots_dir = test_dir.join("screenshots");
        std::fs::create_dir_all(&shots_dir).unwrap();

        let db = Arc::new(Db::open_in_memory().unwrap());
        let auth = Arc::new(AuthState::load(test_dir.join("session.json")));
        if !employee_id.is_empty() {
            let session = Session {
                access_token: "mock_tok".into(),
                refresh_token: "mock_ref".into(),
                email: "alice@company.com".into(),
                business_id: Some("biz_1".into()),
                display_name: "Alice".into(),
                username: "alice".into(),
                account_type: "employee".into(),
                role: "employee".into(),
                employee_id: Some(employee_id.into()),
            };
            auth.store(session).unwrap();
        }

        let policy_manager = Arc::new(ScreenshotPolicyManager::new(
            test_dir.join("policy_cache.json"),
        ));
        let control = Arc::new(TrackerControl::new());
        let adapter = Arc::new(
            ScreenshotPolicyAdapter::new(
                base_url.to_string(),
                auth.clone(),
                policy_manager.clone(),
            )
            .with_control(control.clone()),
        );

        let settings = Arc::new(SettingsState {
            path: test_dir.join("settings.json"),
            current: Mutex::new(Settings {
                device_id: "test_dev_01".into(),
                ..Default::default()
            }),
            managed: Mutex::new(CaptureManaged::default()),
        });

        let ctx = SyncContext::new(
            db,
            auth,
            Arc::new(SyncStatus::default()),
            settings,
            policy_manager,
            adapter,
            control,
            shots_dir,
        );

        (ctx, test_dir)
    }

    #[tokio::test]
    async fn test_worker_polls_and_acknowledges_request() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        // Set active policy in manager
        ctx.policy_manager
            .apply_server_policy(
                ServerScreenshotPolicy {
                    capture_enabled: true,
                    continuous_capture_enabled: true,
                    capture_interval_minutes: 5,
                    manual_capture_enabled: true,
                    policy_version: 1,
                    policy_updated_at: 1000,
                    employee_id: "emp_alice".into(),
                },
                Some(&ctx.control),
            )
            .unwrap();

        // Enqueue 1 pending request
        let req = ManualCaptureRequest {
            request_id: "req_test_01".into(),
            employee_id: "emp_alice".into(),
            policy_version: 1,
            expires_at: crate::now_unix() + 1000,
            created_at: crate::now_unix(),
            requested_by: Some("manager@company.com".into()),
        };
        state.pending_requests.lock().unwrap().push(req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        // Verified processed
        assert!(ctx.policy_manager.is_request_processed("req_test_01"));

        // Verified acknowledged to mock backend
        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].request_id, "req_test_01");
        assert_eq!(acks[0].employee_id, "emp_alice");

        // Second poll does not re-process or send duplicate ack
        drop(acks);
        let outcome2 = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome2, PassOutcome::Ok);
        assert_eq!(state.acks_received.lock().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_rejects_expired_request_and_acks() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        ctx.policy_manager
            .apply_server_policy(
                ServerScreenshotPolicy {
                    capture_enabled: true,
                    continuous_capture_enabled: true,
                    capture_interval_minutes: 5,
                    manual_capture_enabled: true,
                    policy_version: 1,
                    policy_updated_at: 1000,
                    employee_id: "emp_alice".into(),
                },
                None,
            )
            .unwrap();

        let expired_req = ManualCaptureRequest {
            request_id: "req_expired_01".into(),
            employee_id: "emp_alice".into(),
            policy_version: 1,
            expires_at: crate::now_unix() - 100, // expired
            created_at: crate::now_unix() - 200,
            requested_by: None,
        };
        state.pending_requests.lock().unwrap().push(expired_req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].request_id, "req_expired_01");
        assert_eq!(acks[0].status, ManualCaptureStatus::RejectedExpired);
        assert_eq!(acks[0].screenshot_uuid, None);
        assert!(ctx.policy_manager.is_request_processed("req_expired_01"));

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_rejects_wrong_employee_request_and_acks() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        ctx.policy_manager
            .apply_server_policy(
                ServerScreenshotPolicy {
                    capture_enabled: true,
                    continuous_capture_enabled: true,
                    capture_interval_minutes: 5,
                    manual_capture_enabled: true,
                    policy_version: 1,
                    policy_updated_at: 1000,
                    employee_id: "emp_alice".into(),
                },
                None,
            )
            .unwrap();

        let wrong_emp_req = ManualCaptureRequest {
            request_id: "req_wrong_emp_01".into(),
            employee_id: "emp_bob".into(), // does not match emp_alice
            policy_version: 1,
            expires_at: crate::now_unix() + 1000,
            created_at: crate::now_unix(),
            requested_by: None,
        };
        state.pending_requests.lock().unwrap().push(wrong_emp_req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].request_id, "req_wrong_emp_01");
        assert_eq!(acks[0].status, ManualCaptureStatus::RejectedWrongEmployee);
        assert_eq!(acks[0].screenshot_uuid, None);
        assert!(ctx.policy_manager.is_request_processed("req_wrong_emp_01"));

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_rejects_when_policy_revoked() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        // Master capture disabled
        let revoked_policy = ServerScreenshotPolicy {
            capture_enabled: false,
            continuous_capture_enabled: false,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_alice".into(),
        };
        *state.server_policy.lock().unwrap() = Some(revoked_policy.clone());
        ctx.policy_manager
            .apply_server_policy(revoked_policy, None)
            .unwrap();

        let req = ManualCaptureRequest {
            request_id: "req_disabled_01".into(),
            employee_id: "emp_alice".into(),
            policy_version: 1,
            expires_at: crate::now_unix() + 1000,
            created_at: crate::now_unix(),
            requested_by: None,
        };
        state.pending_requests.lock().unwrap().push(req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].status, ManualCaptureStatus::RejectedDisabled);
        assert_eq!(acks[0].screenshot_uuid, None);
        assert!(ctx.policy_manager.is_request_processed("req_disabled_01"));

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_rejects_when_manual_capture_disabled() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        // Manual capture disabled by policy
        let man_disabled_policy = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: false,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_alice".into(),
        };
        *state.server_policy.lock().unwrap() = Some(man_disabled_policy.clone());
        ctx.policy_manager
            .apply_server_policy(man_disabled_policy, None)
            .unwrap();

        let req = ManualCaptureRequest {
            request_id: "req_man_disabled_01".into(),
            employee_id: "emp_alice".into(),
            policy_version: 1,
            expires_at: crate::now_unix() + 1000,
            created_at: crate::now_unix(),
            requested_by: None,
        };
        state.pending_requests.lock().unwrap().push(req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].status, ManualCaptureStatus::RejectedManualDisabled);
        assert_eq!(acks[0].screenshot_uuid, None);
        assert!(ctx
            .policy_manager
            .is_request_processed("req_man_disabled_01"));

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_policy_refreshes_when_request_expects_newer_version() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        // Local policy is version 1
        ctx.policy_manager
            .apply_server_policy(
                ServerScreenshotPolicy {
                    capture_enabled: true,
                    continuous_capture_enabled: true,
                    capture_interval_minutes: 5,
                    manual_capture_enabled: true,
                    policy_version: 1,
                    policy_updated_at: 1000,
                    employee_id: "emp_alice".into(),
                },
                None,
            )
            .unwrap();

        // Server has version 2
        *state.server_policy.lock().unwrap() = Some(ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 2,
            policy_updated_at: 2000,
            employee_id: "emp_alice".into(),
        });

        // Request expects version 2
        let req = ManualCaptureRequest {
            request_id: "req_v2_01".into(),
            employee_id: "emp_alice".into(),
            policy_version: 2,
            expires_at: crate::now_unix() + 1000,
            created_at: crate::now_unix(),
            requested_by: None,
        };
        state.pending_requests.lock().unwrap().push(req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        // Effective policy version was refreshed to version 2!
        assert_eq!(ctx.policy_manager.effective_policy().policy_version, 2);

        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].request_id, "req_v2_01");

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_server_unavailable_fails_closed() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        // Local policy is version 1
        ctx.policy_manager
            .apply_server_policy(
                ServerScreenshotPolicy {
                    capture_enabled: true,
                    continuous_capture_enabled: true,
                    capture_interval_minutes: 5,
                    manual_capture_enabled: true,
                    policy_version: 1,
                    policy_updated_at: 1000,
                    employee_id: "emp_alice".into(),
                },
                None,
            )
            .unwrap();

        // Server returns 500 on policy sync
        state.policy_fail.store(true, Ordering::SeqCst);

        // Request expects version 2 (triggering required server refresh)
        let req = ManualCaptureRequest {
            request_id: "req_stale_fail_closed".into(),
            employee_id: "emp_alice".into(),
            policy_version: 2,
            expires_at: crate::now_unix() + 1000,
            created_at: crate::now_unix(),
            requested_by: None,
        };
        state.pending_requests.lock().unwrap().push(req);

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Ok);

        // Policy is now fail-closed (Unavailable)
        assert_eq!(ctx.policy_manager.state(), PolicyState::Unavailable);

        let acks = state.acks_received.lock().unwrap();
        assert_eq!(acks.len(), 1);
        assert_eq!(acks[0].status, ManualCaptureStatus::RejectedDisabled);
        assert_eq!(acks[0].screenshot_uuid, None);

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_polling_skipped_when_logged_out() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, ""); // empty employee id -> logged out

        let outcome = process_pending_capture_requests(&ctx).await;
        assert_eq!(outcome, PassOutcome::Skipped);
        assert_eq!(state.acks_received.lock().unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_worker_duplicate_and_in_flight_protection() {
        let (ctx, test_dir) = test_context("http://localhost:9999", "emp_alice");

        assert!(ctx.policy_manager.try_start_request("req_unique_1"));
        // Second call while in-flight returns false
        assert!(!ctx.policy_manager.try_start_request("req_unique_1"));

        ctx.policy_manager.mark_request_processed("req_unique_1");

        // Third call once processed returns false
        assert!(!ctx.policy_manager.try_start_request("req_unique_1"));
        assert!(ctx.policy_manager.is_request_processed("req_unique_1"));

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_worker_cancellation_and_stop_signal() {
        let (ctx, test_dir) = test_context("http://localhost:9999", "emp_alice");

        assert!(!ctx.stop_signal.load(Ordering::Relaxed));
        ctx.stop();
        assert!(ctx.stop_signal.load(Ordering::Relaxed));

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[tokio::test]
    async fn test_worker_suspended_employee_halts_screenshot_upload_and_preserves_local_data() {
        let (base_url, state) = start_mock_backend().await;
        let (ctx, test_dir) = test_context(&base_url, "emp_alice");

        // Create a local screenshot in DB
        let shot_path = ctx.screenshots_dir.join("test_shot.webp");
        std::fs::write(&shot_path, b"dummy webp content").unwrap();
        ctx.db
            .insert_screenshot(&crate::storage::Screenshot {
                ts: 1000,
                file_path: shot_path.to_string_lossy().to_string(),
                display_id: Some(0),
                width: Some(1920),
                height: Some(1080),
            })
            .unwrap();

        // 1. Employee is suspended -> server returns 403 Forbidden
        state.suspended.store(true, Ordering::SeqCst);

        // Uploading screenshots must fail and halt
        let upload_res = sync_pending_screenshots(&ctx).await;
        assert!(upload_res.is_err());
        let err_str = upload_res.unwrap_err();
        assert!(err_str.contains("403") || err_str.contains("suspended"));

        // Pending screenshot must be preserved locally (not deleted, not marked synced)
        let pending = ctx.db.pending_screenshots(10).unwrap();
        assert_eq!(pending.len(), 1);

        // run_once must also fail and record the suspended error in status
        let outcome = run_once(&ctx).await;
        assert_eq!(outcome, PassOutcome::Failed);
        let last_err = ctx.status.last_error.lock().unwrap().clone();
        assert!(last_err.contains("403") || last_err.contains("suspended"));

        // 2. Reactivated employee -> server returns 200 OK
        state.suspended.store(false, Ordering::SeqCst);

        // run_once succeeds and synchronizes pending data
        let outcome2 = run_once(&ctx).await;
        assert_eq!(outcome2, PassOutcome::Ok);
        assert_eq!(ctx.status.last_error.lock().unwrap().clone(), "");

        // Pending screenshot is now synced
        let pending_after = ctx.db.pending_screenshots(10).unwrap();
        assert_eq!(pending_after.len(), 0);

        let _ = std::fs::remove_dir_all(&test_dir);
    }
}
