use reqwest::Client;
use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use regex::Regex;
use chrono::{Duration, NaiveDate, Utc};

pub const WODOCIAGI_KATOWICE_URL: &str = "https://wodociagi.katowice.pl/rss_woda";

#[derive(Debug, Clone, Default)]
pub struct WodociagiKatowiceItem {
    pub title: String,
    pub description: String,
    pub raw_description: String,
    pub link: String,
}

pub async fn fetch_rss(client: &Client) -> Result<Vec<WodociagiKatowiceItem>, String> {
    let res = client
        .get(WODOCIAGI_KATOWICE_URL)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .send()
        .await
        .map_err(|e| format!("Request error: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    let text = res.text().await.map_err(|e| e.to_string())?;
    
    // Quick and dirty robust RSS parsing via regex to avoid quick-xml strictness issues with CDATA
    let item_re = Regex::new(r"(?is)<item>(.*?)</item>").unwrap();
    let title_re = Regex::new(r"(?is)<title><!\[CDATA\[(.*?)\]\]></title>").unwrap();
    let desc_re = Regex::new(r"(?is)<description><!\[CDATA\[(.*?)\]\]></description>").unwrap();
    let link_re = Regex::new(r"(?is)<link>(?:<!\[CDATA\[)?(.*?)(?:\]\]>)?</link>").unwrap();

    let mut items = Vec::new();
    for cap in item_re.captures_iter(&text) {
        let block = &cap[1];
        let title = title_re.captures(block).map_or(String::new(), |c| c[1].trim().to_string());
        let raw_description = desc_re.captures(block).map_or(String::new(), |c| c[1].trim().to_string());
        let link = link_re.captures(block).map_or(String::new(), |c| c[1].trim().to_string());
        let description = String::from("Miejscowość: Katowice");

        if !title.is_empty() {
            items.push(WodociagiKatowiceItem {
                title,
                description,
                raw_description,
                link,
            });
        }
    }

    Ok(items)
}

pub struct CompiledWodociagiRegex {
    pub street_candidates: Vec<Regex>,
    pub has_street: bool,
}

impl CompiledWodociagiRegex {
    pub fn new(address: &AddressEntry) -> Self {
        let mut street_candidates = Vec::new();
        let has_street = !address.street_name_1.is_empty();

        if has_street {
            let mut regex_patterns: Vec<String> = crate::utils::configured_street_name_candidates(
                &address.street_name,
                &address.street_name_1,
                address.street_name_2.as_deref(),
            )
            .iter()
            .map(|candidate| regex::escape(candidate))
            .collect();

            // Keep case-inflection handling for Polish adjectival street names.
            if let Some(last) = address.street_name_1.split_whitespace().last()
                && last.chars().count() > 3
                && last.ends_with('a')
                && last.chars().count() > 4
            {
                let mut chars = last.chars();
                chars.next_back();
                regex_patterns.push(format!(r"{}\p{{L}}*", regex::escape(chars.as_str())));
            }

            for p in &regex_patterns {
                let pat = format!(r"(?i){}", p);
                if let Ok(r) = Regex::new(&pat) {
                    street_candidates.push(r);
                }
            }
        }

        Self {
            street_candidates,
            has_street,
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        if !self.has_street {
            return true;
        }
        self.street_candidates.iter().any(|r| r.is_match(text))
    }
}

pub struct KatowickieWodociagiProvider;

fn parse_notice_schedule(
    text: &str,
    termin_re: &Regex,
    time_re: &Regex,
) -> (Option<String>, Option<String>) {
    let date = termin_re
        .captures(text)
        .and_then(|caps| NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok())
        .unwrap_or_else(|| Utc::now().date_naive());

    let parse_time = |value: &str| {
        let (hour, minute) = value.split_once(['.', ':'])?;
        let hour = hour.parse::<u32>().ok()?;
        let minute = minute.parse::<u32>().ok()?;
        (hour < 24 && minute < 60).then_some((hour, minute))
    };

    if let Some(caps) = time_re.captures(text)
        && let (Some((start_hour, start_minute)), Some((end_hour, end_minute))) =
            (parse_time(&caps[1]), parse_time(&caps[2]))
        {
            let start = date.and_hms_opt(start_hour, start_minute, 0).unwrap();
            let mut end = date.and_hms_opt(end_hour, end_minute, 0).unwrap();
            if end <= start {
                end += Duration::hours(24);
            }
            return (
                Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
                Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            );
        }

    let start = date.and_hms_opt(0, 0, 0).unwrap();
    let end = start + Duration::hours(24);
    (
        Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
        Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
    )
}

fn get_notice_text(item: &WodociagiKatowiceItem) -> String {
    let title = item.title.trim();
    let raw_description = item.raw_description.trim();
    if title.is_empty() {
        raw_description.to_string()
    } else if raw_description.is_empty() || title.contains(raw_description) {
        title.to_string()
    } else {
        format!("{} {}", title, raw_description)
    }
}

#[async_trait]
impl AlertProvider for KatowickieWodociagiProvider {
    fn id(&self) -> String {
        "katowickie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::KatowickieWodociagi
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        match fetch_rss(client).await {
            Ok(mut items) => {
                for item in &mut items {
                    if !item.link.is_empty()
                        && let Ok(res) = client
                            .get(&item.link)
                            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
                            .send()
                            .await
                            && res.status().is_success()
                                && let Ok(html) = res.text().await {
                                    let document = scraper::Html::parse_document(&html);
                                    let selector = scraper::Selector::parse(".aktualnosc-tresc").unwrap();
                                    if let Some(element) = document.select(&selector).next() {
                                        let text = element.text().collect::<Vec<_>>().join(" ");
                                        let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
                                        if !clean.is_empty() {
                                            item.title = clean;
                                        }
                                    }
                                }
                }

                let active_addresses: Vec<(usize, std::sync::Arc<CompiledWodociagiRegex>)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::KatowickieWodociagi, a) && 
                        (a.city_name.to_lowercase().contains("katowice") || 
                         a.commune.to_lowercase().contains("katowice") ||
                         a.city_name.to_lowercase().contains("siemianowice") ||
                         a.city_name.to_lowercase().contains("czelad") ||
                         a.city_name.to_lowercase().contains("sosnowiec"))
                    })
                    .map(|(idx, a)| (idx, std::sync::Arc::new(CompiledWodociagiRegex::new(a))))
                    .collect();

                let mut alerts = Vec::new();
                let termin_re = Regex::new(r"Termin:\s*(\d{4}-\d{2}-\d{2})").unwrap();
                let time_re = Regex::new(
                    r"(?i)Orientacyjny\s+czas\s+wystąpienia\s+zakłóceń:\s*(?:godz\.?\s*)?(\d{1,2}[.:]\d{2})\s*[-–—]\s*(\d{1,2}[.:]\d{2})",
                ).unwrap();

                for item in items {
                    let notice_text = get_notice_text(&item);
                    let combined_text = format!("{} {}", notice_text, item.description);

                    let mut local_match_idx = None;
                    for (idx, compiled) in &active_addresses {
                        if compiled.is_match(&combined_text) {
                            local_match_idx = Some(*idx);
                            break;
                        }
                    }

                    let schedule_text = format!("{} {}", notice_text, item.raw_description);
                    let (start_date, end_date) =
                        parse_notice_schedule(&schedule_text, &termin_re, &time_re);

                    let mut alert = UnifiedAlert {
                        source: AlertSource::KatowickieWodociagi,
                        startDate: start_date,
                        endDate: end_date,
                        message: Some(notice_text),
                        location: Some(item.description.clone()),
                        address_index: None,
                        is_local: Some(false),
                        hash: None,
                    };

                    log::debug!("Alert: {:?}", alert);

                    if let Some(idx) = local_match_idx {
                        alert.address_index = Some(idx);
                        alert.is_local = Some(true);
                    }

                    alerts.push(alert);
                }

                (alerts, Vec::new())
            }
            Err(e) => (Vec::new(), vec![format!("KatowickieWodociagi: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiled_wodociagi_regex_matching() {
        let addr = AddressEntry {
            name: "Home".to_string(),
            city_name: "Katowice".to_string(),
            voivodeship: "Śląskie".to_string(),
            district: "Katowice".to_string(),
            commune: "Katowice".to_string(),
            street_name: "Lwowska".to_string(),
            street_name_1: "Lwowska".to_string(),
            street_name_2: None,
            house_no: "44".to_string(),
            city_id: None,
            street_id: None,
            is_active: true,
        };

        let regex = CompiledWodociagiRegex::new(&addr);
        
        let text = "Katowice – ul. Boliny, ul. Bzów, ul. Cmentarna, ul. Grodowa, ul. Hodowców (hodowla), ul. Leśnego Potoku, ul. Lwowska nr 42, 44, 46, 48, 50, 60";
        assert!(regex.is_match(text));

        // Test with mismatching street
        let addr_mismatch = AddressEntry {
            name: "Home".to_string(),
            city_name: "Katowice".to_string(),
            voivodeship: "Śląskie".to_string(),
            district: "Katowice".to_string(),
            commune: "Katowice".to_string(),
            street_name: "Korfantego".to_string(),
            street_name_1: "Korfantego".to_string(),
            street_name_2: None,
            house_no: "44".to_string(),
            city_id: None,
            street_id: None,
            is_active: true,
        };
        let regex_mismatch = CompiledWodociagiRegex::new(&addr_mismatch);
        assert!(!regex_mismatch.is_match(text));
    }
}
