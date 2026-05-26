use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use crate::mpwik::CompiledMpwikRegex;
use async_trait::async_trait;
use std::sync::Arc;
use scraper::{Html, Selector};
use chrono::NaiveDateTime;
use regex::Regex;

/// Parse PWiK Kalisz date format (e.g., "26.05.2026", "08:00") into ISO "YYYY-MM-DDTHH:MM:00".
pub fn parse_pwik_kalisz_date(date_str: &str, time_str: &str) -> Option<String> {
    let date_str = date_str.trim();
    let time_str = time_str.trim();
    let full_str = format!("{} {}", date_str, time_str);
    
    if let Ok(dt) = NaiveDateTime::parse_from_str(&full_str, "%d.%m.%Y %H:%M") {
        Some(dt.format("%Y-%m-%dT%H:%M:00").to_string())
    } else {
        None
    }
}

pub fn extract_article_links(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a").unwrap();
    let mut links = Vec::new();
    
    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            if href.contains("/ArticleID/") {
                // Ensure absolute URL
                let mut full_href = href.to_string();
                if href.starts_with("/") {
                    full_href = format!("https://wodociagi-kalisz.pl{}", href);
                } else if !href.starts_with("http") {
                    full_href = format!("https://wodociagi-kalisz.pl/{}", href);
                }
                
                if !links.contains(&full_href) {
                    links.push(full_href);
                }
            }
        }
    }
    links
}

#[derive(Debug, Clone)]
pub struct PwikKaliszArticle {
    pub url: String,
    pub title: String,
    pub date: String,
    pub time_range: String,
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
            return Err(format!("PWiK Kalisz HTTP error {}: {}", res.status(), url));
        }
        res.text().await.map_err(|e| e.to_string())
    }
}

pub async fn fetch_pwik_kalisz_main(client: &Client) -> Result<Vec<String>, String> {
    let url = "https://wodociagi-kalisz.pl/Wy%C5%82%C4%85czenia";
    let html = fetch_html(url, client).await?;
    Ok(extract_article_links(&html))
}

pub async fn fetch_pwik_kalisz_article(client: &Client, url: &str) -> Result<PwikKaliszArticle, String> {
    let html = fetch_html(url, client).await?;
    parse_article_html(url, &html)
}

fn parse_article_html(url: &str, html: &str) -> Result<PwikKaliszArticle, String> {
    let document = Html::parse_document(html);
    
    let mut full_text = String::new();
    
    // First try og:description which often contains the clean summary
    let og_desc_selector = Selector::parse("meta[property=\"og:description\"]").unwrap();
    if let Some(og_desc) = document.select(&og_desc_selector).next() {
        if let Some(content) = og_desc.value().attr("content") {
            full_text = content.to_string();
        }
    }
    
    if full_text.is_empty() {
        let content_selector = Selector::parse(".eds_articleContent").unwrap();
        if let Some(content_el) = document.select(&content_selector).next() {
            full_text = content_el.text().collect::<Vec<_>>().join(" ");
        } else {
            full_text = document.root_element().text().collect::<Vec<_>>().join(" ");
        }
    }
    
    full_text = full_text.replace(['\r', '\n'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    let date_re = Regex::new(r"Termin:\s*(\d{2}\.\d{2}\.\d{4})").unwrap();
    let time_re = Regex::new(r"czas wystąpienia zakłóceń:\s*(\d{2}:\d{2})\s*-\s*(\d{2}:\d{2})").unwrap();
    
    let date = date_re.captures(&full_text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
        
    let mut time_range = String::new();
    if let Some(caps) = time_re.captures(&full_text) {
        time_range = format!("{} - {}", caps.get(1).unwrap().as_str(), caps.get(2).unwrap().as_str());
    }

    Ok(PwikKaliszArticle {
        url: url.to_string(),
        title: "Planowe wyłączenie wody".to_string(),
        date,
        time_range,
        description: full_text,
    })
}

impl PwikKaliszArticle {
    pub fn to_unified(&self) -> UnifiedAlert {
        let mut start_date = None;
        let mut end_date = None;
        
        let times: Vec<&str> = self.time_range.split(" - ").collect();
        if !self.date.is_empty() && times.len() == 2 {
            start_date = parse_pwik_kalisz_date(&self.date, times[0]);
            end_date = parse_pwik_kalisz_date(&self.date, times[1]);
        } else if !self.date.is_empty() {
            start_date = parse_pwik_kalisz_date(&self.date, "00:00");
        }

        let mut parts = Vec::new();
        parts.push(self.title.clone());
        if !self.description.is_empty() {
            parts.push(self.description.clone());
        }
        
        let message = parts.join(" - ");
        
        // Try to extract ArticleID for hash
        let hash = if let Some(idx) = self.url.find("/ArticleID/") {
            let id_str: String = self.url[idx + 11..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if !id_str.is_empty() {
                Some(format!("pwik_kalisz_{}", id_str))
            } else { None }
        } else { None };

        UnifiedAlert {
            source: AlertSource::PwikKalisz,
            startDate: start_date,
            endDate: end_date,
            message: Some(message),
            location: Some("Miejscowość: Kalisz".to_string()),
            address_index: None,
            is_local: None,
            hash,
        }
    }
}

pub struct PwikKaliszProvider;

#[async_trait]
impl AlertProvider for PwikKaliszProvider {
    fn id(&self) -> String {
        "pwik_kalisz".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikKalisz
    }

    async fn fetch(
        &self,
        _client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<(usize, String, Arc<CompiledMpwikRegex>)> = settings
            .addresses
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_active && crate::api_logic::is_kalisz(a))
            .map(|(idx, a)| {
                // Compile regex based on address. Note that for PWiK Kalisz, street could be just the name
                (idx, a.street_name_1.clone(), Arc::new(CompiledMpwikRegex::new(a)))
            })
            .collect();

        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut errors = Vec::new();
        let mut alerts = Vec::new();

        // PWiK Kalisz uses a non-standard TLS cert chain that rustls can't verify.
        // Build a native-tls (SChannel/SecureTransport) client specifically for this provider.
        let native_client = match crate::network_state::NetworkState::build_client_native_tls() {
            Ok(c) => c,
            Err(e) => {
                let err = format!("PWiK Kalisz: failed to build native-tls client: {}", e);
                log::error!("{}", err);
                return (Vec::new(), vec![err]);
            }
        };

        match retry(|| fetch_pwik_kalisz_main(&native_client), 3).await {
            Ok(links) => {
                for link in links {
                    match retry(|| fetch_pwik_kalisz_article(&native_client, &link), 3).await {
                        Ok(article) => {
                            let unified = article.to_unified();
                            
                            // Check against active addresses
                            for (idx, _street_name, compiled) in &active_addresses {
                                // Match against description which contains streets like "ul. Korczak nr 116, 118"
                                let combined_text = format!("{} {} {}", article.title, article.description, "Kalisz").to_lowercase();
                                if compiled.is_match(&combined_text) {
                                    let mut local_alert = unified.clone();
                                    local_alert.address_index = Some(*idx);
                                    local_alert.is_local = Some(true);
                                    alerts.push(local_alert);
                                }
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("PWiK Kalisz article error ({}): {}", link, e);
                            log::error!("{}", err_msg);
                            errors.push(err_msg);
                        }
                    }
                }
            }
            Err(e) => {
                let err_msg = format!("PWiK Kalisz main list error: {}", e);
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
    fn test_parse_pwik_kalisz_date() {
        assert_eq!(
            parse_pwik_kalisz_date("26.05.2026", "08:00"),
            Some("2026-05-26T08:00:00".to_string())
        );
        assert_eq!(
            parse_pwik_kalisz_date("01.12.2026", "15:30"),
            Some("2026-12-01T15:30:00".to_string())
        );
        assert_eq!(parse_pwik_kalisz_date("invalid", "12:00"), None);
    }
    
    #[test]
    fn test_regex_extraction() {
        let text = "Planowe wyłączenie wody 📆Termin: 26.05.2026 ⏱Orientacyjny czas wystąpienia zakłóceń: 08:00 - 10:00 🔹ul. Korczak nr 116, 118";
        let date_re = Regex::new(r"Termin:\s*(\d{2}\.\d{2}\.\d{4})").unwrap();
        let time_re = Regex::new(r"czas wystąpienia zakłóceń:\s*(\d{2}:\d{2})\s*-\s*(\d{2}:\d{2})").unwrap();
        
        assert_eq!(date_re.captures(text).unwrap().get(1).unwrap().as_str(), "26.05.2026");
        
        let time_caps = time_re.captures(text).unwrap();
        assert_eq!(time_caps.get(1).unwrap().as_str(), "08:00");
        assert_eq!(time_caps.get(2).unwrap().as_str(), "10:00");
    }

    #[tokio::test]
    async fn test_pwik_live() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client_native_tls().unwrap();
        match fetch_pwik_kalisz_main(&client).await {
            Ok(links) => {
                println!("Found {} links", links.len());
                for link in links {
                    println!("Fetching {}", link);
                    match fetch_pwik_kalisz_article(&client, &link).await {
                        Ok(article) => {
                            println!("Article: {:#?}", article);
                        }
                        Err(e) => println!("Error fetching article: {}", e),
                    }
                }
            }
            Err(e) => {
                println!("Skipping PWiK Kalisz integration test (API failed): {:?}", e);
            }
        }
    }
}
