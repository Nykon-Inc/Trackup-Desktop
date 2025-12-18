use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub uuid: String,
    pub name: String,
    pub email: String,
    pub role: String,
    pub token: String,
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
}
