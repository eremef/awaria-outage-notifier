use reqwest::Client;
use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert, AlertProvider, Settings};
use async_trait::async_trait;
use regex::Regex;
use chrono::{Duration, Utc};

pub const WODOCIAGI_KATOWICE_URL: &str = "https://wodociagi.katowice.pl/rss_woda";

#[derive(Debug, Clone, Default)]
pub struct WodociagiKatowiceItem {
    pub title: String,
    pub description: String,
    pub raw_description: String,
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

    let mut items = Vec::new();
    for cap in item_re.captures_iter(&text) {
        let block = &cap[1];
        let title = title_re.captures(block).map_or(String::new(), |c| c[1].trim().to_string());
        let raw_description = desc_re.captures(block).map_or(String::new(), |c| c[1].trim().to_string());
        let description = String::from("Miejscowość: Katowice");

        if !title.is_empty() {
            items.push(WodociagiKatowiceItem {
                title,
                description,
                raw_description,
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
            let mut regex_patterns = Vec::new();
            let n1 = address.street_name_1.trim();
            let n2 = address.street_name_2.as_deref().unwrap_or("").trim();

            if !n2.is_empty() && n2 != "null" {
                regex_patterns.push(regex::escape(&format!("{} {}", n2, n1)));
            }
            regex_patterns.push(regex::escape(n1));

            if let Some(last) = n1.split_whitespace().last() {
                if last.chars().count() > 3 {
                    regex_patterns.push(regex::escape(last));
                    
                    // Simple stemming for Polish adjectival streets
                    // e.g., Dębowa -> Dębowej (locative/genitive), Dębową (instrumental)
                    if last.ends_with('a') && last.chars().count() > 4 {
                        let mut chars = last.chars();
                        chars.next_back(); // remove 'a'
                        let stem = chars.as_str();
                        regex_patterns.push(format!(r"{}\p{{L}}*", regex::escape(stem)));
                    }
                }
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
            Ok(items) => {
                let active_addresses: Vec<(usize, std::sync::Arc<CompiledWodociagiRegex>)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        a.is_active && 
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

                for item in items {
                    let combined_text = format!("{} {}", item.title, item.description);

                    let mut local_match_idx = None;
                    for (idx, compiled) in &active_addresses {
                        if compiled.is_match(&combined_text) {
                            local_match_idx = Some(*idx);
                            break;
                        }
                    }

                    // Try parsing the date from raw_description: "Termin: 2026-05-20"
                    let start_date = if let Some(caps) = termin_re.captures(&item.raw_description) {
                        Some(format!("{}T00:00:00", &caps[1]))
                    } else {
                        // Fallback to today at midnight if date is unknown
                        Some(Utc::now().format("%Y-%m-%dT00:00:00").to_string())
                    };

                    let end_date = start_date.as_ref().and_then(|s| {
                        crate::utils::parse_date(s).map(|dt| {
                            let end = dt + Duration::hours(24);
                            end.format("%Y-%m-%dT%H:%M:00").to_string()
                        })
                    });

                    let mut alert = UnifiedAlert {
                        source: AlertSource::KatowickieWodociagi,
                        startDate: start_date,
                        endDate: end_date,
                        message: Some(item.title.clone()),
                        description: Some(item.description.clone()),
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
