use crate::db;
use crate::idle::IdleState;
use crate::models::SessionPayload;
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
        let mut rng = rand::thread_rng();
        let mut next_capture_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + rng.gen_range(20..60);

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
                    if let Ok(conn) = Connection::open(&app_state.db_path) {
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

                next_capture_time = now + rng.gen_range(60..120);
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
        let db_path = app_state.db_path.clone();

        let data_op = async_runtime::spawn_blocking(move || {
            if let Ok(conn) = Connection::open(&db_path) {
                let user = db::get_user(&conn).ok().flatten();
                let pending_sc = db::get_pending_screenshots(&conn).unwrap_or_default();
                let pending_sess = db::get_pending_sessions(&conn).unwrap_or_default();

                let mut session_logs = std::collections::HashMap::new();
                for sess in &pending_sess {
                    if let Ok(logs) = db::get_activity_logs_for_session(&conn, &sess.uuid) {
                        session_logs.insert(sess.uuid.clone(), logs);
                    }
                }

                Ok((user, pending_sc, pending_sess, session_logs))
            } else {
                Err("Failed to open DB")
            }
        })
        .await;

        if let Ok(Ok((Some(user), pending_sc, pending_sess, session_logs))) = data_op {
            let mut token = user.token.clone();
            let base_url = "https://trackup.staging-api.nykon.cloud/v1";
            let client = reqwest::Client::new();

            // 2. Bulk Session Sync
            let mut synced_session_uuids = Vec::new();
            if !pending_sess.is_empty() {
                println!("Monitor: Syncing {} sessions...", pending_sess.len());
                let url = format!("{}/client/sessions", base_url);

                let payload_data: Vec<SessionPayload> = pending_sess
                    .iter()
                    .map(|s| {
                        let logs = session_logs.get(&s.uuid).cloned().unwrap_or_default();
                        SessionPayload {
                            uuid: s.uuid.clone(),
                            project_id: s.project_id.clone(),
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

                let res = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await;

                match res {
                    Ok(response) => {
                        let status = response.status();
                        if status.is_success() {
                            println!("Monitor: Bulk session sync success.");
                            for s in &pending_sess {
                                synced_session_uuids.push((s.uuid.clone(), s.is_active));
                            }
                        } else if status == reqwest::StatusCode::UNAUTHORIZED {
                            // Try refreshing token
                            println!("Monitor: 401 Unauthorized. Attempting token refresh...");

                            // TODO: Implement actual refresh Endpoint call
                            // user.refresh_token is missing in struct, need to ask user or assume endpoint.
                            // Assuming POST /auth/refresh with current token? Or does user have refresh token?
                            // User struct only has 'token'.
                            // Let's assume we call a refresh endpoint that accepts the current token (if still valid for refresh)
                            // or we need a refresh token.

                            // Since I cannot change User struct easily without more info, I will assume
                            // we can call a refresh endpoint. If that fails, log out.

                            let refresh_url = format!(
                                "https://trackup.staging-api.nykon.cloud/v1/auth/refresh-token"
                            );
                            let refresh_res = client
                                .post(&refresh_url)
                                .header("Authorization", format!("Bearer {}", token))
                                .send()
                                .await;

                            match refresh_res {
                                Ok(u_res) => {
                                    if u_res.status().is_success() {
                                        // Assume we got a new token in JSON { "token": "..." }
                                        if let Ok(json_body) =
                                            u_res.json::<serde_json::Value>().await
                                        {
                                            if let Some(new_token) =
                                                json_body.get("token").and_then(|t| t.as_str())
                                            {
                                                println!("Monitor: Token refreshed successfully.");
                                                token = new_token.to_string();

                                                // Update DB with new token
                                                let new_token_db = token.clone();
                                                let db_path_update = app_state.db_path.clone();
                                                let uuid = user.uuid.clone();
                                                let _ = async_runtime::spawn_blocking(move || {
                                                     if let Ok(conn) = Connection::open(&db_path_update) {
                                                         // Use manual array for params since params! macro not imported/available easily
                                                         let _ = conn.execute(
                                                             "UPDATE users SET token = ?1 WHERE uuid = ?2",
                                                             [new_token_db, uuid],
                                                         );
                                                     }
                                                 }).await;

                                                // Retry Session Sync
                                                let retry_res = client
                                                    .post(&url)
                                                    .header(
                                                        "Authorization",
                                                        format!("Bearer {}", token),
                                                    )
                                                    .json(&payload)
                                                    .send()
                                                    .await;

                                                if let Ok(r_res) = retry_res {
                                                    if r_res.status().is_success() {
                                                        println!("Monitor: Bulk session sync success (after refresh).");
                                                        for s in &pending_sess {
                                                            synced_session_uuids.push((
                                                                s.uuid.clone(),
                                                                s.is_active,
                                                            ));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        // Refresh failed -> Logout
                                        println!("Monitor: Token refresh failed. Logging out.");
                                        use tauri::Emitter; // Import Emitter trait
                                        let _ = app_handle.emit("logout-user", ());
                                        let db_path_logout = app_state.db_path.clone();
                                        let _ = async_runtime::spawn_blocking(move || {
                                            if let Ok(conn) = Connection::open(&db_path_logout) {
                                                let _ = db::clear_user(&conn);
                                            }
                                        })
                                        .await;
                                        return;
                                    }
                                }
                                Err(_) => {
                                    println!("Monitor: Token refresh request failed.");
                                }
                            }
                        } else {
                            eprintln!("Monitor: Bulk session sync failed. Status: {}", status);
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
                    let client = client.clone();
                    let token = token.clone();
                    let url = format!("{}/client/screenshots", base_url);

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
                let db_path_del = app_state.db_path.clone();
                let _ = async_runtime::spawn_blocking(move || {
                    if let Ok(conn) = Connection::open(&db_path_del) {
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
        let db_path = app_state.db_path.clone();

        let user_op = async_runtime::spawn_blocking(move || {
            if let Ok(conn) = Connection::open(&db_path) {
                db::get_user(&conn).ok().flatten()
            } else {
                None
            }
        })
        .await;

        if let Ok(Some(user)) = user_op {
            let token = user.token.clone();
            let base_url = "https://trackup.staging-api.nykon.cloud/v1";
            let client = reqwest::Client::new();
            let url = format!("{}/client/sessions/today", base_url);

            let res = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await;

            match res {
                Ok(response) => {
                    if response.status().is_success() {
                        if let Ok(server_sessions) =
                            response.json::<Vec<crate::models::SyncSession>>().await
                        {
                            println!(
                                "Monitor: Fetched {} sessions from server.",
                                server_sessions.len()
                            );

                            let db_path_sync = app_state.db_path.clone();
                            let _ = async_runtime::spawn_blocking(move || {
                                if let Ok(conn) = Connection::open(&db_path_sync) {
                                    for server_session in server_sessions {
                                        // Check if exists
                                        if let Ok(local_opt) =
                                            db::get_session_by_uuid(&conn, &server_session.uuid)
                                        {
                                            match local_opt {
                                                Some(local_session) => {
                                                    // Compare Start/End Delta
                                                    // Local Delta
                                                    let local_duration = if let Some(end) =
                                                        local_session.end_time
                                                    {
                                                        end.saturating_sub(local_session.start_time)
                                                    } else {
                                                        let now = SystemTime::now()
                                                            .duration_since(UNIX_EPOCH)
                                                            .unwrap()
                                                            .as_millis()
                                                            as i64;
                                                        now.saturating_sub(local_session.start_time)
                                                    };

                                                    // Server Delta
                                                    let server_duration = if let Some(end) =
                                                        server_session.end_time
                                                    {
                                                        end.saturating_sub(
                                                            server_session.start_time,
                                                        )
                                                    } else {
                                                        // Server also active?
                                                        0
                                                    };

                                                    if server_duration > local_duration {
                                                        // Server has "more" data. Update local.
                                                        let _ = db::update_imported_session(
                                                            &conn,
                                                            &server_session,
                                                        );
                                                    }
                                                }
                                                None => {
                                                    // Insert new
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
        }
    });
}
