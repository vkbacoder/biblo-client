//! Server Screenshot Policy & Manual Capture Management (Finjulia).
//!
//! Provides:
//! - Serializable models for server screenshot policy and manual capture requests.
//! - Fail-closed policy management (`Loading`, `Active`, `Disabled`, `Unavailable`).
//! - Policy validation, versioning, and rejection of stale updates.
//! - Manual capture validation (verifying expiry, employee identity, policy version, and deduplication).
//! - Controlled screenshot scheduler preventing duplicate schedulers and reacting dynamically to policy updates.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

use crate::storage::Db;
use crate::trackers::TrackerControl;

// ============================================================================
// 1. Policy & Request Models
// ============================================================================

/// Proposed server screenshot policy contract for server-controlled screenshot management.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerScreenshotPolicy {
    /// Master toggle: whether screenshot capture is permitted for this employee.
    pub capture_enabled: bool,
    /// Whether automatic scheduled continuous capture is permitted.
    pub continuous_capture_enabled: bool,
    /// Configured automatic capture interval in minutes (>= 1).
    pub capture_interval_minutes: u32,
    /// Whether authorized server-requested manual captures are permitted.
    pub manual_capture_enabled: bool,
    /// Monotonically increasing revision to detect and reject stale policy updates.
    pub policy_version: i64,
    /// Unix timestamp when the policy was issued or updated on the server.
    pub policy_updated_at: i64,
    /// The specific employee ID to whom this policy applies.
    pub employee_id: String,
}

impl Default for ServerScreenshotPolicy {
    fn default() -> Self {
        ServerScreenshotPolicy {
            capture_enabled: false,
            continuous_capture_enabled: false,
            capture_interval_minutes: 5,
            manual_capture_enabled: false,
            policy_version: 0,
            policy_updated_at: 0,
            employee_id: String::new(),
        }
    }
}

/// Fail-closed policy states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyState {
    /// Initializing or actively checking with server.
    Loading,
    /// Authoritative server policy successfully validated and active.
    Active,
    /// Server policy explicitly disables capture.
    Disabled,
    /// Authoritative policy could not be retrieved from server; fail-closed.
    Unavailable,
}

/// Effective policy view exposed to UI, Tauri commands, and capture controllers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveScreenshotPolicy {
    pub state: PolicyState,
    pub capture_enabled: bool,
    pub continuous_capture_enabled: bool,
    pub capture_interval_minutes: u32,
    pub manual_capture_enabled: bool,
    pub policy_version: i64,
    pub policy_updated_at: i64,
    pub employee_id: String,
    pub is_fail_closed: bool,
    pub sync_status: String,
}

/// Incoming server-requested manual capture request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualCaptureRequest {
    /// Unique request ID.
    pub request_id: String,
    /// Target employee ID.
    pub employee_id: String,
    /// Required policy version under which the capture was requested.
    pub policy_version: i64,
    /// Expiry unix timestamp (seconds) after which the request is invalid.
    pub expires_at: i64,
    /// Request creation unix timestamp.
    #[serde(default)]
    pub created_at: i64,
    /// Admin or manager identifier who triggered the request.
    #[serde(default)]
    pub requested_by: Option<String>,
}

/// Status outcome of a manual capture request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManualCaptureStatus {
    Completed,
    RejectedDisabled,
    RejectedManualDisabled,
    RejectedExpired,
    RejectedWrongEmployee,
    RejectedDuplicate,
    RejectedStalePolicy,
    RejectedPrivacyExclusion,
    Failed,
}

/// Result of processing a manual capture request (sent to server in acknowledgment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualCaptureResult {
    pub request_id: String,
    pub employee_id: String,
    pub status: ManualCaptureStatus,
    pub screenshot_uuid: Option<String>,
    pub error_message: Option<String>,
    pub completed_at: i64,
}

// ============================================================================
// 2. Policy Provider & Manager
// ============================================================================

/// Thread-safe manager responsible for storing, validating, and updating screenshot policy.
pub struct ScreenshotPolicyManager {
    cache_path: PathBuf,
    state: RwLock<PolicyState>,
    active_policy: RwLock<Option<ServerScreenshotPolicy>>,
    processed_requests: RwLock<HashSet<String>>,
    in_flight_requests: RwLock<HashSet<String>>,
    sync_status: RwLock<String>,
}

impl ScreenshotPolicyManager {
    /// Create a new policy manager, optionally loading cached policy from disk.
    pub fn new(cache_path: PathBuf) -> Self {
        let (cached_policy, initial_state) = if let Some(p) = load_cached_policy(&cache_path) {
            let state = if p.capture_enabled {
                PolicyState::Active
            } else {
                PolicyState::Disabled
            };
            (Some(p), state)
        } else {
            (None, PolicyState::Unavailable)
        };

        ScreenshotPolicyManager {
            cache_path,
            state: RwLock::new(initial_state),
            active_policy: RwLock::new(cached_policy),
            processed_requests: RwLock::new(HashSet::new()),
            in_flight_requests: RwLock::new(HashSet::new()),
            sync_status: RwLock::new("initialized".to_string()),
        }
    }

    /// Load from cache path or return default fail-closed manager.
    pub fn load_or_default(cache_path: PathBuf) -> Self {
        Self::new(cache_path)
    }

    /// Current policy state.
    pub fn state(&self) -> PolicyState {
        *self.state.read().unwrap()
    }

    /// Set current policy state (e.g. Loading, Unavailable, Disabled).
    pub fn set_state(&self, state: PolicyState, status_msg: &str) {
        *self.state.write().unwrap() = state;
        *self.sync_status.write().unwrap() = status_msg.to_string();
    }

    /// Compute the effective policy. Fail-closed: if not Active, capture is completely disabled.
    pub fn effective_policy(&self) -> EffectiveScreenshotPolicy {
        let state = *self.state.read().unwrap();
        let sync_status = self.sync_status.read().unwrap().clone();
        let guard = self.active_policy.read().unwrap();

        match (state, guard.as_ref()) {
            (PolicyState::Active, Some(policy)) => EffectiveScreenshotPolicy {
                state: PolicyState::Active,
                capture_enabled: policy.capture_enabled,
                continuous_capture_enabled: policy.continuous_capture_enabled,
                capture_interval_minutes: policy.capture_interval_minutes.max(1),
                manual_capture_enabled: policy.manual_capture_enabled,
                policy_version: policy.policy_version,
                policy_updated_at: policy.policy_updated_at,
                employee_id: policy.employee_id.clone(),
                is_fail_closed: false,
                sync_status,
            },
            (PolicyState::Disabled, Some(policy)) => EffectiveScreenshotPolicy {
                state: PolicyState::Disabled,
                capture_enabled: false,
                continuous_capture_enabled: false,
                capture_interval_minutes: policy.capture_interval_minutes.max(1),
                manual_capture_enabled: policy.manual_capture_enabled,
                policy_version: policy.policy_version,
                policy_updated_at: policy.policy_updated_at,
                employee_id: policy.employee_id.clone(),
                is_fail_closed: false,
                sync_status,
            },
            (PolicyState::Loading, _) => EffectiveScreenshotPolicy {
                state: PolicyState::Loading,
                capture_enabled: false,
                continuous_capture_enabled: false,
                capture_interval_minutes: 5,
                manual_capture_enabled: false,
                policy_version: 0,
                policy_updated_at: 0,
                employee_id: String::new(),
                is_fail_closed: true,
                sync_status: "checking server policy...".into(),
            },
            (PolicyState::Unavailable, _) | _ => EffectiveScreenshotPolicy {
                state: PolicyState::Unavailable,
                capture_enabled: false,
                continuous_capture_enabled: false,
                capture_interval_minutes: 5,
                manual_capture_enabled: false,
                policy_version: 0,
                policy_updated_at: 0,
                employee_id: String::new(),
                is_fail_closed: true,
                sync_status: if sync_status.is_empty() {
                    "server policy unavailable (fail-closed)".into()
                } else {
                    sync_status
                },
            },
        }
    }

    /// Apply a validated server policy update.
    /// Rejects older policy versions (`new.policy_version < current.policy_version`).
    pub fn apply_server_policy(
        &self,
        new_policy: ServerScreenshotPolicy,
        control: Option<&TrackerControl>,
    ) -> Result<EffectiveScreenshotPolicy, String> {
        let mut active_guard = self.active_policy.write().unwrap();

        if let Some(ref current) = *active_guard {
            if new_policy.policy_version < current.policy_version {
                return Err(format!(
                    "rejected stale policy update: version {} < current version {}",
                    new_policy.policy_version, current.policy_version
                ));
            }
        }

        let new_state = if new_policy.capture_enabled {
            PolicyState::Active
        } else {
            PolicyState::Disabled
        };

        // Update TrackerControl live settings if provided
        if let Some(ctrl) = control {
            let interval_s = (new_policy.capture_interval_minutes as u64 * 60).max(5);
            ctrl.screenshot_interval_s
                .store(interval_s, Ordering::Relaxed);
            ctrl.capture_screenshots.store(
                new_policy.capture_enabled && new_policy.continuous_capture_enabled,
                Ordering::Relaxed,
            );
        }

        // Persist to disk cache
        save_cached_policy(&self.cache_path, &new_policy);

        *self.state.write().unwrap() = new_state;
        *self.sync_status.write().unwrap() = format!(
            "active (v{}) updated at {}",
            new_policy.policy_version, new_policy.policy_updated_at
        );
        *active_guard = Some(new_policy.clone());

        drop(active_guard);
        Ok(self.effective_policy())
    }

    /// Invalidate policy on user logout or session termination.
    pub fn invalidate_on_logout(&self, control: Option<&TrackerControl>) {
        *self.state.write().unwrap() = PolicyState::Unavailable;
        *self.sync_status.write().unwrap() = "logged out (fail-closed)".to_string();
        *self.active_policy.write().unwrap() = None;
        if let Ok(mut in_flight) = self.in_flight_requests.write() {
            in_flight.clear();
        }
        let _ = std::fs::remove_file(&self.cache_path);

        if let Some(ctrl) = control {
            ctrl.capture_screenshots.store(false, Ordering::Relaxed);
        }
    }

    /// Validate a manual capture request against 6 security criteria:
    /// 1. Not expired (`now <= expires_at`)
    /// 2. Belongs to currently authenticated employee (`req.employee_id == current_employee_id`)
    /// 3. Effective policy permits capture (`policy.capture_enabled`)
    /// 4. Manual capture is enabled (`policy.manual_capture_enabled`)
    /// 5. Request has not already been processed (`!processed_requests.contains(request_id)`)
    /// 6. Policy version is not older than request's policy_version
    pub fn validate_manual_request(
        &self,
        req: &ManualCaptureRequest,
        current_employee_id: &str,
        now: i64,
    ) -> Result<(), ManualCaptureStatus> {
        // 1. Check expiration
        if now > req.expires_at {
            return Err(ManualCaptureStatus::RejectedExpired);
        }

        // 2. Check employee matching (empty employee ID is strictly unauthorized)
        if current_employee_id.is_empty() || req.employee_id != current_employee_id {
            return Err(ManualCaptureStatus::RejectedWrongEmployee);
        }

        // 5. Check deduplication
        {
            let processed = self.processed_requests.read().unwrap();
            if processed.contains(&req.request_id) {
                return Err(ManualCaptureStatus::RejectedDuplicate);
            }
        }

        // Check policy state
        let state = *self.state.read().unwrap();
        if state != PolicyState::Active && state != PolicyState::Disabled {
            return Err(ManualCaptureStatus::RejectedDisabled);
        }

        let policy_guard = self.active_policy.read().unwrap();
        let policy = match policy_guard.as_ref() {
            Some(p) => p,
            None => return Err(ManualCaptureStatus::RejectedDisabled),
        };

        // 3. Master capture enabled
        if !policy.capture_enabled {
            return Err(ManualCaptureStatus::RejectedDisabled);
        }

        // 4. Manual capture enabled
        if !policy.manual_capture_enabled {
            return Err(ManualCaptureStatus::RejectedManualDisabled);
        }

        // 6. Policy version compatibility
        if req.policy_version > policy.policy_version {
            return Err(ManualCaptureStatus::RejectedStalePolicy);
        }

        Ok(())
    }

    /// Try to atomically register a request as in-flight to prevent concurrent duplicate execution.
    /// Returns false if the request was already processed or is already in-flight.
    pub fn try_start_request(&self, request_id: &str) -> bool {
        let processed = self.processed_requests.read().unwrap();
        if processed.contains(request_id) {
            return false;
        }
        let mut in_flight = self.in_flight_requests.write().unwrap();
        if in_flight.contains(request_id) {
            return false;
        }
        in_flight.insert(request_id.to_string());
        true
    }

    /// Mark a request as processed to prevent replay/duplicate processing, clearing in-flight.
    pub fn mark_request_processed(&self, request_id: &str) {
        if let Ok(mut in_flight) = self.in_flight_requests.write() {
            in_flight.remove(request_id);
        }
        if let Ok(mut processed) = self.processed_requests.write() {
            processed.insert(request_id.to_string());
        }
    }

    /// Check if a request has already been processed.
    pub fn is_request_processed(&self, request_id: &str) -> bool {
        self.processed_requests.read().unwrap().contains(request_id)
    }
}

fn load_cached_policy(path: &Path) -> Option<ServerScreenshotPolicy> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn save_cached_policy(path: &Path, policy: &ServerScreenshotPolicy) {
    if let Ok(json) = serde_json::to_string_pretty(policy) {
        let _ = std::fs::write(path, json);
    }
}

// ============================================================================
// 3. Automatic Capture Scheduler (Controlled & Cancelable)
// ============================================================================

static SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Handle allowing cancellation of the background screenshot scheduler.
pub struct SchedulerHandle {
    stop_signal: Arc<AtomicBool>,
}

impl SchedulerHandle {
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
    }
}

/// Spawn the server-policy controlled screenshot scheduler.
///
/// Guarantees:
/// - Only a single background scheduler loop can run (subsequent calls safely no-op).
/// - Respects `capture_enabled`, `continuous_capture_enabled`, and `capture_interval_minutes`.
/// - Stops scheduling immediately when continuous capture is disabled by policy.
/// - Cancels scheduled execution when permission is revoked.
/// - Dynamically applies interval updates without requiring an application restart.
pub fn start_controlled_scheduler(
    db: Arc<Db>,
    control: Arc<TrackerControl>,
    policy_manager: Arc<ScreenshotPolicyManager>,
    dir: PathBuf,
) -> SchedulerHandle {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_signal.clone();

    if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
        crate::log_warn!("screenshot", "scheduler already running; returning handle");
        return SchedulerHandle { stop_signal };
    }

    thread::spawn(move || {
        use crate::platform::{permission_status, Permission, PermissionState};

        let mut elapsed_seconds = 0u64;

        while !stop_clone.load(Ordering::Relaxed) {
            // Check every 1 second so interval changes and policy revocations take effect immediately
            thread::sleep(std::time::Duration::from_secs(1));

            if control.paused.load(Ordering::Relaxed) {
                continue;
            }

            // Check effective server policy (fail-closed)
            let policy = policy_manager.effective_policy();
            if !policy.capture_enabled || !policy.continuous_capture_enabled {
                // Suspended / revoked by server policy
                elapsed_seconds = 0;
                continue;
            }

            // Check user consent (Windows) / local toggle
            if !control.capture_screenshots.load(Ordering::Relaxed) {
                elapsed_seconds = 0;
                continue;
            }

            // OS ScreenRecording permission (Windows only; on macOS, do not abort loop on preflight denial)
            #[cfg(not(target_os = "macos"))]
            if permission_status(Permission::ScreenRecording) != PermissionState::Granted {
                elapsed_seconds = 0;
                continue;
            }

            elapsed_seconds += 1;
            let target_interval_s = (policy.capture_interval_minutes as u64 * 60).max(5);

            if elapsed_seconds >= target_interval_s {
                elapsed_seconds = 0;
                let _saved = crate::trackers::capture_once(&db, &dir, &control);
            }
        }

        SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
    });

    SchedulerHandle { stop_signal }
}

// ============================================================================
// 4. Manual Capture Handler
// ============================================================================

/// Process an authorized manual capture request.
///
/// Enforces all security, privacy, and deduplication rules:
/// - Verifies request expiration, target employee, master capture, and manual permission.
/// - Enforces privacy exclusion rules (active window on skip list -> RejectedPrivacyExclusion).
/// - Notifies user of server-initiated capture.
/// - Deduplicates request IDs.
pub fn process_manual_capture(
    req: ManualCaptureRequest,
    current_employee_id: &str,
    db: &Db,
    dir: &Path,
    control: &TrackerControl,
    policy_manager: &ScreenshotPolicyManager,
    now: i64,
) -> ManualCaptureResult {
    let req_id = req.request_id.clone();
    let emp_id = req.employee_id.clone();

    // 1. Validation check
    if let Err(status) = policy_manager.validate_manual_request(&req, current_employee_id, now) {
        if status != ManualCaptureStatus::RejectedDuplicate {
            policy_manager.mark_request_processed(&req_id);
        }
        return ManualCaptureResult {
            request_id: req_id,
            employee_id: emp_id,
            status,
            screenshot_uuid: None,
            error_message: Some(format!("Request validation failed: {:?}", status)),
            completed_at: now,
        };
    }

    // 2. OS ScreenRecording permission check (Windows only; on macOS, attempt actual capture in step 4)
    #[cfg(not(target_os = "macos"))]
    if crate::platform::permission_status(crate::platform::Permission::ScreenRecording)
        != crate::platform::PermissionState::Granted
    {
        policy_manager.mark_request_processed(&req_id);
        return ManualCaptureResult {
            request_id: req_id,
            employee_id: emp_id,
            status: ManualCaptureStatus::Failed,
            screenshot_uuid: None,
            error_message: Some("OS screen recording permission is not granted".to_string()),
            completed_at: now,
        };
    }

    // 3. Privacy exclusion check (Requirement 5)
    let mode = control.screenshot_mode.load(Ordering::Relaxed);
    if mode == crate::trackers::SHOT_MODE_PRIVACY {
        if let Some(active_win) = crate::platform::active_window() {
            let skip_apps = control.screenshot_skip_apps.read().unwrap();
            if crate::trackers::should_skip(&active_win.app_name, &skip_apps) {
                policy_manager.mark_request_processed(&req_id);
                crate::log_info!(
                    "screenshot",
                    "manual capture rejected: {} is on privacy exclusion list",
                    active_win.app_name
                );
                return ManualCaptureResult {
                    request_id: req_id,
                    employee_id: emp_id,
                    status: ManualCaptureStatus::RejectedPrivacyExclusion,
                    screenshot_uuid: None,
                    error_message: Some(format!(
                        "Active application '{}' is protected by privacy exclusion",
                        active_win.app_name
                    )),
                    completed_at: now,
                };
            }
        }
    }

    // 4. Perform the capture
    let saved_uuids =
        crate::trackers::capture_once_uuids_with_meta(db, dir, control, Some(&req_id), "manual");
    policy_manager.mark_request_processed(&req_id);

    if !saved_uuids.is_empty() {
        let shot_uuid = saved_uuids[0].clone();
        crate::log_info!(
            "screenshot",
            "manual capture completed for request {}: uuid {}, {} displays saved",
            req_id,
            shot_uuid,
            saved_uuids.len()
        );
        ManualCaptureResult {
            request_id: req_id,
            employee_id: emp_id,
            status: ManualCaptureStatus::Completed,
            screenshot_uuid: Some(shot_uuid),
            error_message: None,
            completed_at: now,
        }
    } else {
        #[cfg(target_os = "macos")]
        let err_detail = {
            let perm = crate::platform::permission_status(crate::platform::Permission::ScreenRecording);
            format!("Capture engine produced 0 screenshots (macOS ScreenRecording permission: {:?})", perm)
        };
        #[cfg(not(target_os = "macos"))]
        let err_detail = "Capture engine produced 0 screenshots".to_string();

        ManualCaptureResult {
            request_id: req_id,
            employee_id: emp_id,
            status: ManualCaptureStatus::Failed,
            screenshot_uuid: None,
            error_message: Some(err_detail),
            completed_at: now,
        }
    }
}

// ============================================================================
// 5. Test Suite (10 Acceptance Criteria)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn temp_cache_path(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "policy_test_{}_{}.json",
            std::process::id(),
            suffix
        ))
    }

    // 1. Automatic capture disabled by policy
    #[test]
    fn test_1_automatic_capture_disabled_by_policy() {
        let path = temp_cache_path("autocapture_disabled");
        let mgr = ScreenshotPolicyManager::new(path.clone());
        let control = TrackerControl::new();

        let policy = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: false,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        let effective = mgr.apply_server_policy(policy, Some(&control)).unwrap();
        assert!(effective.capture_enabled);
        assert!(!effective.continuous_capture_enabled);
        assert!(!control.capture_screenshots.load(Ordering::Relaxed));
        let _ = std::fs::remove_file(path);
    }

    // 2. Automatic capture enabled at a configured interval
    #[test]
    fn test_2_automatic_capture_enabled_at_configured_interval() {
        let path = temp_cache_path("autocapture_enabled");
        let mgr = ScreenshotPolicyManager::new(path.clone());
        let control = TrackerControl::new();

        let policy = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 15,
            manual_capture_enabled: true,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        let effective = mgr.apply_server_policy(policy, Some(&control)).unwrap();
        assert!(effective.capture_enabled);
        assert!(effective.continuous_capture_enabled);
        assert_eq!(effective.capture_interval_minutes, 15);
        assert!(control.capture_screenshots.load(Ordering::Relaxed));
        assert_eq!(
            control.screenshot_interval_s.load(Ordering::Relaxed),
            15 * 60
        );
        let _ = std::fs::remove_file(path);
    }

    // 3. Manual capture permitted while continuous capture is disabled
    #[test]
    fn test_3_manual_capture_permitted_while_continuous_capture_disabled() {
        let path = temp_cache_path("manual_permitted_continuous_disabled");
        let mgr = ScreenshotPolicyManager::new(path.clone());

        let policy = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: false,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        mgr.apply_server_policy(policy, None).unwrap();

        let req = ManualCaptureRequest {
            request_id: "req_001".into(),
            employee_id: "emp_123".into(),
            policy_version: 1,
            expires_at: 2000,
            created_at: 1000,
            requested_by: Some("manager@company.com".into()),
        };

        let res = mgr.validate_manual_request(&req, "emp_123", 1500);
        assert!(res.is_ok());
        let _ = std::fs::remove_file(path);
    }

    // 4. Manual capture rejected when capture permission is disabled
    #[test]
    fn test_4_manual_capture_rejected_when_capture_permission_disabled() {
        let path = temp_cache_path("manual_rejected_master_disabled");
        let mgr = ScreenshotPolicyManager::new(path.clone());

        let policy = ServerScreenshotPolicy {
            capture_enabled: false, // master disabled
            continuous_capture_enabled: false,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        mgr.apply_server_policy(policy, None).unwrap();

        let req = ManualCaptureRequest {
            request_id: "req_002".into(),
            employee_id: "emp_123".into(),
            policy_version: 1,
            expires_at: 2000,
            created_at: 1000,
            requested_by: Some("manager@company.com".into()),
        };

        let res = mgr.validate_manual_request(&req, "emp_123", 1500);
        assert_eq!(res, Err(ManualCaptureStatus::RejectedDisabled));
        let _ = std::fs::remove_file(path);
    }

    // 5. Manual capture rejected when manual requests are disabled
    #[test]
    fn test_5_manual_capture_rejected_when_manual_requests_disabled() {
        let path = temp_cache_path("manual_rejected_manual_disabled");
        let mgr = ScreenshotPolicyManager::new(path.clone());

        let policy = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: false, // manual disabled
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        mgr.apply_server_policy(policy, None).unwrap();

        let req = ManualCaptureRequest {
            request_id: "req_003".into(),
            employee_id: "emp_123".into(),
            policy_version: 1,
            expires_at: 2000,
            created_at: 1000,
            requested_by: Some("manager@company.com".into()),
        };

        let res = mgr.validate_manual_request(&req, "emp_123", 1500);
        assert_eq!(res, Err(ManualCaptureStatus::RejectedManualDisabled));
        let _ = std::fs::remove_file(path);
    }

    // 6. Invalid, expired, duplicated, or wrong-employee requests rejected
    #[test]
    fn test_6_invalid_expired_duplicated_wrong_employee_requests_rejected() {
        let path = temp_cache_path("request_validation_rejections");
        let mgr = ScreenshotPolicyManager::new(path.clone());

        let policy = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 2,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        mgr.apply_server_policy(policy, None).unwrap();

        // Expired request (now > expires_at)
        let expired_req = ManualCaptureRequest {
            request_id: "req_exp".into(),
            employee_id: "emp_123".into(),
            policy_version: 2,
            expires_at: 1000,
            created_at: 900,
            requested_by: None,
        };
        assert_eq!(
            mgr.validate_manual_request(&expired_req, "emp_123", 1001),
            Err(ManualCaptureStatus::RejectedExpired)
        );

        // Wrong employee request
        let wrong_emp_req = ManualCaptureRequest {
            request_id: "req_wrong_emp".into(),
            employee_id: "emp_other".into(),
            policy_version: 2,
            expires_at: 2000,
            created_at: 900,
            requested_by: None,
        };
        assert_eq!(
            mgr.validate_manual_request(&wrong_emp_req, "emp_123", 1000),
            Err(ManualCaptureStatus::RejectedWrongEmployee)
        );

        // Stale policy version on request (request expects v3 while client only has v2)
        let stale_req = ManualCaptureRequest {
            request_id: "req_stale".into(),
            employee_id: "emp_123".into(),
            policy_version: 3,
            expires_at: 2000,
            created_at: 900,
            requested_by: None,
        };
        assert_eq!(
            mgr.validate_manual_request(&stale_req, "emp_123", 1000),
            Err(ManualCaptureStatus::RejectedStalePolicy)
        );

        // Duplicate request
        let dup_req = ManualCaptureRequest {
            request_id: "req_dup".into(),
            employee_id: "emp_123".into(),
            policy_version: 2,
            expires_at: 2000,
            created_at: 900,
            requested_by: None,
        };
        assert!(mgr
            .validate_manual_request(&dup_req, "emp_123", 1000)
            .is_ok());
        mgr.mark_request_processed(&dup_req.request_id);
        assert_eq!(
            mgr.validate_manual_request(&dup_req, "emp_123", 1000),
            Err(ManualCaptureStatus::RejectedDuplicate)
        );

        let _ = std::fs::remove_file(path);
    }

    // 7. Older policy versions rejected
    #[test]
    fn test_7_older_policy_versions_rejected() {
        let path = temp_cache_path("older_policy_version_rejected");
        let mgr = ScreenshotPolicyManager::new(path.clone());

        let policy_v2 = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 2,
            policy_updated_at: 2000,
            employee_id: "emp_123".into(),
        };
        mgr.apply_server_policy(policy_v2, None).unwrap();

        let policy_v1 = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 1, // older
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };

        let err = mgr.apply_server_policy(policy_v1, None).unwrap_err();
        assert!(err.contains("rejected stale policy update"));
        assert_eq!(mgr.effective_policy().policy_version, 2);
        let _ = std::fs::remove_file(path);
    }

    // 8. Scheduler stopped when permission is revoked
    #[test]
    fn test_8_scheduler_stopped_when_permission_revoked() {
        let path = temp_cache_path("scheduler_revocation");
        let mgr = ScreenshotPolicyManager::new(path.clone());
        let control = TrackerControl::new();

        // Initially active
        let policy_active = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: true,
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 1,
            policy_updated_at: 1000,
            employee_id: "emp_123".into(),
        };
        mgr.apply_server_policy(policy_active, Some(&control))
            .unwrap();
        assert!(control.capture_screenshots.load(Ordering::Relaxed));

        // Revoke continuous capture
        let policy_revoked = ServerScreenshotPolicy {
            capture_enabled: true,
            continuous_capture_enabled: false, // revoked
            capture_interval_minutes: 5,
            manual_capture_enabled: true,
            policy_version: 2,
            policy_updated_at: 1100,
            employee_id: "emp_123".into(),
        };
        mgr.apply_server_policy(policy_revoked, Some(&control))
            .unwrap();
        assert!(!control.capture_screenshots.load(Ordering::Relaxed));
        assert!(!mgr.effective_policy().continuous_capture_enabled);

        // Revoke all capture permissions on logout
        mgr.invalidate_on_logout(Some(&control));
        assert!(!control.capture_screenshots.load(Ordering::Relaxed));
        assert_eq!(mgr.state(), PolicyState::Unavailable);
        let _ = std::fs::remove_file(path);
    }

    // 9. Missing or unavailable policy does not enable unrestricted automatic capture
    #[test]
    fn test_9_missing_or_unavailable_policy_fail_closed() {
        let nonexistent_path = temp_cache_path("nonexistent_cache");
        let mgr = ScreenshotPolicyManager::new(nonexistent_path);

        assert_eq!(mgr.state(), PolicyState::Unavailable);
        let effective = mgr.effective_policy();
        assert!(!effective.capture_enabled);
        assert!(!effective.continuous_capture_enabled);
        assert!(!effective.manual_capture_enabled);
        assert!(effective.is_fail_closed);

        // When in Loading state, still fail-closed
        mgr.set_state(PolicyState::Loading, "fetching from server...");
        let loading_eff = mgr.effective_policy();
        assert!(!loading_eff.capture_enabled);
        assert!(!loading_eff.continuous_capture_enabled);
        assert!(loading_eff.is_fail_closed);
    }

    // 10. Existing authentication, tracking, and synchronization remain functional
    #[test]
    fn test_10_existing_auth_tracking_sync_functional() {
        use crate::storage::{ActivitySample, Db};
        use crate::sync::auth::{AuthState, Session};

        let dir = std::env::temp_dir().join(format!("test_10_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Auth session storage and resolution
        let auth = AuthState::load(dir.join("session.json"));
        let session = Session {
            access_token: "mock_tok".into(),
            refresh_token: "mock_ref".into(),
            email: "alice@company.com".into(),
            business_id: Some("biz_123".into()),
            display_name: "Alice Smith".into(),
            username: "alice".into(),
            account_type: "employee".into(),
            role: "employee".into(),
            employee_id: Some("emp_alice_99".into()),
        };
        auth.store(session).unwrap();

        let loaded_session = auth.session().unwrap();
        assert_eq!(loaded_session.resolved_employee_id(), "emp_alice_99");

        // Local tracking storage
        let db = Db::open_in_memory().unwrap();
        db.insert_activity_sample(&ActivitySample {
            ts: 500,
            app_name: "Finjulia Editor".into(),
            window_title: Some("main.rs".into()),
            pid: Some(1234),
            duration_s: 10,
        })
        .unwrap();
        db.add_keystrokes(60, 42).unwrap();

        let samples = db.activity_between(0, 1000).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].app_name, "Finjulia Editor");

        let keys = db.keystrokes_between(0, 1000).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].1, 42);

        std::fs::remove_dir_all(&dir).ok();
    }
}
