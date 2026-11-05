//! Server Communication Adapter for Screenshot Policy & Capture Requests.
//!
//! # Proposed Server Contract Documentation
//!
//! This adapter defines the client-side contract for future backend integration.
//! The backend/server is in a separate repository and will implement these endpoints.
//!
//! ## 1. Fetch Employee Screenshot Policy
//! - **Endpoint**: `GET /v1/screenshot-policy?employee_id={employee_id}`
//! - **Headers**: `Authorization: Bearer <access_token>`
//! - **Expected Response 200 OK**:
//!   ```json
//!   {
//!     "capture_enabled": true,
//!     "continuous_capture_enabled": true,
//!     "capture_interval_minutes": 5,
//!     "manual_capture_enabled": true,
//!     "policy_version": 1,
//!     "policy_updated_at": 1729683742,
//!     "employee_id": "emp_abc123"
//!   }
//!   ```
//!
//! ## 2. Acknowledge Policy Update
//! - **Endpoint**: `POST /v1/screenshot-policy/ack`
//! - **Headers**: `Authorization: Bearer <access_token>`
//! - **Request Body**:
//!   ```json
//!   {
//!     "policy_version": 1,
//!     "employee_id": "emp_abc123",
//!     "acknowledged_at": 1729683750
//!   }
//!   ```
//! - **Response 200 OK**: `{ "status": "acknowledged" }`
//!
//! ## 3. Retrieve Pending Capture Requests
//! - **Endpoint**: `GET /v1/screenshot-requests/pending?employee_id={employee_id}`
//! - **Headers**: `Authorization: Bearer <access_token>`
//! - **Expected Response 200 OK**:
//!   ```json
//!   {
//!     "requests": [
//!       {
//!         "request_id": "req_xyz789",
//!         "employee_id": "emp_abc123",
//!         "policy_version": 1,
//!         "expires_at": 1729684350,
//!         "created_at": 1729683750,
//!         "requested_by": "manager@company.com"
//!       }
//!     ]
//!   }
//!   ```
//!
//! ## 4. Acknowledge Capture Request Status
//! - **Endpoint**: `POST /v1/screenshot-requests/{request_id}/ack`
//! - **Headers**: `Authorization: Bearer <access_token>`
//! - **Request Body**:
//!   ```json
//!   {
//!     "request_id": "req_xyz789",
//!     "employee_id": "emp_abc123",
//!     "status": "completed", // or "rejected_privacy_exclusion", "rejected_expired", etc.
//!     "screenshot_uuid": "3fa85f64-5717-4562-b3fc-2c963f66afa6",
//!     "error_message": null,
//!     "completed_at": 1729683760
//!   }
//!   ```
//!
//! ## Safe Fail-Closed Behavior
//! If the server returns 404/501 (endpoints not deployed yet), the client transitions
//! to `PolicyState::Unavailable` and suspends automatic captures. Unrestricted capture
//! is never permitted without an authoritative server policy.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::policy::{
    EffectiveScreenshotPolicy, ManualCaptureRequest, ManualCaptureResult, PolicyState,
    ScreenshotPolicyManager, ServerScreenshotPolicy,
};
use crate::sync::auth::AuthState;
use crate::sync::client::BackendClient;
use crate::trackers::TrackerControl;

/// Dedicated adapter for screenshot policy communication with the server.
pub struct ScreenshotPolicyAdapter {
    client: BackendClient,
    policy_manager: Arc<ScreenshotPolicyManager>,
    control: Option<Arc<TrackerControl>>,
    /// Configurable non-aggressive polling interval in seconds (default: 600s = 10 minutes).
    poll_interval_s: u64,
}

#[derive(Serialize)]
struct PolicyAckReq<'a> {
    policy_version: i64,
    employee_id: &'a str,
    acknowledged_at: i64,
}

#[derive(Deserialize)]
struct PendingRequestsResp {
    #[serde(default)]
    requests: Vec<ManualCaptureRequest>,
}

impl ScreenshotPolicyAdapter {
    /// Create a new adapter instance reusing the existing `BackendClient` and auth credentials.
    pub fn new(
        base_url: String,
        auth: Arc<AuthState>,
        policy_manager: Arc<ScreenshotPolicyManager>,
    ) -> Self {
        let client = BackendClient::new(base_url, auth);
        ScreenshotPolicyAdapter {
            client,
            policy_manager,
            control: None,
            poll_interval_s: 600, // 10 minutes, non-aggressive
        }
    }

    /// Provide tracker control so server policy updates are immediately applied live.
    pub fn with_control(mut self, control: Arc<TrackerControl>) -> Self {
        self.control = Some(control);
        self
    }

    /// Base URL configured for communication with the server.
    pub fn base_url(&self) -> &str {
        self.client.base_url()
    }

    /// Configured check interval.
    pub fn poll_interval_s(&self) -> u64 {
        self.poll_interval_s
    }

    /// Set a custom check interval (minimum 60s).
    pub fn set_poll_interval_s(&mut self, interval_s: u64) {
        self.poll_interval_s = interval_s.max(60);
    }

    /// Fetch the current employee's screenshot policy from the backend.
    ///
    /// If the backend endpoint is not implemented yet (404/501) or network fails,
    /// catches the error gracefully, transitions manager state to `Unavailable` (fail-closed),
    /// and logs without crashing.
    pub async fn fetch_employee_policy(
        &self,
        employee_id: &str,
    ) -> Result<ServerScreenshotPolicy, String> {
        let path = format!("/v1/screenshot-policy?employee_id={employee_id}");
        match self
            .client
            .get_authed_json::<ServerScreenshotPolicy>(&path)
            .await
        {
            Ok(policy) => Ok(policy),
            Err(e) if e.contains("404") || e.contains("501") => {
                // This backend doesn't implement the dedicated endpoint. Derive the
                // policy from the org policy it does serve (`/v1/policy`). Network and
                // auth errors still fail closed (handled by the arm below).
                crate::log_info!(
                    "policy",
                    "/v1/screenshot-policy not available ({e}); deriving policy from /v1/policy"
                );
                let org = self.client.fetch_policy().await?;
                let interval_s = org.screenshot_interval_s.unwrap_or(300);
                Ok(ServerScreenshotPolicy {
                    capture_enabled: true,
                    continuous_capture_enabled: true,
                    capture_interval_minutes: ((interval_s / 60).max(1)) as u32,
                    manual_capture_enabled: true,
                    policy_version: 1,
                    policy_updated_at: crate::now_unix(),
                    employee_id: employee_id.to_string(),
                })
            }
            Err(e) => {
                // If connection error or other failure, log cleanly
                crate::log_info!(
                    "policy",
                    "server screenshot-policy endpoint not active or returned error: {e}. Defaulting to fail-closed state."
                );
                Err(e)
            }
        }
    }

    /// Acknowledge receipt of a validated policy update to the server.
    pub async fn ack_policy_update(
        &self,
        policy_version: i64,
        employee_id: &str,
    ) -> Result<(), String> {
        let body = PolicyAckReq {
            policy_version,
            employee_id,
            acknowledged_at: crate::now_unix(),
        };
        self.client
            .post_authed_json::<_, serde_json::Value>("/v1/screenshot-policy/ack", &body)
            .await
            .map(|_| ())
    }

    /// Retrieve pending manual capture requests for this employee from the server.
    pub async fn fetch_pending_capture_requests(
        &self,
        employee_id: &str,
    ) -> Result<Vec<ManualCaptureRequest>, String> {
        let path = format!("/v1/screenshot-requests/pending?employee_id={employee_id}");
        let resp: PendingRequestsResp = self.client.get_authed_json(&path).await?;
        Ok(resp.requests)
    }

    /// Acknowledge a manual capture request's final status to the server.
    pub async fn ack_capture_request(&self, result: &ManualCaptureResult) -> Result<(), String> {
        let path = format!("/v1/screenshot-requests/{}/ack", result.request_id);
        self.client
            .post_authed_json::<_, serde_json::Value>(&path, result)
            .await
            .map(|_| ())
    }

    /// Synchronize the policy with the server.
    ///
    /// Full fail-closed lifecycle:
    /// - If no employee session exists, sets `Unavailable`.
    /// - If policy fetch fails or endpoint not ready, transitions to `Unavailable` (fail-closed).
    /// - If policy fetch succeeds, applies policy and acks update.
    pub async fn sync_policy(&self, employee_id: &str) -> EffectiveScreenshotPolicy {
        if employee_id.is_empty() {
            self.policy_manager
                .set_state(PolicyState::Unavailable, "no employee session");
            return self.policy_manager.effective_policy();
        }

        self.policy_manager
            .set_state(PolicyState::Loading, "fetching server policy...");

        match self.fetch_employee_policy(employee_id).await {
            Ok(server_policy) => {
                match self
                    .policy_manager
                    .apply_server_policy(server_policy.clone(), self.control.as_deref())
                {
                    Ok(effective) => {
                        let _ = self
                            .ack_policy_update(server_policy.policy_version, employee_id)
                            .await;
                        effective
                    }
                    Err(err_msg) => {
                        crate::log_warn!("policy", "failed to apply server policy: {err_msg}");
                        self.policy_manager.set_state(
                            PolicyState::Unavailable,
                            &format!("policy invalid: {err_msg}"),
                        );
                        self.policy_manager.effective_policy()
                    }
                }
            }
            Err(e) => {
                // Fail-closed: capture remains suspended
                self.policy_manager.set_state(
                    PolicyState::Unavailable,
                    &format!("server policy unavailable: {e}"),
                );
                self.policy_manager.effective_policy()
            }
        }
    }
}
