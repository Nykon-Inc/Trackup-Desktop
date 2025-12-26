use crate::db;
use crate::idle::IdleState;
use crate::AppState;
use active_win_pos_rs::get_active_window;
use rusqlite::Connection;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

pub fn get_browser_url(app_name: &str) -> Option<String> {
    if cfg!(target_os = "macos") {
        let script = match app_name {
            "Google Chrome" | "Google Chrome Canary" | "Chromium" | "Brave Browser" => {
                Some("tell application \"Google Chrome\" to get URL of active tab of front window")
            }
            "Safari" | "Safari Technology Preview" => {
                Some("tell application \"Safari\" to get URL of front document")
            }
            "Firefox" => {
                // Firefox doesn't support easy AppleScript execution for URL without specific settings/extensions usually,
                // but sometimes standard suite works. It's flaky. Let's try basic one or ignore.
                // Generic UI scripting is slow and intrusive.
                None
            }
            "Arc" => Some("tell application \"Arc\" to get URL of active tab of front window"),
            _ => None,
        };

        if let Some(s) = script {
            let output = Command::new("osascript").arg("-e").arg(s).output().ok()?;

            if output.status.success() {
                let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !url.is_empty() {
                    return Some(url);
                }
            }
        }
    } else if cfg!(target_os = "windows") {
        // Windows URL fetching often requires UI Automation which is complex for a quick script
        // keeping it simple or empty for now unless active-win-pos-rs exposes it (it doesn't).
    }
    None
}

pub fn start_activity_monitor<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    let app_monitor = app.clone();
    let state_monitor = state.clone();

    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5)); // Check every 5 seconds

            // Only log if monitoring (user is active)
            if state_monitor
                .is_monitoring
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                // Check for active session in DB
                let app_handle = app_monitor.clone();
                let _ = app_monitor.run_on_main_thread(move || {
                    let app_state = app_handle.state::<AppState>();
                    if let Ok(conn) = Connection::open(&app_state.db_path) {
                        if let Ok(Some(user)) = db::get_user(&conn) {
                            if let Some(pid) = user.current_project_id {
                                if let Ok(Some(session)) = db::get_active_session(&conn, &pid) {
                                    // Get Active Window
                                    if let Ok(window) = get_active_window() {
                                        let app_name = window.app_name;
                                        let window_title = window.title;

                                        // Get URL if browser
                                        let url = get_browser_url(&app_name);

                                        // Save to DB
                                        let _ = db::save_activity_log(
                                            &conn,
                                            &session.uuid,
                                            &pid,
                                            &app_name,
                                            &window_title,
                                            url.as_deref(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
    });
}
