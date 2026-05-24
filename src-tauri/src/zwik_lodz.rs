use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Duration};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

fn parse_zwik_date(date_str: &str) -> Option<NaiveDateTime> {
    let cleaned = date_str.to_lowercase()
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
        .replace("grudnia", "12")
        .replace(" g.", "")
        .replace(" godz.", "")
        .replace("godz.", "");

    let words: Vec<&str> = cleaned.split_whitespace().collect();
    for i in 0..words.len() {
        if let Ok(day) = words[i].parse::<u32>() {
            if i + 2 < words.len() {
                if let Ok(month) = words[i+1].parse::<u32>() {
                    let time_parts: Vec<&str> = words[i+2].split('.').collect();
                    if time_parts.len() == 2 {
                        if let (Ok(hour), Ok(minute)) = (time_parts[0].parse::<u32>(), time_parts[1].parse::<u32>()) {
                            let year = Local::now().year();
                            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                                if let Some(time) = NaiveTime::from_hms_opt(hour, minute, 0) {
                                    return Some(NaiveDateTime::new(date, time));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub struct ZwikLodzProvider;

#[async_trait]
impl AlertProvider for ZwikLodzProvider {
    fn id(&self) -> String {
        "zwik_lodz".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::ZwikLodz
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !crate::api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }
        
        let has_lodz_addresses = settings.addresses.iter()
            .any(|a| a.is_active && crate::api_logic::is_lodz(a));

        if !has_lodz_addresses {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        let url = "https://zwik.lodz.pl/pl/artykuly/302/awarie";

        match retry(|| async {
            client.get(url)
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .text()
                .await
                .map_err(|e| e.to_string())
        }, 3).await {
            Ok(html_content) => {
                let document = Html::parse_document(&html_content);
                let selector = Selector::parse("p, h1, h2, h3, h4, h5, h6, ul, ol").unwrap();
                let li_selector = Selector::parse("li").unwrap();
                
                let mut start_date: Option<NaiveDateTime> = None;
                let mut current_section = 0; // 0=None, 1=Awarie, 2=Planowane

                for element in document.select(&selector) {
                    let tag_name = element.value().name();
                    let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
                    
                    if tag_name == "p" || tag_name.starts_with("h") {
                        let text_lower = text.to_lowercase();
                        if start_date.is_none() {
                            if let Some(dt) = parse_zwik_date(&text) {
                                start_date = Some(dt);
                            }
                        }
                        
                        if text_lower.contains("informacje o awariach") || text_lower.contains("awariach wodociągowych") {
                            current_section = 1;
                        } else if text_lower.contains("planowanych pracach") {
                            current_section = 2;
                        } else if !text_lower.is_empty() && text_lower != " " && !text_lower.contains("&nbsp;") && text_lower != " " {
                            if parse_zwik_date(&text).is_none() {
                                current_section = 0;
                            }
                        }
                    } else if tag_name == "ul" || tag_name == "ol" {
                        if current_section == 1 || current_section == 2 {
                            for li in element.select(&li_selector) {
                                let li_text = li.text().collect::<Vec<_>>().join(" ").trim().to_string();
                                let li_text = li_text.replace("&nbsp;", " ").replace('\u{a0}', " ");
                                if li_text.to_lowercase().contains("bez wyłączeń") || li_text.is_empty() {
                                    continue;
                                }
                                
                                let incident_type = if current_section == 1 { "Awaria" } else { "Prace planowane" };
                                let message = format!("{} - {}", incident_type, li_text);
                                
                                let mut alert = UnifiedAlert {
                                    source: AlertSource::ZwikLodz,
                                    startDate: start_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                    endDate: start_date.map(|d| (d + Duration::hours(24)).format("%Y-%m-%dT%H:%M:%S").to_string()),
                                    message: Some(message),
                                    description: Some(format!("Miejscowość: Łódź")),
                                    address_index: None,
                                    is_local: Some(false),
                                    hash: None,
                                };

                                let combined_text = format!("Łódź {}", li_text).to_lowercase();
                                
                                for (idx, a) in settings.addresses.iter().enumerate() {
                                    if !a.is_active || !crate::api_logic::is_lodz(a) {
                                        continue;
                                    }
                                    let mut is_match = false;
                                    if !a.street_name_1.is_empty() {
                                        let s1 = a.street_name_1.to_lowercase();
                                        if combined_text.contains(&s1) {
                                            is_match = true;
                                        }
                                    }
                                    if !a.street_name_2.as_deref().unwrap_or("").is_empty() {
                                        let s2 = a.street_name_2.as_ref().unwrap().to_lowercase();
                                        if combined_text.contains(&s2) {
                                            is_match = true;
                                        }
                                    }
                                    
                                    if is_match {
                                        alert.is_local = Some(true);
                                        alert.address_index = Some(idx);
                                        break;
                                    }
                                }

                                // Create a unique hash
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
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("ZWIK Łódź error: {}", e);
                log::error!("{}", err_msg);
                errors.push(err_msg);
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_zwik_date() {
        assert!(parse_zwik_date("24 maja g. 9.00").is_some());
        assert!(parse_zwik_date("24 05 9.00").is_some());
    }

    #[tokio::test]
    async fn test_zwik_lodz_parsing_logic() {
        use crate::api_logic::{AddressEntry, Settings};
        let html = r#"
            <p class="topic_new" style="text-align: center;"><strong>24 maja g. 9.00</strong></p>
            <p class="topic_new" style="text-align: left;"><span style="text-decoration: underline;"><strong>Informacje o awariach wodociągowych i kanalizacyjnych oraz ograniczeniach w dostawie wody:</strong></span></p>
            <ul>
                <li>Wilcza 11 - awaria przyłącza, dowóz wody cysterną. Naprawa w dniu dzisiejszym.</li>
            </ul>
            <p class="topic_new" style="text-align: left;"><span style="text-decoration: underline;"><strong>Informacje o planowanych pracach na sieci wodociągowej:</strong></span></p>
            <ul>
                <li>bez wyłączeń</li>
            </ul>
            <p><strong>Inne komunikaty:</strong></p>
            <ul>
                <li>Uwaga, zmiana regulaminu</li>
            </ul>
        "#;

        let document = Html::parse_document(&html);
        let selector = Selector::parse("p, h1, h2, h3, h4, h5, h6, ul, ol").unwrap();
        let li_selector = Selector::parse("li").unwrap();
        
        let mut start_date: Option<NaiveDateTime> = None;
        let mut current_section = 0; // 0=None, 1=Awarie, 2=Planowane

        let mut alerts = Vec::new();

        let mut settings = Settings::default();
        settings.addresses.push(AddressEntry {
            name: "Dom".to_string(),
            city_name: "Łódź".to_string(),
            voivodeship: String::new(),
            district: String::new(),
            commune: String::new(),
            street_name: "Wilcza".to_string(),
            street_name_1: "Wilcza".to_string(),
            street_name_2: None,
            house_no: "11".to_string(),
            city_id: Some(958128),
            street_id: None,
            is_active: true,
        });

        for element in document.select(&selector) {
            let tag_name = element.value().name();
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
            
            if tag_name == "p" || tag_name.starts_with("h") {
                let text_lower = text.to_lowercase();
                if start_date.is_none() {
                    if let Some(dt) = parse_zwik_date(&text) {
                        start_date = Some(dt);
                    }
                }
                
                if text_lower.contains("informacje o awariach") || text_lower.contains("awariach wodociągowych") {
                    current_section = 1;
                } else if text_lower.contains("planowanych pracach") {
                    current_section = 2;
                } else if !text_lower.is_empty() && text_lower != " " && !text_lower.contains("&nbsp;") && text_lower != " " {
                    if parse_zwik_date(&text).is_none() {
                        current_section = 0;
                    }
                }
            } else if tag_name == "ul" || tag_name == "ol" {
                if current_section == 1 || current_section == 2 {
                    for li in element.select(&li_selector) {
                        let li_text = li.text().collect::<Vec<_>>().join(" ").trim().to_string();
                        let li_text = li_text.replace("&nbsp;", " ").replace('\u{a0}', " ");
                        if li_text.to_lowercase().contains("bez wyłączeń") || li_text.is_empty() {
                            continue;
                        }
                        
                        let incident_type = if current_section == 1 { "Awaria" } else { "Prace planowane" };
                        let message = format!("{} - {}", incident_type, li_text);
                        
                        let mut alert = UnifiedAlert {
                            source: AlertSource::ZwikLodz,
                            startDate: start_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            endDate: start_date.map(|d| (d + Duration::hours(24)).format("%Y-%m-%dT%H:%M:%S").to_string()),
                            message: Some(message),
                            description: Some(format!("Miejscowość: Łódź")),
                            address_index: None,
                            is_local: Some(false),
                            hash: None,
                        };

                        let combined_text = format!("Łódź {}", li_text).to_lowercase();
                        
                        for (idx, a) in settings.addresses.iter().enumerate() {
                            if !a.is_active || !crate::api_logic::is_lodz(a) {
                                continue;
                            }
                            let mut is_match = false;
                            if !a.street_name_1.is_empty() {
                                let s1 = a.street_name_1.to_lowercase();
                                if combined_text.contains(&s1) {
                                    is_match = true;
                                }
                            }
                            if !a.street_name_2.as_deref().unwrap_or("").is_empty() {
                                let s2 = a.street_name_2.as_ref().unwrap().to_lowercase();
                                if combined_text.contains(&s2) {
                                    is_match = true;
                                }
                            }
                            
                            if is_match {
                                alert.is_local = Some(true);
                                alert.address_index = Some(idx);
                                break;
                            }
                        }

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
            }
        }

        assert_eq!(alerts.len(), 1); // Only 1 alert (the failure) should be parsed, the planned one is "bez wyłączeń", the other is skipped
        let alert = &alerts[0];
        assert_eq!(alert.message.as_deref().unwrap(), "Awaria - Wilcza 11 - awaria przyłącza, dowóz wody cysterną. Naprawa w dniu dzisiejszym.");
        assert_eq!(alert.is_local, Some(true));
    }
}
