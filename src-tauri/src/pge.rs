use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert};
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
    let client = crate::tauron::build_client()?;
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
        UnifiedAlert {
            source: AlertSource::Pge,
            startDate: Some(self.startAt.clone()),
            endDate: Some(self.stopAt.clone()),
            message: self.description.clone().or_else(|| self.regionName.clone()),
            description: self.regionName.clone(),
            address_index: None,
            is_local: None,
        }
    }
}
