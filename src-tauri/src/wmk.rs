use reqwest::Client;
use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use regex::Regex;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WmkItem {
    pub title: String,
    pub status: String,
    pub place: String,
    pub date: String,
    pub desc: String,
}

/// Parse WMK date format "od DD-MM-YYYY HH:mm do DD-MM-YYYY HH:mm" into (startDate, endDate) ISO.
pub fn parse_wmk_dates(date_str: &str) -> (Option<String>, Option<String>) {
    let cleaned = date_str.replace("&nbsp;", " ");
    let re = Regex::new(r"od\s+(\d{2}-\d{2}-\d{4}\s+\d{2}:\d{2})\s+do\s+(\d{2}-\d{2}-\d{4}\s+\d{2}:\d{2})").unwrap();
    if let Some(caps) = re.captures(&cleaned) {
        let start = caps.get(1).unwrap().as_str();
        let end = caps.get(2).unwrap().as_str();
        (parse_date_dmy_hm(start), parse_date_dmy_hm(end))
    } else {
        (None, None)
    }
}

fn parse_date_dmy_hm(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:00",
        date_parts[2], date_parts[1], date_parts[0], parts[1]
    ))
}

pub struct CompiledWmkRegex {
    pub street_candidates: Vec<Regex>,
    pub has_street: bool,
}

impl CompiledWmkRegex {
    pub fn new(address: &AddressEntry) -> Self {
        let mut street_candidates = Vec::new();
        let has_street = !address.street_name_1.is_empty();

        if has_street {
            let mut raw_candidates = Vec::new();
            
            let n1 = address.street_name_1.trim();
            let n2 = address.street_name_2.as_ref().map(|s| s.trim()).unwrap_or("");

            // 1. The original names
            if !n2.is_empty() && n2 != "null" {
                raw_candidates.push(format!("{} {}", n2, n1));
            }
            raw_candidates.push(n1.to_string());

            // 2. Normalized names (remove prefixes like "Plac", "ul.", etc.)
            let mut normalized = Vec::new();
            for cand in &raw_candidates {
                let clean = cand.to_lowercase()
                    .replace("ul.", "")
                    .replace("al.", "")
                    .replace("pl.", "")
                    .replace("plac", "")
                    .replace("ulica", "")
                    .replace("aleja", "")
                    .replace("os.", "")
                    .replace("osiedle", "")
                    .trim()
                    .to_string();
                if !clean.is_empty() && clean != cand.to_lowercase() {
                    normalized.push(clean);
                }
            }
            raw_candidates.extend(normalized);

            // 3. Last word (often the surname which is used alone in reports)
            let mut last_words = Vec::new();
            for cand in &raw_candidates {
                if let Some(last) = cand.split_whitespace().last() {
                    if last.len() > 2 { // Avoid short things like "II"
                        last_words.push(last.to_string());
                    }
                }
            }
            raw_candidates.extend(last_words);

            // 4. Deduplicate and create regexes
            let mut unique_cands: Vec<String> = raw_candidates.into_iter()
                .map(|s| s.to_lowercase())
                .collect();
            unique_cands.sort();
            unique_cands.dedup();

            for word in unique_cands {
                if word.len() < 3 { continue; }
                let p = format!(r"(?i){}", regex::escape(&word));
                street_candidates.push(Regex::new(&p).unwrap());
            }
        }
        Self {
            street_candidates,
            has_street,
        }
    }

    pub fn is_match(&self, place: &str, desc: &str) -> bool {
        if !self.has_street {
            return true;
        }
        // Normalize HTML entities that might be present in the scraper output
        let place_norm = place.replace("&nbsp;", " ");
        let desc_norm = desc.replace("&nbsp;", " ");
        
        self.street_candidates.iter().any(|r| {
            r.is_match(place) || r.is_match(desc) || 
            r.is_match(&place_norm) || r.is_match(&desc_norm)
        })
    }
}

pub async fn fetch_wmk_alerts(client: &Client) -> Result<Vec<WmkItem>, String> {
    let url = "https://wodociagi.krakow.pl/pl/aktualnosci/awarie-i-informacje-o-wylaczeniach-wody";
    let res = client.get(url).send().await.map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("WMK HTTP error: {}", res.status()));
    }

    let html = res.text().await.map_err(|e| e.to_string())?;

    let re = Regex::new(r"(?s)accidents\s*=\s*(\[.*?\])\s*;").unwrap();
    let json_str = match re.captures(&html) {
        Some(caps) => caps.get(1).unwrap().as_str(),
        None => return Ok(vec![]),
    };

    let items: Vec<WmkItem> = serde_json::from_str(json_str).map_err(|e| format!("WMK JSON error: {}", e))?;
    Ok(items)
}

pub struct WmkProvider;

#[async_trait]
impl AlertProvider for WmkProvider {
    fn id(&self) -> String {
        "wmk".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Wmk
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        match retry(|| fetch_wmk_alerts(client), 3).await {
            Ok(items) => {
                let mut alerts = Vec::new();
                let active_addresses: Vec<(usize, Arc<CompiledWmkRegex>)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        let active = a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::Wmk, a);
                        if active {
                            log::info!("WMK: Checking address: {}", a.name);
                        }
                        active
                    })
                    .map(|(idx, a)| {
                        let compiled = CompiledWmkRegex::new(a);
                        log::info!("WMK: Candidates for {}: {:?}", a.name, compiled.street_candidates.iter().map(|r| r.as_str()).collect::<Vec<_>>());
                        (idx, Arc::new(compiled))
                    })
                    .collect();

                for item in items {
                    if item.status == "Zakończone" {
                        continue;
                    }

                    let (start_dt, end_dt) = parse_wmk_dates(&item.date);
                    
                    let mut local_match_idx = None;
                    for (idx, compiled) in &active_addresses {
                        if compiled.is_match(&item.place, &item.desc) {
                            log::info!("WMK: MATCH FOUND for outage at {}", item.place);
                            local_match_idx = Some(*idx);
                            break;
                        }
                    }

                    let mut alert = UnifiedAlert {
                        source: AlertSource::Wmk,
                        startDate: start_dt,
                        endDate: end_dt,
                        message: Some(format!("{} - {}", item.title, item.place.replace("&nbsp;", " "))),
                        location: Some(item.desc.replace("&nbsp;", " ")),
                        address_index: None,
                        is_local: None,
                        hash: None,
                    };

                    if let Some(idx) = local_match_idx {
                        alert.address_index = Some(idx);
                        alert.is_local = Some(true);
                    } else {
                        alert.is_local = Some(false);
                    }
                    alerts.push(alert);
                }
                (alerts, Vec::new())
            }
            Err(e) => (Vec::new(), vec![format!("WMK: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wmk_dates() {
        let date = "od 14-05-2026 09:00 do&nbsp;14-05-2026 13:00";
        let (start, end) = parse_wmk_dates(date);
        assert_eq!(start, Some("2026-05-14T09:00:00".to_string()));
        assert_eq!(end, Some("2026-05-14T13:00:00".to_string()));
    }

    #[test]
    fn test_wmk_deep_sim() {
        let addr = AddressEntry {
            city_name: "Kraków".to_string(),
            street_name_1: "Waligórskiego".to_string(),
            street_name_2: Some("Andrzeja".to_string()),
            city_id: Some(950463),
            ..Default::default()
        };

        assert!(is_krakow(&addr));
        let compiled = CompiledWmkRegex::new(&addr);

        let place = "Pysocice, Waligórskiego";
        let desc = "Wodociągi Miasta Krakowa informują odbiorców, że w dniu: 14.05.2026, w godzinach od 09:00 do 13:00 z powodu planowanych prac na sieci wodociągowej, nastąpi czasowa przerwa w dostawie wody. Wyłączeniem zostały objęte budynki od numeru: 1 do 23 oraz od 30A do 44, bez wody również ulica: Waligórskiego. Za utrudnienia przepraszamy.";

        assert!(compiled.is_match(place, desc));
        
        let desc_escaped = "bez wody również ulica: Waligórskiego.";
        assert!(compiled.is_match("", desc_escaped));
    }

    #[test]
    fn test_wmk_is_match() {
        let addr = AddressEntry {
            street_name_1: "Waligórskiego".to_string(),
            ..Default::default()
        };
        let compiled = CompiledWmkRegex::new(&addr);
        
        assert!(compiled.is_match("Pysocice, Waligórskiego", "..."));
        assert!(compiled.is_match("...", "Ulica Waligórskiego wyłączona"));
        assert!(!compiled.is_match("Inna ulica", "Inny opis"));
    }

    #[tokio::test]
    async fn test_wmk_kossaka() {
        let addr = AddressEntry {
            city_id: Some(950463),
            city_name: "Kraków".to_string(),
            street_name_1: "Kossaka".to_string(),
            street_name_2: Some("Juliusza".to_string()),
            house_no: "1".to_string(),
            is_active: true,
            ..Default::default()
        };
        let compiled = CompiledWmkRegex::new(&addr);
        
        // Match by "place"
        assert!(compiled.is_match("Plac Kossaka", ""));
        
        // Match by "desc"
        assert!(compiled.is_match("", "Wodociągi informują o awarii przy placu Kossaka."));

        // Test with "Plac" in street_name_1 (if user entered it manually)
        let addr2 = AddressEntry {
            city_id: Some(950463),
            city_name: "Kraków".to_string(),
            street_name_1: "Plac Juliusza Kossaka".to_string(),
            house_no: "1".to_string(),
            is_active: true,
            ..Default::default()
        };
        let compiled2 = CompiledWmkRegex::new(&addr2);
        assert!(compiled2.is_match("Plac Kossaka", ""));
    }
}
