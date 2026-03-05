use crate::db;
use crate::AppState;
use rusqlite::Connection;
use serde_json::json;
use tauri::{async_runtime, AppHandle, Emitter, Manager, Runtime};

pub const BASE_URL: &str = "https://trackup.staging-api.nykon.cloud/v1";

pub async fn request<R: Runtime, T: serde::Serialize>(
    app: &AppHandle<R>,
    method: reqwest::Method,
    endpoint: &str,
    payload: Option<&T>,
) -> Result<reqwest::Response, String> {
    let app_handle = app.clone();
    let state = app.state::<AppState>();
    let client = &state.client;
    let url = format!("{}{}", BASE_URL, endpoint);

    // 1. Get current user for token
    let db_path = state.db_path.lock().unwrap().clone();
    let user = async_runtime::spawn_blocking(move || {
        if let Ok(conn) = Connection::open(&db_path) {
            db::get_user(&conn).ok().flatten()
        } else {
            None
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or("User not found")?;

    let mut token = user.token.clone();

    // 2. Initial Attempt
    let build_request = |t: &str| {
        let mut req = client
            .request(method.clone(), &url)
            .header("Authorization", format!("Bearer {}", t))
            .header("x-app-source", "desktop");

        if let Some(p) = payload {
            req = req.json(p);
        }
        req
    };

    let mut response = build_request(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // 3. Handle 401 Unauthorized
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        println!(
            "API: 401 Unauthorized for {}. Attempting token refresh...",
            endpoint
        );

        if let Some(rt) = user.refresh_token.as_ref() {
            let refresh_url = format!("{}/auth/refresh-tokens", BASE_URL);
            let refresh_res = client
                .post(&refresh_url)
                .json(&json!({ "refreshToken": rt }))
                .send()
                .await;

            match refresh_res {
                Ok(u_res) if u_res.status().is_success() => {
                    if let Ok(json_body) = u_res.json::<serde_json::Value>().await {
                        // Correct structure is json_body.credentials.access.token
                        let creds = json_body
                            .get("credentials")
                            .or_else(|| json_body.get("tokens"))
                            .unwrap_or(&json_body);

                        let new_access = creds
                            .get("access")
                            .and_then(|a| a.get("token"))
                            .and_then(|t| t.as_str())
                            .or_else(|| creds.get("token").and_then(|t| t.as_str()));

                        let new_refresh = creds
                            .get("refresh")
                            .and_then(|r| r.get("token"))
                            .and_then(|t| t.as_str());

                        if let Some(new_token) = new_access {
                            println!("API: Token refreshed successfully.");
                            token = new_token.to_string();

                            // Update DB with new token
                            let new_token_db = token.clone();
                            let new_refresh_db = new_refresh.map(|s| s.to_string());
                            let db_path_update = state.db_path.lock().unwrap().clone();
                            let uuid = user.uuid.clone();

                            let _ = async_runtime::spawn_blocking(move || {
                                if let Ok(conn) = Connection::open(&db_path_update) {
                                    if let Some(rt) = new_refresh_db {
                                        let _ = conn.execute(
                                            "UPDATE users SET token = ?1, refresh_token = ?2 WHERE uuid = ?3",
                                            [new_token_db, rt, uuid],
                                        );
                                    } else {
                                        let _ = conn.execute(
                                            "UPDATE users SET token = ?1 WHERE uuid = ?2",
                                            [new_token_db, uuid],
                                        );
                                    }
                                }
                            }).await;

                            // Retry original request
                            response = build_request(&token)
                                .send()
                                .await
                                .map_err(|e| e.to_string())?;
                        } else {
                            println!("API: Token refresh response body: {:?}", json_body);
                            return logout_and_fail(
                                &app_handle,
                                "Token refresh response missing token".to_string(),
                            )
                            .await;
                        }
                    } else {
                        return logout_and_fail(
                            &app_handle,
                            "Failed to parse token refresh response".to_string(),
                        )
                        .await;
                    }
                }
                _ => {
                    return logout_and_fail(&app_handle, "Token refresh failed".to_string()).await;
                }
            }
        } else {
            return logout_and_fail(&app_handle, "No refresh token available".to_string()).await;
        }
    }

    Ok(response)
}

async fn logout_and_fail<R: Runtime>(
    app: &AppHandle<R>,
    error: String,
) -> Result<reqwest::Response, String> {
    println!("API Logout: {}", error);
    let state = app.state::<AppState>();
    let db_path = state.db_path.lock().unwrap().clone();

    let _ = async_runtime::spawn_blocking(move || {
        if let Ok(conn) = Connection::open(&db_path) {
            let _ = db::clear_user(&conn);
        }
    })
    .await;

    crate::update_tray(app, false, "");
    let _ = app.emit("logout-user", ());

    Err(error)
}
