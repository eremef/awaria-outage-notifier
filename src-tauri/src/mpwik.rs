use reqwest::Client;
use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert, AlertProvider, Settings, is_wroclaw};
use crate::utils::retry;
use async_trait::async_trait;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use regex::Regex;

pub const MPWIK_URL_PRODUCTION: &str = "https://www.mpwik.wroc.pl/wp-admin/admin-ajax.php";

fn get_mpwik_url() -> String {
    #[cfg(test)]
    {
        std::env::var("MPWIK_BASE_URL").unwrap_or_else(|_| MPWIK_URL_PRODUCTION.to_string())
    }
    #[cfg(not(test))]
    {
        MPWIK_URL_PRODUCTION.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MpwikFailureItem {
    pub content: Option<String>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MpwikResponse {
    pub failures: Option<Vec<MpwikFailureItem>>,
}

/// Parse MPWiK date format "DD-MM-YYYY HH:mm" into ISO "YYYY-MM-DDTHH:mm:00".
pub fn parse_mpwik_date(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:00",
        date_parts[2], date_parts[1], date_parts[0], parts[1]
    ))
}

pub struct CompiledMpwikRegex {
    pub street_candidates: Vec<Regex>,
    pub has_street: bool,
}

impl CompiledMpwikRegex {
    pub fn new(address: &AddressEntry) -> Self {
        let mut street_candidates = Vec::new();
        let has_street = !address.street_name_1.is_empty();

        if has_street {
            let mut words = Vec::new();
            if let Some(n2) = &address.street_name_2 {
                let compound = format!("{} {}", n2.trim(), address.street_name_1.trim());
                words.push(compound);
            }
            for word in address.street_name_1.split_whitespace() {
                if word.len() >= 3 {
                    words.push(word.to_string());
                }
            }
            if let Some(n2) = &address.street_name_2 {
                for word in n2.split_whitespace() {
                    if word.len() >= 3 {
                        words.push(word.to_string());
                    }
                }
            }

            for word in words {
                let p = format!(r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)", regex::escape(&word));
                if let Ok(r) = Regex::new(&p) {
                    street_candidates.push(r);
                }
            }
        }
        Self {
            street_candidates,
            has_street,
        }
    }

    pub fn is_match(&self, message: &str) -> bool {
        if !self.has_street {
            return true;
        }
        self.street_candidates.iter().any(|r| r.is_match(message))
    }
}


impl MpwikFailureItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Water,
            startDate: self.date_start.as_deref().and_then(parse_mpwik_date),
            endDate: self.date_end.as_deref().and_then(parse_mpwik_date),
            message: self.content.clone(),
            description: None,
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub async fn fetch_water_alerts(client: &Client) -> Result<Vec<MpwikFailureItem>, String> {
    let res = client
        .post(get_mpwik_url())
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

    let data: MpwikResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(data.failures.unwrap_or_default())
}

pub struct MpwikProvider;

#[async_trait]
impl AlertProvider for MpwikProvider {
    fn id(&self) -> String {
        "water".to_string()
    }

    async fn fetch(
        &self,
        _client: &Client,
        client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !settings.addresses.iter().any(|a| a.is_active && is_wroclaw(a)) {
            return (Vec::new(), Vec::new());
        }

        match retry(|| fetch_water_alerts(client_http1), 3).await {
            Ok(items) => {
                let mut alerts = Vec::new();
                let active_addresses: Vec<(usize, Arc<CompiledMpwikRegex>)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.is_active && is_wroclaw(a))
                    .map(|(idx, a)| (idx, Arc::new(CompiledMpwikRegex::new(a))))
                    .collect();

                for item in items {
                    let mut local_match_idx = None;
                    if let Some(content) = &item.content {
                        for (idx, compiled) in &active_addresses {
                            if compiled.is_match(content) {
                                local_match_idx = Some(*idx);
                                break;
                            }
                        }
                    }

                    let mut alert = item.to_unified();
                    alert.description = Some("Miejscowość: Wrocław".to_string());
                    if let Some(idx) = local_match_idx {
                        alert.address_index = Some(idx);
                        alert.is_local = Some(true);
                    } else {
                        alert.is_local = Some(false);
                    }
                    alerts.push(alert);
                }
                (alerts, Vec::new())
            }
            Err(e) => (Vec::new(), vec![format!("MPWiK: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mpwik_date() {
        let date = "12-03-2026 08:30";
        let parsed = parse_mpwik_date(date);
        assert_eq!(parsed, Some("2026-03-12T08:30:00".to_string()));

        let invalid = "invalid date";
        assert_eq!(parse_mpwik_date(invalid), None);
    }

    #[test]
    fn test_mpwik_to_unified() {
        let item = MpwikFailureItem {
            content: Some("Test water outage".to_string()),
            date_start: Some("12-03-2026 08:30".to_string()),
            date_end: Some("12-03-2026 16:00".to_string()),
        };
        let unified = item.to_unified();
        assert_eq!(unified.source, AlertSource::Water);
        assert_eq!(unified.message, Some("Test water outage".to_string()));
        assert_eq!(unified.startDate, Some("2026-03-12T08:30:00".to_string()));
        assert_eq!(unified.endDate, Some("2026-03-12T16:00:00".to_string()));
    }

    #[test]
    fn test_matches_address_wroclaw() {
        let addr = AddressEntry {
            name: "Home".to_string(),
            city_name: "Wrocław".to_string(),
            voivodeship: "".to_string(),
            district: "".to_string(),
            commune: "".to_string(),
            street_name: "ul. Kuźnicza".to_string(),
            street_name_1: "Kuźnicza".to_string(),
            street_name_2: None,
            house_no: "25".to_string(),
            city_id: Some(969400),
            street_id: Some(13900),
            is_active: true,
        };

        let compiled = CompiledMpwikRegex::new(&addr);

        let msg = "Awaria na ul. Kuźnicza";
        println!("Checking msg: {:?}", msg);
        assert!(compiled.is_match(msg));

        let msg_other = "Awaria na ul. Legnickiej";
        assert!(!compiled.is_match(msg_other));

        // Mixed case and without "ul."
        assert!(compiled.is_match("WROCŁAW KUŹNICZA 25"));
    }

    #[tokio::test]
    async fn test_fetch_water_real() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client_http1().unwrap();
        match fetch_water_alerts(&client).await {
            Ok(items) => {
                println!("Fetched {} MPWiK items", items.len());
            }
            Err(e) => {
                println!("Skipping MPWiK integration test (API failed): {}", e);
            }
        }
    }
}
