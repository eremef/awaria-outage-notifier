use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert};
use crate::utils::build_client;

use chrono::{Duration, Utc};
use chrono_tz::Europe::Warsaw;
use serde::{Deserialize, Serialize};

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
    pub addresses: Vec<PgeAddress>,
}

pub async fn fetch_pge_outages() -> Result<Vec<PgeOutage>, String> {
    let client = build_client()?;
    let now = Utc::now().with_timezone(&Warsaw);
    let start_at_to = (now + Duration::days(90)).format("%Y-%m-%d %H:%M:%S").to_string();
    let stop_at_from = now.format("%Y-%m-%d %H:%M:%S").to_string();

    let url = format!(
        "https://power-outage.gkpge.pl/api/power-outage?startAtTo={}&stopAtFrom={}",
        start_at_to.replace(' ', "+").replace(':', "%3A"),
        stop_at_from.replace(' ', "+").replace(':', "%3A")
    );

    log::info!("PGE API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("PGE HTTP error: {}", res.status()));
    }

    let data: Vec<PgeOutage> = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

pub fn matches_address(
    outage: &PgeOutage,
    address: &AddressEntry,
) -> bool {
    // City name should at least match description if TERYT is missing
    let city_match_desc = outage.description.as_ref().map(|d| d.to_lowercase().contains(&address.city_name.to_lowercase())).unwrap_or(false);

    for addr in &outage.addresses {
        if let Some(teryt) = &addr.teryt {
            let voivodeship_match = teryt.voivodeshipName.as_ref().map(|v| v.to_uppercase() == address.voivodeship.to_uppercase()).unwrap_or(false);
            if !voivodeship_match {
                continue;
            }
            let county_match = teryt.countyName.as_ref().map(|c| c.to_lowercase() == address.district.to_lowercase()).unwrap_or(false);
            if !county_match {
                continue;
            }
            let commune_match = teryt.communeName.as_ref().map(|c| c.to_lowercase() == address.commune.to_lowercase()).unwrap_or(false);
            if !commune_match {
                continue;
            }
            let city_match = teryt.cityName.as_ref().map(|c| c.to_lowercase() == address.city_name.to_lowercase()).unwrap_or(false);
            if !city_match {
                continue;
            }
            
            // Check street. PGE streetName includes "ul. " etc.
            let street_query = if address.street_name_1.is_empty() {
                String::new()
            } else {
                address.street_name_1.to_lowercase()
            };

            let street_match = if street_query.is_empty() {
                true 
            } else {
                teryt.streetName.as_ref().map(|s| s.to_lowercase().contains(&street_query)).unwrap_or(false)
            };

            if street_match {
                return true;
            }
        } else if city_match_desc {
            // Fallback to description match if TERYT is missing but city matches
            if address.street_name_1.is_empty() {
                return true;
            }
            let street_match = outage.description.as_ref().map(|d| d.to_lowercase().contains(&address.street_name_1.to_lowercase())).unwrap_or(false);
            if street_match {
                return true;
            }
        }
    }
    false
}

impl PgeOutage {
    pub fn to_unified(&self) -> UnifiedAlert {
        let mut addr_parts = Vec::new();
        for addr in &self.addresses {
            let mut s = String::new();
            if let Some(teryt) = &addr.teryt {
                if let Some(st) = &teryt.streetName {
                    s.push_str(st);
                }
            }
            if let Some(nums) = &addr.numbers {
                if !s.is_empty() {
                    s.push(' ');
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

        let description = if !address_summary.is_empty() {
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
            message: Some(description),
            description: self.description.clone().or_else(|| self.regionName.clone()),
            address_index: None,
            is_local: None,
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
            Some("ul. Wiejska 1, 2, 3; ul. Polna 10-20 (Rejon Gliwice)".to_string())
        );
        assert_eq!(unified.description, Some("Planned maintenance".to_string()));
    }
}
