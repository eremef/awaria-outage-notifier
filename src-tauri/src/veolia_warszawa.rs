use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use crate::utils::retry;
use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Deserializer};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VeoliaItem {
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_text")]
    pub title: String,
    #[serde(default, deserialize_with = "deserialize_text")]
    pub content: String,
    #[serde(default)]
    pub permalink: Option<String>,
    #[serde(default, deserialize_with = "deserialize_streets")]
    pub street: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_optional_text")]
    pub house_number: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_text")]
    pub start_date: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_text")]
    pub end_date: Option<String>,
    #[serde(default)]
    pub addresses: Option<Vec<VeoliaAddress>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VeoliaAddress {
    #[serde(default, deserialize_with = "deserialize_text")]
    pub street: String,
    #[serde(default, deserialize_with = "deserialize_optional_text")]
    pub house_number: Option<String>,
}

fn deserialize_streets<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let values = match value {
        Some(serde_json::Value::String(street)) => vec![street],
        Some(serde_json::Value::Array(streets)) => streets
            .into_iter()
            .filter_map(|street| match street {
                serde_json::Value::String(street) => Some(street),
                serde_json::Value::Number(street) => Some(street.to_string()),
                _ => None,
            })
            .collect(),
        Some(serde_json::Value::Number(street)) => vec![street.to_string()],
        _ => Vec::new(),
    };

    Ok(values
        .into_iter()
        .map(|street| {
            street
                .replace("<br />", " ")
                .replace("<br/>", " ")
                .replace("<br>", " ")
                .trim()
                .to_string()
        })
        .filter(|street| !street.is_empty())
        .collect())
}

fn deserialize_optional_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::String(value)) => Some(value),
        Some(serde_json::Value::Number(value)) => Some(value.to_string()),
        _ => None,
    })
}

fn deserialize_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(deserialize_optional_text(deserializer)?.unwrap_or_default())
}

impl VeoliaItem {
    fn inline_detail(&self) -> VeoliaDetail {
        let description = match (self.title.trim(), self.content.trim()) {
            ("", content) => content.to_string(),
            (title, "") => title.to_string(),
            (title, content) if title == content => content.to_string(),
            (title, content) => format!("{}: {}", title, content),
        };

        let mut affected_addresses: Vec<String> = self
            .addresses
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|address| {
                let house_number = address.house_number.as_deref().unwrap_or("").trim();
                if house_number.is_empty() {
                    address.street.trim().to_uppercase()
                } else {
                    format!("{} {}", address.street.trim(), house_number).to_uppercase()
                }
            })
            .filter(|address| !address.trim().is_empty())
            .collect();

        if affected_addresses.is_empty() {
            affected_addresses.extend(self.street.iter().map(|street| {
                let house_number = self.house_number.as_deref().unwrap_or("").trim();
                if house_number.is_empty() {
                    street.to_uppercase()
                } else {
                    format!("{} {}", street, house_number).to_uppercase()
                }
            }));
        }

        VeoliaDetail {
            description,
            affected_addresses,
        }
    }

    fn dedup_key(&self) -> String {
        if let Some(id) = &self.id {
            return format!("id:{}", id);
        }

        format!(
            "row:{}|{}|{}|{}|{}|{}",
            self.street.join(","),
            self.house_number.as_deref().unwrap_or_default(),
            self.start_date.as_deref().unwrap_or_default(),
            self.end_date.as_deref().unwrap_or_default(),
            self.title,
            self.content
        )
    }
}

pub struct VeoliaDetail {
    pub description: String,
    pub affected_addresses: Vec<String>,
}

/// Parse Veolia date format "DD.MM.YYYY HH:MM" into ISO "YYYY-MM-DDTHH:MM:00".
pub fn parse_veolia_date(s: &str) -> Option<String> {
    let cleaned = s.trim();
    if cleaned.len() >= 16 {
        let day = &cleaned[0..2];
        let month = &cleaned[3..5];
        let year = &cleaned[6..10];
        let time = &cleaned[11..16];
        Some(format!("{}-{}-{}T{}:00", year, month, day, time))
    } else {
        None
    }
}

pub fn parse_veolia_details(html: &str) -> Result<VeoliaDetail, String> {
    use scraper::{Html, Selector};
    let document = Html::parse_document(html);

    let label_value_selector = Selector::parse(".ep-label-value").map_err(|e| e.to_string())?;
    let li_selector = Selector::parse("#container-kontakt ul li").map_err(|e| e.to_string())?;

    let mut description = String::new();
    for el in document.select(&label_value_selector) {
        let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
        if !text.is_empty() && text.contains("Veolia") {
            description = text;
            break;
        }
    }

    if description.is_empty() {
        let p_selector = Selector::parse("#container-kontakt p").map_err(|e| e.to_string())?;
        for el in document.select(&p_selector) {
            let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if text.contains("Veolia Energia Warszawa S.A. informuje") {
                description = text;
                break;
            }
        }
    }

    let mut affected_addresses = Vec::new();
    for el in document.select(&li_selector) {
        let text = el.text().collect::<Vec<_>>().join(" ")
            .replace('\t', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_uppercase();
        if !text.is_empty() {
            affected_addresses.push(text);
        }
    }

    Ok(VeoliaDetail {
        description,
        affected_addresses,
    })
}

pub async fn fetch_veolia_details(client: &Client, permalink: &str) -> Result<VeoliaDetail, String> {
    let res = client
        .get(permalink)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Veolia details HTTP error: {}", res.status()));
    }

    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    let html = String::from_utf8_lossy(&bytes);
    parse_veolia_details(&html)
}

pub async fn fetch_veolia_alerts_for_street(
    client: &Client,
    street: &str,
) -> Result<Vec<VeoliaItem>, String> {
    let url = format!(
        "https://energiadlawarszawy.pl/wp-admin/admin-ajax.php?action=my_ajax_filter_search_waw&street={}",
        urlencoding::encode(street)
    );
    let res = client
        .get(&url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Veolia API HTTP error: {}", res.status()));
    }

    let bytes = match res.bytes().await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("Veolia API decoding error (treating as empty): {}", e);
            return Ok(Vec::new());
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let items: Vec<VeoliaItem> = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(items)
}

impl VeoliaItem {
    pub fn to_unified(&self, detail: &Option<VeoliaDetail>) -> UnifiedAlert {
        let start_date = self.start_date.as_deref().and_then(parse_veolia_date);
        let end_date = self.end_date.as_deref().and_then(parse_veolia_date);

        let inline_detail = self.inline_detail();
        let detail = detail.as_ref().unwrap_or(&inline_detail);
        let detail_desc = detail.description.clone();
        let addresses_str = detail.affected_addresses.join(", ");

        let mut parts = Vec::new();
        parts.push("Przerwa w dostawie ciepła".to_string());
        if !self.street.is_empty() {
            parts.push(format!("ul. {}", self.street.join(", ")));
        }
        if !detail_desc.is_empty() {
            parts.push(detail_desc.to_string());
        }
        if !addresses_str.is_empty() {
            parts.push(format!("posesje: {}", addresses_str));
        }
        let message = parts.join(" - ");

        UnifiedAlert {
            source: AlertSource::VeoliaWarszawa,
            startDate: start_date,
            endDate: end_date,
            message: Some(message),
            location: Some("Miejscowość: Warszawa".to_string()),
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct CompiledVeoliaRegex {
    pub candidates: Vec<Regex>,
    pub api_query: String,
}

impl CompiledVeoliaRegex {
    pub fn new(address: &AddressEntry) -> Self {
        let mut candidates = Vec::new();
        let n1 = address.street_name_1.trim();
        let n2 = address.street_name_2.as_ref().map(|s| s.trim()).unwrap_or("");

        let mut api_query = n1.to_string();

        if n1.is_empty() {
            return Self { candidates, api_query };
        }

        if n1.contains(' ') {
            // Pick the longest word that is at least 3 chars long for a broader API query
            if let Some(longest) = n1.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 3).max_by_key(|w| w.len()) {
                api_query = longest.to_string();
            }
        }

        let mut raw_candidates = crate::utils::configured_street_name_candidates(
            &address.street_name,
            &address.street_name_1,
            address.street_name_2.as_deref(),
        );

        if !n2.is_empty() && n2 != "null" {
            raw_candidates.push(format!("{} {}", n2, n1));
            raw_candidates.push(format!("{} {}", n1, n2));
            if let Some(first_char) = n2.chars().next() {
                raw_candidates.push(format!("{} {}.", n1, first_char));
            }
        } else if n1.contains(' ') {
            // If the user typed "Adama Mickiewicza" into streetName1
            let words: Vec<&str> = n1.split_whitespace().collect();
            if words.len() == 2 {
                let first = words[0];
                let second = words[1];
                raw_candidates.push(format!("{} {}", second, first));
                if let Some(c) = first.chars().next() {
                    raw_candidates.push(format!("{} {}.", second, c));
                }
                if let Some(c) = second.chars().next() {
                    raw_candidates.push(format!("{} {}.", first, c));
                }
                raw_candidates.push(first.to_string());
                raw_candidates.push(second.to_string());
            }
        }

        // Add normalized versions
        let mut normalized = Vec::new();
        for cand in &raw_candidates {
            let clean = cand.replace("\"", "").replace("ul.", "").replace("al.", "").replace("pl.", "").trim().to_string();
            if !clean.is_empty() && clean != *cand {
                normalized.push(clean);
            }
        }
        raw_candidates.extend(normalized);

        // Add just the longest word as a fallback candidate to be safe
        raw_candidates.push(api_query.clone());

        for c in raw_candidates {
            let p = format!(r"(?i){}", regex::escape(&c));
            if let Ok(r) = Regex::new(&p) {
                candidates.push(r);
            }
        }

        Self { candidates, api_query }
    }

    pub fn is_match(&self, item: &VeoliaItem, details: &Option<VeoliaDetail>) -> bool {
        if self.candidates.is_empty() {
            return false;
        }

        let mut full_text = item.street.join(" ").to_lowercase();
        if let Some(house_number) = &item.house_number {
            full_text.push(' ');
            full_text.push_str(&house_number.to_lowercase());
        }
        full_text.push(' ');
        full_text.push_str(&item.title.to_lowercase());
        full_text.push(' ');
        full_text.push_str(&item.content.to_lowercase());
        if let Some(addresses) = &item.addresses {
            for address in addresses {
                full_text.push(' ');
                full_text.push_str(&address.street.to_lowercase());
                if let Some(house_number) = &address.house_number {
                    full_text.push(' ');
                    full_text.push_str(&house_number.to_lowercase());
                }
            }
        }
        if let Some(d) = details {
            full_text.push(' ');
            full_text.push_str(&d.description.to_lowercase());
            full_text.push(' ');
            full_text.push_str(&d.affected_addresses.join(" ").to_lowercase());
        }

        self.candidates.iter().any(|c| c.is_match(&full_text))
    }
}

pub struct VeoliaWarszawaProvider;

#[async_trait]
impl AlertProvider for VeoliaWarszawaProvider {
    fn id(&self) -> String {
        "veolia_warszawa".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::VeoliaWarszawa
    }

    async fn fetch(
        &self,
        _client: &Client,
        client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<(usize, Arc<CompiledVeoliaRegex>)> = settings
            .addresses
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::VeoliaWarszawa, a))
            .map(|(idx, a)| (idx, Arc::new(CompiledVeoliaRegex::new(a))))
            .collect();

        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        let mut alerts = Vec::new();

        for (idx, compiled) in &active_addresses {
            match retry(
                || fetch_veolia_alerts_for_street(client_http1, &compiled.api_query),
                3,
            )
            .await
            {
                Ok(items) => {
                    for item in items {
                        let dedup_key = item.dedup_key();
                        if seen_ids.contains(&dedup_key) {
                            continue;
                        }

                        let details = if item.addresses.is_some() || item.permalink.is_none() {
                            Some(item.inline_detail())
                        } else {
                            match retry(
                                || {
                                    fetch_veolia_details(
                                        client_http1,
                                        item.permalink.as_deref().unwrap_or_default(),
                                    )
                                },
                                3,
                            )
                            .await
                            {
                                Ok(det) => Some(det),
                                Err(e) => {
                                    log::error!(
                                        "Failed to fetch details for Veolia item {:?}: {}",
                                        item.id,
                                        e
                                    );
                                    None
                                }
                            }
                        };

                        if !compiled.is_match(&item, &details) {
                            continue;
                        }

                        seen_ids.insert(dedup_key);

                        let mut alert = item.to_unified(&details);
                        alert.address_index = Some(*idx);
                        alert.is_local = Some(true);
                        alerts.push(alert);
                    }
                }
                Err(e) => {
                    let err_msg = format!("Veolia (zapytanie {}): {}", compiled.api_query, e);
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
    fn test_parse_veolia_date() {
        assert_eq!(
            parse_veolia_date("22.05.2026 09:20"),
            Some("2026-05-22T09:20:00".to_string())
        );
        assert_eq!(parse_veolia_date("invalid"), None);
    }

    #[test]
    fn test_parse_veolia_details() {
        let html = r#"
            <div id="container-kontakt">
                <div class="row">
                    <div class="col-lg-12 col-md-12 col-sm-12">
                        <span class="ep-label-value">Veolia Energia Warszawa S.A. informuje, że w rejonie Warszawa ulica POŻARYSKIEGO M. 24 nastąpiła przerwa w dostawach ciepła.</span>
                    </div>
                    <div class="col-lg-12 col-md-12 col-sm-12">
                        <ul>
                            <li>KOŻUCHOWSKA	7</li>
                            <li>KRUPNICZA	15</li>
                        </ul>
                    </div>
                </div>
            </div>
        "#;

        let parsed = parse_veolia_details(html).unwrap();
        assert_eq!(parsed.description, "Veolia Energia Warszawa S.A. informuje, że w rejonie Warszawa ulica POŻARYSKIEGO M. 24 nastąpiła przerwa w dostawach ciepła.");
        assert_eq!(parsed.affected_addresses, vec!["KOŻUCHOWSKA 7", "KRUPNICZA 15"]);
    }
}
