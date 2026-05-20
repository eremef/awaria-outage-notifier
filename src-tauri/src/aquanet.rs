use reqwest::Client;
use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use scraper::{Html, Selector};
use chrono::{Duration, Utc};
use regex::Regex;

pub const AQUANET_BASE_URL: &str = "https://www.aquanet.pl";
pub const AQUANET_LIST_PATH: &str = "/dla-klienta/awarie-i-prace-planowe";

fn get_aquanet_list_url() -> String {
    #[cfg(test)]
    {
        std::env::var("AQUANET_BASE_URL").unwrap_or_else(|_| AQUANET_BASE_URL.to_string())
            + AQUANET_LIST_PATH
    }
    #[cfg(not(test))]
    {
        format!("{}{}", AQUANET_BASE_URL, AQUANET_LIST_PATH)
    }
}

fn get_aquanet_detail_url(slug: &str) -> String {
    #[cfg(test)]
    {
        let base = std::env::var("AQUANET_BASE_URL").unwrap_or_else(|_| AQUANET_BASE_URL.to_string());
        format!("{}/awaria/{}/", base, slug)
    }
    #[cfg(not(test))]
    {
        format!("{}/awaria/{}/", AQUANET_BASE_URL, slug)
    }
}

/// Poznań communes served by Aquanet
const POZNAN_COMMUNES: &[&str] = &[
    "poznań", "poznan",
    "czerwonak",
    "dopiewo",
    "kleszczewo",
    "komorniki",
    "kórnik", "kornik",
    "luboń", "lubon",
    "mosina",
    "murowana goślina", "murowana goslina",
    "puszczykowo",
    "rokietnica",
    "suchy las",
    "swarzędz", "swarzedz",
    "tarnowo podgórne", "tarnowo podgorne",
    "brodnica",
];

pub fn is_poznan_area(addr: &AddressEntry) -> bool {
    let city = addr.city_name.trim().to_lowercase();
    let commune = addr.commune.trim().to_lowercase();

    POZNAN_COMMUNES.iter().any(|&c| {
        city.starts_with(c) || commune.starts_with(c)
    })
}

#[derive(Debug, Clone, Default)]
pub struct AquanetItem {
    pub title: String,
    pub slug: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub is_emergency: bool,
    pub city: Option<String>,
    pub streets: Option<String>,
    pub impediments: Option<String>,
}

/// Parse Aquanet date text like "20.05.2026 godz. 08:00" → "2026-05-20T08:00:00"
pub fn parse_aquanet_date(date_str: &str) -> Option<String> {
    // Normalize: "20.05.2026 godz. 08:00" → "20-05-2026 08:00"
    let cleaned = date_str
        .replace("godz.", "")
        .replace('.', "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    // Expect "DD-MM-YYYY HH:MM"
    let parts: Vec<&str> = cleaned.splitn(2, ' ').collect();
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

/// Scrape the main list page and return raw items (without detail descriptions yet)
pub async fn fetch_aquanet_list(client: &Client) -> Result<Vec<AquanetItem>, String> {
    let url = get_aquanet_list_url();
    let res = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Aquanet HTTP error: {}", res.status()));
    }

    let html = res.text().await.map_err(|e| e.to_string())?;
    parse_aquanet_list_html(&html)
}

pub fn parse_aquanet_list_html(html: &str) -> Result<Vec<AquanetItem>, String> {
    let document = Html::parse_document(html);

    // Cards on the list page: .accident-list__item or .section-accidents__item
    // Based on the site structure, try multiple selectors
    let card_sel = Selector::parse(".accident-list__item, .section-accidents__item, article.accident-item, .item-accident").ok();
    let link_sel = Selector::parse("a").ok();
    let title_sel = Selector::parse(".accident-list__item-title, .accident-item__title, .item-accident__title, h3, h2").ok();
    let location_sel = Selector::parse(".accident-list__item-location, .accident-item__location, .location, .place, .item-accident__text--lite, .item-accident__text").ok();

    let mut items = Vec::new();

    // Try to find cards; if no cards, fall back to scanning all /awaria/ links
    if let Some(card_selector) = &card_sel {
        for card in document.select(card_selector) {
            let mut slug = String::new();
            let mut title = String::new();
            let mut location = None;

            // Find link to detail page
            if let Some(ls) = &link_sel {
                for a in card.select(ls) {
                    if let Some(href) = a.value().attr("href") {
                        if href.contains("/awaria/") {
                            // Extract slug from /awaria/{slug}/
                            let re = Regex::new(r"/awaria/([^/]+)").unwrap();
                            if let Some(caps) = re.captures(href) {
                                slug = caps.get(1).unwrap().as_str().to_string();
                            }
                            break;
                        }
                    }
                }
            }

            if slug.is_empty() {
                continue;
            }

            // Title
            if let Some(ts) = &title_sel {
                if let Some(t) = card.select(ts).next() {
                    title = t.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
                }
            }
            if title.is_empty() {
                title = card.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect();
            }

            // Dates - look for text patterns like "od XX.XX.XXXX" and "do XX.XX.XXXX"
            let card_text = card.text().collect::<Vec<_>>().join(" ");
            let (start_date, end_date) = parse_aquanet_date_range(&card_text);

            // Location
            if let Some(ls) = &location_sel {
                if let Some(l) = card.select(ls).next() {
                    location = Some(l.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" "));
                }
            }

            let is_emergency = title.to_lowercase().contains("awaria") || 
                               title.to_lowercase().contains("emergency");

            items.push(AquanetItem {
                title,
                slug,
                start_date,
                end_date,
                location,
                description: None,
                is_emergency,
                ..Default::default()
            });
        }
    }

    // Fallback: if no cards found, scan all links for /awaria/ paths
    if items.is_empty() {
        let re = Regex::new(r#"/awaria/([^/"'\s]+)"#).unwrap();
        let mut seen_slugs = std::collections::HashSet::new();
        for caps in re.captures_iter(html) {
            let slug = caps.get(1).unwrap().as_str().to_string();
            if !seen_slugs.insert(slug.clone()) {
                continue;
            }
            items.push(AquanetItem {
                title: String::from("Awaria wodociągowa"),
                slug,
                start_date: None,
                end_date: None,
                location: None,
                description: None,
                is_emergency: true,
                ..Default::default()
            });
        }
    }

    Ok(items)
}

/// Parse "od DD.MM.YYYY [godz.] HH:MM do DD.MM.YYYY [godz.] HH:MM" from text
pub fn parse_aquanet_date_range(text: &str) -> (Option<String>, Option<String>) {
    // Pattern: "od 20.05.2026 godz. 08:00 do 20.05.2026 godz. 16:00"
    let re = Regex::new(
        r"(?i)od\s+(\d{1,2}[.\-]\d{1,2}[.\-]\d{4})\s+(?:godz\.)?\s*(\d{1,2}:\d{2})(?:\s+do\s+(\d{1,2}[.\-]\d{1,2}[.\-]\d{4})\s+(?:godz\.)?\s*(\d{1,2}:\d{2}))?"
    ).unwrap();

    if let Some(caps) = re.captures(text) {
        let start_date = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let start_time = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let start = parse_aquanet_date(&format!("{} {}", start_date, start_time));

        let end = if let (Some(ed), Some(et)) = (caps.get(3), caps.get(4)) {
            parse_aquanet_date(&format!("{} {}", ed.as_str(), et.as_str()))
        } else {
            None
        };
        return (start, end);
    }

    // Try simpler "Termin: DD.MM.YYYY" without time
    let re2 = Regex::new(r"(?i)(?:termin|od|data)[:\s]+(\d{1,2}[.\-]\d{1,2}[.\-]\d{4})").unwrap();
    if let Some(caps) = re2.captures(text) {
        let date_str = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let start = parse_aquanet_date(&format!("{} 00:00", date_str));
        return (start, None);
    }

    (None, None)
}

#[derive(Debug, Clone, Default)]
pub struct AquanetDetail {
    pub description: Option<String>,
    pub location: Option<String>,
    pub dates: Option<(Option<String>, Option<String>)>,
    pub title: Option<String>,
    pub city: Option<String>,
    pub streets: Option<String>,
    pub impediments: Option<String>,
}

fn is_date_like(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("od ") || lower.contains("do ") || lower.contains("godz.") {
        return true;
    }
    if let Ok(re) = Regex::new(r"\d{1,2}[.\-]\d{1,2}[.\-]\d{4}") {
        if re.is_match(text) {
            return true;
        }
    }
    false
}

/// Fetch detail page and extract description and other rich details
pub async fn fetch_aquanet_detail(client: &Client, slug: &str) -> Option<AquanetDetail> {
    let url = get_aquanet_detail_url(slug);
    let res = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .ok()?;

    if !res.status().is_success() {
        return None;
    }

    let html = res.text().await.ok()?;
    let document = Html::parse_document(&html);

    // Extract description from div.accident-content__text-wyswig (note: intentional misspelling on site)
    let description = Selector::parse(".accident-content__text-wyswig, .accident-content__text-wysiwyg, .item-accident__text--lite, .item-accident__text")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .or_else(|| {
            Selector::parse(".accident-content, .entry-content")
                .ok()
                .and_then(|sel| document.select(&sel).next())
        })
        .map(|el| {
            el.text().collect::<Vec<_>>().join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        });

    // Try to parse .accident-map-box structure intelligently using label-value pairs
    let mut parsed_title = None;
    let mut parsed_location = None;
    let mut parsed_dates = None;

    if let Ok(box_sel) = Selector::parse(".accident-map-box, .accident-content") {
        if let Some(box_el) = document.select(&box_sel).next() {
            let label_sel = Selector::parse(".accident-map-box__content-label, .accident-map-box__label, .label, .accident-content__label").ok();
            let val_sel = Selector::parse(".accident-map-box__content-value, .accident-map-box__content_value, .value, .accident-content__value").ok();

            if let (Some(l_sel), Some(v_sel)) = (label_sel, val_sel) {
                let labels: Vec<String> = box_el.select(&l_sel).map(|el| {
                    el.text().collect::<Vec<_>>().join(" ").trim().to_lowercase()
                }).collect();
                let values: Vec<String> = box_el.select(&v_sel).map(|el| {
                    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
                }).collect();

                // Pair them up by index
                for (lbl, v) in labels.into_iter().zip(values.into_iter()) {
                    if lbl.contains("rodzaj") || lbl.contains("typ") {
                        parsed_title = Some(v);
                    } else if lbl.contains("obszar") || lbl.contains("lokalizac") || lbl.contains("miejsce") {
                        parsed_location = Some(v);
                    } else if lbl.contains("termin") || lbl.contains("data") || lbl.contains("czas") {
                        let dates = parse_aquanet_date_range(&v);
                        if dates.0.is_some() || dates.1.is_some() {
                            parsed_dates = Some(dates);
                        }
                    }
                }
            }
        }
    }

    // Try to get location from detail page
    let location = parsed_location.or_else(|| {
        let loc_sel = Selector::parse(".accident-content__location, .location, .accident-detail__location").ok();
        loc_sel.and_then(|sel| {
            document.select(&sel).next().map(|el| {
                el.text().collect::<Vec<_>>().join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
    });

    // Try to extract dates from detail page text (better than list page)
    let dates_opt = parsed_dates.or_else(|| {
        let page_text = document.root_element().text().collect::<Vec<_>>().join(" ");
        let dates = parse_aquanet_date_range(&page_text);
        if dates.0.is_some() || dates.1.is_some() {
            Some(dates)
        } else {
            None
        }
    });

    // Try to get specific title (incident type) from detail page
    let mut title = parsed_title;
    if title.is_none() {
        if let Ok(title_sel) = Selector::parse(".accident-map-box__content-value, .accident-map-box__content_value, .accident-detail__title, .accident-content__title, .item-accident__title") {
            for el in document.select(&title_sel) {
                let txt = el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
                if !txt.is_empty() && !is_date_like(&txt) {
                    title = Some(txt);
                    break;
                }
            }
        }
    }
    if title.is_none() {
        if let Ok(h1_sel) = Selector::parse("h1:not(.accident-header__title)") {
            if let Some(el) = document.select(&h1_sel).next() {
                let txt = el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
                if !txt.is_empty() && !is_date_like(&txt) {
                    title = Some(txt);
                }
            }
        }
    }

    // Extract city from .accident-header__title or .accident-header__commune
    let city_sel = Selector::parse(".accident-header__title, .accident-header__commune").ok();
    let city = city_sel.and_then(|sel| {
        document.select(&sel).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
    });

    // Extract streets from .accident-header__info-bar or .accident-header__streets
    let streets_sel = Selector::parse(".accident-header__info-bar, .accident-header__streets").ok();
    let streets = streets_sel.and_then(|sel| {
        document.select(&sel).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
    });

    // Extract impediments from .accident-content__row--impediments or general .accident-content__row containing "Utrudnienia"
    let imp_sel = Selector::parse(".accident-content__row--impediments, .accident-content__row").ok();
    let impediments = imp_sel.and_then(|sel| {
        document.select(&sel).filter(|el| {
            let class_attr = el.value().attr("class").unwrap_or("");
            if class_attr.contains("accident-content__row--impediments") {
                return true;
            }
            let text = el.text().collect::<Vec<_>>().join(" ").to_lowercase();
            text.contains("utrudnienia")
        }).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
    });

    Some(AquanetDetail {
        description,
        location,
        dates: dates_opt,
        title,
        city,
        streets,
        impediments,
    })
}

/// Full fetch: list + parallel detail pages
pub async fn fetch_aquanet_alerts(client: &Client) -> Result<Vec<AquanetItem>, String> {
    let mut items = retry(|| fetch_aquanet_list(client), 3).await?;

    // Fetch detail pages concurrently (up to 5 at a time)
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(5));
    let client = std::sync::Arc::new(client.clone());
    let mut handles = Vec::new();

    for item in &items {
        let slug = item.slug.clone();
        let c = client.clone();
        let sem = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.ok()?;
            fetch_aquanet_detail(&c, &slug).await
        }));
    }

    let results = futures::future::join_all(handles).await;
    for (item, result) in items.iter_mut().zip(results) {
        if let Ok(Some(detail)) = result {
            if let Some(desc) = detail.description {
                item.description = Some(desc);
            }
            if let Some(l) = detail.location {
                if item.location.is_none() {
                    item.location = Some(l);
                }
            }
            if let Some((start, end)) = detail.dates {
                if item.start_date.is_none() && start.is_some() {
                    item.start_date = start;
                }
                if item.end_date.is_none() && end.is_some() {
                    item.end_date = end;
                }
            }
            if let Some(t) = detail.title {
                if !t.is_empty() && (item.title.is_empty() || item.title == "Awaria wodociągowa") {
                    item.title = t;
                    item.is_emergency = item.title.to_lowercase().contains("awaria") || 
                                         item.title.to_lowercase().contains("emergency");
                }
            }
            if let Some(c) = detail.city {
                item.city = Some(c);
            }
            if let Some(s) = detail.streets {
                item.streets = Some(s);
            }
            if let Some(imp) = detail.impediments {
                item.impediments = Some(imp);
            }
        }
    }

    Ok(items)
}



pub fn clean_aquanet_description(desc: &str) -> String {
    let mut cleaned = desc.to_string();
    
    let markers = [
        "załączniki",
        "zalaczniki",
        "data ostatniej aktualizacji",
        "pliki do pobrania",
    ];
    
    for marker in &markers {
        let lower = cleaned.to_lowercase();
        if let Some(idx) = lower.find(marker) {
            cleaned = cleaned[..idx].to_string();
        }
    }
    
    cleaned.trim().to_string()
}

impl AquanetItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        let end_date = if self.end_date.is_none() && self.is_emergency {
            // Default 24h for emergency outages with no end date
            self.start_date.as_deref().and_then(|s| {
                crate::utils::parse_date(s).map(|dt| {
                    let end = dt + Duration::hours(24);
                    end.format("%Y-%m-%dT%H:%M:00").to_string()
                })
            }).or_else(|| {
                // No start date either, assume current time + 24h
                let end = Utc::now() + Duration::hours(24);
                Some(end.format("%Y-%m-%dT%H:%M:00").to_string())
            })
        } else {
            self.end_date.clone()
        };

        let (city, streets) = match (&self.city, &self.streets) {
            (Some(c), Some(s)) if !c.is_empty() && !s.is_empty() => (c.clone(), s.clone()),
            (Some(c), _) if !c.is_empty() => (c.clone(), String::new()),
            (_, Some(s)) if !s.is_empty() => ("Poznań".to_string(), s.clone()),
            _ => match &self.location {
                Some(loc) if !loc.is_empty() => {
                    if let Some((c, s)) = loc.split_once(',') {
                        (c.trim().to_string(), s.trim().to_string())
                    } else {
                        let loc_trimmed = loc.trim();
                        let loc_lower = loc_trimmed.to_lowercase();
                        let is_commune = POZNAN_COMMUNES.iter().any(|&c| loc_lower.contains(c));
                        if is_commune && !loc_lower.contains("ul.") && !loc_lower.contains("al.") && !loc_lower.contains("os.") {
                            (loc_trimmed.to_string(), String::new())
                        } else {
                            ("Poznań".to_string(), loc_trimmed.to_string())
                        }
                    }
                }
                _ => ("Poznań".to_string(), String::new()),
            }
        };

        let desc_clean = self.description.as_deref()
            .map(clean_aquanet_description)
            .filter(|s| !s.is_empty())
            .unwrap_or_default();

        let mut parts = Vec::new();
        let title_to_use = if self.title.is_empty() {
            if self.is_emergency {
                "Awaria wodociągowa".to_string()
            } else {
                "Planowe wyłączenie wody".to_string()
            }
        } else {
            self.title.clone()
        };
        parts.push(title_to_use);

        if !streets.is_empty() {
            parts.push(streets);
        }

        if !desc_clean.is_empty() {
            parts.push(desc_clean);
        }

        let mut base_message = parts.join(" - ");

        if let Some(imp) = &self.impediments {
            if !imp.is_empty() {
                let trimmed = base_message.trim();
                if !trimmed.to_lowercase().contains(&imp.to_lowercase()) {
                    let separator = if trimmed.ends_with('.') { " " } else { ". " };
                    base_message = format!("{}{}{}", trimmed, separator, imp);
                }
            }
        }

        let message = base_message;
        let description = Some(format!("Miejscowość: {}", city));

        UnifiedAlert {
            source: AlertSource::Aquanet,
            startDate: self.start_date.clone(),
            endDate: end_date,
            message: Some(message),
            description,
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct CompiledAquanetRegex {
    pub street_candidates: Vec<Regex>,
    pub has_street: bool,
}

impl CompiledAquanetRegex {
    pub fn new(address: &AddressEntry) -> Self {
        let mut street_candidates = Vec::new();
        let has_street = !address.street_name_1.is_empty();

        if has_street {
            let mut candidates = Vec::new();
            let n1 = address.street_name_1.trim();
            let n2 = address.street_name_2.as_deref().unwrap_or("").trim();

            if !n2.is_empty() && n2 != "null" {
                candidates.push(format!("{} {}", n2, n1));
            }
            candidates.push(n1.to_string());

            // Also last word for surname-based street names
            if let Some(last) = n1.split_whitespace().last() {
                if last.len() > 3 {
                    candidates.push(last.to_string());
                }
            }

            for cand in &candidates {
                let p = format!(r"(?i){}", regex::escape(cand));
                if let Ok(r) = Regex::new(&p) {
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

pub struct AquanetProvider;

#[async_trait]
impl AlertProvider for AquanetProvider {
    fn id(&self) -> String {
        "aquanet".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Aquanet
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        match fetch_aquanet_alerts(client).await {
            Ok(items) => {
                let active_addresses: Vec<(usize, std::sync::Arc<CompiledAquanetRegex>)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.is_active && is_poznan_area(a))
                    .map(|(idx, a)| (idx, std::sync::Arc::new(CompiledAquanetRegex::new(a))))
                    .collect();

                let mut alerts = Vec::new();

                for item in items {
                    let combined_text = format!(
                        "{} {} {} {} {}",
                        item.title,
                        item.location.as_deref().unwrap_or(""),
                        item.description.as_deref().unwrap_or(""),
                        item.city.as_deref().unwrap_or(""),
                        item.streets.as_deref().unwrap_or("")
                    );

                    let mut local_match_idx = None;
                    for (idx, compiled) in &active_addresses {
                        if compiled.is_match(&combined_text) {
                            local_match_idx = Some(*idx);
                            break;
                        }
                    }

                    let mut alert = item.to_unified();
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
            Err(e) => (Vec::new(), vec![format!("Aquanet: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_aquanet_date_dot_format() {
        assert_eq!(
            parse_aquanet_date("20.05.2026 08:00"),
            Some("2026-05-20T08:00:00".to_string())
        );
    }

    #[test]
    fn test_parse_aquanet_date_with_godz() {
        assert_eq!(
            parse_aquanet_date("20.05.2026 godz. 08:00"),
            Some("2026-05-20T08:00:00".to_string())
        );
    }

    #[test]
    fn test_parse_aquanet_date_invalid() {
        assert_eq!(parse_aquanet_date("nie ma daty"), None);
    }

    #[test]
    fn test_parse_aquanet_date_range_full() {
        let text = "od 20.05.2026 godz. 08:00 do 20.05.2026 godz. 16:00";
        let (start, end) = parse_aquanet_date_range(text);
        assert_eq!(start, Some("2026-05-20T08:00:00".to_string()));
        assert_eq!(end, Some("2026-05-20T16:00:00".to_string()));
    }

    #[test]
    fn test_parse_aquanet_date_range_no_end() {
        let text = "od 20.05.2026 godz. 08:00";
        let (start, end) = parse_aquanet_date_range(text);
        assert_eq!(start, Some("2026-05-20T08:00:00".to_string()));
        assert_eq!(end, None);
    }

    #[test]
    fn test_is_poznan_area_match() {
        let addr = AddressEntry {
            city_name: "Poznań".to_string(),
            commune: "Poznań".to_string(),
            ..Default::default()
        };
        assert!(is_poznan_area(&addr));

        let addr2 = AddressEntry {
            city_name: "Swarzędz".to_string(),
            commune: "Swarzędz".to_string(),
            ..Default::default()
        };
        assert!(is_poznan_area(&addr2));
    }

    #[test]
    fn test_is_poznan_area_no_match() {
        let addr = AddressEntry {
            city_name: "Kraków".to_string(),
            commune: "Kraków".to_string(),
            ..Default::default()
        };
        assert!(!is_poznan_area(&addr));
    }

    #[test]
    fn test_emergency_fallback_24h() {
        let item = AquanetItem {
            title: "Awaria wodociągowa".to_string(),
            slug: "test-awaria".to_string(),
            start_date: Some("2026-05-20T08:00:00".to_string()),
            end_date: None,
            location: Some("ul. Testowa".to_string()),
            description: None,
            is_emergency: true,
            ..Default::default()
        };
        let alert = item.to_unified();
        assert!(alert.endDate.is_some());
        // End should be 24h after start
        let end = alert.endDate.unwrap();
        assert!(end.contains("2026-05-21T08:00:00"));
    }

    #[test]
    fn test_planned_no_fallback() {
        let item = AquanetItem {
            title: "Planowe wyłączenie wody".to_string(),
            slug: "test-planowe".to_string(),
            start_date: Some("2026-05-20T08:00:00".to_string()),
            end_date: None,
            location: None,
            description: None,
            is_emergency: false,
            ..Default::default()
        };
        let alert = item.to_unified();
        // Non-emergency: no fallback end date if no end_date supplied
        assert!(alert.endDate.is_none());
    }

    #[test]
    fn test_compiled_regex_match() {
        let addr = AddressEntry {
            street_name_1: "Testowa".to_string(),
            ..Default::default()
        };
        let compiled = CompiledAquanetRegex::new(&addr);
        assert!(compiled.is_match("Wyłączenie wody na ul. Testowa 5"));
        assert!(!compiled.is_match("Wyłączenie wody na ul. Innej"));
    }

    #[test]
    fn test_compiled_regex_no_street_always_match() {
        let addr = AddressEntry {
            street_name_1: "".to_string(),
            city_name: "Poznań".to_string(),
            ..Default::default()
        };
        let compiled = CompiledAquanetRegex::new(&addr);
        // No street = match everything (city-level)
        assert!(compiled.is_match("Jakikolwiek tekst"));
    }

    #[test]
    fn test_parse_aquanet_list_html_fallback() {
        // Simple HTML with an /awaria/ link
        let html = r#"
        <html><body>
        <div class="accident-list">
          <a href="/awaria/test-slug-123/">Awaria wodociągowa – ul. Poznańska</a>
        </div>
        </body></html>
        "#;

        let items = parse_aquanet_list_html(html).unwrap();
        assert!(!items.is_empty());
        assert_eq!(items[0].slug, "test-slug-123");
    }

    #[test]
    fn test_parse_aquanet_list_html_item_accident_text() {
        let html = r#"
        <div class="accident-list__item">
            <a href="/awaria/chabrowa-fiołkowa/">
                <h3 class="accident-list__item-title">Awaria wodociągowa</h3>
            </a>
            <p class="item-accident__text item-accident__text--lite d-none d-md-inline-block">
                ul. Chabrowa (od Fiołkowej)
            </p>
        </div>
        "#;

        let items = parse_aquanet_list_html(html).unwrap();
        assert!(!items.is_empty());
        assert_eq!(items[0].slug, "chabrowa-fiołkowa");
        assert_eq!(items[0].location, Some("ul. Chabrowa (od Fiołkowej)".to_string()));
    }

    #[test]
    fn test_parse_aquanet_list_html_item_accident_block() {
        let html = r#"
        <div class="item-accident">
            <a href="/awaria/chabrowa-fiołkowa-planowane/">
                <h3 class="item-accident__title">Planowe wyłączenie wody</h3>
            </a>
            <p class="item-accident__text item-accident__text--lite d-none d-md-inline-block">
                ul. Chabrowa (od Fiołkowej)
            </p>
        </div>
        "#;

        let items = parse_aquanet_list_html(html).unwrap();
        assert!(!items.is_empty());
        assert_eq!(items[0].slug, "chabrowa-fiołkowa-planowane");
        assert_eq!(items[0].title, "Planowe wyłączenie wody");
        assert_eq!(items[0].location, Some("ul. Chabrowa (od Fiołkowej)".to_string()));
        assert!(!items[0].is_emergency);
    }

    #[tokio::test]
    async fn test_fetch_aquanet_list_real() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client().unwrap();
        match fetch_aquanet_list(&client).await {
            Ok(items) => {
                println!("Fetched {} Aquanet items", items.len());
                for item in &items {
                    println!("  - {} (slug: {})", item.title, item.slug);
                }
            }
            Err(e) => {
                println!("Skipping Aquanet integration test (fetch failed): {}", e);
            }
        }
    }

    #[test]
    fn test_clean_aquanet_description() {
        let raw = "Suchy Las, Chabrowa (od Fiołkowej) Załączniki: pdf Suchy Las, Chabrowa (od Fiołkowej) pdf , 221.92 KB Data ostatniej aktualizacji: 20.05.2026, 11:28";
        assert_eq!(clean_aquanet_description(raw), "Suchy Las, Chabrowa (od Fiołkowej)");
        
        let raw2 = "ul. Poznańska. Data ostatniej aktualizacji: 19.05.2026";
        assert_eq!(clean_aquanet_description(raw2), "ul. Poznańska.");
    }

    #[test]
    fn test_to_unified_formatting() {
        // Case 1: Location with city and street + description
        let item1 = AquanetItem {
            title: "Awaria wodociągowa".to_string(),
            slug: "test-awaria-1".to_string(),
            start_date: Some("2026-05-20T08:00:00".to_string()),
            end_date: None,
            location: Some("Poznań, ul. Bratumiły".to_string()),
            description: Some("Suchy Las, Chabrowa (od Fiołkowej) Załączniki: pdf Suchy Las, Chabrowa pdf, 221 KB".to_string()),
            is_emergency: true,
            ..Default::default()
        };
        let alert1 = item1.to_unified();
        assert_eq!(alert1.message, Some("Awaria wodociągowa - ul. Bratumiły - Suchy Las, Chabrowa (od Fiołkowej)".to_string()));
        assert_eq!(alert1.description, Some("Miejscowość: Poznań".to_string()));

        // Case 2: Location with commune only, no street, no description
        let item2 = AquanetItem {
            title: "Planowane wyłączenie".to_string(),
            slug: "test-planowe-1".to_string(),
            start_date: Some("2026-05-20T08:00:00".to_string()),
            end_date: None,
            location: Some("Swarzędz".to_string()),
            description: None,
            is_emergency: false,
            ..Default::default()
        };
        let alert2 = item2.to_unified();
        assert_eq!(alert2.message, Some("Planowane wyłączenie".to_string()));
        assert_eq!(alert2.description, Some("Miejscowość: Swarzędz".to_string()));

        // Case 3: Location with street only, fallback to Poznań, no description
        let item3 = AquanetItem {
            title: "Awaria wodociągowa".to_string(),
            slug: "test-awaria-2".to_string(),
            start_date: Some("2026-05-20T08:00:00".to_string()),
            end_date: None,
            location: Some("ul. Testowa 10".to_string()),
            description: None,
            is_emergency: true,
            ..Default::default()
        };
        let alert3 = item3.to_unified();
        assert_eq!(alert3.message, Some("Awaria wodociągowa - ul. Testowa 10".to_string()));
        assert_eq!(alert3.description, Some("Miejscowość: Poznań".to_string()));
    }

    #[test]
    fn test_parse_aquanet_detail_html() {
        let html = r#"
        <html>
        <body>
            <div class="accident-header">
                <h1 class="accident-header__title">Suchy Las</h1>
                <div class="accident-header__info-bar">ul. Chabrowa (od Fiołkowej)</div>
            </div>
            <div class="accident-map-box">
                <span class="accident-map-box__content-value">Planowe wyłączenie wody</span>
            </div>
            <div class="accident-content__text-wyswig">
                W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody.
            </div>
            <div class="accident-content__row accident-content__row--impediments">
                Utrudnienia: Brak wody
            </div>
        </body>
        </html>
        "#;

        let document = Html::parse_document(html);
        
        let city_sel = Selector::parse(".accident-header__title, .accident-header__commune").unwrap();
        let city = document.select(&city_sel).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
        });
        assert_eq!(city, Some("Suchy Las".to_string()));

        let streets_sel = Selector::parse(".accident-header__info-bar, .accident-header__streets").unwrap();
        let streets = document.select(&streets_sel).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
        });
        assert_eq!(streets, Some("ul. Chabrowa (od Fiołkowej)".to_string()));

        let title_sel = Selector::parse(".accident-map-box__content-value, .accident-map-box__content_value").unwrap();
        let title = document.select(&title_sel).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
        });
        assert_eq!(title, Some("Planowe wyłączenie wody".to_string()));

        let desc_sel = Selector::parse(".accident-content__text-wyswig").unwrap();
        let description = document.select(&desc_sel).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
        });
        assert_eq!(description, Some("W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody.".to_string()));

        let imp_sel = Selector::parse(".accident-content__row--impediments, .accident-content__row").unwrap();
        let impediments = document.select(&imp_sel).filter(|el| {
            let class_attr = el.value().attr("class").unwrap_or("");
            if class_attr.contains("accident-content__row--impediments") {
                return true;
            }
            let text = el.text().collect::<Vec<_>>().join(" ").to_lowercase();
            text.contains("utrudnienia")
        }).next().map(|el| {
            el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
        });
        assert_eq!(impediments, Some("Utrudnienia: Brak wody".to_string()));
    }

    #[test]
    fn test_to_unified_with_rich_details() {
        let item = AquanetItem {
            title: "Planowe wyłączenie wody".to_string(),
            slug: "chabrowa-fiołkowa-planowane".to_string(),
            start_date: Some("2026-05-21T07:30:00".to_string()),
            end_date: Some("2026-05-21T15:00:00".to_string()),
            city: Some("Suchy Las".to_string()),
            streets: Some("ul. Chabrowa (od Fiołkowej)".to_string()),
            description: Some("W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody. Informujemy, że godzina włączenia wody jest szacunkowa i może ulec zmianie. Jeśli jest to niezbędne prosimy o przygotowanie zapasów wody.".to_string()),
            impediments: Some("Utrudnienia: Brak wody".to_string()),
            is_emergency: false,
            ..Default::default()
        };

        let alert = item.to_unified();
        assert_eq!(alert.description, Some("Miejscowość: Suchy Las".to_string()));
        assert_eq!(
            alert.message,
            Some("Planowe wyłączenie wody - ul. Chabrowa (od Fiołkowej) - W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody. Informujemy, że godzina włączenia wody jest szacunkowa i może ulec zmianie. Jeśli jest to niezbędne prosimy o przygotowanie zapasów wody. Utrudnienia: Brak wody".to_string())
        );
    }

    #[test]
    fn test_to_unified_with_duplicated_impediments() {
        let item = AquanetItem {
            title: "Planowe wyłączenie wody".to_string(),
            slug: "chabrowa-fiołkowa-planowane".to_string(),
            start_date: Some("2026-05-21T07:30:00".to_string()),
            end_date: Some("2026-05-21T15:00:00".to_string()),
            city: Some("Suchy Las".to_string()),
            streets: Some("ul. Chabrowa (od Fiołkowej)".to_string()),
            description: Some("W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody. Utrudnienia: Brak wody".to_string()),
            impediments: Some("Utrudnienia: Brak wody".to_string()),
            is_emergency: false,
            ..Default::default()
        };

        let alert = item.to_unified();
        assert_eq!(alert.description, Some("Miejscowość: Suchy Las".to_string()));
        assert_eq!(
            alert.message,
            Some("Planowe wyłączenie wody - ul. Chabrowa (od Fiołkowej) - W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody. Utrudnienia: Brak wody".to_string())
        );
    }

    #[test]
    fn test_is_date_like_robust() {
        assert!(is_date_like("od 21.05.2026 07:30"));
        assert!(is_date_like("do 21.05.2026 15:00"));
        assert!(is_date_like("godz. 08:00"));
        assert!(is_date_like("20.05.2026"));
        assert!(is_date_like("21 maja 2026"));
        assert!(is_date_like("15:30"));
        assert!(is_date_like("rok 2026"));
        
        assert!(!is_date_like("Planowe wyłączenie wody"));
        assert!(!is_date_like("Awaria wodociągowa"));
    }

    #[test]
    fn test_is_location_or_street_robust() {
        assert!(is_location_or_street("Suchy Las"));
        assert!(is_location_or_street("Poznań"));
        assert!(is_location_or_street("poznan"));
        assert!(is_location_or_street("ul. Chabrowa"));
        assert!(is_location_or_street("al. Niepodległości"));
        assert!(is_location_or_street("os. Chrobrego"));
        
        assert!(!is_location_or_street("Planowe wyłączenie wody"));
        assert!(!is_location_or_street("Awaria wodociągowa"));
    }

    #[test]
    fn test_parse_detail_html_with_date_and_location_filtering() {
        let html = r#"
        <html>
        <body>
            <div class="accident-header">
                <h1 class="accident-header__title">Suchy Las</h1>
                <div class="accident-header__info-bar">ul. Chabrowa (od Fiołkowej)</div>
            </div>
            <!-- Potential title polluters -->
            <h1>Suchy Las</h1>
            <div class="accident-detail__title">Planowe wyłączenie wody</div>
            <div class="accident-content__text-wyswig">
                W związku z pracami inwestycyjnymi na sieci wodociągowej nastąpi przerwa w dostawie wody.
            </div>
        </body>
        </html>
        "#;

        let detail = parse_aquanet_detail_html(html);
        assert_eq!(detail.title, Some("Planowe wyłączenie wody".to_string()));
        assert_eq!(detail.city, Some("Suchy Las".to_string()));
        assert_eq!(detail.streets, Some("ul. Chabrowa (od Fiołkowej)".to_string()));
    }

    #[test]
    fn test_aquanet_matching_logic() {
        let address = AddressEntry {
            name: "Home".to_string(),
            city_name: "Suchy Las".to_string(),
            street_name_1: "Chabrowa".to_string(),
            street_name_2: Some("ul.".to_string()),
            is_active: true,
            ..Default::default()
        };

        let item = AquanetItem {
            title: "Planowe wyłączenie wody".to_string(),
            slug: "suchy-las-chabrowa".to_string(),
            start_date: Some("2026-05-21T07:30:00".to_string()),
            end_date: Some("2026-05-21T15:00:00".to_string()),
            location: Some("Suchy Las".to_string()),
            description: Some("W związku z pracami inwestycyjnymi...".to_string()),
            is_emergency: false,
            city: Some("Suchy Las".to_string()),
            streets: Some("ul. Chabrowa (od Fiołkowej)".to_string()),
            impediments: Some("Brak wody".to_string()),
        };

        // Old combined_text logic (failed to match)
        let combined_text_old = format!(
            "{} {} {}",
            item.title,
            item.location.as_deref().unwrap_or(""),
            item.description.as_deref().unwrap_or("")
        );
        let compiled = CompiledAquanetRegex::new(&address);
        assert!(!compiled.is_match(&combined_text_old));

        // New combined_text logic (successfully matches)
        let combined_text_new = format!(
            "{} {} {} {} {}",
            item.title,
            item.location.as_deref().unwrap_or(""),
            item.description.as_deref().unwrap_or(""),
            item.city.as_deref().unwrap_or(""),
            item.streets.as_deref().unwrap_or("")
        );
        assert!(compiled.is_match(&combined_text_new));
    }
}

