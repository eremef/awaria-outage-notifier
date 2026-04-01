use rusqlite::Connection;
use serde::Serialize;
use tauri::AppHandle;
use tauri::Manager;

#[derive(Debug, Serialize)]
pub struct TerytCity {
    pub voivodeship: String,
    pub district: String,
    pub commune: String,
    pub city: String,
    pub city_id: u64,
}

#[derive(Debug, Serialize)]
pub struct TerytStreet {
    pub full_street_name: String,
    pub city_id: u64,
    pub street_id: u64,
    pub street_name_1: String,
    pub street_name_2: Option<String>,
}

fn db_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    // Use app_data_dir for the working copy of the database
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {}", e))?;
    std::fs::create_dir_all(&app_data).map_err(|e| e.to_string())?;
    let db = app_data.join("teryt");

    if db.exists() {
        return Ok(db);
    }

    // Try using Tauri 2 path resolver (works for bundled resources)
    use tauri::path::BaseDirectory;
    use tauri_plugin_fs::FsExt;
    if let Ok(resource_path) = app.path().resolve("teryt", BaseDirectory::Resource) {
        if let Ok(bytes) = app.fs().read(&resource_path) {
            if std::fs::write(&db, bytes).is_ok() {
                return Ok(db);
            }
        }
    }

    // Fallback: dev mode relative to Cargo.toml
    let dev_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("teryt");
    if dev_path.exists() && std::fs::copy(&dev_path, &db).is_ok() {
        return Ok(db);
    }

    Err(format!(
        "Teryt database not found in any location. Target: {:?}",
        db
    ))
}

pub fn lookup_cities(app: &AppHandle, city_name: &str) -> Result<Vec<TerytCity>, String> {
    let path = db_path(app)?;
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Failed to open teryt DB: {}", e))?;
    _lookup_cities(&conn, city_name)
}

fn _lookup_cities(conn: &Connection, city_name: &str) -> Result<Vec<TerytCity>, String> {
    let sql = "SELECT voivodeship.nazwa, district.nazwa, commune.nazwa, city.nazwa, city.sym \
               FROM simc city \
               LEFT JOIN terc voivodeship ON city.woj = voivodeship.woj \
                   AND voivodeship.pow IS NULL AND voivodeship.gmi IS NULL \
               LEFT JOIN terc district ON city.woj = district.woj \
                   AND city.pow = district.pow AND district.gmi IS NULL \
               LEFT JOIN terc commune ON city.woj = commune.woj \
                   AND city.pow = commune.pow AND city.gmi = commune.gmi \
                   AND city.rodz_gmi = commune.rodz \
               WHERE city.sym = city.sympod \
                   AND city.nazwa like ?1 COLLATE NOCASE \
               ORDER BY city.nazwa, voivodeship.nazwa, district.nazwa, commune.nazwa \
               LIMIT 20";

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("Failed to prepare city query: {}", e))?;
    let pattern = format!("{}%", city_name);
    let rows = stmt
        .query_map([pattern], |row| {
            Ok(TerytCity {
                voivodeship: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                district: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                commune: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                city: row.get(3)?,
                city_id: row.get::<_, i64>(4)? as u64,
            })
        })
        .map_err(|e| format!("City query failed: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| format!("City row error: {}", e))?);
    }
    Ok(results)
}

pub fn lookup_streets(
    app: &AppHandle,
    city_id: u64,
    street_name: &str,
) -> Result<Vec<TerytStreet>, String> {
    let path = db_path(app)?;
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Failed to open teryt DB: {}", e))?;
    _lookup_streets(&conn, city_id, street_name)
}

fn _lookup_streets(
    conn: &Connection,
    city_id: u64,
    street_name: &str,
) -> Result<Vec<TerytStreet>, String> {
    let sql = "SELECT distinct street.cecha || IFNULL(' ' || street.nazwa_2, '') || ' ' || street.nazwa_1 AS full_name,
                       ?1 sym, street.sym_ul, street.nazwa_1, street.nazwa_2 \
               FROM ulic street \
               LEFT JOIN simc city ON street.sym = city.sym \
               LEFT JOIN simc city_part ON city_part.sym = street.sym \
               WHERE (city.sym = ?1 \
                    OR city_part.sympod = ?1) \
                   AND full_name LIKE ?2 COLLATE NOCASE \
               ORDER BY full_name \
               LIMIT 30";

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("Failed to prepare street query: {}", e))?;
    let pattern = format!("%{}%", street_name);
    let city_id_i64 = city_id as i64;
    let rows = stmt
        .query_map([&city_id_i64 as &dyn rusqlite::ToSql, &pattern], |row| {
            Ok(TerytStreet {
                full_street_name: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                city_id: row.get::<_, i64>(1)? as u64,
                street_id: row.get::<_, i64>(2)? as u64,
                street_name_1: row.get(3)?,
                street_name_2: row.get(4)?,
            })
        })
        .map_err(|e| format!("Street query failed: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| format!("Street row error: {}", e))?);
    }
    Ok(results)
}

pub fn city_has_streets(app: &AppHandle, city_id: u64) -> Result<bool, String> {
    let path = db_path(app)?;
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Failed to open teryt DB: {}", e))?;
    _city_has_streets(&conn, city_id)
}

fn _city_has_streets(conn: &Connection, city_id: u64) -> Result<bool, String> {
    let sql = "SELECT count(1)
               FROM ulic street
               LEFT JOIN simc city ON street.sym = city.sym
               LEFT JOIN simc city_part ON city_part.sym = street.sym 
               WHERE city.sym = ?1
                   OR city_part.sympod = ?1";

    let count: i64 = conn
        .query_row(sql, [city_id as i64], |row| row.get(0))
        .map_err(|e| format!("Street count query failed: {}", e))?;

    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_mock_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Setup minimal TERYT schema
        conn.execute_batch("
            CREATE TABLE terc (woj TEXT, pow TEXT, gmi TEXT, rodz TEXT, nazwa TEXT);
            CREATE TABLE simc (woj TEXT, pow TEXT, gmi TEXT, rodz_gmi TEXT, nazwa TEXT, sym INTEGER, sympod INTEGER);
            CREATE TABLE ulic (sym INTEGER, sym_ul INTEGER, cecha TEXT, nazwa_1 TEXT, nazwa_2 TEXT);

            -- Insert Voivodeship
            INSERT INTO terc VALUES ('02', NULL, NULL, NULL, 'DOLNOŚLĄSKIE');
            -- Insert District
            INSERT INTO terc VALUES ('02', '64', NULL, NULL, 'Wrocław');
            -- Insert City
            INSERT INTO simc VALUES ('02', '64', '01', '1', 'Wrocław', 969400, 969400);
            -- Insert Street
            INSERT INTO ulic VALUES (969400, 13900, 'ul.', 'Kuźnicza', NULL);
        ").unwrap();
        conn
    }

    #[test]
    fn test_lookup_cities() {
        let conn = setup_mock_db();
        let results = _lookup_cities(&conn, "Wroc").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].city, "Wrocław");
        assert_eq!(results[0].city_id, 969400);
    }

    #[test]
    fn test_lookup_streets() {
        let conn = setup_mock_db();
        let results = _lookup_streets(&conn, 969400, "Kuź").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].street_name_1, "Kuźnicza");
        assert_eq!(results[0].full_street_name, "ul. Kuźnicza");
    }

    #[test]
    fn test_city_has_streets() {
        let conn = setup_mock_db();
        assert!(_city_has_streets(&conn, 969400).unwrap());
        assert!(!_city_has_streets(&conn, 12345).unwrap());
    }
}
