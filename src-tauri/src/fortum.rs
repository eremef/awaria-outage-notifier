use crate::api_logic::{AlertSource, UnifiedAlert};
use crate::utils::build_client;
use serde::{Deserialize, Serialize};

pub const FORTUM_URL: &str = "https://formularz.fortum.pl/api/v1/switchoffs";
pub const FORTUM_CITIES_URL: &str = "https://formularz.fortum.pl/api/v1/teryt/cities";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FortumCity {
    pub city_guid: String,
    pub city_name: String,
    pub region_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct FortumResponse {
    #[serde(default)]
    pub points: Vec<FortumPoint>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FortumPoint {
    pub switch_off_id: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub message: Option<String>,
}

impl FortumPoint {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Fortum,
            startDate: self.start_date.clone(),
            endDate: self.end_date.clone(),
            message: self.message.clone(),
            description: None,
            address_index: None,
            is_local: None,
        }
    }
}

pub async fn fetch_fortum_cities() -> Result<Vec<FortumCity>, String> {
    let client = build_client()?;
    log::info!("Fortum: GET {}", FORTUM_CITIES_URL);
    let res = client
        .get(FORTUM_CITIES_URL)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("Fortum cities HTTP error: {}", res.status()));
    }

    res.json().await.map_err(|e| e.to_string())
}

pub async fn fetch_fortum_alerts(city_guid: &str, region_id: u32) -> Result<Vec<UnifiedAlert>, String> {
    let client = build_client()?;

    let planned_url = format!(
        "{}?cityGuid={}&regionId={}&current=false",
        FORTUM_URL, city_guid, region_id
    );
    let current_url = format!(
        "{}?cityGuid={}&regionId={}&current=true",
        FORTUM_URL, city_guid, region_id
    );

    log::info!("Fortum API: planned={}, current={}", planned_url, current_url);

    let (planned_res, current_res) = tokio::join!(
        client
            .get(&planned_url)
            .header("accept", "application/json")
            .send(),
        client
            .get(&current_url)
            .header("accept", "application/json")
            .send()
    );

    let planned_data: FortumResponse = planned_res
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let current_data: FortumResponse = current_res
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut seen_ids = std::collections::HashSet::new();
    let mut all_points = planned_data.points;
    all_points.extend(current_data.points);

    let alerts: Vec<UnifiedAlert> = all_points
        .into_iter()
        .filter(|p| seen_ids.insert(p.switch_off_id.clone()))
        .map(|p| p.to_unified())
        .collect();

    Ok(alerts)
}

pub fn matches_street_only(
    message: &Option<String>,
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
    fn test_fortum_matches_street_only() {
        assert!(matches_street_only(
            &Some("Wrocław, ul. Legnicka 10".to_string()),
            "Legnicka",
            &None
        ));

        assert!(matches_street_only(
            &Some("Wrocław, ul. Henryka Probusa 12".to_string()),
            "Probusa",
            &Some("Henryka".to_string())
        ));

        assert!(!matches_street_only(
            &Some("Wrocław, ul. Legnicka 10".to_string()),
            "Probusa",
            &None
        ));
    }

    #[tokio::test]
    async fn test_fetch_fortum_real() {
        // Wrocław GUID
        let test_guid = "9b6e8284-904d-45f1-8316-d98c2536c4b2";
        let test_region = 1421312;
        let alerts = fetch_fortum_alerts(test_guid, test_region).await.unwrap();
        println!("Fetched {} Fortum alerts for Wrocław", alerts.len());
        // Even if empty, we check it doesn't crash
    }
}
