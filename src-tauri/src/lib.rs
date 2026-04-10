mod api_logic;
mod enea;
mod energa;
mod fortum;
mod mpwik;
mod pge;
mod state_db;
mod stoen;
mod tauron;
mod teryt;
mod utils;

use api_logic::{
    load_settings_from_path, save_settings_to_path,
    AddressEntry, AlertSource, Settings, UnifiedAlert,
    AlertProvider, is_wroclaw, is_warszawa,
};
use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use futures::future::join_all;
use teryt::{TerytCity, TerytStreet};


const MAX_CONCURRENT_REQUESTS: usize = 5;


fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("settings.json"))
}


// ── TERYT local lookups ───────────────────────────────────

#[command]
async fn teryt_lookup_city(app: AppHandle, city_name: String) -> Result<Vec<TerytCity>, String> {
    teryt::lookup_cities(&app, &city_name)
}

#[command]
async fn teryt_lookup_street(
    app: AppHandle,
    city_id: u64,
    street_name: String,
) -> Result<Vec<TerytStreet>, String> {
    teryt::lookup_streets(&app, city_id, &street_name)
}

// ── Settings persistence ──────────────────────────────────

#[command]
async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app)?;
    save_settings_to_path(&path, &settings)
}

#[command]
async fn load_settings(app: AppHandle) -> Result<Option<Settings>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;
    log::info!("load_settings: loaded={:?}", settings.is_some());
    if let Some(ref s) = settings {
        log::info!("load_settings: addresses={}", s.addresses.len());
    }
    Ok(settings)
}

#[command]
async fn add_address(app: AppHandle, address: AddressEntry) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if settings.addresses.len() >= 20 {
        return Err("Maximum of 20 addresses allowed".to_string());
    }

    settings.addresses.push(address);
    if settings.primary_address_index.is_none() {
        settings.primary_address_index = Some(0);
    }

    save_settings_to_path(&path, &settings)?;
    Ok(settings)
}

#[command]
async fn remove_address(app: AppHandle, index: usize) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }

    settings.addresses.remove(index);

    if let Some(ref mut primary) = settings.primary_address_index {
        if *primary >= settings.addresses.len() {
            *primary = settings.addresses.len().saturating_sub(1);
        }
        if settings.addresses.is_empty() {
            *primary = 0;
        }
    }
    if settings.addresses.is_empty() {
        settings.primary_address_index = None;
    }

    save_settings_to_path(&path, &settings)?;
    Ok(settings)
}

#[command]
async fn set_primary_address(app: AppHandle, index: usize) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }

    settings.primary_address_index = Some(index);
    save_settings_to_path(&path, &settings)?;
    Ok(settings)
}

#[command]
async fn update_address(
    app: AppHandle,
    index: usize,
    address: AddressEntry,
) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?.unwrap_or_default();

    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }

    settings.addresses[index] = address;
    save_settings_to_path(&path, &settings)?;
    Ok(settings)
}


fn get_providers() -> Vec<Box<dyn AlertProvider>> {
    vec![
        Box::new(tauron::TauronProvider),
        Box::new(mpwik::MpwikProvider),
        Box::new(fortum::FortumProvider),
        Box::new(energa::EnergaProvider),
        Box::new(enea::EneaProvider),
        Box::new(pge::PgeProvider),
        Box::new(stoen::StoenProvider),
    ]
}

#[command]
async fn fetch_all_alerts(app: AppHandle, sources: Option<Vec<String>>) -> Result<Vec<UnifiedAlert>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;
    let settings_orig = settings.clone();

    let mut all_alerts: Vec<UnifiedAlert> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let mut enabled_sources = settings
        .as_ref()
        .and_then(|s| s.enabled_sources.clone())
        .unwrap_or_default();

    if let Some(requested) = sources {
        enabled_sources.retain(|s| requested.contains(s));
    }

    log::info!(
        "fetch_all_alerts: enabled_sources={:?}, addresses={}",
        enabled_sources,
        settings.as_ref().map(|s| s.addresses.len()).unwrap_or(0)
    );

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let mut tasks = Vec::new();

    if let Some(s) = settings {
        let s_arc = Arc::new(s.clone());
        let providers = get_providers();

        for provider in providers {
            if !enabled_sources.contains(&provider.id()) {
                continue;
            }

            let s_p = Arc::clone(&s_arc);
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                provider.fetch(&s_p).await
            }));
        }

        let results = join_all(tasks).await;

        for res in results {
            match res {
                Ok((mut alerts, errs)) => {
                    for alert in &mut alerts {
                        alert.hash = Some(alert.to_hash());
                    }
                    all_alerts.extend(alerts);
                    errors.extend(errs);
                }
                Err(e) => errors.push(format!("Task execution error: {}", e)),
            }
        }

        // --- DEDUPLICATE BY HASH ---
        let mut seen_hashes = std::collections::HashSet::new();
        all_alerts.retain(|alert| {
            if let Some(h) = &alert.hash {
                seen_hashes.insert(h.clone())
            } else {
                true // Keep if no hash (shouldn't happen)
            }
        });

        // --- PROCESS NEW ALERTS AND NOTIFY ---
        for alert in &all_alerts {
            if alert.is_local == Some(true) {
                let source_key = alert.source.to_string();
                
                // Only notify if source is enabled in settings
                if !enabled_sources.contains(&source_key) {
                    continue;
                }

                let notified_enabled = s
                    .notification_preferences
                    .get(&source_key)
                    .copied()
                    .unwrap_or(false);

                if notified_enabled {
                    let hash = alert.to_hash();
                    
                    // --- UPCOMING NOTIFICATION ---
                    if s.upcoming_notification_enabled {
                        if let Some(start_str) = &alert.startDate {
                            if let Some(start_dt) = utils::parse_date(start_str) {
                                let now_utc = chrono::Utc::now();
                                let diff_hours = (start_dt - now_utc).num_hours();
                                
                                if diff_hours >= 0 && diff_hours <= s.upcoming_notification_hours as i64 {
                                    let upcoming_hash = format!("upcoming_{}", hash);
                                    match state_db::is_alert_seen(&app, &source_key, &upcoming_hash) {
                                        Ok(seen) => {
                                            if !seen {
                                                let title = format_notification_title(&alert, &s, true);
                                                let body = format_notification_body(&alert);

                                                log::info!(
                                                    "Triggering upcoming notification for {}. Title: '{}', Body: '{}'",
                                                    source_key,
                                                    title,
                                                    body
                                                );
                                                app.notification()
                                                    .builder()
                                                    .title(title)
                                                    .body(body.clone())
                                                    .large_body(body)
                                                    .icon("ic_notification")
                                                    .extra("hash", hash.clone())
                                                    .show()
                                                    .ok();

                                                state_db::mark_alert_as_seen(&app, &source_key, &upcoming_hash).ok();
                                            }
                                        }
                                        Err(e) => log::error!("Database error while checking upcoming alert status: {}", e),
                                    }
                                }
                            }
                        }
                    }
                    
                    // --- NEW ALERT NOTIFICATION ---
                    match state_db::is_alert_seen(&app, &source_key, &hash) {
                        Ok(seen) => {
                            if !seen {
                                // Trigger notification
                                let title = format_notification_title(&alert, &s, false);
                                let body = format_notification_body(&alert);

                                log::info!("Triggering notification for {}. Title: '{}', Body: '{}'", source_key, title, body);
                                app.notification()
                                    .builder()
                                    .title(title)
                                    .body(body.clone())
                                    .large_body(body)
                                    .icon("ic_notification")
                                    .extra("hash", hash.clone())
                                    .show()
                                    .ok();

                                // Mark as seen
                                state_db::mark_alert_as_seen(&app, &source_key, &hash).ok();
                            }
                        }
                        Err(e) => log::error!("Database error while checking alert status: {}", e),
                    }
                }
            }
        }
    }

    if all_alerts.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }

    // Final filter to ensure no alerts from disabled addresses/cities slip through
    if let Some(ref s) = settings_orig {
        all_alerts.retain(|alert| {
            if let Some(idx) = alert.address_index {
                if idx < s.addresses.len() {
                    return s.addresses[idx].is_active;
                }
            }
            
            // For general city alerts
            if alert.is_local == Some(false) {
                if let Some(desc) = &alert.description {
                    if desc.contains("Wrocław") {
                        return s.addresses.iter().any(|a| a.is_active && is_wroclaw(a));
                    }
                    if desc.contains("Warszawa") {
                        return s.addresses.iter().any(|a| a.is_active && is_warszawa(a));
                    }
                    // For other cities (dynamic check based on active addresses)
                    for addr in s.addresses.iter().filter(|a| a.is_active) {
                        if desc.contains(&addr.city_name) {
                            return true;
                        }
                    }
                    return false; // Skip if no active address in this city
                }
            }

            true
        });
    }

    Ok(all_alerts)
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_enea_outages_real_backend() {
        let client = build_client().unwrap();
        let items = enea::fetch_all_enea_outages(&client, &[7]).await.unwrap();
        
        let kicin_items: Vec<_> = items.into_iter()
            .filter(|i| i.matches_address("Kicin", "", "Poznańska", &None))
            .collect();
            
        println!("Found Kicin / Poznańska items: {}", kicin_items.len());
        assert!(!kicin_items.is_empty());

        let unified = kicin_items[0].to_unified();
        println!("Unified structure: {:?}", unified);
    }
}


#[command]
async fn teryt_city_has_streets(app: AppHandle, city_id: u64) -> Result<bool, String> {
    teryt::city_has_streets(&app, city_id)
}

fn format_notification_title(alert: &UnifiedAlert, settings: &Settings, is_upcoming: bool) -> String {
    let is_pl = settings.language.as_ref().map(|l| l.contains("pl")).unwrap_or(true);
    let label = match alert.source {
        AlertSource::Tauron | AlertSource::Energa | AlertSource::Enea | AlertSource::Pge | AlertSource::Stoen => {
            if is_pl { "awaria prądu" } else { "power outage" }
        }
        AlertSource::Water => {
            if is_pl { "awaria wody" } else { "water outage" }
        }
        AlertSource::Fortum => {
            if is_pl { "awaria ogrzewania" } else { "heat outage" }
        }
    };
    
    let prefix = if is_upcoming {
        if is_pl { "Nadchodząca" } else { "Upcoming" }
    } else {
        if is_pl { "Nowa" } else { "New" }
    };
    
    let title = format!("{} {}", prefix, label);
    
    if let Some(idx) = alert.address_index {
        if let Some(addr) = settings.addresses.get(idx) {
            return format!("{}: {}", addr.name, title);
        }
    }
    title
}

fn format_notification_body(alert: &UnifiedAlert) -> String {
    let mut body = alert.message.clone().unwrap_or_default();
    
    let mut time_info = Vec::new();
    if let Some(start) = &alert.startDate {
        if let Some(dt) = utils::parse_date(start) {
            time_info.push(utils::format_date(dt));
        }
    }
    if let Some(end) = &alert.endDate {
        if let Some(dt) = utils::parse_date(end) {
            time_info.push(utils::format_date(dt));
        }
    }
    
    if !time_info.is_empty() {
        let times = time_info.join(" - ");
        // Only append if it's not already in the message (simple check)
        if !body.contains(&times) {
            if !body.is_empty() {
                body.push_str("\n");
            }
            body.push_str(&times);
        }
    }
    body
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            state_db::init_db(app.handle())?;
            state_db::prune_old_alerts(app.handle(), 30)?;
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fetch_all_alerts,
            teryt_lookup_city,
            teryt_lookup_street,
            teryt_city_has_streets,
            save_settings,
            load_settings,
            add_address,
            remove_address,
            set_primary_address,
            update_address,
            get_app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
