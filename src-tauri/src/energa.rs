use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use regex::Regex;
use serde::Deserialize;
use async_trait::async_trait;
use std::sync::Arc;

pub const ENERGA_BASE_URL_PRODUCTION: &str = "https://energa-operator.pl";
pub const ENERGA_PAGE_URL_PRODUCTION: &str =
    "https://energa-operator.pl/uslugi/awarie-i-wylaczenia/wylaczenia-planowane";

fn get_energa_base_url() -> String {
    #[cfg(test)]
    {
        std::env::var("ENERGA_BASE_URL").unwrap_or_else(|_| ENERGA_BASE_URL_PRODUCTION.to_string())
    }
    #[cfg(not(test))]
    {
        ENERGA_BASE_URL_PRODUCTION.to_string()
    }
}

fn get_energa_page_url() -> String {
    #[cfg(test)]
    {
        std::env::var("ENERGA_PAGE_URL").unwrap_or_else(|_| ENERGA_PAGE_URL_PRODUCTION.to_string())
    }
    #[cfg(not(test))]
    {
        ENERGA_PAGE_URL_PRODUCTION.to_string()
    }
}

#[derive(Debug, Deserialize)]
pub struct EnergaResponse {
    pub document: EnergaDocument,
}

#[derive(Debug, Deserialize)]
pub struct EnergaDocument {
    pub payload: EnergaPayload,
}

#[derive(Debug, Deserialize)]
pub struct EnergaPayload {
    pub shutdowns: Vec<EnergaShutdown>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EnergaShutdown {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub message: Option<String>,
    pub areas: Option<Vec<String>>,
}

impl EnergaShutdown {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Energa,
            startDate: self.start_date.clone(),
            endDate: self.end_date.clone(),
            message: self.message.clone(),
            description: None,
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct CompiledEnergaRegex {
    pub city: Regex,
    pub commune: Regex,
    pub street_candidates: Vec<Regex>,
}

impl CompiledEnergaRegex {
    pub fn new(city: &str, commune: &str, street_name_1: &str, street_name_2: &Option<String>) -> Self {
        let city_pattern = format!(r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)", regex::escape(city));
        let city_regex = Regex::new(&city_pattern).unwrap_or_else(|_| Regex::new("").unwrap());

        let commune_pattern = format!(r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)", regex::escape(commune));
        let commune_regex = Regex::new(&commune_pattern).unwrap_or_else(|_| Regex::new("").unwrap());

        let mut street_candidates = Vec::new();
        if !street_name_1.is_empty() {
            let mut words = Vec::new();
            if let Some(n2) = street_name_2 {
                words.push(format!("{} {}", n2.trim(), street_name_1.trim()));
            }
            for word in street_name_1.split_whitespace() {
                if word.len() >= 3 {
                    words.push(word.to_string());
                }
            }
            if let Some(n2) = street_name_2 {
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
            city: city_regex,
            commune: commune_regex,
            street_candidates,
        }
    }

    pub fn is_match(&self, outage: &EnergaShutdown) -> bool {
        let Some(message) = &outage.message else {
            return false;
        };

        if !self.city.is_match(message) {
            return false;
        }

        if let Some(areas) = &outage.areas {
            if !areas.iter().any(|a| self.commune.is_match(a)) {
                return false;
            }
        }

        if self.street_candidates.is_empty() {
            return true;
        }
        self.street_candidates.iter().any(|r| r.is_match(message))
    }
}

impl EnergaShutdown {
    pub fn matches_address_compiled(&self, compiled: &CompiledEnergaRegex) -> bool {
        compiled.is_match(self)
    }
}

pub async fn extract_energa_api_url(client: &Client) -> Result<String, String> {
    let res = client
        .get(get_energa_page_url())
        .header("accept", "text/html")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch Energa page: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Energa HTML page HTTP error: {}", res.status()));
    }

    let html = res
        .text()
        .await
        .map_err(|e| format!("Failed to read Energa HTML text: {}", e))?;

    let re = Regex::new(r#"data-shutdowns="([^"]+)""#)
        .map_err(|e| format!("Regex compilation failed: {}", e))?;

    if let Some(caps) = re.captures(&html) {
        if let Some(suffix) = caps.get(1) {
            let url = format!("{}{}", get_energa_base_url(), suffix.as_str());
            return Ok(url);
        }
    }

    Err("Could not extract data-shutdowns URL suffix from Energa page HTML".to_string())
}

pub async fn fetch_energa_alerts(client: &Client) -> Result<Vec<EnergaShutdown>, String> {
    let url = extract_energa_api_url(client).await?;
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

    let data: EnergaResponse = res.json().await.map_err(|e| e.to_string())?;
    Ok(data.document.payload.shutdowns)
}

pub struct EnergaProvider;

#[async_trait]
impl AlertProvider for EnergaProvider {
    fn id(&self) -> String {
        "energa".to_string()
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        fn is_in_energa_region(addr: &crate::api_logic::AddressEntry) -> bool {
            let v = addr.voivodeship.to_lowercase();
            v.contains("pomorskie") || v.contains("warmińsko") || v.contains("zachodniopomorskie") || 
            v.contains("wielkopolskie") || v.contains("kujawsko") || v.contains("mazowieckie")
        }

        if !settings.addresses.iter().any(|a| a.is_active && is_in_energa_region(a)) {
            return (Vec::new(), Vec::new());
        }

        match retry(|| fetch_energa_alerts(client), 3).await {
                Ok(shutdowns) => {
                    let mut alerts = Vec::new();
                    let active_addresses: Vec<(usize, Arc<CompiledEnergaRegex>, String)> = settings
                        .addresses
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| a.is_active)
                        .map(|(idx, a)| {
                            (idx, Arc::new(CompiledEnergaRegex::new(&a.city_name, &a.commune, &a.street_name_1, &a.street_name_2)), a.city_name.clone())
                        })
                        .collect();

                    for (idx, compiled, city_name) in active_addresses {
                        let local_shutdowns: Vec<UnifiedAlert> = shutdowns
                            .iter()
                            .filter(|sd| sd.matches_address_compiled(&compiled))
                            .map(|sd| {
                                let mut alert = sd.to_unified();
                                alert.address_index = Some(idx);
                                alert.is_local = Some(true);
                                alert.description = Some(format!("Miejscowość: {}", city_name));
                                alert
                            })
                            .collect();
                        alerts.extend(local_shutdowns);
                    }
                    (alerts, Vec::new())
                }
            Err(e) => (Vec::new(), vec![format!("Energa: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_energa_address_match() {
        let shutdown = EnergaShutdown {
            start_date: None,
            end_date: None,
            message: Some("Tuliszków ulica Długa 1".to_string()),
            areas: Some(vec!["Tuliszków obszar wiejski w gminie miejsko-wiejskiej".to_string()]),
        };

        // Complete match -> true
        let compiled_tuliszkow = CompiledEnergaRegex::new("Tuliszków", "Tuliszków", "Długa", &None);
        assert!(shutdown.matches_address_compiled(&compiled_tuliszkow));

        // Wrong commune -> false
        let compiled_commune_fail = CompiledEnergaRegex::new("Tuliszków", "Wrocław", "Długa", &None);
        assert!(!shutdown.matches_address_compiled(&compiled_commune_fail));

        // Wrong city -> false
        let compiled_city_fail = CompiledEnergaRegex::new("Gdańsk", "Tuliszków", "Długa", &None);
        assert!(!shutdown.matches_address_compiled(&compiled_city_fail));

        // Matching city but completely wrong street -> should fail
        let compiled_street_fail = CompiledEnergaRegex::new("Tuliszków", "Tuliszków", "Króótka", &None);
        assert!(!shutdown.matches_address_compiled(&compiled_street_fail));
    }

    #[tokio::test]
    async fn test_fetch_energa_real() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client().unwrap();
        match extract_energa_api_url(&client).await {
            Ok(url) => {
                println!("Extracted Energa URL: {}", url);
                let res = client.get(&url).send().await.unwrap();
                assert!(res.status().is_success());
                let data: EnergaResponse = res.json().await.unwrap();
                println!("Found {} Energa shutdowns", data.document.payload.shutdowns.len());
            }
            Err(e) => {
                println!("Skipping Energa integration test (URL extract failed): {}", e);
            }
        }
    }
}
