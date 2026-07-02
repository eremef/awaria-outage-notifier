use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use chrono::NaiveDateTime;
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;
use serde_json::Value;

pub struct MpwikLublinProvider;

// Helper to parse dates like "24-04-2024 10:00"
fn parse_date(text: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(text, "%d-%m-%Y %H:%M") {
        return Some(dt);
    }
    None
}

// Extractor to locate start/end dates in text
fn extract_dates(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let lower_text = text.to_lowercase()
        .replace("stycznia", "01")
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
        .replace("grudnia", "12");

    let re_date = Regex::new(r"dniu\s+(\d{1,2})[\s\.]+(\d{1,2})[\s\.]+(\d{4})").unwrap();
    let re_start_time = Regex::new(r"od\s+(?:godz\.\s*|godziny\s+)?(\d{1,2}:\d{2})").unwrap();
    let re_end_time = Regex::new(r"około\s+(?:godz\.\s*|godziny\s+)?(\d{1,2}:\d{2})").unwrap();

    let mut start_date = None;
    let mut end_date = None;

    if let Some(date_caps) = re_date.captures(&lower_text) {
        let d = format!("{:02}", date_caps[1].parse::<u32>().unwrap_or(1));
        let m = format!("{:02}", date_caps[2].parse::<u32>().unwrap_or(1));
        let y = &date_caps[3];
        let date_str = format!("{}-{}-{}", d, m, y);

        let mut start_time = "00:00";
        if let Some(start_caps) = re_start_time.captures(&lower_text) {
            start_time = start_caps.get(1).map(|m| m.as_str()).unwrap_or("00:00");
        }

        if let Some(dt) = parse_date(&format!("{} {}", date_str, start_time)) {
            start_date = Some(dt);
            
            if let Some(end_caps) = re_end_time.captures(&lower_text) {
                let end_time = end_caps.get(1).map(|m| m.as_str()).unwrap_or("23:59");
                if let Some(e_dt) = parse_date(&format!("{} {}", date_str, end_time)) {
                    end_date = Some(e_dt);
                }
            }
        }
    }
    
    (start_date, end_date)
}

fn check_local_matching(alert: &mut UnifiedAlert, settings: &Settings, combined_text: &str) {
    let mut is_local = false;
    let mut address_index = None;
    
    let mut active_addresses = Vec::new();
    for (idx, addr) in settings.addresses.iter().enumerate() {
        if addr.is_active && crate::api_logic::is_address_applicable_for_provider(&crate::api_logic::AlertSource::MpwikLublin, addr) {
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

#[async_trait]
impl AlertProvider for MpwikLublinProvider {
    fn id(&self) -> String {
        "mpwik_lublin".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MpwikLublin
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

        let url = "https://www.mpwik.lublin.pl/strefa-klienta/awarie-i-wylaczenia/";

        let res = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("MPWiK Lublin reqwest error: {}", e));
                return (alerts, errors);
            }
        };

        if !res.status().is_success() {
            errors.push(format!("Failed to fetch MPWiK Lublin data: {}", res.status()));
            return (alerts, errors);
        }

        let html = match res.text().await {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("MPWiK Lublin text read error: {}", e));
                return (alerts, errors);
            }
        };

        let document = Html::parse_document(&html);
        let list_selector = match Selector::parse("div.lista-wylaczen > div.listing-item") {
            Ok(s) => s,
            Err(_) => return (alerts, vec!["Failed to parse list selector".to_string()]),
        };
        let p_selector = match Selector::parse("div.content > p") {
            Ok(s) => s,
            Err(_) => return (alerts, vec!["Failed to parse p selector".to_string()]),
        };
        let map_selector = match Selector::parse("div.wp_mapit_multipin_map") {
            Ok(s) => s,
            Err(_) => return (alerts, vec!["Failed to parse map selector".to_string()]),
        };

        let now = chrono::Utc::now().naive_utc();

        for item in document.select(&list_selector) {
            let mut description = String::new();
            for p in item.select(&p_selector) {
                let p_text = p.text().collect::<Vec<_>>().join(" ");
                let clean_text = p_text.trim();
                if !clean_text.is_empty() {
                    if !description.is_empty() {
                        description.push('\n');
                    }
                    description.push_str(clean_text);
                }
            }
            description = description.trim().to_string();

            // Extract streets from map pins
            let mut streets = Vec::new();
            let mut incident_type = "Awaria wody".to_string();
            
            if let Some(map_el) = item.select(&map_selector).next() {
                if let Some(data_pins) = map_el.value().attr("data-pins") {
                    if let Ok(pins_val) = serde_json::from_str::<Value>(data_pins) {
                        if let Some(pins_arr) = pins_val.as_array() {
                            for pin in pins_arr {
                                if let Some(street_name) = pin.get("marker_title").and_then(|v| v.as_str()) {
                                    streets.push(street_name.to_string());
                                }
                                if let Some(content) = pin.get("marker_content").and_then(|v| v.as_str()) {
                                    if !content.trim().is_empty() {
                                        incident_type = content.trim().to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let (start_dt, end_dt) = extract_dates(&description);
            let final_description = description.split_whitespace().collect::<Vec<_>>().join(" ");
            let start_date_str = start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
                .unwrap_or_else(|| now.format("%Y-%m-%dT%H:%M:%S").to_string());
            let end_date_str = end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string());

            let mut message = incident_type.clone();
            if !final_description.is_empty() {
                message.push_str(" - ");
                message.push_str(&final_description);
            }

            let mut alert = UnifiedAlert {
                source: AlertSource::MpwikLublin,
                startDate: Some(start_date_str),
                endDate: end_date_str,
                message: Some(message),
                location: Some("Miejscowość: Lublin".to_string()),
                address_index: None,
                is_local: Some(false),
                hash: None,
            };

            let combined_text = format!("Lublin {} {}", final_description, streets.join(" ")).to_lowercase();
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

        (alerts, errors)
    }
}
