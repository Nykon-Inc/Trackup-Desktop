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
        let script = if app_name.contains("Safari") {
            format!(
                "tell application \"{}\" to get URL of front document",
                app_name
            )
        } else if app_name.contains("Chrome")
            || app_name.contains("Brave")
            || app_name.contains("Edge")
            || app_name.contains("Arc")
            || app_name.contains("Vivaldi")
            || app_name.contains("Opera")
            || app_name.contains("Chromium")
        {
            format!(
                "tell application \"{}\" to get URL of active tab of front window",
                app_name
            )
        } else if app_name.contains("Firefox") {
            format!(
                "tell application \"System Events\" to tell process \"{}\" to get value of UI element 1 of combo box 1 of toolbar 1 of group 1 of UI element 1 of window 1",
                app_name
            )
        } else {
            return None;
        };

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .ok()?;

        if output.status.success() {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !url.is_empty() && (url.starts_with("http") || url.contains("://")) {
                return Some(url);
            }
        }
    }
    None
}

pub fn start_activity_loop<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    // Ensure only one loop runs
    if state
        .is_activity_loop_running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }

    let app_monitor = app.clone();
    let state_monitor = state.clone();

    thread::spawn(move || {
        println!("Activity: Starting Activity Loop");
        loop {
            thread::sleep(Duration::from_secs(5)); // Check every 5 seconds

            // EXIT LOOP if monitoring stopped
            if !state_monitor
                .is_monitoring
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                println!("Activity: Stopping Activity Loop (Inactive)");
                state_monitor
                    .is_activity_loop_running
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                break;
            }

            // Get state for credentials/paths
            let app_state = app_monitor.state::<AppState>();
            let db_path = app_state.db_path.clone();

            // Perform check in background thread
            if let Ok(conn) = Connection::open(&db_path) {
                if let Ok(Some(user)) = db::get_user(&conn) {
                    if let Some(pid) = user.current_project_id {
                        if let Ok(Some(session)) = db::get_active_session(&conn, &pid) {
                            // Get Active Window (OS call)
                            if let Ok(window) = get_active_window() {
                                let app_name = window.app_name;
                                let window_title = window.title;

                                // Get URL if browser (osascript call on macOS)
                                let url = get_browser_url(&app_name);

                                println!(
                                    "Activity Log: app=\"{}\", title=\"{}\", url={:?}",
                                    app_name, window_title, url
                                );

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
        }
    });
}
