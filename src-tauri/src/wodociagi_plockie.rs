use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Duration};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use regex::Regex;

pub struct WodociagiPlockieProvider;

fn parse_plock_date(text: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let date_re = Regex::new(r"(\d{1,2})\.(\d{2})\.(\d{4})").unwrap();
    let time_re = Regex::new(r"(\d{1,2}):(\d{2})").unwrap();

    let mut base_date = Local::now().naive_local().date();

    if let Some(caps) = date_re.captures(text) {
        if let (Ok(d), Ok(m), Ok(y)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>(), caps[3].parse::<i32>()) {
            if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                base_date = date;
            }
        }
    } else {
        return (None, None); // No date found
    }

    let mut start_time = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
    let mut end_time = NaiveTime::from_hms_opt(16, 0, 0).unwrap();

    if let Some(caps) = time_re.captures(text) {
        if let (Ok(h), Ok(m)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
            if let Some(t) = NaiveTime::from_hms_opt(h, m, 0) {
                start_time = t;
                // If it mentions "do godzin popołudniowych", assume end of day.
                if text.to_lowercase().contains("popołudniowych") {
                    end_time = NaiveTime::from_hms_opt(23, 59, 59).unwrap();
                } else {
                    end_time = t + Duration::hours(8); 
                }
            }
        }
    }

    let start_dt = NaiveDateTime::new(base_date, start_time);
    let mut end_dt = NaiveDateTime::new(base_date, end_time);

    if end_dt <= start_dt {
        end_dt += Duration::hours(8);
    }

    (Some(start_dt), Some(end_dt))
}

#[async_trait]
impl AlertProvider for WodociagiPlockieProvider {
    fn id(&self) -> String {
        "wodociagi_plockie".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::WodociagiPlockie
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

        let urls = [
            ("https://wodociagi.pl/planowane-wylaczenia/", "Prace planowane"),
            ("https://wodociagi.pl/category/awarie-powazne/", "Awaria"),
        ];

        for (url, incident_type) in urls.iter() {
            match retry(|| async {
                client.get(*url)
                    .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?
                    .text()
                    .await
                    .map_err(|e| e.to_string())
            }, 3).await {
                Ok(html_content) => {
                    let document = Html::parse_document(&html_content);
                    
                    // We parse the entry-content. In planowane-wylaczenia, it's one big block separated by hr.
                    // In awarie-powazne, there might be multiple articles.
                    
                    let content_selector = Selector::parse(".entry-content").unwrap();
                    let p_selector = Selector::parse("p").unwrap();
                    let ul_selector = Selector::parse("ul").unwrap();
                    let li_selector = Selector::parse("li").unwrap();
                    let _hr_selector = Selector::parse("hr").unwrap();

                    // To handle both: just extract all text nodes inside entry-content, but it's better to process each entry-content.
                    for content_element in document.select(&content_selector) {
                        let html = content_element.inner_html();
                        // Split by <hr> for planowane-wylaczenia which uses one big content block.
                        let blocks: Vec<&str> = html.split("<hr").collect();
                        
                        for block in blocks {
                            let block_doc = Html::parse_fragment(block);
                            let mut full_text = Vec::new();
                            let mut streets = Vec::new();
                            for p in block_doc.select(&p_selector) {
                                let text = p.text().collect::<Vec<_>>().join(" ").trim().to_string();
                                if !text.is_empty() {
                                    full_text.push(text.clone());
                                }
                            }

                            for ul in block_doc.select(&ul_selector) {
                                for li in ul.select(&li_selector) {
                                    let li_text = li.text().collect::<Vec<_>>().join(" ").trim().to_string();
                                    if !li_text.is_empty() {
                                        streets.push(li_text);
                                    }
                                }
                            }

                            if full_text.is_empty() && streets.is_empty() {
                                continue;
                            }

                            let combined_desc = full_text.join(" ");
                            if combined_desc.is_empty() {
                                continue;
                            }

                            let (start_dt, end_dt) = parse_plock_date(&combined_desc);
                            
                            let city = "Płock".to_string();
                            let message = if !streets.is_empty() {
                                format!("{} - ulica: {}. {}", incident_type, streets.join(", "), combined_desc)
                            } else {
                                format!("{} - {}", incident_type, combined_desc)
                            };

                            let mut alert = UnifiedAlert {
                                source: AlertSource::WodociagiPlockie,
                                startDate: start_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                endDate: end_dt.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                                message: Some(message),
                                location: Some(format!("Miejscowość: {}", city)),
                                address_index: None,
                                is_local: Some(false),
                                hash: None,
                            };

                            let combined_text = format!("{} {} {}", city, combined_desc, streets.join(" ")).to_lowercase().replace("\"", "");

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
                                            .replace("plac", "")
                                            .replace("\"", "");
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
                Err(e) => {
                    let err_msg = format!("Wodociagi Plockie error ({}): {}", incident_type, e);
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
    fn test_parse_plock_date() {
        let text = "Wodociągi Płockie Sp. z o.o. informują, że w dniu 25.05.2026r. od godz. 8:00 do godzin popołudniowych nastąpi przerwa";
        let (start, end) = parse_plock_date(text);
        assert_eq!(start.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-05-25 08:00");
        assert_eq!(end.unwrap().format("%Y-%m-%d %H:%M").to_string(), "2026-05-25 23:59");
    }
}
