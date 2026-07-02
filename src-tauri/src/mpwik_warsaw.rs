use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use crate::mpwik::CompiledMpwikRegex;
use async_trait::async_trait;

use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct MpwikWarszawaItem {
    pub district: String,
    pub street: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub details: Option<String>,
    pub is_emergency: bool,
}

pub fn parse_warszawa_date(s: &str) -> Option<String> {
    let cleaned = s.trim();
    if cleaned.len() >= 16 {
        let date_part = &cleaned[0..10];
        let time_part = &cleaned[11..16];
        Some(format!("{}T{}:00", date_part, time_part))
    } else {
        None
    }
}

pub fn parse_mpwik_warszawa_html(html: &str, is_emergency: bool) -> Result<Vec<MpwikWarszawaItem>, String> {
    use scraper::{Html, Selector};
    let document = Html::parse_document(html);
    let mut items = Vec::new();

    let dist_selector = Selector::parse("div.dzielnica").map_err(|e| e.to_string())?;
    let h3_selector = Selector::parse("h3").map_err(|e| e.to_string())?;
    let tr_selector = Selector::parse("table.awarie tr").map_err(|e| e.to_string())?;
    let td_selector = Selector::parse("td").map_err(|e| e.to_string())?;
    let details_div_selector = Selector::parse("div.zbior").map_err(|e| e.to_string())?;

    for dist_div in document.select(&dist_selector) {
        let mut district = String::new();
        if let Some(h3) = dist_div.select(&h3_selector).next() {
            district = h3.text().collect::<Vec<_>>().join(" ")
                .replace("Warszawa", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_uppercase();
        }

        for tr in dist_div.select(&tr_selector) {
            if tr.value().attr("class").map(|c| c.contains("headrow")).unwrap_or(false) {
                continue;
            }

            let tds: Vec<_> = tr.select(&td_selector).collect();
            if tds.len() < 4 {
                continue;
            }

            let street_name = tds[0].text().collect::<Vec<_>>().join(" ")
                .replace("pokaż na mapie", "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();

            if street_name.is_empty() {
                continue;
            }

            let start_str = tds[1].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let start_date = parse_warszawa_date(&start_str);

            let end_str = tds[2].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let end_date = parse_warszawa_date(&end_str);

            let details = tds[3].select(&details_div_selector).next().map(|div| {
                div.text()
                    .map(|t| t.trim())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            });

            items.push(MpwikWarszawaItem {
                district: district.clone(),
                street: street_name,
                start_date,
                end_date,
                details,
                is_emergency,
            });
        }
    }

    Ok(items)
}

pub async fn fetch_mpwik_warszawa_page(client: &Client, url: &str, is_emergency: bool) -> Result<Vec<MpwikWarszawaItem>, String> {
    let res = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("MPWiK Warszawa HTTP error: {}", res.status()));
    }

    let html = res.text().await.map_err(|e| e.to_string())?;
    parse_mpwik_warszawa_html(&html, is_emergency)
}

pub async fn fetch_mpwik_warszawa_alerts(client: &Client) -> Result<Vec<MpwikWarszawaItem>, String> {
    let planned_url = "https://www.mpwik.com.pl/view/planowane";
    let failure_url = "https://www.mpwik.com.pl/view/awarie";

    let (planned_res, failure_res) = tokio::join!(
        retry(|| fetch_mpwik_warszawa_page(client, planned_url, false), 3),
        retry(|| fetch_mpwik_warszawa_page(client, failure_url, true), 3)
    );

    if planned_res.is_err() && failure_res.is_err() {
        return Err(format!(
            "Both MPWiK Warszawa requests failed. Planned error: {:?}, Failure error: {:?}",
            planned_res.err(),
            failure_res.err()
        ));
    }

    let mut combined = Vec::new();
    match planned_res {
        Ok(items) => combined.extend(items),
        Err(e) => log::error!("Failed to fetch planned Warszawa outages: {}", e),
    }
    match failure_res {
        Ok(items) => combined.extend(items),
        Err(e) => log::error!("Failed to fetch failure Warszawa outages: {}", e),
    }

    Ok(combined)
}

impl MpwikWarszawaItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        let mut parts = Vec::new();
        let type_prefix = if self.is_emergency {
            "Awaria wodociągowa"
        } else {
            "Planowe wyłączenie wody"
        };
        parts.push(type_prefix.to_string());
        parts.push(format!("ul. {}", self.street));
        if let Some(dets) = &self.details {
            if !dets.is_empty() {
                parts.push(format!("posesje: {}", dets));
            }
        }
        let message = parts.join(" - ");

        UnifiedAlert {
            source: AlertSource::MpwikWarszawa,
            startDate: self.start_date.clone(),
            endDate: self.end_date.clone(),
            message: Some(message),
            location: Some(format!("Miejscowość: Warszawa (Dzielnica: {})", self.district)),
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct MpwikWarszawaProvider;

#[async_trait]
impl AlertProvider for MpwikWarszawaProvider {
    fn id(&self) -> String {
        "mpwik_warszawa".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MpwikWarszawa
    }

    async fn fetch(
        &self,
        _client: &Client,
        client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        match fetch_mpwik_warszawa_alerts(client_http1).await {
            Ok(items) => {
                let mut alerts = Vec::new();
                let active_addresses: Vec<(usize, Arc<CompiledMpwikRegex>)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::MpwikWarszawa, a))
                    .map(|(idx, a)| (idx, Arc::new(CompiledMpwikRegex::new(a))))
                    .collect();

                for item in items {
                    let mut local_match_idx = None;
                    for (idx, compiled) in &active_addresses {
                        if compiled.is_match(&item.street) {
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
            Err(e) => (Vec::new(), vec![format!("MPWiK Warszawa: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_warszawa_date() {
        assert_eq!(
            parse_warszawa_date("2026-05-25 16:00"),
            Some("2026-05-25T16:00:00".to_string())
        );
        assert_eq!(parse_warszawa_date("invalid"), None);
    }

    #[test]
    fn test_parse_mpwik_warszawa_html() {
        let html = r#"
            <div class="dzielnica">
                <h3 class="dzielnicaopen">Warszawa MOKOTÓW <i class="fa fa-arrow-down"></i></h3>
                <div class="tablicaawarii">
                <table class="awarie">
                    <tr class="headrow">
                        <th class="a">Adres</th>
                        <th class="p">od</th>
                        <th class="p">do</th>
                        <th class="u">Ulice</th>
                    </tr>
                    <tr>
                        <td headers="a">
                        Zawrat<br>
                        <a href="javascript:;">pokaż na mapie</a>
                        </td>
                        <td headers="p p1">2026-05-25 16:00</td>
                        <td headers="p p2">2026-05-26 00:00</td>
                        <td headers="u">
                            <a href="javascript:;">wyłączone posesje</a>
                            <div class="zbior">
                                Zawrat 18<br>
                                Zawrat 22<br>
                            </div>
                        </td>
                    </tr>
                </table>
                </div>
            </div>
        "#;

        let parsed = parse_mpwik_warszawa_html(html, false).unwrap();
        assert_eq!(parsed.len(), 1);
        let item = &parsed[0];
        assert_eq!(item.district, "MOKOTÓW");
        assert_eq!(item.street, "Zawrat");
        assert_eq!(item.start_date, Some("2026-05-25T16:00:00".to_string()));
        assert_eq!(item.end_date, Some("2026-05-26T00:00:00".to_string()));
        assert_eq!(item.details, Some("Zawrat 18, Zawrat 22".to_string()));
        assert_eq!(item.is_emergency, false);
    }

    #[test]
    fn test_mpwik_warszawa_to_unified() {
        let item = MpwikWarszawaItem {
            district: "MOKOTÓW".to_string(),
            street: "Zawrat".to_string(),
            start_date: Some("2026-05-25T16:00:00".to_string()),
            end_date: Some("2026-05-26T00:00:00".to_string()),
            details: Some("Zawrat 18, Zawrat 22".to_string()),
            is_emergency: false,
        };

        let unified = item.to_unified();
        assert_eq!(unified.source, AlertSource::MpwikWarszawa);
        assert_eq!(
            unified.message,
            Some("Planowe wyłączenie wody - ul. Zawrat - posesje: Zawrat 18, Zawrat 22".to_string())
        );
        assert_eq!(
            unified.location,
            Some("Miejscowość: Warszawa (Dzielnica: MOKOTÓW)".to_string())
        );
    }
}
