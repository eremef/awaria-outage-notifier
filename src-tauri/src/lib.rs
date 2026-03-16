mod api_logic;

use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use chrono::{Utc, SecondsFormat};
use api_logic::{
    GeoItem, Settings, UnifiedAlert, BASE_URL, MPWIK_URL,
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
async fn fetch_outages(app: AppHandle) -> Result<api_logic::OutageResponse, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?
        .ok_or_else(|| "No settings configured. Please set up your location first.".to_string())?;

    let now = Utc::now();
    let from_date = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let cache_bust = now.timestamp_millis().to_string();
    
    let query = get_outages_query(
        settings.cityGAID,
        settings.streetGAID,
        &settings.houseNo,
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
async fn fetch_all_alerts(app: AppHandle) -> Result<Vec<UnifiedAlert>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;

    let mut all_alerts: Vec<UnifiedAlert> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Fetch Tauron alerts (requires settings)
    if let Some(ref s) = settings {
        match fetch_outages_internal(s).await {
            Ok(tauron_alerts) => all_alerts.extend(tauron_alerts),
            Err(e) => errors.push(format!("Tauron: {}", e)),
        }
    }

    // Fetch MPWiK water alerts (no settings needed)
    match fetch_water_alerts().await {
        Ok(water_alerts) => all_alerts.extend(water_alerts),
        Err(e) => errors.push(format!("MPWiK: {}", e)),
    }

    if all_alerts.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }

    Ok(all_alerts)
}

/// Internal helper: fetch Tauron outages and convert to UnifiedAlert vec.
async fn fetch_outages_internal(settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let now = Utc::now();
    let from_date = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let cache_bust = now.timestamp_millis().to_string();

    let query = get_outages_query(
        settings.cityGAID,
        settings.streetGAID,
        &settings.houseNo,
        &from_date,
        &cache_bust,
    );

    let client = build_client()?;
    let res = client
        .get(&format!("{}/outages/address", BASE_URL))
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

    let data = res
        .json::<api_logic::OutageResponse>()
        .await
        .map_err(|e| e.to_string())?;

    let alerts: Vec<UnifiedAlert> = data
        .OutageItems
        .unwrap_or_default()
        .iter()
        .map(|item| item.to_unified())
        .collect();

    Ok(alerts)
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
        fetch_outages,
        fetch_all_alerts,
        fetch_water_alerts,
        lookup_city,
        lookup_street,
        save_settings,
        load_settings
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
