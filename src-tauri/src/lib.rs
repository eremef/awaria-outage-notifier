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
};
use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

use std::fs;
use std::path::PathBuf;
use utils::{build_client, retry};
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
    load_settings_from_path(&path)
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


async fn fetch_energa_alerts() -> Result<Vec<energa::EnergaShutdown>, String> {

    let client = build_client()?;
    let url = energa::extract_energa_api_url(&client).await?;
    log::info!("Energa API calculated URL: {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Energa HTTP error: {}", res.status()));
    }

    let data: energa::EnergaResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(data.document.payload.shutdowns)
}

#[command]
async fn fetch_all_alerts(app: AppHandle, sources: Option<Vec<String>>) -> Result<Vec<UnifiedAlert>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;

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
        if s.addresses.is_empty() {
            log::info!("fetch_all_alerts: No addresses found, skipping fetch.");
            return Ok(all_alerts);
        }
        let s = Arc::new(s);

        // --- Tauron (Parallel per address) ---
        if enabled_sources.contains(&"tauron".to_string()) {
            for (idx, addr) in s.addresses.iter().enumerate() {
                let addr = addr.clone();
                let sem = semaphore.clone();
                tasks.push(tokio::spawn(async move {
                    let _permit = sem.acquire().await.ok();
                    match retry(|| tauron::fetch_tauron_outages(&addr), 3).await {
                        Ok(response) => {
                            let alerts: Vec<UnifiedAlert> = response
                                .OutageItems
                                .unwrap_or_default()
                                .into_iter()
                                .map(|item| {
                                    let mut alert = item.to_unified();
                                    alert.address_index = Some(idx);
                                    let city_prefix = format!("Miejscowość: {}", addr.city_name);
                                    alert.description = Some(match alert.description {
                                        Some(d) if !d.is_empty() => format!("{}. {}", city_prefix, d),
                                        _ => city_prefix,
                                    });
                                    alert.is_local = Some(tauron::matches_address(
                                        &item.Message,
                                        &addr.city_name,
                                        &addr.street_name_1,
                                        &addr.street_name_2,
                                    ));
                                    alert
                                })
                                .collect();
                            (alerts, Vec::<String>::new())
                        }
                        Err(e) => (Vec::new(), vec![format!("Tauron[{}]: {}", idx, e)]),
                    }
                }));
            }
        }

        // --- MPWiK Water ---
        if enabled_sources.contains(&"water".to_string()) {
            let s_water = s.clone();
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                match retry(|| mpwik::fetch_water_alerts(), 3).await {
                    Ok(items) => {
                        let mut alerts = Vec::new();
                        for item in items {
                            let mut matched = false;
                            for (idx, addr) in s_water.addresses.iter().enumerate() {
                                if mpwik::matches_address(&item.content, addr) {
                                    let mut alert = item.to_unified();
                                    alert.address_index = Some(idx);
                                    alert.is_local = Some(true);
                                    alert.description = Some(format!("Miejscowość: {}", addr.city_name));
                                    alerts.push(alert);
                                    matched = true;
                                }
                            }
                            if !matched {
                                let mut alert = item.to_unified();
                                alert.is_local = Some(false);
                                alert.description = Some("Miejscowość: Wrocław".to_string());
                                alerts.push(alert);
                            }
                        }
                        (alerts, Vec::new())
                    }
                    Err(e) => (Vec::new(), vec![format!("MPWiK: {}", e)]),
                }
            }));
        }

        // --- Fortum ---
        if enabled_sources.contains(&"fortum".to_string()) {
            let s_fortum = s.clone();
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                match retry(|| fortum::fetch_fortum_cities(), 3).await {
                    Ok(cities) => {
                        let mut city_map = std::collections::HashMap::new();
                        for (idx, addr) in s_fortum.addresses.iter().enumerate() {
                            if let Some(fc) = cities.iter().find(|c| {
                                c.city_name.to_lowercase() == addr.city_name.to_lowercase()
                            }) {
                                city_map
                                    .entry((fc.city_guid.clone(), fc.region_id, fc.city_name.clone()))
                                    .or_insert_with(Vec::new)
                                    .push((idx, addr));
                            }
                        }

                        let mut fortum_alerts = Vec::new();
                        let mut fortum_errors = Vec::new();

                        for ((guid, rid, city_name), addrs) in city_map {
                            match retry(|| fortum::fetch_fortum_alerts(&guid, rid), 3).await {
                                Ok(alerts) => {
                                    for a in alerts {
                                        let mut matched_any = false;
                                        for (idx, addr) in &addrs {
                                            if fortum::matches_street_only(
                                                &a.message,
                                                &addr.street_name_1,
                                                &addr.street_name_2,
                                            ) {
                                                let mut alert = a.clone();
                                                alert.address_index = Some(*idx);
                                                alert.is_local = Some(true);
                                                let city_prefix = format!("Miejscowość: {}", addr.city_name);
                                                alert.description = Some(match alert.description {
                                                    Some(d) if !d.is_empty() => format!("{}. {}", city_prefix, d),
                                                    _ => city_prefix,
                                                });
                                                fortum_alerts.push(alert);
                                                matched_any = true;
                                            }
                                        }
                                        if !matched_any {
                                            if let Some((idx, addr)) = addrs.first() {
                                                let mut alert = a.clone();
                                                alert.address_index = Some(*idx);
                                                alert.is_local = Some(false);
                                                let city_prefix = format!("Miejscowość: {}", addr.city_name);
                                                alert.description = Some(match alert.description {
                                                    Some(d) if !d.is_empty() => format!("{}. {}", city_prefix, d),
                                                    _ => city_prefix,
                                                });
                                                fortum_alerts.push(alert);
                                            }
                                        }
                                    }
                                }
                                Err(e) => fortum_errors.push(format!("Fortum ({}): {}", city_name, e)),
                            }
                        }
                        (fortum_alerts, fortum_errors)
                    }
                    Err(e) => (Vec::new(), vec![format!("Fortum cities: {}", e)]),
                }
            }));
        }

        // --- Energa ---
        if enabled_sources.contains(&"energa".to_string()) {
            let s_energa = s.clone();
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                match retry(|| fetch_energa_alerts(), 3).await {
                    Ok(shutdowns) => {
                        let mut alerts = Vec::new();
                        for (idx, addr) in s_energa.addresses.iter().enumerate() {
                            let local_shutdowns: Vec<UnifiedAlert> = shutdowns
                                .iter()
                                .filter(|sd| {
                                    sd.matches_address(
                                        &addr.city_name,
                                        &addr.commune,
                                        &addr.street_name_1,
                                        &addr.street_name_2,
                                    )
                                })
                                .map(|sd| {
                                    let mut alert = sd.to_unified();
                                    alert.address_index = Some(idx);
                                    alert.is_local = Some(true);
                                    alert.description = Some(format!("Miejscowość: {}", addr.city_name));
                                    alert
                                })
                                .collect();
                            alerts.extend(local_shutdowns);
                        }
                        (alerts, Vec::new())
                    }
                    Err(e) => (Vec::new(), vec![format!("Energa: {}", e)]),
                }
            }));
        }

        // --- Enea ---
        if enabled_sources.contains(&"enea".to_string()) {
            let s_enea = s.clone();
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                let mut target_regions = Vec::new();
                for addr in &s_enea.addresses {
                    target_regions.extend(enea::get_enea_regions_for_district(&addr.district));
                }
                target_regions.sort();
                target_regions.dedup();

                if target_regions.is_empty() {
                    return (Vec::new(), Vec::new());
                }

                match build_client() {
                    Ok(client) => match retry(|| enea::fetch_all_enea_outages(&client, &target_regions), 3).await {
                        Ok(items) => {
                            let mut alerts = Vec::new();
                            for (idx, addr) in s_enea.addresses.iter().enumerate() {
                                let local_items: Vec<UnifiedAlert> = items
                                    .iter()
                                    .filter(|item| {
                                        item.matches_address(
                                            &addr.city_name,
                                            &addr.commune,
                                            &addr.street_name_1,
                                            &addr.street_name_2,
                                        )
                                    })
                                    .map(|item| {
                                        let mut alert = item.to_unified();
                                        alert.address_index = Some(idx);
                                        alert.is_local = Some(true);
                                        alert.description = Some(format!("Miejscowość: {}", addr.city_name));
                                        alert
                                    })
                                    .collect();
                                alerts.extend(local_items);
                            }
                            (alerts, Vec::new())
                        }
                        Err(e) => (Vec::new(), vec![format!("Enea API Error: {}", e)]),
                    },
                    Err(e) => (Vec::new(), vec![format!("Enea Client Error: {}", e)]),
                }
            }));
        }

        // --- PGE ---
        if enabled_sources.contains(&"pge".to_string()) {
            let s_pge = s.clone();
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                match retry(|| pge::fetch_pge_outages(), 3).await {
                    Ok(outages) => {
                        let mut alerts = Vec::new();
                        for (idx, addr) in s_pge.addresses.iter().enumerate() {
                            let local_outages: Vec<UnifiedAlert> = outages
                                .iter()
                                .filter(|po| pge::matches_address(po, addr))
                                .map(|po| {
                                    let mut alert = po.to_unified();
                                    alert.address_index = Some(idx);
                                    alert.is_local = Some(true);
                                    alert.description = Some(format!("Miejscowość: {}", addr.city_name));
                                    alert
                                })
                                .collect();
                            alerts.extend(local_outages);
                        }
                        (alerts, Vec::new())
                    }
                    Err(e) => (Vec::new(), vec![format!("PGE: {}", e)]),
                }
            }));
        }

        // --- Stoen ---
        if enabled_sources.contains(&"stoen".to_string()) {
            let s_stoen = s.clone();
            let sem = semaphore.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.ok();
                match retry(|| stoen::fetch_stoen_outages(), 3).await {
                    Ok(outages) => {
                        let mut alerts = Vec::new();
                        for outage in outages {
                            let mut matched = false;
                            for (idx, addr) in s_stoen.addresses.iter().enumerate() {
                                if stoen::matches_address(&outage, addr) {
                                    let mut alert = outage.to_unified();
                                    alert.address_index = Some(idx);
                                    alert.is_local = Some(true);
                                    alert.description = Some(format!("Miejscowość: {}", addr.city_name));
                                    alerts.push(alert);
                                    matched = true;
                                }
                            }
                            if !matched {
                                let mut alert = outage.to_unified();
                                alert.is_local = Some(false);
                                alert.description = Some("Miejscowość: Warszawa".to_string());
                                alerts.push(alert);
                            }
                        }
                        (alerts, Vec::new())
                    }
                    Err(e) => (Vec::new(), vec![format!("STOEN: {}", e)]),
                }
            }));
        }

        // Wait for all tasks to complete
        let results = join_all(tasks).await;
        for res in results {
            match res {
                Ok((alerts, errs)) => {
                    all_alerts.extend(alerts);
                    errors.extend(errs);
                }
                Err(e) => errors.push(format!("Task panic: {}", e)),
            }
        }

        // --- PROCESS NEW ALERTS AND NOTIFY ---
        for alert in &all_alerts {
            if alert.is_local == Some(true) {
                let source_key = alert.source.to_string();
                let notified_enabled = s
                    .notification_preferences
                    .get(&source_key)
                    .copied()
                    .unwrap_or(false);

                if notified_enabled {
                    let hash = alert.to_hash();
                    match state_db::is_alert_seen(&app, &source_key, &hash) {
                        Ok(seen) => {
                            if !seen {
                                // Trigger notification
                                let title = match alert.source {
                                    AlertSource::Tauron
                                    | AlertSource::Energa
                                    | AlertSource::Enea
                                    | AlertSource::Pge
                                    | AlertSource::Stoen => "Nowa awaria prądu",
                                    AlertSource::Water => "Nowa awaria wody",
                                    AlertSource::Fortum => "Nowa awaria ogrzewania",
                                };
                                let body = alert.message.clone().unwrap_or_default();

                                log::info!("Triggering notification for {}: {}", source_key, body);
                                app.notification()
                                    .builder()
                                    .title(title)
                                    .body(body)
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
