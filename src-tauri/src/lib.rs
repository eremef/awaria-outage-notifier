mod api_logic;

use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use chrono::{Utc, SecondsFormat};
use api_logic::{
    GeoItem, Settings, AddressEntry, UnifiedAlert, BASE_URL, MPWIK_URL, FORTUM_URL, FORTUM_CITY_GUID, FORTUM_REGION_ID,
    get_cities_query, get_streets_query, get_outages_query,
    save_settings_to_path, load_settings_from_path
};
use std::fs;
use std::path::PathBuf;

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("settings.json"))
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())
}

#[command]
async fn lookup_city(city_name: String) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let query = get_cities_query(&city_name, &cache_bust);

    let res = client
        .get(&format!("{}/enum/geo/cities", BASE_URL))
        .query(&query)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

#[command]
async fn lookup_street(street_name: String, city_gaid: u64) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let query = get_streets_query(&street_name, city_gaid, &cache_bust);

    let res = client
        .get(&format!("{}/enum/geo/streets", BASE_URL))
        .query(&query)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

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
    let mut settings = load_settings_from_path(&path)?
        .unwrap_or_else(|| Settings::default());
    
    if settings.addresses.len() >= 5 {
        return Err("Maximum of 5 addresses allowed".to_string());
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
    let mut settings = load_settings_from_path(&path)?
        .unwrap_or_else(|| Settings::default());
    
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
    let mut settings = load_settings_from_path(&path)?
        .unwrap_or_else(|| Settings::default());
    
    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }
    
    settings.primary_address_index = Some(index);
    save_settings_to_path(&path, &settings)?;
    Ok(settings)
}

#[command]
async fn update_address(app: AppHandle, index: usize, address: AddressEntry) -> Result<Settings, String> {
    let path = settings_path(&app)?;
    let mut settings = load_settings_from_path(&path)?
        .unwrap_or_else(|| Settings::default());
    
    if index >= settings.addresses.len() {
        return Err("Invalid address index".to_string());
    }
    
    settings.addresses[index] = address;
    save_settings_to_path(&path, &settings)?;
    Ok(settings)
}

#[command]
async fn fetch_outages_for_address(index: usize) -> Result<api_logic::OutageResponse, String> {
    let path = settings_path_from_app()?;
    let settings = load_settings_from_path(&path)?
        .ok_or_else(|| "No settings configured. Please set up your location first.".to_string())?;

    let address = settings.addresses.get(index)
        .ok_or_else(|| "Invalid address index".to_string())?;

    fetch_outages_for_addr(address).await
}

fn settings_path_from_app() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir()
        .ok_or("Could not determine data directory")?
        .join("xyz.eremef.awaria");
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("settings.json"))
}

async fn fetch_outages_for_addr(address: &AddressEntry) -> Result<api_logic::OutageResponse, String> {
    let now = Utc::now();
    let from_date = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let cache_bust = now.timestamp_millis().to_string();
    
    let query = get_outages_query(
        address.city_gaid,
        address.street_gaid,
        &address.house_no,
        &from_date,
        &cache_bust
    );

    let client = build_client()?;
    let res = client.get(&format!("{}/outages/address", BASE_URL))
        .query(&query)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error! status: {}", res.status()));
    }

    let mut data = res.json::<api_logic::OutageResponse>()
        .await
        .map_err(|e| e.to_string())?;

    let query_str = query.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    data.debug_query = Some(format!("{}/outages/address?{}", BASE_URL, query_str));

    Ok(data)
}

#[command]
async fn fetch_water_alerts() -> Result<Vec<UnifiedAlert>, String> {
    let client = build_client()?;
    let res = client
        .post(MPWIK_URL)
        .header("content-type", "application/x-www-form-urlencoded; charset=UTF-8")
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("origin", "https://www.mpwik.wroc.pl")
        .header("referer", "https://www.mpwik.wroc.pl/")
        .body("action=all")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("MPWiK HTTP error: {}", res.status()));
    }

    let data: api_logic::MpwikResponse = res.json().await.map_err(|e| e.to_string())?;
    let alerts: Vec<UnifiedAlert> = data
        .failures
        .unwrap_or_default()
        .iter()
        .map(|f| f.to_unified())
        .collect();

    Ok(alerts)
}

#[command]
async fn fetch_fortum_alerts() -> Result<Vec<UnifiedAlert>, String> {
    let client = build_client()?;
    
    let planned_url = format!("{}?cityGuid={}&regionId={}&current=false", FORTUM_URL, FORTUM_CITY_GUID, FORTUM_REGION_ID);
    let current_url = format!("{}?cityGuid={}&regionId={}&current=true", FORTUM_URL, FORTUM_CITY_GUID, FORTUM_REGION_ID);
    
    let (planned_res, current_res) = tokio::join!(
        client.get(&planned_url).header("accept", "application/json").send(),
        client.get(&current_url).header("accept", "application/json").send()
    );

    let planned_data: api_logic::FortumResponse = planned_res
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let current_data: api_logic::FortumResponse = current_res
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut seen_ids = std::collections::HashSet::new();
    let mut all_points = planned_data.points;
    all_points.extend(current_data.points);
    
    let alerts: Vec<UnifiedAlert> = all_points
        .into_iter()
        .filter(|p| seen_ids.insert(p.switch_off_id.clone()))
        .map(|p| p.to_unified())
        .collect();

    Ok(alerts)
}

#[command]
async fn fetch_all_alerts(app: AppHandle) -> Result<Vec<UnifiedAlert>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;

    let mut all_alerts: Vec<UnifiedAlert> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Fetch Tauron alerts for each address
    if let Some(ref s) = settings {
        for (idx, addr) in s.addresses.iter().enumerate() {
            match fetch_outages_for_addr(addr).await {
                Ok(response) => {
                    let alerts: Vec<UnifiedAlert> = response
                        .OutageItems
                        .unwrap_or_default()
                        .iter()
                        .map(|item| {
                            let mut alert = item.to_unified();
                            alert.address_index = Some(idx);
                            alert.is_local = Some(item.matches_street(&addr.street_name));
                            alert
                        })
                        .collect();
                    all_alerts.extend(alerts);
                }
                Err(e) => errors.push(format!("Tauron[{}]: {}", idx, e)),
            }
        }
    }

    // Fetch MPWiK water alerts (no settings needed)
    match fetch_water_alerts().await {
        Ok(water_alerts) => all_alerts.extend(water_alerts),
        Err(e) => errors.push(format!("MPWiK: {}", e)),
    }

    // Fetch Fortum alerts (Wrocław only, no settings needed)
    match fetch_fortum_alerts().await {
        Ok(fortum_alerts) => all_alerts.extend(fortum_alerts),
        Err(e) => errors.push(format!("Fortum: {}", e)),
    }

    if all_alerts.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }

    Ok(all_alerts)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
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
        fetch_outages_for_address,
        fetch_all_alerts,
        fetch_water_alerts,
        fetch_fortum_alerts,
        lookup_city,
        lookup_street,
        save_settings,
        load_settings,
        add_address,
        remove_address,
        set_primary_address,
        update_address
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
