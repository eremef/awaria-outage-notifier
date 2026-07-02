use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;

pub const SEC_URL: &str = "https://sec.com.pl/przerwy-w-dostawie-ciepla/";

pub struct SecProvider;

fn check_local_matching(alert: &mut UnifiedAlert, settings: &Settings, combined_text: &str) {
    let mut is_local = false;
    let mut address_index = None;
    
    let mut active_addresses = Vec::new();
    for (idx, addr) in settings.addresses.iter().enumerate() {
        if addr.is_active && crate::api_logic::is_address_applicable_for_provider(&crate::api_logic::AlertSource::Sec, addr) {
            let raw_street_name = if !addr.street_name_1.is_empty() {
                &addr.street_name_1
            } else {
                &addr.street_name
            };
            
            let street_name = crate::utils::strip_street_prefixes(raw_street_name);
            
            if !street_name.is_empty() {
                let pattern = format!(r"(?i)\b{}\b", regex::escape(street_name));
                if let Ok(re) = Regex::new(&pattern) {
                    active_addresses.push((idx, street_name.to_string(), re));
                }
            }
        }
    }
    
    if active_addresses.is_empty() {
        return;
    }
    
    for (idx, _street_name, compiled) in active_addresses {
        if compiled.is_match(combined_text) {
            is_local = true;
            address_index = Some(idx);
            break;
        }
    }
    
    alert.is_local = Some(is_local);
    alert.address_index = address_index;
}

// Helper to parse dates like "19.11.2025 godz. 07:00"
fn parse_date_with_godz(text: &str) -> Option<NaiveDateTime> {
    let cleaned = text.replace("godz.", "").replace("r.", "").replace("Godz.", "");
    let cleaned = cleaned.trim();

    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%d.%m.%Y %H:%M") {
        return Some(dt);
    }
    // Fallback for just date
    if let Ok(d) = NaiveDate::parse_from_str(cleaned, "%d.%m.%Y") {
        return Some(NaiveDateTime::new(d, NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    }
    None
}

// Extractor to locate start/end dates in text
fn extract_dates(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    // Look for date ranges separated by endash or hyphen
    // e.g. "19.11.2025 godz. 07:00 – 11.12.2025 godz. 01:14"
    let parts: Vec<&str> = text.split(['–', '-']).collect();
    if parts.len() >= 2 {
        let start = parse_date_with_godz(parts[0]);
        let end = parse_date_with_godz(parts[1]);
        return (start, end);
    } else if parts.len() == 1 {
        let start = parse_date_with_godz(parts[0]);
        return (start, None);
    }

    (None, None)
}

#[async_trait]
impl AlertProvider for SecProvider {
    fn id(&self) -> String {
        "sec".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Sec
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        let res = match client.get(SEC_URL).send().await {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("SEC reqwest error: {}", e));
                return (alerts, errors);
            }
        };
        
        let html_content = match res.text().await {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("SEC response body error: {}", e));
                return (alerts, errors);
            }
        };

        let document = Html::parse_document(&html_content);
        
        let malfunction_selector = Selector::parse("div.malfunction").unwrap();
        let h2_selector = Selector::parse("h2").unwrap();
        let row_selector = Selector::parse(".mf-row.mf-content").unwrap();
        let area_selector = Selector::parse(".area").unwrap();
        let date_selector = Selector::parse(".date").unwrap();
        
        for malfunction in document.select(&malfunction_selector) {
            let mut incident_type = "Planowana";
            if let Some(h2) = malfunction.select(&h2_selector).next() {
                let h2_text = h2.inner_html().trim().to_string();
                if h2_text.eq_ignore_ascii_case("Wyłączenia awaryjne") {
                    incident_type = "Awaria";
                }
            }

            for row in malfunction.select(&row_selector) {
                let area_text = row.select(&area_selector).next().map(|el| el.inner_html().trim().to_string()).unwrap_or_default();
                let date_text = row.select(&date_selector).next().map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string()).unwrap_or_default();
                
                let (start_dt, end_dt) = extract_dates(&date_text);
                
                let mut alert = UnifiedAlert {
                    source: AlertSource::Sec,
                    startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                    endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                    message: Some(format!("{} - {}", incident_type, area_text)),
                    location: Some("Miejscowość: Szczecin".to_string()),
                    address_index: None,
                    is_local: Some(false),
                    hash: None,
                };
                
                let combined_text = format!("Szczecin {} {}", area_text, incident_type).to_lowercase();
                check_local_matching(&mut alert, settings, &combined_text);

                let mut hasher = DefaultHasher::new();
                alert.source.hash(&mut hasher);
                if let Some(msg) = &alert.message {
                    msg.hash(&mut hasher);
                }
                if let Some(start) = &alert.startDate {
                    start.hash(&mut hasher);
                }
                alert.hash = Some(format!("{:x}", hasher.finish()));

                alerts.push(alert);
            }
        }

        (alerts, errors)
    }
}
