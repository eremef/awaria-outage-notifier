use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings, AddressEntry};
use reqwest::Client;
use crate::utils::retry;
use async_trait::async_trait;
use futures::future::join_all;
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub const BASE_URL: &str = "https://cieplo.tauron.pl";

fn get_base_url() -> String {
    #[cfg(test)]
    {
        std::env::var("TAURON_HEAT_BASE_URL").unwrap_or_else(|_| BASE_URL.to_string())
    }
    #[cfg(not(test))]
    {
        BASE_URL.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct HeatGeoItem {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Gaid")]
    pub gaid: String,
    #[serde(rename = "ProvinceName")]
    pub province_name: Option<String>,
    #[serde(rename = "DistrictName")]
    pub district_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HeatGeoResponse {
    #[serde(rename = "List")]
    pub list: Vec<HeatGeoItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OutageInfoItem {
    pub reply: String,
    pub details: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OutageInfoList {
    pub info: Vec<OutageInfoItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OutagesData {
    #[serde(rename = "Present")]
    pub present: bool,
    #[serde(rename = "outageInfo")]
    pub outage_info: Option<OutageInfoList>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OutageResponse {
    #[serde(rename = "Outages")]
    pub outages: OutagesData,
}

pub async fn lookup_city(
    client: &Client,
    city_name: &str,
    voivodeship: &str,
) -> Result<Vec<HeatGeoItem>, String> {
    let call_id = Utc::now().timestamp_millis() % 10000;
    let url = format!(
        "{}/iapi/city/GetCities?partName={}&callid={}",
        get_base_url(), urlencoding::encode(city_name), call_id
    );

    log::info!("Tauron Heat API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    let response: HeatGeoResponse = res.json().await.map_err(|e| e.to_string())?;
    
    // Filter by voivodeship
    let filtered: Vec<HeatGeoItem> = if voivodeship.is_empty() {
        response.list
    } else {
        response.list
            .into_iter()
            .filter(|c| {
                c.province_name
                    .as_ref()
                    .map(|p| p.to_lowercase().contains(&voivodeship.to_lowercase()))
                    .unwrap_or(false)
            })
            .collect()
    };

    Ok(filtered)
}

pub async fn lookup_street(
    client: &Client,
    street_name: &str,
    city_gaid: &str,
) -> Result<Vec<HeatGeoItem>, String> {
    let call_id = Utc::now().timestamp_millis() % 10000;
    let url = format!(
        "{}/iapi/street/GetStreets?partName={}&ownerGaid={}&callid={}",
        get_base_url(), urlencoding::encode(street_name), city_gaid, call_id
    );

    log::info!("Tauron Heat API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    let response: HeatGeoResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(response.list)
}

pub async fn fetch_tauron_heat_outages(
    client: &Client,
    address: &AddressEntry,
) -> Result<OutageResponse, String> {
    log::info!(
        "Tauron Heat: fetching for city='{}', street='{}'",
        address.city_name,
        address.street_name_1
    );

    let cities = lookup_city(client, &address.city_name, &address.voivodeship).await?;
    let city = cities
        .into_iter()
        .next()
        .ok_or_else(|| format!("City '{}' not found in Tauron Heat", address.city_name))?;

    let street_query = match &address.street_name_2 {
        Some(n2) if !n2.is_empty() && n2 != "null" => format!("{} {}", n2.trim(), address.street_name_1.trim()),
        _ => address.street_name_1.clone(),
    };

    let mut streets = if street_query.is_empty() {
        Vec::new()
    } else {
        lookup_street(client, &street_query, &city.gaid).await?
    };

    // Fallbacks similar to Tauron Power
    if streets.is_empty() && !address.street_name_1.is_empty() && street_query != address.street_name_1 {
        streets = lookup_street(client, &address.street_name_1, &city.gaid).await.unwrap_or_default();
    }

    if streets.is_empty() {
        let base_name = if !address.street_name_1.is_empty() { &address.street_name_1 } else { &street_query };
        let prefixes = ["Plac ", "Pl. ", "ul. ", "ulica ", "Aleja ", "Al. "];
        for p in prefixes {
            if base_name.to_lowercase().starts_with(&p.to_lowercase()) {
                let clean_name = base_name[p.len()..].trim().to_string();
                if !clean_name.is_empty() {
                    streets = lookup_street(client, &clean_name, &city.gaid).await.unwrap_or_default();
                }
                if !streets.is_empty() { break; }
            }
        }
    }

    if streets.is_empty() {
        let base_name = if !address.street_name_1.is_empty() { &address.street_name_1 } else { &street_query };
        if let Some(last_word) = base_name.split_whitespace().last() {
            if last_word != base_name {
                streets = lookup_street(client, last_word, &city.gaid).await.unwrap_or_default();
            }
        }
    }

    if streets.is_empty() {
        log::warn!("Street '{}' not found in Tauron Heat for city {}.", street_query, city.name);
        return Ok(OutageResponse {
            outages: OutagesData { present: false, outage_info: None }
        });
    }

    let street = streets.into_iter().next().unwrap();

    let url = format!(
        "{}/iapi/warmoutage/getoutages?ulica={}",
        get_base_url(), street.gaid
    );

    log::info!("Tauron Heat API (outages): GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

pub struct TauronHeatProvider;

#[async_trait]
impl AlertProvider for TauronHeatProvider {
    fn id(&self) -> String {
        "tauron_heat".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::TauronHeat
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let mut tasks = Vec::new();

        for (idx, addr) in settings.addresses.iter().enumerate().filter(|(_, a)| a.is_active) {
            let addr = addr.clone();
            let client_c = client.clone();
            tasks.push(tauri::async_runtime::spawn(async move {
                match retry(|| fetch_tauron_heat_outages(&client_c, &addr), 3).await {
                    Ok(response) => {
                        let mut alerts = Vec::new();
                        if response.outages.present {
                            if let Some(info_list) = response.outages.outage_info {
                                for item in info_list.info {
                                    alerts.push(UnifiedAlert {
                                        source: AlertSource::TauronHeat,
                                        startDate: None,
                                        endDate: None,
                                        message: Some(item.reply),
                                        description: item.details,
                                        address_index: Some(idx),
                                        is_local: Some(true), // Since we queried by street GAID
                                        hash: None,
                                    });
                                }
                            }
                        }
                        (alerts, Vec::<String>::new())
                    }
                    Err(e) => (Vec::new(), vec![format!("TauronHeat[{}]: {}", idx, e)]),
                }
            }));
        }

        let results = join_all(tasks).await;
        let mut all_alerts = Vec::new();
        let mut all_errors = Vec::new();

        for res in results {
            match res {
                Ok((alerts, errs)) => {
                    all_alerts.extend(alerts);
                    all_errors.extend(errs);
                }
                Err(e) => all_errors.push(format!("TauronHeat task execution error: {}", e)),
            }
        }

        (all_alerts, all_errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[tokio::test]
    async fn test_fetch_tauron_heat_mocked() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let mut server = mockito::Server::new_async().await;
        let url = server.url();
        std::env::set_var("TAURON_HEAT_BASE_URL", url.clone());

        // Mock city lookup
        let _m1 = server.mock("GET", mockito::Matcher::Regex(r"^/iapi/city/GetCities.*".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"List": [{"Name": "Katowice", "Gaid": "9", "ProvinceName": "Śląskie"}]}"#)
            .create_async().await;

        // Mock street lookup
        let _m2 = server.mock("GET", mockito::Matcher::Regex(r"^/iapi/street/GetStreets.*".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"List": [{"Name": "Korfantego", "Gaid": "253518"}]}"#)
            .create_async().await;

        // Mock outage lookup (Present: true)
        let _m3 = server.mock("GET", mockito::Matcher::Regex(r"^/iapi/warmoutage/getoutages.*".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"Outages": {"Present": true, "outageInfo": {"info": [{"reply": "Brak ciepła", "details": "Awaria rury"}]}}}"#)
            .create_async().await;

        let client = Client::new();
        let addr = AddressEntry {
            city_name: "Katowice".to_string(),
            street_name_1: "Korfantego".to_string(),
            voivodeship: "Śląskie".to_string(),
            is_active: true,
            ..Default::default()
        };

        let result = fetch_tauron_heat_outages(&client, &addr).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.outages.present);
        assert_eq!(response.outages.outage_info.unwrap().info[0].reply, "Brak ciepła");

        std::env::remove_var("TAURON_HEAT_BASE_URL");
    }

    #[tokio::test]
    async fn test_fetch_tauron_heat_no_outages() {
        let _guard = TEST_MUTEX.lock().unwrap();
        let mut server = mockito::Server::new_async().await;
        let url = server.url();
        std::env::set_var("TAURON_HEAT_BASE_URL", url.clone());

        let _m1 = server.mock("GET", mockito::Matcher::Regex(r"^/iapi/city/GetCities.*".to_string()))
            .with_status(200)
            .with_body(r#"{"List": [{"Name": "Katowice", "Gaid": "9", "ProvinceName": "Śląskie"}]}"#)
            .create_async().await;

        let _m2 = server.mock("GET", mockito::Matcher::Regex(r"^/iapi/street/GetStreets.*".to_string()))
            .with_status(200)
            .with_body(r#"{"List": [{"Name": "Korfantego", "Gaid": "253518"}]}"#)
            .create_async().await;

        let _m3 = server.mock("GET", mockito::Matcher::Regex(r"^/iapi/warmoutage/getoutages.*".to_string()))
            .with_status(200)
            .with_body(r#"{"Outages":{"info":{"address":"Katowice","reply":"Nie mamy informacji"},"Present":false}}"#)
            .create_async().await;

        let client = Client::new();
        let addr = AddressEntry {
            city_name: "Katowice".to_string(),
            street_name_1: "Korfantego".to_string(),
            is_active: true,
            ..Default::default()
        };

        let result = fetch_tauron_heat_outages(&client, &addr).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.outages.present);

        std::env::remove_var("TAURON_HEAT_BASE_URL");
    }
}
