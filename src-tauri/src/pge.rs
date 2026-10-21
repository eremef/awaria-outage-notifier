use reqwest::Client;
use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert, AlertProvider, Settings};
use crate::utils::retry_with_delay;

use chrono::{Duration, Utc};
use chrono_tz::Europe::Warsaw;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct PgeTeryt {
    pub voivodeshipName: Option<String>,
    pub countyName: Option<String>,
    pub communeName: Option<String>,
    pub cityName: Option<String>,
    pub streetName: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PgeAddress {
    pub teryt: Option<PgeTeryt>,
    pub numbers: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct PgeOutage {
    pub id: i64,
    pub startAt: String,
    pub stopAt: String,
    pub description: Option<String>,
    pub regionName: Option<String>,
    pub revoked: Option<bool>,
    pub addresses: Vec<PgeAddress>,
}

pub const PGE_URL: &str = "https://power-outage.gkpge.pl/api/power-outage";

fn get_pge_url() -> String {
    #[cfg(test)]
    {
        std::env::var("PGE_BASE_URL").unwrap_or_else(|_| PGE_URL.to_string())
    }
    #[cfg(not(test))]
    {
        PGE_URL.to_string()
    }
}

pub async fn fetch_pge_outages(client: &Client) -> Result<Vec<PgeOutage>, String> {
    fetch_pge_outages_with_retry(client, &get_pge_url()).await
}

async fn fetch_pge_outages_with_retry(client: &Client, base_url: &str) -> Result<Vec<PgeOutage>, String> {
    // PGE intermittently returns a plain-text 404 from the same working endpoint.
    // Space attempts out, but still report persistent failures to the caller.
    retry_with_delay(
        || fetch_pge_outages_once(client, base_url),
        5,
        std::time::Duration::from_secs(1),
    ).await
}

async fn fetch_pge_outages_once(client: &Client, base_url: &str) -> Result<Vec<PgeOutage>, String> {
    let now = Utc::now().with_timezone(&Warsaw);
    let start_at_to = (now + Duration::days(30)).format("%Y-%m-%d %H:%M:%S").to_string();
    let stop_at_from = now.format("%Y-%m-%d %H:%M:%S").to_string();

    let url = format!(
        "{}?startAtTo={}&stopAtFrom={}",
        base_url,
        start_at_to.replace(' ', "+").replace(':', "%3A"),
        stop_at_from.replace(' ', "+").replace(':', "%3A")
    );

    log::info!("PGE API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        // Let each retry reach a fresh upstream connection instead of reusing
        // a connection that may be pinned to a failing PGE backend.
        .header("connection", "close")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("PGE HTTP error: {}", res.status()));
    }

    let mut data: Vec<PgeOutage> = res.json().await.map_err(|e| e.to_string())?;
    data.retain(|outage| outage.revoked != Some(true));
    Ok(data)
}

pub struct PgeProvider;

pub struct PgePreparedAddress<'a> {
    pub original: &'a AddressEntry,
    pub index: usize,
    pub city_name_lower: String,
    pub voivodeship_upper: String,
    pub district_lower: String,
    pub commune_lower: String,
    pub street_name_candidates_lower: Vec<String>,
}

impl<'a> PgePreparedAddress<'a> {
    pub fn new(address: &'a AddressEntry, index: usize) -> Self {
        Self {
            original: address,
            index,
            city_name_lower: address.city_name.to_lowercase(),
            voivodeship_upper: address.voivodeship.to_uppercase(),
            district_lower: address.district.to_lowercase(),
            commune_lower: address.commune.to_lowercase(),
            street_name_candidates_lower: if address.street_name_1.is_empty() {
                Vec::new()
            } else {
                crate::utils::configured_street_name_candidates(
                    &address.street_name,
                    &address.street_name_1,
                    address.street_name_2.as_deref(),
                )
                .into_iter()
                .map(|candidate| candidate.to_lowercase())
                .collect()
            },
        }
    }
}

pub fn matches_prepared_address(
    outage: &PgeOutage,
    prep: &PgePreparedAddress,
) -> bool {
    let outage_desc_lower = outage.description.as_ref().map(|d| d.to_lowercase());
    let city_match_desc = outage_desc_lower.as_ref()
        .map(|d| d.contains(&prep.city_name_lower))
        .unwrap_or(false);

    for addr in &outage.addresses {
        if let Some(teryt) = &addr.teryt {
            if let Some(v) = &teryt.voivodeshipName
                && v.to_uppercase() != prep.voivodeship_upper {
                    continue;
                }
            if let Some(c) = &teryt.countyName {
                let c_lower = c.to_lowercase();
                if !c_lower.contains(&prep.district_lower) && !prep.district_lower.contains(&c_lower) {
                    continue;
                }
            }
            if let Some(c) = &teryt.communeName {
                let c_lower = c.to_lowercase();
                if !c_lower.contains(&prep.commune_lower) && !prep.commune_lower.contains(&c_lower) {
                    continue;
                }
            }
            
            let city_match = teryt.cityName.as_ref().map(|c| c.to_lowercase() == prep.city_name_lower).unwrap_or(false);
            if !city_match {
                continue;
            }
            
            // Check street. PGE streetName includes "ul. " etc.
            let street_match = if prep.street_name_candidates_lower.is_empty() {
                true 
            } else {
                teryt.streetName.as_ref().is_some_and(|street| {
                    let street = street.to_lowercase();
                    prep.street_name_candidates_lower
                        .iter()
                        .any(|candidate| street.contains(candidate))
                })
            };

            if street_match {
                return true;
            }
        } else if city_match_desc {
            // Fallback to description match if TERYT is missing but city matches
            if prep.street_name_candidates_lower.is_empty() {
                return true;
            }
            let street_match = outage_desc_lower.as_ref()
                .map(|description| {
                    prep.street_name_candidates_lower
                        .iter()
                        .any(|candidate| description.contains(candidate))
                })
                .unwrap_or(false);
            if street_match {
                return true;
            }
        }
    }
    false
}

#[async_trait]
impl AlertProvider for PgeProvider {
    fn id(&self) -> String {
        "pge".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Pge
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        match fetch_pge_outages(client).await {
            Ok(outages) => {
                let mut alerts = Vec::new();
                
                // Pre-build prepared addresses once to avoid hundreds of thousands of heap allocations in the nested filter loops
                let prepared_addresses: Vec<PgePreparedAddress> = settings.addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.is_active && crate::api_logic::is_address_applicable_for_provider(&AlertSource::Pge, a))
                    .map(|(idx, addr)| PgePreparedAddress::new(addr, idx))
                    .collect();

                for prep in &prepared_addresses {
                    let local_outages: Vec<UnifiedAlert> = outages
                        .iter()
                        .filter(|po| matches_prepared_address(po, prep))
                        .map(|po| {
                            let mut alert = po.to_unified();
                            alert.address_index = Some(prep.index);
                            alert.is_local = Some(true);
                            alert.location = Some(format!("Miejscowość: {}", prep.original.city_name));
                            alert
                        })
                        .collect();
                    alerts.extend(local_outages);
                }
                (alerts, Vec::new())
            }
            Err(e) => (Vec::new(), vec![format!("PGE: {}", e)]),
        }
    }
}

#[allow(dead_code)]
pub fn matches_address(
    outage: &PgeOutage,
    address: &AddressEntry,
) -> bool {
    let prep = PgePreparedAddress::new(address, 0);
    matches_prepared_address(outage, &prep)
}

impl PgeOutage {
    pub fn to_unified(&self) -> UnifiedAlert {
        let mut addr_parts = Vec::new();
        for addr in &self.addresses {
            let mut s = String::new();
            if let Some(teryt) = &addr.teryt {
                if let Some(city) = &teryt.cityName {
                    s.push_str(city);
                }
                if let Some(st) = &teryt.streetName {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(st);
                }
            }
            if let Some(nums) = &addr.numbers {
                if !s.is_empty() {
                    s.push_str(", Numery: ");
                }
                s.push_str(nums);
            }
            if !s.is_empty() {
                addr_parts.push(s);
            }
        }

        let address_summary = if !addr_parts.is_empty() {
            addr_parts.join("; ")
        } else {
            String::new()
        };

        let location = if !address_summary.is_empty() {
            if let Some(region) = &self.regionName {
                format!("{} ({})", address_summary, region)
            } else {
                address_summary
            }
        } else {
            self.regionName.clone().unwrap_or_default()
        };

        UnifiedAlert {
            source: AlertSource::Pge,
            startDate: Some(self.startAt.clone()),
            endDate: Some(self.stopAt.clone()),
            message: Some(location),
            location: self.description.clone().or_else(|| self.regionName.clone()),
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pge_to_unified_formatting() {
        let outage = PgeOutage {
            id: 1,
            startAt: "2026-03-31 10:00:00".to_string(),
            stopAt: "2026-03-31 14:00:00".to_string(),
            description: Some("Planned maintenance".to_string()),
            regionName: Some("Rejon Gliwice".to_string()),
            revoked: None,
            addresses: vec![
                PgeAddress {
                    teryt: Some(PgeTeryt {
                        voivodeshipName: None,
                        countyName: None,
                        communeName: None,
                        cityName: None,
                        streetName: Some("ul. Wiejska".to_string()),
                    }),
                    numbers: Some("1, 2, 3".to_string()),
                },
                PgeAddress {
                    teryt: Some(PgeTeryt {
                        voivodeshipName: None,
                        countyName: None,
                        communeName: None,
                        cityName: None,
                        streetName: Some("ul. Polna".to_string()),
                    }),
                    numbers: Some("10-20".to_string()),
                },
            ],
        };

        let unified = outage.to_unified();
        assert_eq!(unified.source, AlertSource::Pge);
        assert_eq!(
            unified.message,
            Some("ul. Wiejska, Numery: 1, 2, 3; ul. Polna, Numery: 10-20 (Rejon Gliwice)".to_string())
        );
        assert_eq!(unified.location, Some("Planned maintenance".to_string()));
    }

    #[test]
    fn test_pge_matches_address() {
        let addr = AddressEntry {
            name: "Home".to_string(),
            city_name: "Wrocław".to_string(),
            voivodeship: "DOLNOŚLĄSKIE".to_string(),
            district: "Wrocław".to_string(),
            commune: "Wrocław".to_string(),
            street_name: "ul. Kuźnicza".to_string(),
            street_name_1: "Kuźnicza".to_string(),
            street_name_2: None,
            house_no: "25".to_string(),
            city_id: Some(969400),
            street_id: Some(13900),
            is_active: true,
        };

        let outage = PgeOutage {
            id: 1,
            startAt: "2026-03-31 10:00:00".to_string(),
            stopAt: "2026-03-31 14:00:00".to_string(),
            description: Some("Kuźnicza 12-25".to_string()),
            regionName: None,
            revoked: None,
            addresses: vec![PgeAddress {
                teryt: Some(PgeTeryt {
                    voivodeshipName: Some("DOLNOŚLĄSKIE".to_string()),
                    countyName: Some("Wrocław".to_string()),
                    communeName: Some("Wrocław".to_string()),
                    cityName: Some("Wrocław".to_string()),
                    streetName: Some("ul. Kuźnicza".to_string()),
                }),
                numbers: Some("12-25".to_string()),
            }],
        };

        assert!(matches_address(&outage, &addr));

        let mut addr_wrong = addr.clone();
        addr_wrong.city_name = "Warszawa".to_string();
        assert!(!matches_address(&outage, &addr_wrong));
    }

    #[test]
    fn test_pge_revoked_exclusion() {
        let mut data = vec![
            PgeOutage {
                id: 1,
                startAt: "2026-03-31 10:00:00".to_string(),
                stopAt: "2026-03-31 14:00:00".to_string(),
                description: Some("Planned maintenance 1".to_string()),
                regionName: Some("Rejon Gliwice".to_string()),
                revoked: Some(false),
                addresses: vec![],
            },
            PgeOutage {
                id: 2,
                startAt: "2026-03-31 10:00:00".to_string(),
                stopAt: "2026-03-31 14:00:00".to_string(),
                description: Some("Revoked maintenance".to_string()),
                regionName: Some("Rejon Gliwice".to_string()),
                revoked: Some(true),
                addresses: vec![],
            },
            PgeOutage {
                id: 3,
                startAt: "2026-03-31 10:00:00".to_string(),
                stopAt: "2026-03-31 14:00:00".to_string(),
                description: Some("Planned maintenance 2".to_string()),
                regionName: Some("Rejon Gliwice".to_string()),
                revoked: None,
                addresses: vec![],
            },
        ];

        data.retain(|outage| outage.revoked != Some(true));

        assert_eq!(data.len(), 2);
        assert_eq!(data[0].id, 1);
        assert_eq!(data[2-1].id, 3);
    }

    #[tokio::test]
    async fn test_fetch_pge_recovers_after_404() {
        let mut server = mockito::Server::new_async().await;
        let unavailable = server.mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(404)
            .with_body("404 page not found\n")
            .expect(3)
            .create_async().await;
        let available = server.mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":1,"startAt":"2026-10-16 10:00:00","stopAt":"2026-10-16 14:00:00","addresses":[]}]"#)
            .expect(1)
            .create_async().await;

        let outages = fetch_pge_outages_with_retry(
            &Client::new(), &format!("{}/api/power-outage", server.url()),
        ).await.expect("should recover after a transient 404");

        assert_eq!(outages.len(), 1);
        assert_eq!(outages[0].id, 1);
        unavailable.assert_async().await;
        available.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_pge_reports_persistent_404() {
        let mut server = mockito::Server::new_async().await;
        let unavailable = server.mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(404)
            .expect(5)
            .create_async().await;

        let error = fetch_pge_outages_with_retry(
            &Client::new(), &format!("{}/api/power-outage", server.url()),
        ).await.unwrap_err();

        assert_eq!(error, "PGE HTTP error: 404 Not Found");
        unavailable.assert_async().await;
    }

    #[tokio::test]
    #[ignore = "requires the live PGE API"]
    async fn test_fetch_pge_real() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client().unwrap();
        let outages = fetch_pge_outages(&client).await.expect("PGE live API failed after retries");
        println!("Fetched {} PGE outages", outages.len());
    }
}
