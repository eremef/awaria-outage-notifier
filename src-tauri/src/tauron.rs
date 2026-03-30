use crate::api_logic::{AlertSource, UnifiedAlert};
use crate::utils::build_client;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub const BASE_URL: &str = "https://www.tauron-dystrybucja.pl/waapi";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct GeoItem {
    pub GAID: u64,
    pub Name: String,
    pub ProvinceName: Option<String>,
    pub DistrictName: Option<String>,
    pub CommuneName: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct OutageItem {
    pub GAID: Option<u64>,
    pub Message: Option<String>,
    pub StartDate: Option<String>,
    pub EndDate: Option<String>,
    pub Description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct OutageResponse {
    pub OutageItems: Option<Vec<OutageItem>>,
    pub debug_query: Option<String>,
}

impl OutageItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Tauron,
            startDate: self.StartDate.clone(),
            endDate: self.EndDate.clone(),
            message: self.Message.clone(),
            description: self.Description.clone(),
            address_index: None,
            is_local: None,
        }
    }
}

pub async fn lookup_city(
    city_name: &str,
    voivodeship: &str,
    district: &str,
    commune: &str,
) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let encoded_name = city_name.replace(' ', "%20");
    let url = format!(
        "{}/enum/geo/cities?partName={}&_={}",
        BASE_URL, encoded_name, cache_bust
    );

    log::info!("Tauron API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    let cities: Vec<GeoItem> = res.json().await.map_err(|e| e.to_string())?;

    // Filter by administrative units
    let filtered: Vec<GeoItem> = if voivodeship.is_empty() {
        cities
    } else {
        cities
            .into_iter()
            .filter(|c| {
                let p_match = c
                    .ProvinceName
                    .as_ref()
                    .map(|p| p.to_lowercase() == voivodeship.to_lowercase())
                    .unwrap_or(false);
                let d_match = c
                    .DistrictName
                    .as_ref()
                    .map(|d| d.to_lowercase() == district.to_lowercase())
                    .unwrap_or(false);
                let c_match = c
                    .CommuneName
                    .as_ref()
                    .map(|cm| cm.to_lowercase() == commune.to_lowercase())
                    .unwrap_or(false);
                p_match && d_match && c_match
            })
            .collect()
    };

    Ok(filtered)
}

pub async fn lookup_street(street_name: &str, city_gaid: u64) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let encoded_name = street_name.replace(' ', "%20");
    let url = format!(
        "{}/enum/geo/streets?partName={}&ownerGAID={}&_={}",
        BASE_URL, encoded_name, city_gaid, cache_bust
    );

    log::info!("Tauron API: GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

pub async fn lookup_only_one_street(city_gaid: u64) -> Result<Vec<GeoItem>, String> {
    let client = build_client()?;
    let cache_bust = Utc::now().timestamp_millis().to_string();
    let url = format!(
        "{}/enum/geo/onlyonestreet?ownerGAID={}&_={}",
        BASE_URL, city_gaid, cache_bust
    );

    log::info!("Tauron API (onlyonestreet): GET {}", url);

    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

pub async fn fetch_tauron_outages(address: &crate::api_logic::AddressEntry) -> Result<OutageResponse, String> {
    let street_query = match &address.street_name_2 {
        Some(n2) => format!("{} {}", n2.trim(), address.street_name_1.trim()),
        None => address.street_name_1.clone(),
    };

    log::info!(
        "Tauron: fetching for city='{}' ({}/{}/{}), street='{}'",
        address.city_name,
        address.voivodeship,
        address.district,
        address.commune,
        street_query
    );

    // Look up Tauron GAIDs dynamically from address names
    let cities = lookup_city(
        &address.city_name,
        &address.voivodeship,
        &address.district,
        &address.commune,
    )
    .await?;
    let city = cities
        .into_iter()
        .next()
        .ok_or_else(|| format!("City '{}' not found in Tauron", address.city_name))?;

    log::info!("Tauron: found city '{}' GAID={}", city.Name, city.GAID);

    let streets = if address.street_name_1.is_empty() {
        lookup_only_one_street(city.GAID).await?
    } else {
        lookup_street(&street_query, city.GAID).await?
    };

    if streets.is_empty() {
        return Err(format!(
            "Street '{}' not found in Tauron (no results)",
            street_query
        ));
    }

    for s in &streets {
        log::info!("Tauron: street candidate: '{}' (GAID={})", s.Name, s.GAID);
    }

    let street = streets.into_iter().next().unwrap();

    log::info!(
        "Tauron: found street '{}' GAID={} (queried as '{}')",
        street.Name,
        street.GAID,
        street_query
    );

    let now = Utc::now();
    let from_date = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let cache_bust = now.timestamp_millis().to_string();

    let url = format!(
        "{}/outages/address?cityGAID={}&streetGAID={}&houseNo={}&fromDate={}&getLightingSupport=false&getServicedSwitchingoff=true&_={}",
        BASE_URL, city.GAID, street.GAID, address.house_no.replace(' ', "%20"), from_date.replace(' ', "%20"), cache_bust
    );

    log::info!("Tauron API (outages){}", url);

    let client = build_client()?;
    let res = client
        .get(&url)
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("Referer", "https://www.tauron-dystrybucja.pl/wylaczenia")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("HTTP error! status: {}", res.status()));
    }

    let mut data = res
        .json::<OutageResponse>()
        .await
        .map_err(|e| e.to_string())?;

    data.debug_query = Some(url.clone());

    Ok(data)
}

pub fn matches_address(
    message: &Option<String>,
    city_name: &str,
    street_name_1: &str,
    street_name_2: &Option<String>,
) -> bool {
    let Some(message) = message else {
        return false;
    };

    fn word_match(text: &str, word: &str) -> bool {
        let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
        regex::Regex::new(&pattern)
            .map(|r| r.is_match(text))
            .unwrap_or(false)
    }

    // City must match
    if !word_match(message, city_name) {
        return false;
    }

    if street_name_1.is_empty() {
        return true;
    }

    // Build priority list: compound name first (if nazwa_2 exists), then individual words
    let mut candidates: Vec<String> = Vec::new();
    if let Some(n2) = street_name_2 {
        let compound = format!("{} {}", n2.trim(), street_name_1.trim());
        candidates.push(compound);
    }

    // Add significant individual words (>= 3 chars)
    for word in street_name_1.split_whitespace() {
        if word.len() >= 3 {
            candidates.push(word.to_string());
        }
    }
    if let Some(n2) = street_name_2 {
        for word in n2.split_whitespace() {
            if word.len() >= 3 {
                candidates.push(word.to_string());
            }
        }
    }

    candidates.iter().any(|c| word_match(message, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tauron_to_unified() {
        let item = OutageItem {
            GAID: Some(123),
            Message: Some("Test power outage".to_string()),
            StartDate: Some("2026-03-12T08:30:00".to_string()),
            EndDate: Some("2026-03-12T16:00:00".to_string()),
            Description: Some("Testing".to_string()),
        };
        let unified = item.to_unified();
        assert_eq!(unified.source, AlertSource::Tauron);
        assert_eq!(unified.message, Some("Test power outage".to_string()));
        assert_eq!(unified.description, Some("Testing".to_string()));
    }
}
