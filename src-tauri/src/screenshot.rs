use crate::db;
use crate::idle::IdleState;
use crate::AppState;
use base64::{engine::general_purpose, Engine as _};
use image::imageops::FilterType;
use rand::Rng;
use rusqlite::Connection;
use serde_json::json;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{async_runtime, AppHandle, Manager, Runtime};

pub fn capture_screen() -> Result<String, String> {
    use xcap::Monitor;

    let monitors = Monitor::all().map_err(|e| e.to_string())?;
    // Prefer the first monitor or primary
    let monitor = monitors.first().ok_or("No monitor found")?;

    // xcap capture returns an image::RgbaImage buffer in recent versions
    let image_buffer = monitor.capture_image().map_err(|e| e.to_string())?;

    // Convert to DynamicImage for resizing
    let dynamic_image = image::DynamicImage::ImageRgba8(image_buffer);

    // Resize (e.g., width 800, maintain aspect ratio)
    let resized = dynamic_image.resize(800, 600, FilterType::Lanczos3);

    let (w, h) = (resized.width(), resized.height());
    // Convert resized back to rgba8
    let raw_data = resized.to_rgba8().into_raw();

    let encoder = webp::Encoder::from_rgba(&raw_data, w, h);
    let webp_memory = encoder.encode(75.0); // 75% quality

    // Convert to Base64
    let b64 = general_purpose::STANDARD.encode(&*webp_memory);

    Ok(b64)
}

pub fn start_screenshot_monitor<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    let app_monitor = app.clone();
    let state_monitor = state.clone();

    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        // Initial random delay 5-10 mins
        let mut next_capture_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + rng.gen_range(3..10);

        let mut next_upload_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 120; // 10 mins

        loop {
            thread::sleep(Duration::from_secs(10)); // Check every 10s
            println!("Monitor: loop");
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // 1. Capture Logic
            if state_monitor
                .is_monitoring
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                if now >= next_capture_time {
                    println!("Monitor: Time to capture screenshot");

                    // Dispatch to Main Thread for Capture
                    let app_inner = app_monitor.clone();
                    let _ = app_monitor.run_on_main_thread(move || {
                        // Access DB to get active session
                        let app_state = app_inner.state::<AppState>();
                        if let Ok(conn) = Connection::open(&app_state.db_path) {
                            if let Ok(Some(user)) = db::get_user(&conn) {
                                if let Some(pid) = user.current_project_id {
                                    if let Ok(Some(session)) = db::get_active_session(&conn, &pid) {
                                        // Capture
                                        match capture_screen() {
                                            Ok(b64) => {
                                                if let Err(e) = db::save_pending_screenshot(
                                                    &conn,
                                                    &session.uuid,
                                                    &pid,
                                                    &b64,
                                                ) {
                                                    eprintln!(
                                                        "Monitor: Failed to save screenshot: {}",
                                                        e
                                                    );
                                                } else {
                                                    println!("Monitor: Screenshot saved.");
                                                }
                                            }
                                            Err(e) => eprintln!("Monitor: Capture failed: {}", e),
                                        }
                                    }
                                }
                            }
                        }
                    });

                    // Schedule next capture
                    next_capture_time = now + rng.gen_range(30..60);
                }
            } else {
                // If not monitoring, push next_capture_time forward so we don't snap immediately on resume
                // Logic: keep pushing it so it's always "5-10 mins from now" if idle
                if now >= next_capture_time {
                    next_capture_time = now + rng.gen_range(30..60);
                }
            }

            // 2. Upload Logic (Run regardless of idle state, as long as app is open)
            if now >= next_upload_time {
                upload_pending_screenshots(&app_monitor);
                next_upload_time = now + 120;
            }
        }
    });
}

pub fn upload_pending_screenshots<R: Runtime>(app: &AppHandle<R>) {
    println!("Monitor: Time to upload screenshots");
    let app_handle = app.clone();

    async_runtime::spawn(async move {
        // 1. Fetch Data (Blocking DB op)
        let app_state = app_handle.state::<AppState>();
        let db_path = app_state.db_path.clone(); // PathBuf is cloneable

        let data_op = async_runtime::spawn_blocking(move || {
            if let Ok(conn) = Connection::open(&db_path) {
                let user = db::get_user(&conn).ok().flatten();
                let pending = db::get_pending_screenshots(&conn).unwrap_or_default();
                Ok((user, pending))
            } else {
                Err("Failed to open DB")
            }
        })
        .await;

        // Unwrap the Move result and Result
        if let Ok(Ok((Some(user), pending))) = data_op {
            if pending.is_empty() {
                println!("Monitor: No pending screenshots.");
                return;
            }

            let token = user.token;
            // Assuming API URL from env or fallback
            // We can't easily access Vite env here, so we default to localhost or need config
            let api_url = "http://localhost:8000/v1/screenshots";
            let client = reqwest::Client::new();
            let mut handles = Vec::new();

            // 2. Spawn parallel upload tasks
            for (id, session_uuid, project_id, timestamp, image_data) in pending {
                let client = client.clone();
                let token = token.clone();
                let url = api_url.to_string();

                let task = async_runtime::spawn(async move {
                    println!(
                        "Monitor: Uploading screenshot {} for session {}",
                        id, session_uuid
                    );

                    let payload = json!({
                        "session_uuid": session_uuid,
                        "project_id": project_id,
                        "timestamp": timestamp,
                        "image": image_data, // Base64
                        "file_ext": "webp"
                    });

                    let res = client
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", token))
                        .json(&payload)
                        .send()
                        .await;

                    match res {
                        Ok(response) => {
                            if response.status().is_success() {
                                println!("Monitor: Upload success for {}", id);
                                Some(id)
                            } else {
                                eprintln!(
                                    "Monitor: Upload failed for {}. Status: {}",
                                    id,
                                    response.status()
                                );
                                None
                            }
                        }
                        Err(e) => {
                            eprintln!("Monitor: Request error for {}: {}", id, e);
                            None
                        }
                    }
                });
                handles.push(task);
            }

            // 3. Collect successful IDs
            let mut successful_ids = Vec::new();
            for handle in handles {
                if let Ok(Some(id)) = handle.await {
                    successful_ids.push(id);
                }
            }

            // 4. Batch Delete (Blocking DB op)
            if !successful_ids.is_empty() {
                let db_path_del = app_state.db_path.clone();
                let _ = async_runtime::spawn_blocking(move || {
                    if let Ok(conn) = Connection::open(&db_path_del) {
                        for id in successful_ids {
                            let _ = db::delete_pending_screenshot(&conn, id);
                        }
                    }
                })
                .await;
            }
        } else {
            println!("Monitor: Could not fetch user or pending screenshots (or db error).");
        }
    });
}
