use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct VeoliaPoznanItem {
    pub id: u64,
    pub title: String,
    pub street: Vec<String>,
    pub start_date: String,
    pub end_date: String,
}

/// Parse Veolia Poznań date format "YYYY-MM-DD HH:MM:SS" into ISO "YYYY-MM-DDTHH:MM:00".
pub fn parse_veolia_poznan_date(s: &str) -> Option<String> {
    let cleaned = s.trim();
    if cleaned.len() >= 16 {
        let date_part = &cleaned[0..10];
        let time_part = &cleaned[11..16];
        Some(format!("{}T{}:00", date_part, time_part))
    } else {
        None
    }
}

pub async fn fetch_veolia_poznan_alerts_for_street(
    client: &Client,
    street: &str,
) -> Result<Vec<VeoliaPoznanItem>, String> {
    let url = format!(
        "https://energiadlapoznania.pl/wp-admin/admin-ajax.php?action=exclusions_search&search={}",
        urlencoding::encode(street)
    );
    let res = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Veolia Poznań API HTTP error: {}", res.status()));
    }

    let items: Vec<VeoliaPoznanItem> = res.json().await.map_err(|e| e.to_string())?;
    Ok(items)
}

impl VeoliaPoznanItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        let start_date = parse_veolia_poznan_date(&self.start_date);
        let end_date = parse_veolia_poznan_date(&self.end_date);

        let addresses_str = self.street.join(", ");
        let mut parts = Vec::new();
        parts.push("Przerwa w dostawie ciepła".to_string());
        if !self.title.is_empty() {
            let clean_title = self.title
                .replace("&#8211;", "-")
                .replace("&#8212;", "-")
                .replace("&nbsp;", " ")
                .replace("&amp;", "&");
            parts.push(clean_title);
        }
        if !addresses_str.is_empty() {
            parts.push(format!("posesje: {}", addresses_str));
        }
        let message = parts.join(" - ");

        UnifiedAlert {
            source: AlertSource::VeoliaPoznan,
            startDate: start_date,
            endDate: end_date,
            message: Some(message),
            description: Some("Miejscowość: Poznań".to_string()),
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct VeoliaPoznanProvider;

#[async_trait]
impl AlertProvider for VeoliaPoznanProvider {
    fn id(&self) -> String {
        "veolia_poznan".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::VeoliaPoznan
    }

    async fn fetch(
        &self,
        _client: &Client,
        client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<(usize, String)> = settings
            .addresses
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_active && crate::aquanet::is_poznan_area(a))
            .map(|(idx, a)| (idx, a.street_name_1.clone()))
            .collect();

        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        let mut alerts = Vec::new();

        for (idx, street_name) in &active_addresses {
            match retry(|| fetch_veolia_poznan_alerts_for_street(client_http1, street_name), 3).await {
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
                    let err_msg = format!("Veolia Poznań (ul. {}): {}", street_name, e);
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
    fn test_parse_veolia_poznan_date() {
        assert_eq!(
            parse_veolia_poznan_date("2026-03-03 07:40:00"),
            Some("2026-03-03T07:40:00".to_string())
        );
        assert_eq!(parse_veolia_poznan_date("invalid"), None);
    }
}
