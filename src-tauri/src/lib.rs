use chrono::Local;
use rusqlite::Connection;
use std::path::PathBuf;
use std::process::Command;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Listener, Manager,
};
mod db;

mod activity;
mod idle;
mod models;
mod screenshot;
mod tray_generator;

// Use relevant types from the plugin or underlying crates if needed
// but for commands we can just call them if they are re-exported.

// ...

use std::sync::atomic::Ordering;
use std::sync::Arc;
// use std::time::Instant;

use idle::IdleState;

use models::User;
// we don't need `Project` in lib.rs anymore unless we use it explicitly, but it's part of User.

pub struct AppState {
    pub db_path: PathBuf,
    pub idle_state: Arc<IdleState>,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn login(app: AppHandle, user: User) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    db::save_user(&mut conn, &user).map_err(|e| e.to_string())?;

    update_tray(&app, true, &user.email);
    // Sync Daily Sessions from server
    screenshot::sync_daily_sessions(&app);
    Ok(())
}

#[tauri::command]
fn set_current_project(app: AppHandle, project_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    db::set_current_project(&conn, &project_id).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn logout(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    db::clear_user(&conn).map_err(|e| e.to_string())?;

    update_tray(&app, false, "");
    Ok(())
}

#[tauri::command]
fn check_auth(app: AppHandle) -> Result<Option<User>, String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    db::get_user(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn start_timer(app: AppHandle) -> Result<(), String> {
    start_timer_internal(&app)
}

fn start_timer_internal(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    let user_opt = db::get_user(&conn).map_err(|e| e.to_string())?;
    if let Some(user) = user_opt {
        if let Some(project_id) = user.current_project_id {
            // Check if already active
            let active = db::get_active_session(&conn, &project_id).map_err(|e| e.to_string())?;
            if active.is_none() {
                db::start_session(&conn, &project_id).map_err(|e| e.to_string())?;
                update_tray(&app, true, &user.email); // Refresh menu state

                // Enable Idle Monitoring
                state.idle_state.is_monitoring.store(true, Ordering::SeqCst);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                state
                    .idle_state
                    .last_activity_timestamp
                    .store(now, Ordering::Relaxed);

                // Reset Activity Counts for new session
                state.idle_state.keyboard_count.store(0, Ordering::Relaxed);
                state.idle_state.mouse_count.store(0, Ordering::Relaxed);

                // Start Monitoring Loops (If not already running)
                screenshot::start_capture_loop(app.clone(), state.idle_state.clone());
                activity::start_activity_loop(app.clone(), state.idle_state.clone());

                let _ = app.emit("timer-active", true);
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn stop_timer(app: AppHandle) -> Result<(), String> {
    stop_timer_internal(&app)
}

fn stop_timer_internal(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    let user_opt = db::get_user(&conn).map_err(|e| e.to_string())?;
    if let Some(user) = user_opt {
        if let Some(project_id) = user.current_project_id {
            db::stop_session(&conn, &project_id).map_err(|e| e.to_string())?;
            update_tray(&app, true, &user.email); // Refresh menu state

            // Disable Idle Monitoring
            state
                .idle_state
                .is_monitoring
                .store(false, Ordering::SeqCst);

            let _ = app.emit("timer-active", false);
        }
    }
    Ok(())
}

#[tauri::command]
fn process_idle_choice(app: AppHandle, idle_time: i64, keep: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    let user_opt = db::get_user(&conn).map_err(|e| e.to_string())?;
    if let Some(user) = user_opt {
        if let Some(project_id) = user.current_project_id {
            // Logic:
            // 1. We assume the session was JUST stopped (because modal is shown).
            //    So we need to find the latest (closed) session to update.
            //    Or we could just update "last session for this project".

            // Wait, db::process_idle_time updates "active" session.
            // BUT we stopped the session!
            // So we need a new db function or modify process_idle_time to target the latest closed session.
            // Let's modify db.rs later? Or write a raw query here?
            // Cleanest is to have db::process_last_session_idle_time

            // For now let's assume we implement `process_last_session_idle_time` in db.rs
            // Or we check `db.rs` manually.

            // Using a manual update for expediency to match the logic requested:
            // "if user says discard... subtract idle time from total time... restart timer"

            // Deduct logic:
            // The session is closed. total_time = end - start - deducted.
            // We want to increase `deducted` by idle_time if Discard.
            // We want to increase `idle_seconds` by idle_time if Keep.

            let inc_idle = idle_time;
            let inc_deducted = if !keep { idle_time } else { 0 };

            conn.execute(
                "UPDATE sessions 
                 SET idle_seconds = idle_seconds + ?1, 
                     deducted_seconds = deducted_seconds + ?2 
                 WHERE id = (SELECT id FROM sessions WHERE project_id = ?3 ORDER BY start_time DESC LIMIT 1)",
                (inc_idle, inc_deducted, &project_id),
            ).map_err(|e| e.to_string())?;

            // Restart Timer
            start_timer_internal(&app)?;
        }
    }
    Ok(())
}

#[tauri::command]
fn force_quit(_app: AppHandle) {
    std::process::exit(0);
}

#[tauri::command]
fn upload_and_quit(app: AppHandle) {
    screenshot::upload_pending_screenshots(&app);
    std::process::exit(0);
}

#[tauri::command]
fn get_project_today_total(app: AppHandle, project_id: String) -> Result<String, String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    let total_secs = db::get_today_total_time(&conn, &project_id).map_err(|e| e.to_string())?;
    Ok(format_duration(total_secs))
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

#[tauri::command]
async fn check_permissions() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        serde_json::json!({
            "accessibility": tauri_plugin_macos_permissions::check_accessibility_permission().await,
            "screenRecording": tauri_plugin_macos_permissions::check_screen_recording_permission().await,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        serde_json::json!({
            "accessibility": true,
            "screenRecording": true,
        })
    }
}

#[tauri::command]
async fn open_permissions_settings(type_name: String) {
    println!("Opening permissions settings for: {}", type_name);
    #[cfg(target_os = "macos")]
    {
        match type_name.as_str() {
            "accessibility" => {
                println!("Requesting accessibility permission and opening settings");
                tauri_plugin_macos_permissions::request_accessibility_permission().await;
                let _ = Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                    .spawn();
            }
            "screenRecording" => {
                println!("Requesting screen recording permission and opening settings");
                tauri_plugin_macos_permissions::request_screen_recording_permission().await;
                let _ = Command::new("open")
                    .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                    .spawn();
            }
            _ => {
                println!("Unknown permission type: {}", type_name);
            }
        }
    }
}

fn update_tray(app: &AppHandle, is_logged_in: bool, email: &str) {
    let state = app.state::<AppState>();
    // We need to fetch current state to enable/disable items correctly
    let mut current_project_name = "None".to_string();
    let mut has_active_session = false;
    let mut is_project_selected = false;
    let permissions_granted = {
        #[cfg(target_os = "macos")]
        {
            // Simple sync check for tray (re-implementing bits to avoid async in tray)
            let mut granted = macos_accessibility_client::accessibility::application_is_trusted();

            // For screen recording, we preflight via extern
            extern "C" {
                fn CGPreflightScreenCaptureAccess() -> bool;
            }
            unsafe {
                granted = granted && CGPreflightScreenCaptureAccess();
            }
            granted
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    };

    if is_logged_in {
        if let Ok(conn) = Connection::open(&state.db_path) {
            if let Ok(Some(user)) = db::get_user(&conn) {
                if let Some(pid) = &user.current_project_id {
                    is_project_selected = true;
                    // Find project name
                    if let Some(p) = user.projects.iter().find(|p| &p.id == pid) {
                        current_project_name = p.name.clone();
                    }

                    if let Ok(Some(_)) = db::get_active_session(&conn, pid) {
                        has_active_session = true;
                    }
                }
            }
        }
    }

    let db_status = if is_logged_in {
        format!("Logged in as {}", email)
    } else {
        "Not Logged In".to_string()
    };

    let project_item_text = format!("Project: {}", current_project_name);

    if let Ok(menu) = Menu::new(app) {
        let _ = menu.append(
            &MenuItem::with_id(app, "show", "Show StaffWatch", true, None::<&str>).unwrap(),
        );
        let _ = menu
            .append(&MenuItem::with_id(app, "status", &db_status, false, None::<&str>).unwrap());

        if is_logged_in {
            let _ = menu.append(
                &MenuItem::with_id(
                    app,
                    "project_display",
                    &project_item_text,
                    false,
                    None::<&str>,
                )
                .unwrap(),
            );

            // Start Timer (Enabled if NO active session and project selected and permissions granted)
            let start_enabled = !has_active_session && is_project_selected && permissions_granted;
            let start_text = if !permissions_granted {
                "Start Timer (Permissions Missing)"
            } else {
                "Start Timer"
            };
            let _ = menu.append(
                &MenuItem::with_id(app, "start_timer", start_text, start_enabled, None::<&str>)
                    .unwrap(),
            );

            // Stop Timer (Enabled if active session)
            let stop_enabled = has_active_session;
            let _ = menu.append(
                &MenuItem::with_id(app, "stop_timer", "Stop Timer", stop_enabled, None::<&str>)
                    .unwrap(),
            );

            let _ = menu
                .append(&MenuItem::with_id(app, "logout", "Logout", true, None::<&str>).unwrap());
        } else {
            let _ =
                menu.append(&MenuItem::with_id(app, "login", "Login", true, None::<&str>).unwrap());
        }

        let _ = menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap());

        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle();
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("auth_v2.db");

            let idle_state = Arc::new(IdleState::new());

            app.manage(AppState {
                db_path: db_path.clone(),
                idle_state: idle_state.clone(),
            });

            // Init DB
            if let Err(e) = db::init_db(&db_path) {
                eprintln!("Failed to init db: {}", e);
            }

            // Ensure timer is stopped on startup
            let _ = stop_timer_internal(&app_handle);

            // process pending screenshots on startup
            screenshot::upload_pending_screenshots(&app_handle);
            // Sync Daily Sessions from server
            screenshot::sync_daily_sessions(&app_handle);

            // Start Idle Check (Event Tap)
            idle::start_idle_check(app_handle.clone(), idle_state.clone());
            // Start Permanent Sync Loop (screenshots and sessions)
            screenshot::start_screenshot_monitor(app_handle.clone());
            // (Capture and Activity loops only start when timer is ON)

            // Listen for Internal Idle Event
            let app_handle_for_idle = app_handle.clone();
            app.listen("internal:idle_gap_detected", move |event| {
                if let Ok(duration) = serde_json::from_str::<u64>(&event.payload()) {
                    // Logic: Stop Timer -> Show Window -> Emit idle_ended
                    let _ = stop_timer_internal(&app_handle_for_idle);

                    let app_inner = app_handle_for_idle.clone();
                    let _ = app_handle_for_idle.run_on_main_thread(move || {
                        if let Some(window) = app_inner.get_webview_window("idle") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            // Emit to specific window or global?
                            // Global emit handles it for now, React component in IdleWindow listens.
                            let _ = app_inner.emit("idle_ended", duration);
                        }
                    });
                }
            });

            // Initial auth check
            let mut is_logged_in = false;
            let mut email = String::new();
            if let Ok(conn) = Connection::open(&db_path) {
                if let Ok(mut stmt) = conn.prepare("SELECT email FROM users LIMIT 1") {
                    if let Ok(mut rows) = stmt.query([]) {
                        if let Ok(Some(row)) = rows.next() {
                            is_logged_in = true;
                            email = row.get(0).unwrap_or_default();
                        }
                    }
                }
            }

            // Build Tray
            let menu = Menu::new(app).unwrap();

            let mut tray_builder = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "quit" => app.exit(0),
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "login" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.emit("request-login", ());
                        }
                    }
                    "logout" => {
                        let _ = logout(app.clone());
                        let _ = app.emit("logout-user", ());
                    }
                    "start_timer" => {
                        let _ = start_timer(app.clone());
                    }
                    "stop_timer" => {
                        let _ = stop_timer(app.clone());
                    }
                    _ => {}
                });

            if let Some(icon) = tray_generator::generate_tray_icon("--:--:--") {
                tray_builder = tray_builder.icon(icon);
            } else {
                eprintln!("Failed to generate tray icon, using default.");
                tray_builder = tray_builder.icon(app.default_window_icon().unwrap().clone());
            }

            tray_builder.build(app)?;

            update_tray(&app_handle, is_logged_in, &email);

            if is_logged_in {
                let app_handle_clone = app_handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    let _ = app_handle_clone.emit("request-login", ());
                });
            }

            // Spawn a thread to update the tray icon every second
            let app_handle_for_thread = app_handle.clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(1));

                    let state = app_handle_for_thread.state::<AppState>();

                    let mut time_str = "--:--:--".to_string();
                    let mut should_update_db = false;
                    let mut active_session_id = None;

                    if let Ok(conn) = Connection::open(&state.db_path) {
                        if let Ok(Some(user)) = db::get_user(&conn) {
                            if let Some(project_id) = user.current_project_id {
                                // Check active session
                                if let Ok(Some(session)) =
                                    db::get_active_session(&conn, &project_id)
                                {
                                    // Check if session exceeds 10 minutes (600s * 1000ms)
                                    let now_ms = Local::now().timestamp_millis();
                                    let duration = now_ms - session.start_time;

                                    if duration >= 10 * 60 * 1000 {
                                        // Stop current session
                                        let _ = db::stop_session(&conn, &project_id);
                                        // Start new session
                                        let _ = db::start_session(&conn, &project_id);

                                        // Reset activity counts for the new session
                                        state.idle_state.keyboard_count.store(0, Ordering::Relaxed);
                                        state.idle_state.mouse_count.store(0, Ordering::Relaxed);

                                        // Refresh active session info
                                        if let Ok(Some(new_session)) =
                                            db::get_active_session(&conn, &project_id)
                                        {
                                            should_update_db = true;
                                            active_session_id = new_session.id;
                                        }
                                    } else {
                                        should_update_db = true;
                                        active_session_id = session.id;
                                    }

                                    // Calculate total time
                                    if let Ok(total) = db::get_today_total_time(&conn, &project_id)
                                    {
                                        time_str = format_duration(total);
                                    }
                                } else {
                                    // No active session, just show total
                                    if let Ok(total) = db::get_today_total_time(&conn, &project_id)
                                    {
                                        time_str = format_duration(total);
                                    } else {
                                        time_str = "00:00:00".to_string();
                                    }
                                }
                            } else {
                                // Logged in but no project -> 00:00:00 per specs? or --:--:--?
                                // "if logged in and no sessions for current project" -> implies project selected.
                                // If no project selected, maybe 00:00:00?
                                time_str = "00:00:00".to_string();
                            }
                        }
                    }

                    // Update DB if active session
                    if should_update_db {
                        if let Some(sid) = active_session_id {
                            if let Ok(conn) = Connection::open(&state.db_path) {
                                let k_count =
                                    state.idle_state.keyboard_count.load(Ordering::Relaxed) as i64;
                                let m_count =
                                    state.idle_state.mouse_count.load(Ordering::Relaxed) as i64;
                                let _ = db::update_session_heartbeat(&conn, sid, k_count, m_count);
                            }
                        }
                    }

                    if let Some(tray) = app_handle_for_thread.tray_by_id("main") {
                        if let Some(icon) = tray_generator::generate_tray_icon(&time_str) {
                            let _ = tray.set_icon(Some(icon));
                        }
                    }

                    // Emit time update to Vite app
                    let _ = app_handle_for_thread.emit("time-update", &time_str);
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_macos_permissions::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            greet,
            login,
            logout,
            check_auth,
            set_current_project,
            start_timer,
            stop_timer,
            process_idle_choice,
            force_quit,
            upload_and_quit,
            get_project_today_total,
            check_permissions,
            open_permissions_settings
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let state = app_handle.state::<AppState>();
            let mut has_pending = false;
            if let Ok(conn) = Connection::open(&state.db_path) {
                if let Ok(pending) = db::get_pending_screenshots(&conn) {
                    if !pending.is_empty() {
                        has_pending = true;
                    }
                }
            }

            if has_pending {
                api.prevent_exit();
                if let Some(window) = app_handle.get_webview_window("quit") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    // We don't need to emit "request-quit-confirmation", the QuitWindow just appears.
                    // But if we want to pass data or trigger something, we could.
                    // The QuitWindow component invokes "upload_and_quit" on confirmation.
                }
            }
        }
    });
}
