use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct StoenAddress {
    pub streetName: Option<String>,
    pub houseNumbers: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct StoenOutage {
    pub id: i64,
    pub outageStart: String,
    pub outageEnd: String,
    pub addresses: Vec<StoenAddress>,
    pub comment: Option<String>,
}

#[derive(Serialize)]
struct StoenPayloadPage {
    limit: i32,
    offset: i32,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
struct StoenPayload {
    id: Option<i64>,
    area: Option<String>,
    outageStart: Option<String>,
    outageEnd: Option<String>,
    page: StoenPayloadPage,
}

pub async fn fetch_stoen_outages() -> Result<Vec<StoenOutage>, String> {
    let client = crate::tauron::build_client()?;
    let url = "https://awaria.stoen.pl/public/api/planned-outage/search/compressed-report";

    let payload = StoenPayload {
        id: None,
        area: None,
        outageStart: None,
        outageEnd: None,
        page: StoenPayloadPage {
            limit: 9999,
            offset: 0,
        },
    };

    let res = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Referer", "https://awaria.stoen.pl/public/planned?pagelimit=9999")
        .header("Origin", "https://awaria.stoen.pl")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("STOEN HTTP error: {}", res.status()));
    }

    let data: Vec<StoenOutage> = res.json().await.map_err(|e| e.to_string())?;
    Ok(data)
}

pub fn matches_address(outage: &StoenOutage, address: &AddressEntry) -> bool {
    // STOEN is strictly for Warszawa
    let city_lower = address.city_name.to_lowercase();
    let is_warszawa = city_lower == "warszawa" || city_lower == "warsaw" || address.city_id == Some(918123);
    
    if !is_warszawa {
        return false;
    }

    if address.street_name_1.is_empty() {
        return true;
    }

    let street_query = address.street_name_1.to_lowercase();

    for addr in &outage.addresses {
        if let Some(street) = &addr.streetName {
            let street_norm = street.to_lowercase()
                .replace("ul. ", "")
                .replace("al. ", "")
                .replace("pl. ", "")
                .replace("os. ", "")
                .trim()
                .to_string();
            
            if street_norm.contains(&street_query) || street_query.contains(&street_norm) {
                return true;
            }
        }
    }

    false
}

impl StoenOutage {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Stoen,
            startDate: Some(self.outageStart.clone()),
            endDate: Some(self.outageEnd.clone()),
            message: self.comment.clone().or_else(|| Some("Planowane wyłączenie prądu".to_string())),
            description: Some("Obszar: Warszawa".to_string()),
            address_index: None,
            is_local: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_stoen_real() {
        let outages = fetch_stoen_outages().await.unwrap();
        println!("Fetched {} STOEN outages", outages.len());
        for outage in &outages {
            for addr in &outage.addresses {
                if let Some(street) = &addr.streetName {
                    if street.to_lowercase().contains("grzybowska") {
                        println!("Found Grzybowska: {} with numbers {:?}", street, addr.houseNumbers);
                    }
                }
            }
        }
        assert!(!outages.is_empty());
    }
}
