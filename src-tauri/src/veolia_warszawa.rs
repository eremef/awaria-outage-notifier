use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use crate::mpwik::CompiledMpwikRegex;
use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VeoliaItem {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub permalink: String,
    pub street: String,
    pub house_number: String,
    pub start_date: String,
    pub end_date: String,
}

pub struct VeoliaDetail {
    pub description: String,
    pub affected_addresses: Vec<String>,
}

/// Parse Veolia date format "DD.MM.YYYY HH:MM" into ISO "YYYY-MM-DDTHH:MM:00".
pub fn parse_veolia_date(s: &str) -> Option<String> {
    let cleaned = s.trim();
    if cleaned.len() >= 16 {
        let day = &cleaned[0..2];
        let month = &cleaned[3..5];
        let year = &cleaned[6..10];
        let time = &cleaned[11..16];
        Some(format!("{}-{}-{}T{}:00", year, month, day, time))
    } else {
        None
    }
}

pub fn parse_veolia_details(html: &str) -> Result<VeoliaDetail, String> {
    use scraper::{Html, Selector};
    let document = Html::parse_document(html);

    let label_value_selector = Selector::parse(".ep-label-value").map_err(|e| e.to_string())?;
    let li_selector = Selector::parse("#container-kontakt ul li").map_err(|e| e.to_string())?;

    let mut description = String::new();
    for el in document.select(&label_value_selector) {
        let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
        if !text.is_empty() && text.contains("Veolia") {
            description = text;
            break;
        }
    }

    if description.is_empty() {
        let p_selector = Selector::parse("#container-kontakt p").map_err(|e| e.to_string())?;
        for el in document.select(&p_selector) {
            let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if text.contains("Veolia Energia Warszawa S.A. informuje") {
                description = text;
                break;
            }
        }
    }

    let mut affected_addresses = Vec::new();
    for el in document.select(&li_selector) {
        let text = el.text().collect::<Vec<_>>().join(" ")
            .replace('\t', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_uppercase();
        if !text.is_empty() {
            affected_addresses.push(text);
        }
    }

    Ok(VeoliaDetail {
        description,
        affected_addresses,
    })
}

pub async fn fetch_veolia_details(client: &Client, permalink: &str) -> Result<VeoliaDetail, String> {
    let res = client
        .get(permalink)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Veolia details HTTP error: {}", res.status()));
    }

    let html = res.text().await.map_err(|e| e.to_string())?;
    parse_veolia_details(&html)
}

pub async fn fetch_veolia_alerts_for_street(
    client: &Client,
    street: &str,
) -> Result<Vec<VeoliaItem>, String> {
    let url = format!(
        "https://www.energiadlawarszawy.pl/wp-admin/admin-ajax.php?action=my_ajax_filter_search_waw&street={}",
        urlencoding::encode(street)
    );
    let res = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Veolia API HTTP error: {}", res.status()));
    }

    let items: Vec<VeoliaItem> = res.json().await.map_err(|e| e.to_string())?;
    Ok(items)
}

impl VeoliaItem {
    pub fn to_unified(&self, detail: &Option<VeoliaDetail>) -> UnifiedAlert {
        let start_date = parse_veolia_date(&self.start_date);
        let end_date = parse_veolia_date(&self.end_date);

        let detail_desc = detail.as_ref().map(|d| d.description.clone()).unwrap_or_else(|| self.content.clone());
        let addresses_str = detail.as_ref().map(|d| d.affected_addresses.join(", ")).unwrap_or_default();

        let mut parts = Vec::new();
        parts.push("Przerwa w dostawie ciepła".to_string());
        if !self.street.is_empty() {
            parts.push(format!("ul. {}", self.street));
        }
        if !detail_desc.is_empty() {
            parts.push(detail_desc.to_string());
        }
        if !addresses_str.is_empty() {
            parts.push(format!("posesje: {}", addresses_str));
        }
        let message = parts.join(" - ");

        UnifiedAlert {
            source: AlertSource::VeoliaWarszawa,
            startDate: start_date,
            endDate: end_date,
            message: Some(message),
            description: Some("Miejscowość: Warszawa".to_string()),
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct VeoliaWarszawaProvider;

#[async_trait]
impl AlertProvider for VeoliaWarszawaProvider {
    fn id(&self) -> String {
        "veolia_warszawa".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::VeoliaWarszawa
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
            .filter(|(_, a)| a.is_active && crate::api_logic::is_warszawa(a))
            .map(|(idx, a)| (idx, a.street_name_1.clone(), Arc::new(CompiledMpwikRegex::new(a))))
            .collect();

        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        let mut alerts = Vec::new();

        for (idx, street_name, _compiled) in &active_addresses {
            match retry(|| fetch_veolia_alerts_for_street(client_http1, street_name), 3).await {
                Ok(items) => {
                    for item in items {
                        if !seen_ids.insert(item.id) {
                            continue;
                        }

                        let details = match retry(|| fetch_veolia_details(client_http1, &item.permalink), 3).await {
                            Ok(det) => Some(det),
                            Err(e) => {
                                log::error!("Failed to fetch details for Veolia item {}: {}", item.id, e);
                                None
                            }
                        };

                        let mut alert = item.to_unified(&details);
                        alert.address_index = Some(*idx);
                        alert.is_local = Some(true);
                        alerts.push(alert);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Veolia (ul. {}): {}", street_name, e);
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
    fn test_parse_veolia_date() {
        assert_eq!(
            parse_veolia_date("22.05.2026 09:20"),
            Some("2026-05-22T09:20:00".to_string())
        );
        assert_eq!(parse_veolia_date("invalid"), None);
    }

    #[test]
    fn test_parse_veolia_details() {
        let html = r#"
            <div id="container-kontakt">
                <div class="row">
                    <div class="col-lg-12 col-md-12 col-sm-12">
                        <span class="ep-label-value">Veolia Energia Warszawa S.A. informuje, że w rejonie Warszawa ulica POŻARYSKIEGO M. 24 nastąpiła przerwa w dostawach ciepła.</span>
                    </div>
                    <div class="col-lg-12 col-md-12 col-sm-12">
                        <ul>
                            <li>KOŻUCHOWSKA	7</li>
                            <li>KRUPNICZA	15</li>
                        </ul>
                    </div>
                </div>
            </div>
        "#;

        let parsed = parse_veolia_details(html).unwrap();
        assert_eq!(parsed.description, "Veolia Energia Warszawa S.A. informuje, że w rejonie Warszawa ulica POŻARYSKIEGO M. 24 nastąpiła przerwa w dostawach ciepła.");
        assert_eq!(parsed.affected_addresses, vec!["KOŻUCHOWSKA 7", "KRUPNICZA 15"]);
    }
}
