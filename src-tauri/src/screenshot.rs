use crate::api;
use crate::db;
use crate::idle::IdleState;
use crate::models::SessionPayload;
use crate::AppState;
use base64::{engine::general_purpose, Engine as _};
use image::imageops::FilterType;
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

pub fn start_screenshot_monitor<R: Runtime>(app: AppHandle<R>) {
    // 1. Permanent Sync Loop (Runs every 3 mins regardless of timer)
    let app_sync = app.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(180));
        upload_pending_screenshots(&app_sync);
    });
}

pub fn start_capture_loop<R: Runtime>(app: AppHandle<R>, state: Arc<IdleState>) {
    // Ensure only one loop runs
    if state
        .is_capture_loop_running
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }

    let app_monitor = app.clone();
    let state_monitor = state.clone();

    thread::spawn(move || {
        println!("Monitor: Starting Capture Loop");
        // Use a fixed 2-minute interval (120 seconds)
        let mut next_capture_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 120;

        loop {
            thread::sleep(Duration::from_secs(10));

            // EXIT LOOP if monitoring stopped
            if !state_monitor
                .is_monitoring
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                println!("Monitor: Stopping Capture Loop (Inactive)");
                state_monitor
                    .is_capture_loop_running
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                break;
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if now >= next_capture_time {
                println!("Monitor: Time to capture screenshot");

                let app_inner = app_monitor.clone();

                let _ = app_monitor.run_on_main_thread(move || {
                    let app_state = app_inner.state::<AppState>();
                    let db_path = app_state.db_path.lock().unwrap().clone();
                    if let Ok(conn) = Connection::open(&db_path) {
                        if let Ok(Some(user)) = db::get_user(&conn) {
                            if let Some(pid) = user.current_project_id {
                                if let Ok(Some(session)) = db::get_active_session(&conn, &pid) {
                                    match capture_screen() {
                                        Ok(b64) => {
                                            let _ = db::save_pending_screenshot(
                                                &conn,
                                                &session.uuid,
                                                &pid,
                                                &b64,
                                            );
                                            println!("Monitor: Screenshot saved.");
                                        }
                                        Err(e) => eprintln!("Monitor: Capture failed: {}", e),
                                    }
                                }
                            }
                        }
                    }
                });

                next_capture_time = now + 120;
            }
        }
    });
}

pub fn upload_pending_screenshots<R: Runtime>(app: &AppHandle<R>) {
    println!("Monitor: Time to upload pending items");
    let app_handle = app.clone();

    async_runtime::spawn(async move {
        // 1. Fetch Data (Blocking DB op)
        let app_state = app_handle.state::<AppState>();
        let db_path = app_state.db_path.lock().unwrap().clone();

        let db_path_fetch = db_path.clone();
        let data_op = async_runtime::spawn_blocking(move || {
            if let Ok(conn) = Connection::open(&db_path_fetch) {
                let _user = db::get_user(&conn).ok().flatten();
                let pending_sc = db::get_pending_screenshots(&conn).unwrap_or_default();
                let pending_sess = db::get_pending_sessions(&conn).unwrap_or_default();

                let mut session_logs = std::collections::HashMap::new();
                for sess in &pending_sess {
                    if let Ok(logs) = db::get_activity_logs_for_session(&conn, &sess.uuid) {
                        session_logs.insert(sess.uuid.clone(), logs);
                    }
                }

                Ok((_user, pending_sc, pending_sess, session_logs))
            } else {
                Err("Failed to open DB")
            }
        })
        .await;

        if let Ok(Ok((_, pending_sc, pending_sess, session_logs))) = data_op {
            // 2. Bulk Session Sync
            let mut synced_session_uuids = Vec::new();
            if !pending_sess.is_empty() {
                println!("Monitor: Syncing {} sessions...", pending_sess.len());
                let endpoint = "/desktop/sessions";

                // Log total activity for this sync batch
                let mut total_kb = 0;
                let mut total_ms = 0;
                for s in &pending_sess {
                    total_kb += s.keyboard_events;
                    total_ms += s.mouse_events;
                    println!(
                        "Activity Log [Syncing Session {}]: Keyboards={}, Mouse={}",
                        s.uuid, s.keyboard_events, s.mouse_events
                    );
                }
                println!(
                    "Activity Log [Bulk Sync Total]: Keyboards={}, Mouse={}",
                    total_kb, total_ms
                );

                let payload_data: Vec<SessionPayload> = pending_sess
                    .iter()
                    .map(|s| {
                        let logs = session_logs.get(&s.uuid).cloned().unwrap_or_default();
                        SessionPayload {
                            uuid: s.uuid.clone(),
                            project_id: s.project_id.clone(),
                            project_type: s.project_type.clone(),
                            duration_minutes: s.duration_minutes,
                            target_name: s.target_name.clone(),
                            start_time: s.start_time,
                            end_time: s.end_time,
                            is_active: s.is_active,
                            idle_seconds: s.idle_seconds,
                            deducted_seconds: s.deducted_seconds,
                            keyboard_events: s.keyboard_events,
                            mouse_events: s.mouse_events,
                            activity_logs: if logs.is_empty() { None } else { Some(logs) },
                        }
                    })
                    .collect();

                let payload = json!(payload_data);

                match api::request(&app_handle, reqwest::Method::POST, endpoint, Some(&payload))
                    .await
                {
                    Ok(response) => {
                        if response.status().is_success() {
                            println!("Monitor: Bulk session sync success.");
                            for s in &pending_sess {
                                synced_session_uuids.push((s.uuid.clone(), s.is_active));
                            }
                        } else {
                            eprintln!(
                                "Monitor: Bulk session sync failed. Status: {}",
                                response.status()
                            );
                        }
                    }
                    Err(e) => eprintln!("Monitor: Session bulk request error: {}", e),
                }
            }

            // 3. Parallel Screenshot Upload (One by One)
            let mut uploaded_screenshot_ids = Vec::new();
            let mut screenshot_handles = Vec::new();

            if !pending_sc.is_empty() {
                println!(
                    "Monitor: Uploading {} screenshots individually...",
                    pending_sc.len()
                );

                for (id, session_uuid, project_id, timestamp, image_data) in pending_sc {
                    let app_inner = app_handle.clone();

                    let task = async_runtime::spawn(async move {
                        println!(
                            "Monitor: Uploading screenshot {} for session {}",
                            id, session_uuid
                        );

                        let payload = json!({
                            "sessionUuid": session_uuid,
                            "projectId": project_id,
                            "timestamp": timestamp,
                            "image": image_data,
                            "fileExt": "webp"
                        });

                        match api::request(
                            &app_inner,
                            reqwest::Method::POST,
                            "/desktop/screenshots",
                            Some(&payload),
                        )
                        .await
                        {
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
                    screenshot_handles.push(task);
                }
            }

            // Collect results
            for handle in screenshot_handles {
                if let Ok(Some(id)) = handle.await {
                    uploaded_screenshot_ids.push(id);
                }
            }

            // 4. Batch Update/Delete (Blocking DB op)
            if !synced_session_uuids.is_empty() || !uploaded_screenshot_ids.is_empty() {
                let db_path_sync = db_path.clone();
                let _ = async_runtime::spawn_blocking(move || {
                    if let Ok(conn) = Connection::open(&db_path_sync) {
                        // Use a transaction for safety
                        if let Ok(tx) = conn.unchecked_transaction() {
                            for (uuid, is_active) in synced_session_uuids {
                                if !is_active {
                                    let _ = tx.execute(
                                        "UPDATE sessions SET status = 'done' WHERE uuid = ?1",
                                        [&uuid],
                                    );
                                    let _ = db::delete_activity_logs_for_session(&tx, &uuid);
                                }
                            }
                            for id in uploaded_screenshot_ids {
                                let _ = tx
                                    .execute("DELETE FROM pending_screenshots WHERE id = ?1", [id]);
                            }
                            let _ = tx.commit();
                        }
                    }
                })
                .await;
            }
        } else {
            // Quiet failure
        }
    });
}

pub fn sync_daily_sessions<R: Runtime>(app: &AppHandle<R>) {
    println!("Monitor: Syncing daily sessions from server...");
    let app_handle = app.clone();

    async_runtime::spawn(async move {
        let app_state = app_handle.state::<AppState>();
        let db_path = app_state.db_path.lock().unwrap().clone();

        let endpoint = "/desktop/sessions/today";
        match api::request::<R, ()>(&app_handle, reqwest::Method::GET, endpoint, None).await {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(server_sessions) =
                        response.json::<Vec<crate::models::SyncSession>>().await
                    {
                        println!(
                            "Monitor: Fetched {} sessions from server.",
                            server_sessions.len()
                        );
                        let _ = async_runtime::spawn_blocking(move || {
                            if let Ok(conn) = Connection::open(&db_path) {
                                for server_session in server_sessions {
                                    if let Ok(local_opt) =
                                        db::get_session_by_uuid(&conn, &server_session.uuid)
                                    {
                                        match local_opt {
                                            Some(local_session) => {
                                                let local_duration =
                                                    if let Some(end) = local_session.end_time {
                                                        end.saturating_sub(local_session.start_time)
                                                    } else {
                                                        let now = SystemTime::now()
                                                            .duration_since(UNIX_EPOCH)
                                                            .unwrap()
                                                            .as_millis()
                                                            as i64;
                                                        now.saturating_sub(local_session.start_time)
                                                    };

                                                let server_duration = if let Some(end) =
                                                    server_session.end_time
                                                {
                                                    end.saturating_sub(server_session.start_time)
                                                } else {
                                                    0
                                                };

                                                if server_duration > local_duration {
                                                    let _ = db::update_imported_session(
                                                        &conn,
                                                        &server_session,
                                                    );
                                                }
                                            }
                                            None => {
                                                let _ = db::create_imported_session(
                                                    &conn,
                                                    &server_session,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        })
                        .await;
                    }
                } else {
                    eprintln!(
                        "Monitor: Failed to fetch daily sessions. Status: {}",
                        response.status()
                    );
                }
            }
            Err(e) => eprintln!("Monitor: Fetch daily sessions error: {}", e),
        }
    });
}
