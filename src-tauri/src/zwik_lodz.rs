use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, Duration};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;

fn parse_zwik_li_date(li_text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let re = Regex::new(r"w dniu (\d{1,2})\.(\d{1,2})\.?(?:(20\d{2})r?\.?)?.*?g(?:odz)?\.\s*(\d{1,2})(?::(\d{2}))?-(\d{1,2})(?::(\d{2}))?").unwrap();
    if let Some(caps) = re.captures(li_text) {
        if let (Ok(d), Ok(m), Ok(h_start), Ok(h_end)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>(), caps[4].parse::<u32>(), caps[6].parse::<u32>()) {
            let year = caps.get(3).map_or(Local::now().year(), |m| m.as_str().parse::<i32>().unwrap_or(Local::now().year()));
            let m_start = caps.get(5).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));
            let m_end = caps.get(7).map_or(0, |m| m.as_str().parse::<u32>().unwrap_or(0));

            if let Some(date) = NaiveDate::from_ymd_opt(year, m, d) {
                if let (Some(t_start), Some(t_end)) = (NaiveTime::from_hms_opt(h_start, m_start, 0), NaiveTime::from_hms_opt(h_end, m_end, 0)) {
                    return (Some(NaiveDateTime::new(date, t_start)), Some(NaiveDateTime::new(date, t_end)));
                }
            }
        }
    }
    (None, None)
}

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

fn check_street(street: &str, combined_text: &str) -> bool {
    if street.is_empty() { return false; }
    let s_lower = street.to_lowercase()
        .replace("ul. ", "")
        .replace("ul.", "")
        .replace("al. ", "")
        .replace("al.", "")
        .replace("pl. ", "")
        .replace("pl.", "")
        .replace("\"", "");
    
    let words: Vec<&str> = s_lower.split_whitespace().collect();
    let significant_words: Vec<&str> = words.into_iter()
        .filter(|w| w.chars().count() >= 3 && !w.chars().all(|c| c.is_numeric()))
        .collect();

    if significant_words.is_empty() {
        return combined_text.contains(&s_lower);
    }

    for w in significant_words {
        let chars: Vec<char> = w.chars().collect();
        let len = chars.len();
        let mut stem = w.to_string();

        if w.ends_with("ego") && len > 3 {
            stem = chars[..len - 3].iter().collect();
        } else if (w.ends_with("ej") || w.ends_with("ych") || w.ends_with("ich")) && len > 2 {
            stem = chars[..len - 2].iter().collect();
        } else if (w.ends_with("a") || w.ends_with("y") || w.ends_with("i") || w.ends_with("e") || w.ends_with("ą") || w.ends_with("ę")) && len > 3 {
            stem = chars[..len - 1].iter().collect();
        }

        if !combined_text.contains(&stem) {
            return false;
        }
    }
    true
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
                    let text = element.text().collect::<Vec<_>>().concat().trim().to_string();
                    
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
                        } else if !text_lower.is_empty() && text_lower != " " && !text_lower.contains("&nbsp;") && text_lower != " " && parse_zwik_date(&text).is_none() {
                            current_section = 0;
                        }
                    } else if (tag_name == "ul" || tag_name == "ol") && (current_section == 1 || current_section == 2) {
                        for li in element.select(&li_selector) {
                            let li_text = li.text().collect::<Vec<_>>().concat().trim().to_string();
                            let li_text = li_text.replace("&nbsp;", " ").replace('\u{a0}', " ");
                            if li_text.to_lowercase().contains("bez wyłączeń") || li_text.to_lowercase().contains("bez awarii") || li_text.is_empty() {
                                continue;
                            }
                                
                            let incident_type = if current_section == 1 { "Awaria" } else { "Prace planowane" };
                            let message = format!("{} - {}", incident_type, li_text);
                            
                            let (mut li_start, mut li_end) = parse_zwik_li_date(&li_text);
                            if li_start.is_none() {
                                li_start = start_date;
                                li_end = start_date.map(|d| d + Duration::hours(24));
                            }
                                
                            let mut alert = UnifiedAlert {
                                source: AlertSource::ZwikLodz,
                                startDate: li_start.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                endDate: li_end.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                message: Some(message),
                                location: Some("Miejscowość: Łódź".to_string()),
                                address_index: None,
                                is_local: Some(false),
                                hash: None,
                            };

                            let combined_text = format!("Łódź {}", li_text).to_lowercase();
                                
                            for (idx, a) in settings.addresses.iter().enumerate() {
                                let mut is_match = false;
                                if !a.is_active || !crate::api_logic::is_lodz(a) {
                                    continue;
                                }
                                if check_street(&a.street_name_1, &combined_text) {
                                    is_match = true;
                                }
                                if let Some(s2) = &a.street_name_2 {
                                    if check_street(s2, &combined_text) {
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
        
        let (start, end) = parse_zwik_li_date("Milionowa /wyłącz. od Przędzalnianej do pos. Milionowa 25/27 /- w dniu 26.05. g. 8-13 wyłączenie wody");
        assert!(start.is_some());
        assert_eq!(start.unwrap().format("%m-%d %H:%M").to_string(), "05-26 08:00");
        assert_eq!(end.unwrap().format("%m-%d %H:%M").to_string(), "05-26 13:00");

        let (start, end) = parse_zwik_li_date("w dniu 26.05.2026 r. g. 8-13");
        assert!(start.is_some());
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-05-26 08:00");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-05-26 13:00");

        let (start, end) = parse_zwik_li_date("w dniu 26.05. g. 08:30-13:45");
        assert!(start.is_some());
        assert_eq!(start.unwrap().format("%m-%d %H:%M").to_string(), "05-26 08:30");
        assert_eq!(end.unwrap().format("%m-%d %H:%M").to_string(), "05-26 13:45");
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
            let text = element.text().collect::<Vec<_>>().concat().trim().to_string();
            
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
                        let li_text = li.text().collect::<Vec<_>>().concat().trim().to_string();
                        let li_text = li_text.replace("&nbsp;", " ").replace('\u{a0}', " ");
                        if li_text.to_lowercase().contains("bez wyłączeń") || li_text.to_lowercase().contains("bez awarii") || li_text.is_empty() {
                            continue;
                        }
                        
                        let incident_type = if current_section == 1 { "Awaria" } else { "Prace planowane" };
                        let message = format!("{} - {}", incident_type, li_text);
                        
                        let mut alert = UnifiedAlert {
                            source: AlertSource::ZwikLodz,
                            startDate: start_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            endDate: start_date.map(|d| (d + Duration::hours(24)).format("%Y-%m-%dT%H:%M:%S").to_string()),
                            message: Some(message),
                            location: Some(format!("Miejscowość: Łódź")),
                            address_index: None,
                            is_local: Some(false),
                            hash: None,
                        };

                        let combined_text = format!("Łódź {}", li_text).to_lowercase();
                        
                        for (idx, a) in settings.addresses.iter().enumerate() {
                            let mut is_match = false;
                            if !a.is_active || !crate::api_logic::is_lodz(a) {
                                continue;
                            }
                            if check_street(&a.street_name_1, &combined_text) {
                                is_match = true;
                            }
                            if let Some(s2) = &a.street_name_2 {
                                if check_street(s2, &combined_text) {
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

    #[tokio::test]
    async fn test_zwik_lodz_parsing_logic_broken_html() {
        use crate::api_logic::{AddressEntry, Settings};
        let html = r#"
            <p class="topic_new" style="text-align: center;"><strong>24 maja g. 9.00</strong></p>
            <p><em><strong><span style="text-decoration: underline;">Informacj</span></strong></em><strong><span style="text-decoration: underline;">e o planowa</span></strong><em><strong><span style="text-decoration: underline;">nych pracach na sieci wodociągowej:</span></strong></em></p>
            <ul>
                <li>Milionowa /wyłącz. od Przędzalnianej do pos. Milionowa 25/27 /- w dniu 26.05. g. 8-13 wyłączenie wody.</li>
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
            voivodeship: "ŁÓDZKIE".to_string(),
            district: String::new(),
            commune: String::new(),
            street_name: "Milionowa".to_string(),
            street_name_1: "Milionowa".to_string(),
            street_name_2: None,
            house_no: "25".to_string(),
            city_id: Some(958128),
            street_id: None,
            is_active: true,
        });

        for element in document.select(&selector) {
            let tag_name = element.value().name();
            let text = element.text().collect::<Vec<_>>().concat().trim().to_string();
            
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
                        let li_text = li.text().collect::<Vec<_>>().concat().trim().to_string();
                        let li_text = li_text.replace("&nbsp;", " ").replace('\u{a0}', " ");
                        if li_text.to_lowercase().contains("bez wyłączeń") || li_text.to_lowercase().contains("bez awarii") || li_text.is_empty() {
                            continue;
                        }
                        
                        let incident_type = if current_section == 1 { "Awaria" } else { "Prace planowane" };
                        let message = format!("{} - {}", incident_type, li_text);
                        
                        let (mut li_start, mut li_end) = parse_zwik_li_date(&li_text);
                        if li_start.is_none() {
                            li_start = start_date;
                            li_end = start_date.map(|d| d + Duration::hours(24));
                        }

                        let mut alert = UnifiedAlert {
                            source: AlertSource::ZwikLodz,
                            startDate: li_start.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            endDate: li_end.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            message: Some(message),
                            location: Some(format!("Miejscowość: Łódź")),
                            address_index: None,
                            is_local: Some(false),
                            hash: None,
                        };

                        let combined_text = format!("Łódź {}", li_text).to_lowercase();
                        
                        for (idx, a) in settings.addresses.iter().enumerate() {
                            let mut is_match = false;
                            if !a.is_active || !crate::api_logic::is_lodz(a) {
                                continue;
                            }
                            if check_street(&a.street_name_1, &combined_text) {
                                is_match = true;
                            }
                            
                            if is_match {
                                alert.is_local = Some(true);
                                alert.address_index = Some(idx);
                                break;
                            }
                        }

                        alerts.push(alert);
                    }
                }
            }
        }

        assert_eq!(alerts.len(), 1, "Should have parsed one alert despite broken HTML header text");
    }

    #[test]
    fn test_check_street_declensions() {
        let text1 = "jędrowizny 26 - 26.02. g. 8-10 wyłączenie wodociągu.";
        assert!(check_street("ul. Jędrowizna", text1));
        assert!(check_street("Jędrowizna", text1));
        
        let text2 = "milionowa (przędzalniana - do pos. 25/27) - 26.02. g. 8-13 wyłączenie wodociągu.";
        assert!(check_street("Milionowa", text2));
    }
}

