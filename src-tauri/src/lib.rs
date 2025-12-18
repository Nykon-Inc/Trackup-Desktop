use rusqlite::Connection;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
mod db;

mod models;
mod tray_generator;

use models::User;
// we don't need `Project` in lib.rs anymore unless we use it explicitly, but it's part of User.

struct AppState {
    db_path: PathBuf,
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
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn stop_timer(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = Connection::open(&state.db_path).map_err(|e| e.to_string())?;

    let user_opt = db::get_user(&conn).map_err(|e| e.to_string())?;
    if let Some(user) = user_opt {
        if let Some(project_id) = user.current_project_id {
            db::stop_session(&conn, &project_id).map_err(|e| e.to_string())?;
            update_tray(&app, true, &user.email); // Refresh menu state
        }
    }
    Ok(())
}

fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, secs)
}

fn update_tray(app: &AppHandle, is_logged_in: bool, email: &str) {
    let state = app.state::<AppState>();
    // We need to fetch current state to enable/disable items correctly
    let mut current_project_name = "None".to_string();
    let mut has_active_session = false;
    let mut is_project_selected = false;

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

            // Start Timer (Enabled if NO active session and project selected)
            let start_enabled = !has_active_session && is_project_selected;
            let _ = menu.append(
                &MenuItem::with_id(
                    app,
                    "start_timer",
                    "Start Timer",
                    start_enabled,
                    None::<&str>,
                )
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
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle();
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("auth_v2.db");

            app.manage(AppState {
                db_path: db_path.clone(),
            });

            // Init DB
            if let Err(e) = db::init_db(&db_path) {
                eprintln!("Failed to init db: {}", e);
            }

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
                                    should_update_db = true;
                                    active_session_id = session.id;

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
                                let _ = db::update_session_heartbeat(&conn, sid);
                            }
                        }
                    }

                    if let Some(tray) = app_handle_for_thread.tray_by_id("main") {
                        if let Some(icon) = tray_generator::generate_tray_icon(&time_str) {
                            let _ = tray.set_icon(Some(icon));
                        }
                    }
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            greet,
            login,
            logout,
            check_auth,
            set_current_project,
            start_timer,
            stop_timer
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
