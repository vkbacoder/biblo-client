//! ctracking — local-only macOS activity tracker.
//! Module layout per docs/01-architecture.md.

mod commands;
mod obs;
mod platform;
pub mod policy;
mod server;
mod settings;
mod storage;
pub mod sync;
mod trackers;
mod tray;

use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Current unix time in seconds. Shared helper for sync bookkeeping.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Apply the "run without an app-launcher presence" setting. macOS toggles the
/// Dock icon via the activation policy; Windows hides the taskbar button (the app
/// stays reachable from the tray). No-op elsewhere.
#[cfg(target_os = "macos")]
fn apply_dock_policy(app: &tauri::AppHandle, hide: bool) {
    let _ = app.set_activation_policy(if hide {
        tauri::ActivationPolicy::Accessory
    } else {
        tauri::ActivationPolicy::Regular
    });
}
#[cfg(target_os = "windows")]
fn apply_dock_policy(app: &tauri::AppHandle, hide: bool) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_skip_taskbar(hide);
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn apply_dock_policy(_app: &tauri::AppHandle, _hide: bool) {}

/// Fetch the curated sensitive-app list from the backend (skip-list suggestions
/// + prefill), falling back to the baked-in copy when the backend is
/// unreachable (offline / personal use).
pub async fn fetch_privacy_apps_or_baked() -> Vec<sync::client::PrivacyAppCategory> {
    match sync::client::fetch_privacy_apps(&settings::backend_base_url()).await {
        Ok(categories) if !categories.is_empty() => categories,
        Ok(_) | Err(_) => trackers::DEFAULT_PRIVACY_APPS
            .iter()
            .map(|(key, apps)| sync::client::PrivacyAppCategory {
                key: key.to_string(),
                apps: apps.iter().map(|a| a.to_string()).collect(),
            })
            .collect(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
#[cfg(target_os = "macos")]
fn migrate_legacy_macos_data(new_dir: &std::path::Path) {
    if let Some(parent) = new_dir.parent() {
        let legacy_dir = parent.join("com.briannguyen.bibotracking");
        if legacy_dir.is_dir() {
            let files_to_migrate = ["session.json", "settings.json", "data.db"];
            for fname in &files_to_migrate {
                let target = new_dir.join(fname);
                let source = legacy_dir.join(fname);
                if !target.exists() && source.exists() {
                    if let Ok(_) = std::fs::copy(&source, &target) {
                        crate::log_info!("init", "migrated legacy {} from {:?}", fname, source);
                    }
                }
            }
        }
    }
}

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::set_paused,
            commands::set_in_setup,
            commands::is_paused,
            commands::tracking_state,
            commands::dashboard_data,
            commands::keystroke_buckets,
            commands::screenshot_list,
            commands::screenshot_summary,
            commands::browser_visits,
            commands::get_settings,
            commands::set_settings,
            commands::set_locale,
            commands::export_csv,
            commands::export_json,
            commands::permissions_status,
            commands::open_permission_settings,
            commands::request_screen_recording,
            commands::request_input_monitoring,
            commands::request_accessibility,
            commands::capture_now,
            commands::browser_link,
            commands::signup_url,
            commands::admin_url,
            commands::admin_businesses,
            commands::admin_roster,
            commands::admin_employee_activity,
            commands::admin_employee_keystrokes,
            commands::admin_employee_browser,
            commands::admin_employee_screenshots,
            commands::admin_screenshot_data,
            commands::admin_create_business,
            commands::admin_create_employee,
            commands::login,
            commands::logout,
            commands::current_session,
            commands::sync_status,
            commands::apply_org_policy,
            commands::capture_policy,
            commands::privacy_apps,
            commands::messages_fetch,
            commands::message_respond,
            commands::message_event,
            commands::screenshot_policy_status,
            commands::screenshot_policy_refresh,
            commands::screenshot_manual_request,
            commands::screenshot_policy_simulate_update,
            commands::admin_my_access,
            commands::admin_business_detail,
            commands::admin_members,
            commands::admin_teams,
            commands::admin_team_members,
            commands::admin_create_team,
            commands::admin_update_team,
            commands::admin_delete_team,
            commands::admin_assign_team_member,
            commands::admin_remove_team_member,
            commands::admin_promote_member,
            commands::admin_demote_member,
            commands::admin_suspend_member,
            commands::admin_reactivate_member,
            commands::admin_remove_member,
            commands::admin_member_permissions,
            commands::admin_grant_permission,
            commands::admin_revoke_permission,
            commands::admin_audit_logs,
            commands::admin_screenshot_settings,
            commands::admin_update_screenshot_settings,
            commands::admin_employee_screenshot_policy,
            commands::admin_update_employee_screenshot_policy,
            commands::admin_reset_employee_screenshot_policy,
            commands::admin_create_screenshot_request,
            commands::admin_employee_screenshot_requests,
            commands::admin_retry_screenshot_request,
            commands::admin_update_business_settings,
        ])
        .setup(|app| {
            // Open the local SQLite DB under the app data dir.
            let data_dir = app.path().app_data_dir().expect("resolve app data dir");
            std::fs::create_dir_all(&data_dir).expect("create app data dir");

            #[cfg(target_os = "macos")]
            migrate_legacy_macos_data(&data_dir);
            let db_path = data_dir.join("data.db");
            let db = Arc::new(storage::Db::open(&db_path).expect("open database"));

            // Shared control surface for the trackers (pause, intervals, privacy).
            let control = Arc::new(trackers::TrackerControl::new());

            // Load persisted settings and apply them to the live control.
            let settings_path = data_dir.join("settings.json");
            let loaded = settings::load_with_device_id(&settings_path);
            settings::apply(&loaded, &control);
            let hide_dock = loaded.hide_dock;

            let settings_state = Arc::new(settings::SettingsState {
                path: settings_path,
                current: std::sync::Mutex::new(loaded),
                managed: std::sync::Mutex::new(settings::CaptureManaged::default()),
            });
            app.manage(settings_state.clone());
            // Manage control early so the tray can read pause state.
            app.manage(control.clone());

            // Menu bar item (Start/Stop/Open) + Dock visibility per settings.
            tray::build(&app.handle(), control.clone())?;
            apply_dock_policy(&app.handle(), hide_dock);

            // Keep the window behavior and notify the UI when the window comes to the front.

            if let Some(win) = app.get_webview_window("main") {
                let w = win.clone();

                win.on_window_event(move |event| match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = w.hide();
                    }
                    tauri::WindowEvent::Focused(true) => {
                        // Preserve the event used by the in-app message popup.
                        let _ = w.emit("app-focused", ());
                    }
                    _ => {}
                });
            }

            // Permissions are NOT requested at startup — the UI detects missing ones
            // and routes the user to the Permissions screen, where every request is
            // user-initiated (see App.tsx startup check).

            // Screenshot Policy Manager (Server-controlled screenshot management)
            let policy_cache_path = data_dir.join("screenshot_policy_cache.json");
            let policy_manager = Arc::new(policy::ScreenshotPolicyManager::load_or_default(
                policy_cache_path,
            ));
            app.manage(policy_manager.clone());

            // Start the trackers, keyboard counter, controlled screenshots, retention cleanup.
            let shots_dir = data_dir.join("screenshots");
            trackers::start(db.clone(), control.clone());
            trackers::start_keyboard(db.clone(), control.clone());
            policy::start_controlled_scheduler(
                db.clone(),
                control.clone(),
                policy_manager.clone(),
                shots_dir.clone(),
            );
            trackers::start_cleanup(db.clone(), control.clone());

            // Start the local ingest server for the browser extension.
            let link = server::start(db.clone(), control.clone());
            app.manage(link);

            // Auth/session (task 51): load any persisted session from disk.
            let auth = Arc::new(sync::AuthState::load(data_dir.join("session.json")));
            app.manage(auth.clone());

            // Dedicated adapter for screenshot policy communication with backend
            let screenshot_adapter = Arc::new(
                sync::ScreenshotPolicyAdapter::new(
                    settings::backend_base_url(),
                    auth.clone(),
                    policy_manager.clone(),
                )
                .with_control(control.clone()),
            );
            app.manage(screenshot_adapter.clone());

            // Sync worker (task 53): pushes pending rows to the backend in the
            // background and polls pending screenshot requests. No-op while logged out / offline.
            let status = Arc::new(sync::worker::SyncStatus::default());
            app.manage(status.clone());
            sync::worker::start(sync::worker::SyncContext {
                db: db.clone(),
                auth: auth.clone(),
                status,
                settings: settings_state,
                policy_manager: policy_manager.clone(),
                adapter: screenshot_adapter.clone(),
                control: control.clone(),
                screenshots_dir: shots_dir,
                stop_signal: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            });

            // Initial policy refresh if already authenticated
            if let Some(session) = auth.session() {
                let emp_id = session.resolved_employee_id();
                if !emp_id.is_empty() {
                    let adapter_for_boot = screenshot_adapter.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = adapter_for_boot.sync_policy(&emp_id).await;
                    });
                }
            }

            // Manage remaining state so commands can reach the DB.
            app.manage(db);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
