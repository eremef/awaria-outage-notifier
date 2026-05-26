use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use crate::mpwik::CompiledMpwikRegex;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;
use scraper::Html;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VeoliaLodzItem {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub permalink: String,
    pub street: String,
    pub house_number: String,
    pub start_date: String,
    pub end_date: String,
}

/// Parse Veolia Łódź date format "3 września, 2026 00:00" into ISO "YYYY-MM-DDTHH:MM:00".
pub fn parse_veolia_lodz_date(s: &str) -> Option<String> {
    let s = s.trim().to_lowercase();
    let s = s.replace("stycznia", "01")
        .replace("lutego", "02")
        .replace("marca", "03")
        .replace("kwietnia", "04")
        .replace("maja", "05")
        .replace("czerwca", "06")
        .replace("lipca", "07")
        .replace("sierpnia", "08")
        .replace("września", "09")
        .replace("października", "10")
        .replace("listopada", "11")
        .replace("grudnia", "12")
        .replace(",", "");
        
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 4 {
        let day = format!("{:02}", parts[0].parse::<u32>().ok()?);
        let month = parts[1];
        let year = parts[2];
        let time = parts[3];
        Some(format!("{}-{}-{}T{}:00", year, month, day, time))
    } else {
        None
    }
}

pub fn extract_text_from_html(html: &str) -> String {
    let document = Html::parse_document(html);
    document.root_element().text().collect::<Vec<_>>().join(" ")
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub async fn fetch_veolia_lodz_alerts_for_street(
    client: &Client,
    street: &str,
) -> Result<Vec<VeoliaLodzItem>, String> {
    let url = format!(
        "https://www.energiadlalodzi.pl/wp-admin/admin-ajax.php?action=my_ajax_filter_search_ldz&street={}",
        urlencoding::encode(street)
    );
    let res = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Veolia Łódź API HTTP error: {}", res.status()));
    }

    let items: Vec<VeoliaLodzItem> = res.json().await.map_err(|e| e.to_string())?;
    Ok(items)
}

impl VeoliaLodzItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        let start_date = parse_veolia_lodz_date(&self.start_date);
        let end_date = parse_veolia_lodz_date(&self.end_date);

        let detail_desc = extract_text_from_html(&self.content);

        let mut parts = Vec::new();
        parts.push("Przerwa w dostawie ciepła".to_string());
        if !self.street.is_empty() {
            parts.push(format!("ul. {}", self.street));
        }
        if !detail_desc.is_empty() {
            parts.push(detail_desc);
        }
        
        let message = parts.join(" - ");

        UnifiedAlert {
            source: AlertSource::VeoliaLodz,
            startDate: start_date,
            endDate: end_date,
            message: Some(message),
            location: Some("Miejscowość: Łódź".to_string()),
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct VeoliaLodzProvider;

#[async_trait]
impl AlertProvider for VeoliaLodzProvider {
    fn id(&self) -> String {
        "veolia_lodz".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::VeoliaLodz
    }

    async fn fetch(
        &self,
        _client: &Client,
        client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<(usize, String, Arc<CompiledMpwikRegex>)> = settings
            .addresses
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_active && crate::api_logic::is_lodz(a))
            .map(|(idx, a)| (idx, a.street_name_1.clone(), Arc::new(CompiledMpwikRegex::new(a))))
            .collect();

        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        let mut alerts = Vec::new();

        for (idx, street_name, _compiled) in &active_addresses {
            match retry(|| fetch_veolia_lodz_alerts_for_street(client_http1, street_name), 3).await {
                Ok(items) => {
                    for item in items {
                        if !seen_ids.insert(item.id) {
                            continue;
                        }

                        let mut alert = item.to_unified();
                        alert.address_index = Some(*idx);
                        alert.is_local = Some(true);
                        alerts.push(alert);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Veolia Łódź (ul. {}): {}", street_name, e);
                    log::error!("{}", err_msg);
                    errors.push(err_msg);
                }
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_veolia_lodz_date() {
        assert_eq!(
            parse_veolia_lodz_date("3 września, 2026 00:00"),
            Some("2026-09-03T00:00:00".to_string())
        );
        assert_eq!(
            parse_veolia_lodz_date("10 Maja, 2026 15:30"),
            Some("2026-05-10T15:30:00".to_string())
        );
        assert_eq!(parse_veolia_lodz_date("invalid"), None);
    }

    #[test]
    fn test_extract_text_from_html() {
        let html = "<strong><span style=\"color: green;\">Przerwa w dostawie.</span></strong> Przepraszamy.";
        assert_eq!(extract_text_from_html(html), "Przerwa w dostawie. Przepraszamy.");
    }
}
