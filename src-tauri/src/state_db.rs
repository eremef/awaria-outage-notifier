use rusqlite::{params, Connection};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

pub fn get_state_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {}", e))?;
    std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    Ok(app_data.join("state.db"))
}

pub fn init_db(app: &AppHandle) -> Result<(), String> {
    let path = get_state_db_path(app)?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS seen_alerts (
            provider TEXT,
            alert_hash TEXT,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (provider, alert_hash)
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn is_alert_seen(app: &AppHandle, provider: &str, hash: &str) -> Result<bool, String> {
    let path = get_state_db_path(app)?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare("SELECT 1 FROM seen_alerts WHERE provider = ?1 AND alert_hash = ?2")
        .map_err(|e| e.to_string())?;

    let exists = stmt.exists(params![provider, hash]).map_err(|e| e.to_string())?;
    Ok(exists)
}

pub fn mark_alert_as_seen(app: &AppHandle, provider: &str, hash: &str) -> Result<(), String> {
    let path = get_state_db_path(app)?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;

    conn.execute(
        "INSERT OR IGNORE INTO seen_alerts (provider, alert_hash) VALUES (?1, ?2)",
        params![provider, hash],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn prune_old_alerts(app: &AppHandle, days: i32) -> Result<(), String> {
    let path = get_state_db_path(app)?;
    let conn = Connection::open(path).map_err(|e| e.to_string())?;

    conn.execute(
        "DELETE FROM seen_alerts WHERE created_at < datetime('now', '-' || ?1 || ' days')",
        params![days],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}
