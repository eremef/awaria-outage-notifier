use crate::api_logic::{AddressEntry, AlertSource, UnifiedAlert};
use crate::utils::build_client;
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
    let client = build_client()?;
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
        let mut addr_parts = Vec::new();
        for addr in &self.addresses {
            let street = addr.streetName.as_deref().unwrap_or("?");
            let nums = addr.houseNumbers.as_deref().unwrap_or("");
            let part = format!("{} {}", street, nums).trim().to_string();
            if !part.is_empty() {
                addr_parts.push(part);
            }
        }

        let base_msg = self
            .comment
            .clone()
            .unwrap_or_else(|| "Planowane wyłączenie prądu".to_string());
        let full_msg = if addr_parts.is_empty() {
            base_msg
        } else {
            format!("{}. Adresy: {}", base_msg.trim_end_matches('.'), addr_parts.join(", "))
        };

        UnifiedAlert {
            source: AlertSource::Stoen,
            startDate: Some(self.outageStart.clone()),
            endDate: Some(self.outageEnd.clone()),
            message: Some(full_msg),
            description: None,
            address_index: None,
            is_local: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stoen_matches_address() {
        let addr = AddressEntry {
            name: "Home".to_string(),
            city_name: "Warszawa".to_string(),
            voivodeship: "".to_string(),
            district: "".to_string(),
            commune: "".to_string(),
            street_name: "ul. Grzybowska".to_string(),
            street_name_1: "Grzybowska".to_string(),
            street_name_2: None,
            house_no: "10".to_string(),
            city_id: Some(918123),
            street_id: None,
        };

        let outage = StoenOutage {
            id: 1,
            outageStart: "2026-03-31 10:00:00".to_string(),
            outageEnd: "2026-03-31 14:00:00".to_string(),
            comment: None,
            addresses: vec![StoenAddress {
                streetName: Some("ul. Grzybowska".to_string()),
                houseNumbers: Some("1, 2, 10, 15".to_string()),
            }],
        };

        assert!(matches_address(&outage, &addr));

        let mut addr_wrong = addr.clone();
        addr_wrong.street_name_1 = "Marszałkowska".to_string();
        assert!(!matches_address(&outage, &addr_wrong));
        
        let mut addr_wrocl = addr.clone();
        addr_wrocl.city_name = "Wrocław".to_string();
        addr_wrocl.city_id = Some(969400);
        assert!(!matches_address(&outage, &addr_wrocl));
    }

    #[tokio::test]
    async fn test_fetch_stoen_real() {
        match fetch_stoen_outages().await {
            Ok(outages) => {
                println!("Fetched {} STOEN outages", outages.len());
                assert!(!outages.is_empty());
            }
            Err(e) => {
                println!("Skipping STOEN integration test (API failed): {}", e);
            }
        }
    }
}
