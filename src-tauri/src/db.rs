use rusqlite::Connection;
use std::path::PathBuf;
use crate::models::{User, Project};

struct DbColumn {
    name: &'static str,
    def: &'static str,
    type_affinity: &'static str,
}

struct DbTable {
    name: &'static str,
    columns: &'static [DbColumn],
    constraints: Option<&'static str>,
}

const SCHEMA: &[DbTable] = &[
    DbTable {
        name: "users",
        columns: &[
            DbColumn { name: "uuid", def: "TEXT PRIMARY KEY", type_affinity: "TEXT" },
            DbColumn { name: "name", def: "TEXT NOT NULL", type_affinity: "TEXT" },
            DbColumn { name: "email", def: "TEXT NOT NULL", type_affinity: "TEXT" },
            DbColumn { name: "token", def: "TEXT NOT NULL", type_affinity: "TEXT" },
            DbColumn { name: "current_project_id", def: "TEXT", type_affinity: "TEXT" },
        ],
        constraints: None,
    },
    DbTable {
        name: "projects",
        columns: &[
            DbColumn { name: "id", def: "TEXT NOT NULL", type_affinity: "TEXT" },
            DbColumn { name: "name", def: "TEXT NOT NULL", type_affinity: "TEXT" },
        ],
        constraints: Some("PRIMARY KEY (id)"),
    },
    DbTable {
        name: "sessions",
        columns: &[
            DbColumn { name: "id", def: "INTEGER PRIMARY KEY AUTOINCREMENT", type_affinity: "INTEGER" },
            DbColumn { name: "uuid", def: "TEXT NOT NULL", type_affinity: "TEXT" },
            DbColumn { name: "project_id", def: "TEXT NOT NULL", type_affinity: "TEXT" },
            DbColumn { name: "start_time", def: "INTEGER NOT NULL", type_affinity: "INTEGER" },
            DbColumn { name: "end_time", def: "INTEGER", type_affinity: "INTEGER" },
            DbColumn { name: "is_active", def: "BOOLEAN DEFAULT 0", type_affinity: "INTEGER" },
        ],
        constraints: None,
    },
];

pub fn init_db(path: &PathBuf) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    
    // Disable foreign keys temporarily to allow dropping tables out of order if needed
    conn.execute("PRAGMA foreign_keys = OFF", [])?;

    for table in SCHEMA {
        let mut needs_recreation = false;

        // Check if table exists and get columns
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table.name))?;
        let existing_columns: Vec<(String, String)> = stmt.query_map([], |row| {
            Ok((row.get(1)?, row.get(2)?)) // name, type
        })?
        .collect::<Result<Vec<_>, _>>()?;

        if existing_columns.is_empty() {
             needs_recreation = true;
        } else {
             // Basic Check: Count match?
             if existing_columns.len() != table.columns.len() {
                 needs_recreation = true;
             } else {
                 // Detailed Check: Names and Types match?
                 for col in table.columns {
                     let match_found = existing_columns.iter().any(|(ex_name, ex_type)| {
                         ex_name == col.name && ex_type.eq_ignore_ascii_case(col.type_affinity)
                     });
                     if !match_found {
                         needs_recreation = true;
                         break;
                     }
                 }
             }
        }

        if needs_recreation {
            conn.execute(&format!("DROP TABLE IF EXISTS {}", table.name), [])?;
            
            let cols_sql: Vec<String> = table.columns.iter()
                .map(|c| format!("{} {}", c.name, c.def))
                .collect();
            
            let create_sql = format!(
                "CREATE TABLE {} ({}{})", 
                table.name, 
                cols_sql.join(", "),
                table.constraints.map(|c| format!(", {}", c)).unwrap_or_default()
            );
            
            conn.execute(&create_sql, [])?;
        }
    }
    
    // Re-enable foreign keys
    conn.execute("PRAGMA foreign_keys = ON", [])?;

    Ok(conn)
}

pub fn save_user(conn: &mut Connection, user: &User) -> Result<(), rusqlite::Error> {
    let tx = conn.transaction()?;

    // Clear existing data (single user mode)
    tx.execute("DELETE FROM projects", [])?;
    tx.execute("DELETE FROM users", [])?;

    // Insert user
    tx.execute(
        "INSERT INTO users (uuid, name, email, token, current_project_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        [
            &user.uuid,
            &user.name,
            &user.email,
            &user.token,
            user.current_project_id.as_deref().unwrap_or_default(), 
        ],
    )?;
     
     // Insert projects
     for project in &user.projects {
         tx.execute(
             "INSERT INTO projects (id, name) VALUES (?1, ?2)",
             [&project.id, &project.name],
         )?;
     }

    tx.commit()?;
    Ok(())
}

pub fn clear_user(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM projects", [])?;
    conn.execute("DELETE FROM users", [])?;
    conn.execute("DELETE FROM sessions", [])?;
    Ok(())
}

pub fn set_current_project(conn: &Connection, project_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE users SET current_project_id = ?1",
        [project_id],
    )?;
    Ok(())
}

// Session Management

use crate::models::Session;
use chrono::Local;
use uuid::Uuid;

pub fn get_active_session(conn: &Connection, project_id: &str) -> Result<Option<Session>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, project_id, start_time, end_time, is_active 
         FROM sessions 
         WHERE project_id = ?1 AND is_active = 1 
         LIMIT 1"
    )?;
    
    let mut rows = stmt.query([project_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Session {
            id: Some(row.get(0)?),
            uuid: row.get(1)?,
            project_id: row.get(2)?,
            start_time: row.get(3)?,
            end_time: row.get(4)?,
            is_active: row.get(5)?,
        }))
    } else {
        Ok(None)
    }
}

pub fn start_session(conn: &Connection, project_id: &str) -> Result<(), rusqlite::Error> {
    let start_time = Local::now().timestamp();
    let uuid = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sessions (uuid, project_id, start_time, is_active) VALUES (?1, ?2, ?3, 1)",
        (uuid, project_id, start_time),
    )?;
    Ok(())
}

pub fn stop_session(conn: &Connection, project_id: &str) -> Result<(), rusqlite::Error> {
    let end_time = Local::now().timestamp();
    conn.execute(
        "UPDATE sessions SET is_active = 0, end_time = ?1 WHERE project_id = ?2 AND is_active = 1",
        (end_time, project_id),
    )?;
    Ok(())
}

pub fn update_session_heartbeat(conn: &Connection, session_id: i64) -> Result<(), rusqlite::Error> {
    // We update end_time to now, effectively tracking "up to now" duration.
    // This is useful if the app crashes, we know the last heartbeat time.
    let now = Local::now().timestamp();
    conn.execute(
        "UPDATE sessions SET end_time = ?1 WHERE id = ?2",
        (now, session_id),
    )?;
    Ok(())
}

pub fn get_today_total_time(conn: &Connection, project_id: &str) -> Result<u64, rusqlite::Error> {
    // Get start of today (local time)
    let now = Local::now();
    let start_of_day = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).unwrap().timestamp();
    
    // Sum duration of closed sessions
    // Duration = end_time - start_time
    // Filter by start_time >= start_of_day
    let mut stmt = conn.prepare(
        "SELECT start_time, end_time, is_active 
         FROM sessions 
         WHERE project_id = ?1 AND start_time >= ?2"
    )?;
    
    let current_ts = now.timestamp();
    let mut total_seconds: u64 = 0;
    
    let rows = stmt.query_map([project_id, &start_of_day.to_string()], |row| {
        let start: i64 = row.get(0)?;
        let end: Option<i64> = row.get(1)?;
        let active: bool = row.get(2)?;
        Ok((start, end, active))
    })?;

    for r in rows {
        let (start, end, active) = r?;
        if active {
             // For active session, duration is now - start
             // Or if we updated end_time via heartbeat, we could use that, 
             // but using 'current_ts' for display calculation is smoother.
             if current_ts > start {
                 total_seconds += (current_ts - start) as u64;
             }
        } else {
             if let Some(e) = end {
                 if e > start {
                     total_seconds += (e - start) as u64;
                 }
             }
        }
    }

    Ok(total_seconds)
}

pub fn get_user(conn: &Connection) -> Result<Option<User>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT uuid, name, email, token, current_project_id FROM users LIMIT 1")?;
    
    let mut user_iter = stmt.query_map([], |row| {
        let uuid: String = row.get(0)?;
        api_user_from_row(row, conn, uuid)
    })?;

    if let Some(user_result) = user_iter.next() {
        return Ok(Some(user_result?));
    }
    Ok(None)
}

fn api_user_from_row(row: &rusqlite::Row, conn: &Connection, uuid: String) -> Result<User, rusqlite::Error> {
    let mut projects_stmt = conn.prepare("SELECT id, name FROM projects")?;
    let projects = projects_stmt.query_map([], |p_row| {
        Ok(Project {
            id: p_row.get(0)?,
            name: p_row.get(1)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    
    let current_project_id: Option<String> = row.get(4).ok().filter(|s: &String| !s.is_empty());

    Ok(User {
        uuid: uuid,
        name: row.get(1)?,
        email: row.get(2)?,
        token: row.get(3)?,
        current_project_id,
        projects,
    })
}
