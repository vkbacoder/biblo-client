//! RBAC & Admin commands for BiBo / Finjulia Desktop Client.
//! Communicates with the authoritative backend endpoints for organization hierarchy,
//! team management, member roles, screenshot policy & requests, audit logs, and access control.

use std::sync::Arc;
use serde_json::Value;
use tauri::State;

use crate::commands::backend_url;
use crate::sync::auth::AuthState;
use crate::sync::client::BackendClient;

/// Check caller's effective access, permissions, and assigned team IDs for the business.
#[tauri::command]
pub async fn admin_my_access(
    business_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}/my-access")).await
}

/// Fetch business details by ID.
#[tauri::command]
pub async fn admin_business_detail(
    business_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}")).await
}

/// List all members in the organization.
#[tauri::command]
pub async fn admin_members(
    business_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}/members")).await
}

/// List all teams (hierarchical) in the organization.
#[tauri::command]
pub async fn admin_teams(
    business_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}/teams")).await
}

/// List assigned members for a specific team.
#[tauri::command]
pub async fn admin_team_members(
    business_id: String,
    team_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}/teams/{team_id}/members")).await
}

/// Create a new team in the business.
#[tauri::command]
pub async fn admin_create_team(
    business_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(&format!("/v1/businesses/{business_id}/teams"), &body).await
}

/// Update an existing team's name, description, or parent team.
#[tauri::command]
pub async fn admin_update_team(
    business_id: String,
    team_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.patch_authed_json(&format!("/v1/businesses/{business_id}/teams/{team_id}"), &body).await
}

/// Delete an existing team.
#[tauri::command]
pub async fn admin_delete_team(
    business_id: String,
    team_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.delete_authed_json(&format!("/v1/businesses/{business_id}/teams/{team_id}")).await
}

/// Assign an employee/member to a team.
#[tauri::command]
pub async fn admin_assign_team_member(
    business_id: String,
    team_id: String,
    user_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/teams/{team_id}/members/{user_id}"),
        &serde_json::json!({}),
    ).await
}

/// Remove a member from a team.
#[tauri::command]
pub async fn admin_remove_team_member(
    business_id: String,
    team_id: String,
    user_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.delete_authed_json(
        &format!("/v1/businesses/{business_id}/teams/{team_id}/members/{user_id}"),
    ).await
}

/// Promote an employee to Team Lead.
#[tauri::command]
pub async fn admin_promote_member(
    business_id: String,
    user_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/promote"),
        &body,
    ).await
}

/// Demote a Team Lead to standard Employee.
#[tauri::command]
pub async fn admin_demote_member(
    business_id: String,
    user_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/demote"),
        &body,
    ).await
}

/// Suspend an employee/member.
#[tauri::command]
pub async fn admin_suspend_member(
    business_id: String,
    user_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/suspend"),
        &body,
    ).await
}

/// Reactivate a suspended member.
#[tauri::command]
pub async fn admin_reactivate_member(
    business_id: String,
    user_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/reactivate"),
        &serde_json::json!({}),
    ).await
}

/// Remove a member from the organization completely.
#[tauri::command]
pub async fn admin_remove_member(
    business_id: String,
    user_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.delete_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}"),
    ).await
}

/// Get granular permissions for a member.
#[tauri::command]
pub async fn admin_member_permissions(
    business_id: String,
    user_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/permissions"),
    ).await
}

/// Grant a granular permission to a member.
#[tauri::command]
pub async fn admin_grant_permission(
    business_id: String,
    user_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/permissions"),
        &body,
    ).await
}

/// Revoke a granular permission from a member.
#[tauri::command]
pub async fn admin_revoke_permission(
    business_id: String,
    user_id: String,
    permission: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.delete_authed_json(
        &format!("/v1/businesses/{business_id}/members/{user_id}/permissions/{permission}"),
    ).await
}

/// Retrieve immutable organizational audit logs.
#[tauri::command]
pub async fn admin_audit_logs(
    business_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}/audit-logs")).await
}

/// Retrieve org-level screenshot configuration & defaults.
#[tauri::command]
pub async fn admin_screenshot_settings(
    business_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(&format!("/v1/businesses/{business_id}/screenshot-settings")).await
}

/// Update org-level screenshot configuration.
#[tauri::command]
pub async fn admin_update_screenshot_settings(
    business_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.patch_authed_json(
        &format!("/v1/businesses/{business_id}/screenshot-settings"),
        &body,
    ).await
}

/// Get effective per-employee screenshot policy override.
#[tauri::command]
pub async fn admin_employee_screenshot_policy(
    business_id: String,
    employee_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(
        &format!("/v1/businesses/{business_id}/employees/{employee_id}/policy"),
    ).await
}

/// Set per-employee screenshot policy override.
#[tauri::command]
pub async fn admin_update_employee_screenshot_policy(
    business_id: String,
    employee_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.patch_authed_json(
        &format!("/v1/businesses/{business_id}/employees/{employee_id}/policy"),
        &body,
    ).await
}

/// Reset per-employee screenshot policy to org default.
#[tauri::command]
pub async fn admin_reset_employee_screenshot_policy(
    business_id: String,
    employee_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.delete_authed_json(
        &format!("/v1/businesses/{business_id}/employees/{employee_id}/policy"),
    ).await
}

/// Create a manager-authorized manual screenshot capture request.
#[tauri::command]
pub async fn admin_create_screenshot_request(
    business_id: String,
    employee_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/screenshot-requests"),
        &serde_json::json!({ "employee_id": employee_id }),
    ).await
}

/// List screenshot requests for an employee.
#[tauri::command]
pub async fn admin_employee_screenshot_requests(
    business_id: String,
    employee_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.get_authed_json(
        &format!("/v1/businesses/{business_id}/employees/{employee_id}/screenshot-requests"),
    ).await
}

/// Retry an expired or failed manual screenshot request.
#[tauri::command]
pub async fn admin_retry_screenshot_request(
    business_id: String,
    request_id: String,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.post_authed_json(
        &format!("/v1/businesses/{business_id}/screenshot-requests/{request_id}/retry"),
        &serde_json::json!({}),
    ).await
}

/// Update organization settings.
#[tauri::command]
pub async fn admin_update_business_settings(
    business_id: String,
    body: Value,
    auth: State<'_, Arc<AuthState>>,
) -> Result<Value, String> {
    let client = BackendClient::new(backend_url(), auth.inner().clone());
    client.patch_authed_json(
        &format!("/v1/businesses/{business_id}"),
        &body,
    ).await
}
