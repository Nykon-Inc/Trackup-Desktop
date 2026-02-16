use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub uuid: String,
    pub name: String,
    pub email: String,
    pub token: String,
    pub refresh_token: Option<String>,
    pub projects: Vec<Project>,
    pub current_project_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub id: Option<i64>,
    pub uuid: String,
    pub project_id: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub is_active: bool,
    pub idle_seconds: i64,
    pub deducted_seconds: i64,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub keyboard_events: i64,
    #[serde(default)]
    pub mouse_events: i64,
}

fn default_status() -> String {
    "pending".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLog {
    pub timestamp: i64,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPayload {
    pub uuid: String,
    pub project_id: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub is_active: bool,
    pub idle_seconds: i64,
    pub deducted_seconds: i64,
    pub keyboard_events: i64,
    pub mouse_events: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_logs: Option<Vec<ActivityLog>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncSession {
    pub uuid: String,
    pub project_id: String,
    pub user_id: String,
    pub organization_id: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub is_active: bool,
    pub idle_seconds: i64,
    pub deducted_seconds: i64,
}
