use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use scraper::Html;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;
use serde::Deserialize;

pub const LPEC_URL: &str = "https://lpec.pl/wp-json/wp/v2/posts?categories=10&per_page=15";

#[derive(Deserialize)]
struct RenderedField {
    rendered: String,
}

#[derive(Deserialize)]
struct WpPost {
    date: Option<String>,
    title: RenderedField,
    content: RenderedField,
}

pub struct LpecProvider;

fn check_local_matching(alert: &mut UnifiedAlert, settings: &Settings, combined_text: &str) {
    if !settings.filter_by_house_no {
        return;
    }
    
    let mut is_local = false;
    let mut address_index = None;
    
    let mut active_addresses = Vec::new();
    for (idx, addr) in settings.addresses.iter().enumerate() {
        if addr.is_active && crate::api_logic::is_address_applicable_for_provider(&crate::api_logic::AlertSource::Lpec, addr) {
            let street_name = if !addr.street_name_1.is_empty() {
                &addr.street_name_1
            } else {
                &addr.street_name
            };
            
            if !street_name.is_empty() {
                let pattern = format!(r"(?i)\b{}\b", regex::escape(street_name));
                if let Ok(re) = Regex::new(&pattern) {
                    active_addresses.push((idx, street_name.clone(), re));
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

// Helper to parse dates like "26.06.2026 godz. 2.00"
fn parse_date_with_godz(text: &str) -> Option<NaiveDateTime> {
    let cleaned = text.replace("godz.", "").replace("r.", "").replace("Godz.", "");
    let cleaned = cleaned.trim();

    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%d.%m.%Y %H:%M") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%d.%m.%Y %H.%M") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%dT%H:%M:%S") { // ISO 8601 fallback
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
    // Look for date ranges separated by od/do or -
    let date_range_re = Regex::new(
        r"(?i)(?:od\s+)?(\d{1,2}\.\d{2}\.\d{4}(?:\s+godz\.\s*\d{1,2}[:.]\d{2})?|\d{4}-\d{2}-\d{2}(?:\s+godz\.\s*\d{1,2}[:.]\d{2})?)\s+(?:do\s+|-)?(\d{1,2}\.\d{2}\.\d{4}(?:\s+godz\.\s*\d{1,2}[:.]\d{2})?|\d{4}-\d{2}-\d{2}(?:\s+godz\.\s*\d{1,2}[:.]\d{2})?)"
    ).unwrap();

    if let Some(caps) = date_range_re.captures(text) {
        let start = parse_date_with_godz(&caps[1]);
        let end = parse_date_with_godz(&caps[2]);
        return (start, end);
    }

    // Try single date
    let single_date_re = Regex::new(r"\d{1,2}\.\d{2}\.\d{4}(?:\s+godz\.\s*\d{1,2}[:.]\d{2})?|\d{4}-\d{2}-\d{2}(?:\s+godz\.\s*\d{1,2}[:.]\d{2})?").unwrap();
    let matches: Vec<_> = single_date_re.find_iter(text).collect();
    if matches.len() >= 2 {
        let start = parse_date_with_godz(matches[0].as_str());
        let end = parse_date_with_godz(matches[1].as_str());
        return (start, end);
    } else if matches.len() == 1 {
        let start = parse_date_with_godz(matches[0].as_str());
        return (start, None);
    }

    (None, None)
}

#[async_trait]
impl AlertProvider for LpecProvider {
    fn id(&self) -> String {
        "lpec".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Lpec
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

        let res = match client.get(LPEC_URL).send().await {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("LPEC reqwest error: {}", e));
                return (alerts, errors);
            }
        };
        
        let posts: Vec<WpPost> = match res.json().await {
            Ok(p) => p,
            Err(e) => {
                errors.push(format!("LPEC json parse error: {}", e));
                return (alerts, errors);
            }
        };

        for post in posts {
            let title = post.title.rendered.trim();
            if title.to_lowercase().contains("[odwołana]") {
                continue;
            }

            let mut incident_type = "Planowana";
            if title.to_lowercase().contains("awari") {
                incident_type = "Awaria";
            }

            // Strip HTML tags
            let fragment = Html::parse_fragment(&post.content.rendered);
            let raw_text = fragment.root_element().text().collect::<Vec<_>>().join(" ");
            let clean_text = raw_text.split_whitespace().collect::<Vec<_>>().join(" ");

            let (mut start_dt, end_dt) = extract_dates(&clean_text);
            
            if start_dt.is_none() {
                if let Some(d) = &post.date {
                    start_dt = parse_date_with_godz(d);
                }
            }

            let message = format!("{} - {}", incident_type, clean_text);
            
            let mut alert = UnifiedAlert {
                source: AlertSource::Lpec,
                startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                message: Some(message.clone()),
                location: Some("Miejscowość: Lublin".to_string()),
                address_index: None,
                is_local: Some(false),
                hash: None,
            };
            
            let combined_text = format!("Lublin {}", clean_text).to_lowercase();
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
