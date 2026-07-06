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
    let mut is_local = false;
    let mut address_index = None;
    
    let mut active_addresses = Vec::new();
    for (idx, addr) in settings.addresses.iter().enumerate() {
        if addr.is_active && crate::api_logic::is_address_applicable_for_provider(&crate::api_logic::AlertSource::Lpec, addr) {
            let raw_street_name = if !addr.street_name_1.is_empty() {
                &addr.street_name_1
            } else {
                &addr.street_name
            };
            
            let street_name = crate::utils::strip_street_prefixes(raw_street_name);
            
            if !street_name.is_empty() {
                let mut pattern_options = vec![format!(r"\b{}\b", regex::escape(street_name))];
                
                let parts: Vec<&str> = street_name.split_whitespace().collect();
                if parts.len() > 1 {
                    let first_char = parts[0].chars().next().unwrap();
                    let rest = parts[1..].join(" ");
                    pattern_options.push(format!(r"\b{}\.\s*{}\b", regex::escape(&first_char.to_string()), regex::escape(&rest).replace(r"\ ", r"\s+")));
                }
                
                let pattern = format!("(?i)(?:{})", pattern_options.join("|"));
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
fn format_time(time: &str) -> String {
    let t = time.replace(".", ":");
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() == 2 {
        let h = format!("{:02}", parts[0].parse::<u32>().unwrap_or(0));
        let m = format!("{:02}", parts[1].parse::<u32>().unwrap_or(0));
        format!("{}:{}", h, m)
    } else {
        t
    }
}

fn extract_dates(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let lower_text = text.to_lowercase();
    
    // First, try the standard "w dniu DD.MM.YYYY od godz. HH:MM do godzin popołudniowych/wieczornych"
    let re_descriptive = Regex::new(
        r"(?:dniu\s+)?(\d{1,2}\.\d{2}\.\d{4})\s*od\s*(?:godz\.\s*)?(\d{1,2}[:.]\d{2})\s*do\s*godzin\s*(popołudniowych|wieczornych)"
    ).unwrap();
    
    if let Some(caps) = re_descriptive.captures(&lower_text) {
        let date_str = &caps[1];
        let start_time = format_time(&caps[2]);
        let end_time = if &caps[3] == "popołudniowych" { "17:00" } else { "22:00" };
        
        let start = parse_date_with_godz(&format!("{} {}", date_str, start_time));
        let end = parse_date_with_godz(&format!("{} {}", date_str, end_time));
        
        return (start, end);
    }
    
    // Check for "w dniu DD.MM.YYYY od godz. HH:MM do HH:MM"
    let re_time_range = Regex::new(
        r"(?:dniu\s+)?(\d{1,2}\.\d{2}\.\d{4})\s*od\s*(?:godz\.\s*)?(\d{1,2}[:.]\d{2})\s*do\s*(?:godz\.\s*)?(\d{1,2}[:.]\d{2})"
    ).unwrap();
    
    if let Some(caps) = re_time_range.captures(&lower_text) {
        let date_str = &caps[1];
        let start_time = format_time(&caps[2]);
        let end_time = format_time(&caps[3]);
        
        let start = parse_date_with_godz(&format!("{} {}", date_str, start_time));
        let end = parse_date_with_godz(&format!("{} {}", date_str, end_time));
        
        return (start, end);
    }

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

            // Filter out outdated outages
            let now = chrono::Utc::now().naive_utc();
            if let Some(end) = end_dt {
                if end < now {
                    continue;
                }
            } else if let Some(start) = start_dt {
                if start + chrono::Duration::hours(24) < now {
                    continue;
                }
            } else if let Some(d) = &post.date {
                if let Some(dt) = parse_date_with_godz(d) {
                    if dt + chrono::Duration::hours(24) < now {
                        continue;
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_dates_with_dniu() {
        let text = "w dniu 07.07.2026 od godz. 6.30 do godzin popołudniowych nastąpi przerwa";
        let (start, end) = extract_dates(text);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-07-07 06:30");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-07-07 17:00");
    }

    #[test]
    fn test_extract_dates_without_dniu() {
        let text = "Informujemy, że w związku z pracami na sieci ciepłowniczej 07.07.2026 od godz. 6.30 do godzin popołudniowych nastąpi przerwa w dostawie ciepłej wody do budynku przy ul. Urbanowicza 24 . Przepraszamy za utrudnienia.";
        let (start, end) = extract_dates(text);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-07-07 06:30");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-07-07 17:00");
    }

    #[test]
    fn test_local_matching_abbreviation() {
        let text = "w dostawie ciepłej wody do budynków przy ul .: Z. Augusta 41,43, Królowej Jadwigi 14,16,27 . Przepraszamy za utrudnienia.";
        let mut alert = crate::api_logic::UnifiedAlert {
            source: crate::api_logic::AlertSource::Lpec,
            startDate: None,
            endDate: None,
            message: None,
            location: None,
            address_index: None,
            is_local: None,
            hash: None,
        };
        let settings = crate::api_logic::Settings {
            addresses: vec![crate::api_logic::AddressEntry {
                name: "1".to_string(),
                city_name: "Lublin".to_string(),
                voivodeship: "".to_string(),
                district: "".to_string(),
                commune: "".to_string(),
                street_name: "ul. Zygmunta Augusta".to_string(),
                street_name_1: "".to_string(),
                street_name_2: None,
                house_no: "41".to_string(),
                city_id: None,
                street_id: None,
                is_active: true,
            }],
            ..Default::default()
        };
        check_local_matching(&mut alert, &settings, &text.to_lowercase());
        assert_eq!(alert.is_local, Some(true));
    }
}
