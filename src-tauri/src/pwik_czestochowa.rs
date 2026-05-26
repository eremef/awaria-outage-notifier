use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Duration};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;

fn extract_date_and_times(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let date_re = Regex::new(r"(\d{2})\.(\d{2})\.(\d{4})").unwrap();
    let time_re = Regex::new(r"(\d{1,2}):(\d{2})").unwrap();

    let mut base_date = Local::now().naive_local().date();

    if let Some(caps) = date_re.captures(text) {
        if let (Ok(d), Ok(m), Ok(y)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>(), caps[3].parse::<i32>()) {
            if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                base_date = date;
            }
        }
    }

    let mut times = Vec::new();
    for caps in time_re.captures_iter(text) {
        if let (Ok(h), Ok(m)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
            if let Some(time) = NaiveTime::from_hms_opt(h, m, 0) {
                times.push(time);
            }
        }
    }

    let start_time = times.first().copied().unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let end_time = times.get(1).copied().unwrap_or_else(|| NaiveTime::from_hms_opt(23, 59, 59).unwrap());

    let start_dt = NaiveDateTime::new(base_date, start_time);
    let mut end_dt = NaiveDateTime::new(base_date, end_time);

    if end_dt <= start_dt {
        end_dt += Duration::days(1);
    }

    (Some(start_dt), Some(end_dt))
}

pub struct PwikCzestochowaProvider;

#[async_trait]
impl AlertProvider for PwikCzestochowaProvider {
    fn id(&self) -> String {
        "pwik_czestochowa".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikCzestochowa
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

        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        let url = "https://www.pwik.czest.pl/awarie-i-planowane-wylaczenia";

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
                let middle_selector = Selector::parse("div.middle").unwrap();
                let h3_selector = Selector::parse("h3").unwrap();
                let p_selector = Selector::parse("div.content_text p").unwrap();
                let city_extraction_re = Regex::new(r"(?:w miejscowości|miejscowości|dla mieszkańców)\s+([A-ZŁŚĆŻŹ][a-ząćęłńóśźż]+(?:-[A-ZŁŚĆŻŹ][a-ząćęłńóśźż]+)?)").unwrap();

                for middle_element in document.select(&middle_selector) {
                    if let Some(h3_element) = middle_element.select(&h3_selector).next() {
                        let title = h3_element.text().collect::<Vec<_>>().join(" ").trim().to_string();
                        
                        if let Some(p_element) = middle_element.select(&p_selector).next() {
                            let content = p_element.text().collect::<Vec<_>>().join(" ").trim().to_string();
                            
                            if title.is_empty() && content.is_empty() {
                                continue;
                            }

                            // Try to determine incident type
                            let title_lower = title.to_lowercase();
                            let incident_type = if title_lower.contains("planowan") || content.to_lowercase().contains("rozbudową wodociągu") {
                                "Prace planowane"
                            } else {
                                "Awaria"
                            };

                            // Try to extract city from title or content
                            let combined_text_for_city = format!("{} {}", title, content).to_lowercase();
                            let mut city = "Częstochowa".to_string(); // default
                            
                            // Gminy i miejscowości obsługiwane przez PWiK Częstochowa z ich rdzeniami dla dopasowania w tekście
                            let municipalities = [
                                ("Blachownia", "blachowni"),
                                ("Kłobuck", "kłobuc"),
                                ("Konopiska", "konopisk"),
                                ("Miedźno", "miedźn"),
                                ("Mykanów", "mykan"),
                                ("Olsztyn", "olsztyn"),
                                ("Poczesna", "poczesn"),
                                ("Rędziny", "rędzin"),
                                ("Łobodno", "łobodn"),
                                ("Kamyk", "kamyk"),
                                ("Mstów", "mstow"),
                                ("Nowa Wieś", "nowa wieś"),
                                ("Nowa Wieś", "nowej wsi")
                            ];
                            
                            // 1. Try to match from the predefined list
                            for (m_name, stem) in municipalities {
                                if combined_text_for_city.contains(stem) {
                                    city = m_name.to_string();
                                    break;
                                }
                            }
                            
                            // 2. Try to extract from common phrases like "miejscowości Łobodno" or "mieszkańców Łobodno"
                            if city == "Częstochowa" {
                                if let Some(caps) = city_extraction_re.captures(&content) {
                                    let extracted = caps.get(1).unwrap().as_str().to_string();
                                    // Make sure we don't accidentally match "Częstochowa" or street names
                                    if !extracted.to_lowercase().starts_with("ul") && extracted != "Częstochowie" {
                                        city = extracted;
                                    }
                                }
                            }

                            let (start_dt, end_dt) = extract_date_and_times(&content);

                            let message = format!("{} - {}", incident_type, content);

                            let mut alert = UnifiedAlert {
                                source: AlertSource::PwikCzestochowa,
                                startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                message: Some(message),
                                location: Some(format!("Miejscowość: {}", city)),
                                address_index: None,
                                is_local: Some(false),
                                hash: None,
                            };

                            let combined_text = format!("{} {} {}", city, title, content).to_lowercase();

                            for (idx, a) in settings.addresses.iter().enumerate() {
                                if !a.is_active {
                                    continue;
                                }
                                
                                let mut is_match = false;
                                let a_city = a.city_name.to_lowercase();
                                if a_city == city.to_lowercase() || a_city.is_empty() {
                                    let check_street = |street: &str| -> bool {
                                        if street.is_empty() { return false; }
                                        let s_lower = street.to_lowercase();
                                        let cleaned = s_lower
                                            .replace("ul.", "")
                                            .replace("ulica", "")
                                            .replace("al.", "")
                                            .replace("aleja", "")
                                            .replace("pl.", "")
                                            .replace("plac", "");
                                        let words: Vec<&str> = cleaned.split_whitespace().collect();
                                        let significant_words: Vec<&str> = words.into_iter()
                                            .filter(|w| w.chars().count() > 3 && !w.chars().all(|c| c.is_numeric()))
                                            .collect();
                                        if significant_words.is_empty() {
                                            return combined_text.contains(&s_lower);
                                        }
                                        for w in significant_words {
                                            let stem = if w.ends_with('a') && w.chars().count() > 4 {
                                                let mut chars = w.chars();
                                                chars.next_back();
                                                chars.as_str()
                                            } else {
                                                w
                                            };
                                            if !combined_text.contains(stem) {
                                                return false;
                                            }
                                        }
                                        true
                                    };

                                    if check_street(&a.street_name_1) {
                                        is_match = true;
                                    }
                                    if let Some(s2) = &a.street_name_2 {
                                        if check_street(s2) {
                                            is_match = true;
                                        }
                                    }
                                    // If no street is configured but the city matches, mark it as local.
                                    if a.street_name_1.is_empty() && a.street_name_2.as_deref().unwrap_or("").is_empty() && a_city == city.to_lowercase() {
                                        is_match = true;
                                    }
                                }

                                if is_match {
                                    alert.is_local = Some(true);
                                    alert.address_index = Some(idx);
                                    break;
                                }
                            }

                            // Hash
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
                let err_msg = format!("PWiK Częstochowa error: {}", e);
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
    fn test_extract_date_and_times() {
        let text = "W związku z awarią wodociągu w dniu 25.05.2026 roku wystąpi przerwa w dostawie wody dla mieszkańców Mykanów ul. Kościuszki. Przewidywany czas usuwania awarii w godzinach 6:00 - 14:00. Za powstałe utrudnienia przepraszamy.";
        let (start, end) = extract_date_and_times(text);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-05-25 06:00");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-05-25 14:00");
    }

    #[tokio::test]
    async fn test_pwik_czestochowa_parsing() {
        use crate::api_logic::{AddressEntry, Settings};
        let html = r#"
            <div class="middle">
                <h3>Brak wody Mykanów ul. Kościuszki</h3>
                <div class="content_text">
                    <p>W związku z awarią wodociągu w dniu 25.05.2026 roku wystąpi przerwa w dostawie wody dla mieszkańców Mykanów ul. Kościuszki. Przewidywany czas usuwania awarii w godzinach 6:00 - 14:00. Za powstałe utrudnienia przepraszamy.</p>
                </div>
            </div>
            <div class="middle">
                <h3>Brak wody w Częstochowie w dniu 26.05.2026</h3>
                <div class="content_text">
                    <p>W związku z rozbudową wodociągu w dniu 26.05.2026 roku w godzinach 08:00 - 16:00 wystąpi przerwa w dostawie wody dla mieszkańców ul. Krakowskiej oraz ul. Katedralnej. Za powstałe utrudnienia przepraszamy.</p>
                </div>
            </div>
            <div class="middle">
                <div class="box_icon"></div>
                <div class="box_title">Inne</div>
            </div>
        "#;

        let document = Html::parse_document(&html);
        let middle_selector = Selector::parse("div.middle").unwrap();
        let h3_selector = Selector::parse("h3").unwrap();
        let p_selector = Selector::parse("div.content_text p").unwrap();

        let mut settings = Settings::default();
        settings.addresses.push(AddressEntry {
            name: "Dom".to_string(),
            city_name: "Częstochowa".to_string(),
            voivodeship: String::new(),
            district: String::new(),
            commune: String::new(),
            street_name: "Krakowska".to_string(),
            street_name_1: "Krakowska".to_string(),
            street_name_2: None,
            house_no: "11".to_string(),
            city_id: None,
            street_id: None,
            is_active: true,
        });

        let mut alerts = Vec::new();
        
        for middle_element in document.select(&middle_selector) {
            if let Some(h3_element) = middle_element.select(&h3_selector).next() {
                let title = h3_element.text().collect::<Vec<_>>().join(" ").trim().to_string();
                if let Some(p_element) = middle_element.select(&p_selector).next() {
                    let content = p_element.text().collect::<Vec<_>>().join(" ").trim().to_string();
                    
                    let title_lower = title.to_lowercase();
                    let incident_type = if title_lower.contains("planowan") || content.to_lowercase().contains("rozbudową wodociągu") {
                        "Prace planowane"
                    } else {
                        "Awaria"
                    };

                    let combined_text_for_city = format!("{} {}", title, content).to_lowercase();
                    let mut city = "Częstochowa".to_string(); // default
                    
                    let municipalities = [
                        ("Blachownia", "blachowni"),
                        ("Kłobuck", "kłobuc"),
                        ("Konopiska", "konopisk"),
                        ("Miedźno", "miedźn"),
                        ("Mykanów", "mykan"),
                        ("Olsztyn", "olsztyn"),
                        ("Poczesna", "poczesn"),
                        ("Rędziny", "rędzin")
                    ];
                    
                    for (m_name, stem) in municipalities {
                        if combined_text_for_city.contains(stem) {
                            city = m_name.to_string();
                            break;
                        }
                    }

                    let (start_dt, end_dt) = extract_date_and_times(&content);

                    let message = format!("{} - {}", incident_type, content);

                    let mut alert = UnifiedAlert {
                        source: AlertSource::PwikCzestochowa,
                        startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                        endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                        message: Some(message),
                        location: Some(format!("Miejscowość: {}", city)),
                        address_index: None,
                        is_local: Some(false),
                        hash: None,
                    };

                    let combined_text = format!("{} {} {}", city, title, content).to_lowercase();

                    for (idx, a) in settings.addresses.iter().enumerate() {
                        if !a.is_active { continue; }
                        let a_city = a.city_name.to_lowercase();
                        if a_city == city.to_lowercase() || a_city.is_empty() {
                            let check_street = |street: &str| -> bool {
                                if street.is_empty() { return false; }
                                let s_lower = street.to_lowercase();
                                let cleaned = s_lower
                                    .replace("ul.", "")
                                    .replace("ulica", "")
                                    .replace("al.", "")
                                    .replace("aleja", "")
                                    .replace("pl.", "")
                                    .replace("plac", "");
                                let words: Vec<&str> = cleaned.split_whitespace().collect();
                                let significant_words: Vec<&str> = words.into_iter()
                                    .filter(|w| w.chars().count() > 3 && !w.chars().all(|c| c.is_numeric()))
                                    .collect();
                                if significant_words.is_empty() {
                                    return combined_text.contains(&s_lower);
                                }
                                for w in significant_words {
                                    let stem = if w.ends_with('a') && w.chars().count() > 4 {
                                        let mut chars = w.chars();
                                        chars.next_back();
                                        chars.as_str()
                                    } else {
                                        w
                                    };
                                    if !combined_text.contains(stem) {
                                        return false;
                                    }
                                }
                                true
                            };

                            let mut is_match = false;
                            if check_street(&a.street_name_1) {
                                is_match = true;
                            }
                            if let Some(s2) = &a.street_name_2 {
                                if check_street(s2) {
                                    is_match = true;
                                }
                            }
                            if a.street_name_1.is_empty() && a.street_name_2.as_deref().unwrap_or("").is_empty() && a_city == city.to_lowercase() {
                                is_match = true;
                            }

                            if is_match {
                                alert.is_local = Some(true);
                                alert.address_index = Some(idx);
                                break;
                            }
                        }
                    }

                    alerts.push(alert);
                }
            }
        }

        assert_eq!(alerts.len(), 2);
        let alert1 = &alerts[0];
        assert_eq!(alert1.is_local, Some(false));
        assert_eq!(alert1.location.as_deref().unwrap(), "Miejscowość: Mykanów");
        
        let alert2 = &alerts[1];
        assert_eq!(alert2.is_local, Some(true));
        assert_eq!(alert2.location.as_deref().unwrap(), "Miejscowość: Częstochowa");
        assert!(alert2.message.as_ref().unwrap().starts_with("Prace planowane"));
    }
}
