use reqwest::Client;
use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry;
use async_trait::async_trait;
use scraper::{Html, Selector};

pub const PSG_URL: &str = "https://www.psgaz.pl/przerwy-w-dostawie-gazu";
pub const PSG_AJAX_URL: &str = "https://www.psgaz.pl/przerwy-w-dostawie-gazu?p_p_id=supplyinterruptions_WAR_supplyinterruptionsportlet&p_p_lifecycle=2&p_p_resource_id=getSupplyInterruptions";

pub struct PsgProvider;

#[async_trait]
impl AlertProvider for PsgProvider {
    fn id(&self) -> String {
        "psg".to_string()
    }

    async fn fetch(
        &self,
        _client: &Client,
        _client_http1: &Client,
        settings: &Settings,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let active_addresses: Vec<_> = settings.addresses.iter().filter(|a| a.is_active).collect();
        if active_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        // We use a custom client with cookie store for PSG because it requires session cookies
        let client: Client = match Client::builder().cookie_store(true).build() {
            Ok(c) => c,
            Err(e) => return (Vec::new(), vec![format!("PSG client error: {}", e)]),
        };

        match retry(|| async {
            // First fetch to establish session
            let _ = client.get(PSG_URL)
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .send()
                .await?;
            
            // Second fetch to get portlet data
            let text = client.get(PSG_AJAX_URL)
                .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .header("X-Requested-With", "XMLHttpRequest")
                .send()
                .await?
                .text()
                .await?;
            
            Ok::<String, reqwest::Error>(text)
        }, 3).await {
            Ok(html) => {
                let alerts = parse_psg_html(&html, settings);
                (alerts, Vec::new())
            }
            Err(e) => (Vec::new(), vec![format!("PSG error: {}", e)]),
        }
    }
}

pub fn parse_psg_html(html_content: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let mut alerts = Vec::new();
    let document = Html::parse_document(html_content);
    
    // The table rows for PSG
    let row_selector = Selector::parse("tr").unwrap();
    let td_selector = Selector::parse("td").unwrap();

    for row in document.select(&row_selector) {
        let cells: Vec<_> = row.select(&td_selector).collect();
        if cells.len() >= 8 {
            let city = cells[1].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let area = cells[2].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let start_date = cells[3].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let end_date = cells[4].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let message = cells[5].text().collect::<Vec<_>>().join(" ").trim().to_string();
            let status = cells[7].text().collect::<Vec<_>>().join(" ").trim().to_string();

            if status.to_lowercase().contains("zakończona") {
                continue; // We only want Active and Planned
            }

            let mut matched_index = None;
            let mut is_local = false;

            for (idx, addr) in settings.addresses.iter().enumerate().filter(|(_, a)| a.is_active) {
                if city.to_lowercase() == addr.city_name.to_lowercase() {
                    if area.to_lowercase().contains(&addr.street_name_1.to_lowercase()) {
                        matched_index = Some(idx);
                        is_local = true;
                        break;
                    }
                }
            }

            if is_local {
                alerts.push(UnifiedAlert {
                    source: AlertSource::Psg,
                    startDate: Some(start_date),
                    endDate: Some(end_date),
                    message: Some(message.clone()),
                    description: Some(format!("Miejscowość: {}, Obszar: {}", city, area)),
                    address_index: matched_index,
                    is_local: Some(true),
                    hash: None,
                });
            }
        }
    }

    alerts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_logic::{AddressEntry, Settings};

    #[tokio::test]
    async fn test_fetch_psg_real() {
        let provider = PsgProvider;
        let settings = Settings {
            addresses: vec![
                AddressEntry {
                    city_name: "Wrocław".to_string(),
                    street_name_1: "Legnicka".to_string(),
                    is_active: true,
                    ..Default::default()
                }
            ],
            ..Default::default()
        };
        
        let client = Client::new();
        let (alerts, errors) = provider.fetch(&client, &client, &settings).await;
        
        println!("Alerts: {:?}", alerts);
        println!("Errors: {:?}", errors);
        
        // Even if there are no outages right now, we should at least check if the fetch succeeded without client errors
        for err in &errors {
            assert!(!err.contains("client error"));
        }
    }
}

