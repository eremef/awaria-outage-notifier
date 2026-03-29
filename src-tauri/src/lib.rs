mod api_logic;
mod enea;
mod energa;
mod tauron;
mod teryt;

use api_logic::{
    load_settings_from_path, matches_address, matches_street_only, save_settings_to_path,
    AddressEntry, FortumCity, Settings, UnifiedAlert, FORTUM_CITIES_URL, FORTUM_URL, MPWIK_URL,
};
use chrono::{SecondsFormat, Utc};
use std::fs;
use std::path::PathBuf;
use tauri::command;
use tauri::AppHandle;
use tauri::Manager;
use tauron::{build_client, GeoItem, OutageResponse, BASE_URL};
use teryt::{TerytCity, TerytStreet};

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("settings.json"))
}

// ── Tauron API lookups (internal, for outage fetching) ───

async fn lookup_city(
    city_name: &str,
    voivodeship: &str,
    district: &str,
    commune: &str,
) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let encoded_name = city_name.replace(' ', "%20");
    let url = format!(
        "{}/enum/geo/cities?partName={}&_={}",
        BASE_URL, encoded_name, cache_bust
    );

    log::info!("Tauron API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    let cities: Vec<GeoItem> = res.json().await.map_err(|e| e.to_string())?;

    // Filter by administrative units
    let filtered: Vec<GeoItem> = if voivodeship.is_empty() {
        cities
    } else {
        cities
            .into_iter()
            .filter(|c| {
                let p_match = c
                    .ProvinceName
                    .as_ref()
                    .map(|p| p.to_lowercase() == voivodeship.to_lowercase())
                    .unwrap_or(false);
                let d_match = c
                    .DistrictName
                    .as_ref()
                    .map(|d| d.to_lowercase() == district.to_lowercase())
                    .unwrap_or(false);
                let c_match = c
                    .CommuneName
                    .as_ref()
                    .map(|cm| cm.to_lowercase() == commune.to_lowercase())
                    .unwrap_or(false);
                p_match && d_match && c_match
            })
            .collect()
    };

    Ok(filtered)
}
async fn lookup_street(street_name: &str, city_gaid: u64) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let encoded_name = street_name.replace(' ', "%20");
    let url = format!(
        "{}/enum/geo/streets?partName={}&ownerGAID={}&_={}",
        BASE_URL, encoded_name, city_gaid, cache_bust
    );

    log::info!("Tauron API: GET {}", url);

    let res = client
        .get(&url)
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

async fn lookup_only_one_street(city_gaid: u64) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let url = format!(
        "{}/enum/geo/onlyonestreet?ownerGAID={}&_={}",
        BASE_URL, city_gaid, cache_bust
    );

    log::info!("Tauron API (onlyonestreet): GET {}", url);

    let res = client
        .get(&url)
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

// ── Outage fetching ───────────────────────────────────────

async fn fetch_tauron_outages(address: &AddressEntry) -> Result<OutageResponse, String> {
    let street_query = match &address.street_name_2 {
        Some(n2) => format!("{} {}", n2.trim(), address.street_name_1.trim()),
        None => address.street_name_1.clone(),
    };

    log::info!(
        "Tauron: fetching for city='{}' ({}/{}/{}), street='{}'",
        address.city_name,
        address.voivodeship,
        address.district,
        address.commune,
        street_query
    );

    // Look up Tauron GAIDs dynamically from address names
    let cities = lookup_city(
        &address.city_name,
        &address.voivodeship,
        &address.district,
        &address.commune,
    )
    .await?;
    let city = cities
        .into_iter()
        .next()
        .ok_or_else(|| format!("City '{}' not found in Tauron", address.city_name))?;

    log::info!("Tauron: found city '{}' GAID={}", city.Name, city.GAID);

    let streets = if address.street_name_1.is_empty() {
        lookup_only_one_street(city.GAID).await?
    } else {
        lookup_street(&street_query, city.GAID).await?
    };

    if streets.is_empty() {
        return Err(format!(
            "Street '{}' not found in Tauron (no results)",
            street_query
        ));
    }

    for s in &streets {
        log::info!("Tauron: street candidate: '{}' (GAID={})", s.Name, s.GAID);
    }

    let street = streets.into_iter().next().unwrap();

    log::info!(
        "Tauron: found street '{}' GAID={} (queried as '{}')",
        street.Name,
        street.GAID,
        street_query
    );

    let now = Utc::now();
    let from_date = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let cache_bust = now.timestamp_millis().to_string();

    let url = format!(
        "{}/outages/address?cityGAID={}&streetGAID={}&houseNo={}&fromDate={}&getLightingSupport=false&getServicedSwitchingoff=true&_={}",
        BASE_URL, city.GAID, street.GAID, address.house_no.replace(' ', "%20"), from_date.replace(' ', "%20"), cache_bust
    );

    log::info!("Tauron API (outages){}", url);

    let client = build_client()?;
    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error! status: {}", res.status()));
    }

    let mut data = res
        .json::<OutageResponse>()
        .await
        .map_err(|e| e.to_string())?;

    data.debug_query = Some(url.clone());

    Ok(data)
}

async fn fetch_water_alerts() -> Result<Vec<UnifiedAlert>, String> {
    let client = build_client()?;
    let res = client
        .post(MPWIK_URL)
        .header(
            "content-type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        )
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

async fn fetch_fortum_cities() -> Result<Vec<FortumCity>, String> {
    let client = build_client()?;
    log::info!("Fortum: GET {}", FORTUM_CITIES_URL);
    let res = client
        .get(FORTUM_CITIES_URL)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Fortum cities HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

async fn fetch_fortum_alerts(city_guid: &str, region_id: u32) -> Result<Vec<UnifiedAlert>, String> {
    let client = build_client()?;

    let planned_url = format!(
        "{}?cityGuid={}&regionId={}&current=false",
        FORTUM_URL, city_guid, region_id
    );
    let current_url = format!(
        "{}?cityGuid={}&regionId={}&current=true",
        FORTUM_URL, city_guid, region_id
    );

    log::info!("Fortum API: planned={}, current={}", planned_url, current_url);

    let (planned_res, current_res) = tokio::join!(
        client
            .get(&planned_url)
            .header("accept", "application/json")
            .send(),
        client
            .get(&current_url)
            .header("accept", "application/json")
            .send()
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
async fn fetch_all_alerts(app: AppHandle) -> Result<Vec<UnifiedAlert>, String> {
    let path = settings_path(&app)?;
    let settings = load_settings_from_path(&path)?;

    let mut all_alerts: Vec<UnifiedAlert> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let enabled_sources = settings
        .as_ref()
        .and_then(|s| s.enabled_sources.clone())
        .unwrap_or_default();

    log::info!(
        "fetch_all_alerts: enabled_sources={:?}, addresses={}",
        enabled_sources,
        settings.as_ref().map(|s| s.addresses.len()).unwrap_or(0)
    );

    // Fetch Tauron alerts for each address (only if provider enabled)
    if enabled_sources.contains(&"tauron".to_string()) {
        if let Some(ref s) = settings {
            for (idx, addr) in s.addresses.iter().enumerate() {
                match fetch_tauron_outages(addr).await {
                    Ok(response) => {
                        let alerts: Vec<UnifiedAlert> = response
                            .OutageItems
                            .unwrap_or_default()
                            .iter()
                            .map(|item| {
                                let mut alert = item.to_unified();
                                alert.address_index = Some(idx);
                                alert.is_local = Some(
                                    matches_address(&item.Message, &addr.city_name, &addr.street_name_1, &addr.street_name_2),
                                );
                                alert
                            })
                            .collect();
                        all_alerts.extend(alerts);
                    }
                    Err(e) => {
                        log::error!("Tauron[{}]: {}", idx, e);
                        errors.push(format!("Tauron[{}]: {}", idx, e));
                    }
                }
            }
        }
    }

    // Fetch MPWiK water alerts (only if provider enabled)
    if enabled_sources.contains(&"water".to_string()) {
        match fetch_water_alerts().await {
            Ok(water_alerts) => all_alerts.extend(water_alerts),
            Err(e) => errors.push(format!("MPWiK: {}", e)),
        }
    }

    // Fetch Fortum alerts (only if provider enabled)
    if enabled_sources.contains(&"fortum".to_string()) {
        match fetch_fortum_cities().await {
            Ok(cities) => {
                if let Some(ref s) = settings {
                    // Group addresses by Fortum city to minimize API calls
                    let mut city_map = std::collections::HashMap::new();
                    for (idx, addr) in s.addresses.iter().enumerate() {
                        if let Some(fc) = cities.iter().find(|c| {
                            c.city_name.to_lowercase() == addr.city_name.to_lowercase()
                        }) {
                            city_map
                                .entry((fc.city_guid.clone(), fc.region_id, fc.city_name.clone()))
                                .or_insert_with(Vec::new)
                                .push((idx, addr));
                        }
                    }

                    for ((guid, rid, city_name), addrs) in city_map {
                        match fetch_fortum_alerts(&guid, rid).await {
                            Ok(alerts) => {
                                for a in &alerts {
                                    let mut matched_any = false;
                                    for (idx, addr) in &addrs {
                                        if matches_street_only(
                                            &a.message,
                                            &addr.street_name_1,
                                            &addr.street_name_2,
                                        ) {
                                            let mut alert = a.clone();
                                            alert.address_index = Some(*idx);
                                            alert.is_local = Some(true);
                                            all_alerts.push(alert);
                                            matched_any = true;
                                        }
                                    }

                                    if !matched_any {
                                        if let Some((idx, _)) = addrs.first() {
                                            let mut alert = a.clone();
                                            alert.address_index = Some(*idx);
                                            alert.is_local = Some(false);
                                            all_alerts.push(alert);
                                        }
                                    }
                                }
                            }
                            Err(e) => errors.push(format!("Fortum ({}): {}", city_name, e)),
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("Fortum cities: {}", e)),
        }
    }

    // Fetch Energa alerts
    if enabled_sources.contains(&"energa".to_string()) {
        match fetch_energa_alerts().await {
            Ok(shutdowns) => {
                if let Some(ref s) = settings {
                    for (idx, addr) in s.addresses.iter().enumerate() {
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
                                alert
                            })
                            .collect();
                        all_alerts.extend(local_shutdowns);
                    }
                }
            }
            Err(e) => errors.push(format!("Energa: {}", e)),
        }
    }

    // Fetch Enea alerts
    if enabled_sources.contains(&"enea".to_string()) {
        if let Some(ref s) = settings {
            let mut target_regions = Vec::new();
            for addr in &s.addresses {
                target_regions.extend(enea::get_enea_regions_for_district(&addr.district));
            }
            target_regions.sort();
            target_regions.dedup();

            if !target_regions.is_empty() {
                match build_client() {
                    Ok(client) => match enea::fetch_all_enea_outages(&client, &target_regions).await {
                        Ok(items) => {
                            for (idx, addr) in s.addresses.iter().enumerate() {
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
                                        alert
                                    })
                                    .collect();
                                all_alerts.extend(local_items);
                            }
                        }
                        Err(e) => errors.push(format!("Enea API Error: {}", e)),
                    },
                    Err(e) => errors.push(format!("Enea Client Error: {}", e)),
                }
            }
        }
    }

    if all_alerts.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }

    Ok(all_alerts)
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
        .plugin(tauri_plugin_fs::init())
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
            fetch_all_alerts,
            teryt_lookup_city,
            teryt_lookup_street,
            teryt_city_has_streets,
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
