use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri::Manager;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

pub fn get_state_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {}", e))?;
    std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    Ok(app_data.join("state.db"))
}

pub fn init_db(app: &AppHandle) -> Result<Connection, String> {
    let path = get_state_db_path(app)?;
    let mut conn = Connection::open(path).map_err(|e| e.to_string())?;
    _init_db(&mut conn)?;
    Ok(conn)
}

fn _init_db(conn: &mut Connection) -> Result<(), String> {
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
 
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value TEXT,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn is_alert_seen(conn: &Connection, provider: &str, hash: &str) -> Result<bool, String> {
    _is_alert_seen(conn, provider, hash)
}

fn _is_alert_seen(conn: &Connection, provider: &str, hash: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM seen_alerts WHERE provider = ?1 AND alert_hash = ?2")
        .map_err(|e| e.to_string())?;

    let exists = stmt.exists(params![provider, hash]).map_err(|e| e.to_string())?;
    Ok(exists)
}

pub fn mark_alert_as_seen(conn: &Connection, provider: &str, hash: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare("INSERT OR IGNORE INTO seen_alerts (provider, alert_hash) VALUES (?1, ?2)")
        .map_err(|e| e.to_string())?;
    stmt.execute(params![provider, hash]).map_err(|e| e.to_string())?;
    Ok(())
}

fn _mark_alert_as_seen(conn: &mut Connection, provider: &str, hash: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO seen_alerts (provider, alert_hash) VALUES (?1, ?2)",
        params![provider, hash],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn prune_old_alerts(conn: &Connection, days: i32) -> Result<(), String> {
    _prune_old_alerts(conn, days)
}

fn _prune_old_alerts(conn: &Connection, days: i32) -> Result<(), String> {
    conn.execute(
        "DELETE FROM seen_alerts WHERE created_at < datetime('now', '-' || ?1 || ' days')",
        params![days],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[allow(dead_code)]
pub fn set_kv(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO kv_store (key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_kv(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare("SELECT value FROM kv_store WHERE key = ?1")
        .map_err(|e| e.to_string())?;

    let mut rows = stmt.query(params![key]).map_err(|e| e.to_string())?;
    if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let val: String = row.get(0).map_err(|e| e.to_string())?;
        Ok(Some(val))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_db_basic_flow() {
        let mut conn = Connection::open_in_memory().unwrap();
        _init_db(&mut conn).unwrap();

        assert!(!_is_alert_seen(&conn, "tauron", "hash1").unwrap());

        _mark_alert_as_seen(&mut conn, "tauron", "hash1").unwrap();
        assert!(_is_alert_seen(&conn, "tauron", "hash1").unwrap());

        // Test insert or ignore
        _mark_alert_as_seen(&mut conn, "tauron", "hash1").unwrap(); 
        assert!(_is_alert_seen(&conn, "tauron", "hash1").unwrap());
    }

    #[test]
    fn test_pruning() {
        let mut conn = Connection::open_in_memory().unwrap();
        _init_db(&mut conn).unwrap();

        _mark_alert_as_seen(&mut conn, "energa", "old").unwrap();
        
        // Manual override for testing: set old date
        conn.execute(
            "UPDATE seen_alerts SET created_at = datetime('now', '-10 days') WHERE alert_hash = 'old'",
            [],
        ).unwrap();

        _prune_old_alerts(&mut conn, 5).unwrap();
        assert!(!_is_alert_seen(&conn, "energa", "old").unwrap());

        _mark_alert_as_seen(&mut conn, "energa", "fresh").unwrap();
        _prune_old_alerts(&mut conn, 5).unwrap();
        assert!(_is_alert_seen(&conn, "energa", "fresh").unwrap());
    }
}
