use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use crate::mpwik::CompiledMpwikRegex;
use async_trait::async_trait;
use std::sync::Arc;
use scraper::{Html, Selector};
use chrono::NaiveDateTime;
use regex::Regex;
use futures::future::join_all;

/// Parse date and time into ISO format "YYYY-MM-DDTHH:MM:00"
pub fn parse_puk_rokietnica_date(date_str: &str, time_str: &str) -> Option<String> {
    let date_str = date_str.trim();
    let mut time_str = time_str.trim().replace('.', ":");
    
    // If time is just an hour e.g. "9", format it as "09:00"
    if !time_str.contains(':') {
        if let Ok(hour) = time_str.parse::<u32>() {
            time_str = format!("{:02}:00", hour);
        }
    } else {
        // Ensure 2-digit hour, e.g. "9:00" -> "09:00"
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            if let Ok(hour) = parts[0].parse::<u32>() {
                if let Ok(minute) = parts[1].parse::<u32>() {
                    time_str = format!("{:02}:{:02}", hour, minute);
                }
            }
        }
    }

    let full_str = format!("{} {}", date_str, time_str);
    if let Ok(dt) = NaiveDateTime::parse_from_str(&full_str, "%d.%m.%Y %H:%M") {
        Some(dt.format("%Y-%m-%dT%H:%M:00").to_string())
    } else if let Ok(dt) = NaiveDateTime::parse_from_str(&full_str, "%d-%m-%Y %H:%M") {
        Some(dt.format("%Y-%m-%dT%H:%M:00").to_string())
    } else {
        None
    }
}

pub fn extract_article_links(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(".wpisy .wpis a.wpis_img, .wpisy .wpis .zajawka a.zajawka_tytul").unwrap();
    let mut links = Vec::new();
    
    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            let mut full_href = href.to_string();
            if !href.starts_with("http") {
                if href.starts_with("/") {
                    full_href = format!("https://puk.com.pl{}", href);
                } else {
                    full_href = format!("https://puk.com.pl/{}", href);
                }
            }
            if !links.contains(&full_href) {
                links.push(full_href);
            }
        }
    }
    links
}

#[derive(Debug, Clone)]
pub struct PukRokietnicaArticle {
    pub url: String,
    pub title: String,
    pub pub_date: String,
    pub description: String,
}

pub async fn fetch_html(url: &str, _client: &Client) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        crate::fetch_url_via_android(url).await
    }
    #[cfg(not(target_os = "android"))]
    {
        let res = _client.get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("PUK Rokietnica HTTP error {}: {}", res.status(), url));
        }
        res.text().await.map_err(|e| e.to_string())
    }
}

pub async fn fetch_puk_rokietnica_main(client: &Client) -> Result<Vec<String>, String> {
    let url = "https://puk.com.pl/awarie";
    let html = fetch_html(url, client).await?;
    Ok(extract_article_links(&html))
}

pub async fn fetch_puk_rokietnica_article(client: &Client, url: &str) -> Result<PukRokietnicaArticle, String> {
    let html = fetch_html(url, client).await?;
    parse_article_html(url, &html)
}

fn parse_article_html(url: &str, html: &str) -> Result<PukRokietnicaArticle, String> {
    let document = Html::parse_document(html);
    
    let title_selector = Selector::parse(".tresc_szczegoly strong.font, .tresc_szczegoly h1").unwrap();
    let title = document.select(&title_selector)
        .next()
        .map(|el| el.text().collect::<Vec<_>>().join(" ").trim().to_string())
        .unwrap_or_else(|| "Awaria wody".to_string());

    let date_selector = Selector::parse(".tresc_szczegoly small").unwrap();
    let pub_date = document.select(&date_selector)
        .next()
        .map(|el| {
            let text = el.text().collect::<Vec<_>>().join(" ");
            text.replace("\n", " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_default();
    
    let desc_selector = Selector::parse(".opis_szczegoly").unwrap();
    let description = document.select(&desc_selector)
        .next()
        .map(|el| el.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
        .replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    Ok(PukRokietnicaArticle {
        url: url.to_string(),
        title,
        pub_date,
        description,
    })
}

impl PukRokietnicaArticle {
    pub fn to_unified(&self) -> UnifiedAlert {
        let mut start_date = None;
        let mut end_date = None;
        
        let combined_text = format!("{} {}", self.title, self.description);

        // Find outage date. Format dd.mm.yyyy, e.g. "15.04.2026"
        let date_re = Regex::new(r"(\d{2})\.(\d{2})\.(\d{4})").unwrap();
        let date_match = date_re.captures(&combined_text)
            .map(|caps| caps.get(0).unwrap().as_str().to_string());

        if let Some(date_str) = date_match {
            // Find time ranges, e.g. "godz. od 9.00 -12.00" or "w godzinach 10:00 do 13:00" or "od 9.00 do 12.00"
            // We match hours: "od X to Y"
            let time_re = Regex::new(r"(?:godz|godzinach)?\s*od\s*(\d{1,2}(?:[\.:]\d{2})?)\s*(?:-|do)\s*(\d{1,2}(?:[\.:]\d{2})?)").unwrap();
            if let Some(caps) = time_re.captures(&combined_text) {
                let start_t = caps.get(1).unwrap().as_str();
                let end_t = caps.get(2).unwrap().as_str();
                start_date = parse_puk_rokietnica_date(&date_str, start_t);
                end_date = parse_puk_rokietnica_date(&date_str, end_t);
            } else {
                start_date = parse_puk_rokietnica_date(&date_str, "00:00");
            }
        }

        let message = if !self.description.is_empty() {
            format!("{} - {}", self.title, self.description)
        } else {
            self.title.clone()
        };

        // Determine if it's a planned outage or failure
        let is_planned = combined_text.to_lowercase().contains("planowan");
        let incident_type = if is_planned { "Planowane wyłączenie" } else { "Awaria" };
        let full_message = format!("{} - {}", incident_type, message);

        // Extract city from title or description
        let cities = vec![
            "rokietnica", "bytkowo", "cerekwica", "kiekrz", "krzyszkowo", "mrowino",
            "napachanie", "przybroda", "rostworowo", "rogierowko", "sobota", "starzyny",
            "zydowo", "dalekie"
        ];
        let lower_text = combined_text.to_lowercase();
        let matched_city = cities.iter()
            .find(|&&city| lower_text.contains(city))
            .map(|&city| city[..1].to_uppercase() + &city[1..])
            .unwrap_or_else(|| "Rokietnica".to_string());

        let location = format!("Miejscowość: {}", matched_city);

        // Parse hash using the ws identifier
        let hash = if let Some(idx) = self.url.find("-ws-") {
            let id_str: String = self.url[idx + 4..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id_str.is_empty() {
                Some(format!("puk_rokietnica_{}", id_str))
            } else {
                None
            }
        } else {
            None
        };

        UnifiedAlert {
            source: AlertSource::PukRokietnica,
            startDate: start_date,
            endDate: end_date,
            message: Some(full_message),
            location: Some(location),
            address_index: None,
            is_local: None,
            hash,
        }
    }
}

pub struct PukRokietnicaProvider;

#[async_trait]
impl AlertProvider for PukRokietnicaProvider {
    fn id(&self) -> String {
        "puk_rokietnica".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PukRokietnica
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<(usize, String, Arc<CompiledMpwikRegex>)> = settings
            .addresses
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::PukRokietnica, a) && crate::api_logic::is_rokietnica(a))
            .map(|(idx, a)| {
                (idx, a.street_name_1.clone(), Arc::new(CompiledMpwikRegex::new(a)))
            })
            .collect();

        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        match retry(|| fetch_puk_rokietnica_main(client), 3).await {
            Ok(links) => {
                let mut futures_list = Vec::new();
                for link in links {
                    let client = client.clone();
                    futures_list.push(async move {
                        let article_res: Result<PukRokietnicaArticle, String> = retry(|| fetch_puk_rokietnica_article(&client, &link), 3).await;
                        (link, article_res)
                    });
                }

                let results = join_all(futures_list).await;

                for (link, res) in results {
                    match res {
                        Ok(article) => {
                            let unified = article.to_unified();
                            
                            // Check against active addresses
                            for (idx, _street_name, compiled) in &active_addresses {
                                let combined_text = format!("{} {} {}", article.title, article.description, unified.location.clone().unwrap_or_default()).to_lowercase();
                                if compiled.is_match(&combined_text) {
                                    let mut local_alert = unified.clone();
                                    local_alert.address_index = Some(*idx);
                                    local_alert.is_local = Some(true);
                                    alerts.push(local_alert);
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("PUK Rokietnica article error ({}): {}", link, e);
                            log::error!("{}", err_msg);
                            errors.push(err_msg);
                        }
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("PUK Rokietnica main list error: {}", e);
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
    fn test_parse_puk_rokietnica_date() {
        assert_eq!(
            parse_puk_rokietnica_date("15.04.2026", "9.00"),
            Some("2026-04-15T09:00:00".to_string())
        );
        assert_eq!(
            parse_puk_rokietnica_date("15.04.2026", "12.00"),
            Some("2026-04-15T12:00:00".to_string())
        );
        assert_eq!(
            parse_puk_rokietnica_date("31.03.2026", "10:30"),
            Some("2026-03-31T10:30:00".to_string())
        );
        assert_eq!(
            parse_puk_rokietnica_date("31.03.2026", "14"),
            Some("2026-03-31T14:00:00".to_string())
        );
    }

    #[test]
    fn test_article_parsing_to_unified() {
        let article = PukRokietnicaArticle {
            url: "https://puk.com.pl/UWAGA--PRZERWA-W-DOSTAWIE-WODY-15.04.2026-r.---Rokietnica-ws-2893".to_string(),
            title: "UWAGA !!! PRZERWA W DOSTAWIE WODY 15.04.2026 r. - Rokietnica".to_string(),
            pub_date: "13.04.2026".to_string(),
            description: "informujemy, że w dniu 15.04.2026 r. w godz. od 9.00 -12.00 w miejscowości Rokietnica".to_string(),
        };

        let alert = article.to_unified();
        assert_eq!(alert.startDate, Some("2026-04-15T09:00:00".to_string()));
        assert_eq!(alert.endDate, Some("2026-04-15T12:00:00".to_string()));
        assert_eq!(alert.hash, Some("puk_rokietnica_2893".to_string()));
        assert_eq!(alert.location, Some("Miejscowość: Rokietnica".to_string()));
    }
}
