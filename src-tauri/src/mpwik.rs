use crate::api_logic::{AlertSource, UnifiedAlert};
use crate::tauron::build_client;
use serde::{Deserialize, Serialize};

pub const MPWIK_URL: &str = "https://www.mpwik.wroc.pl/wp-admin/admin-ajax.php";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MpwikFailureItem {
    pub content: Option<String>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MpwikResponse {
    pub failures: Option<Vec<MpwikFailureItem>>,
}

/// Parse MPWiK date format "DD-MM-YYYY HH:mm" into ISO "YYYY-MM-DDTHH:mm:00".
pub fn parse_mpwik_date(date_str: &str) -> Option<String> {
    let parts: Vec<&str> = date_str.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:00",
        date_parts[2], date_parts[1], date_parts[0], parts[1]
    ))
}

impl MpwikFailureItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        UnifiedAlert {
            source: AlertSource::Water,
            startDate: self.date_start.as_deref().and_then(parse_mpwik_date),
            endDate: self.date_end.as_deref().and_then(parse_mpwik_date),
            message: self.content.clone(),
            description: None,
            address_index: None,
            is_local: None,
        }
    }
}

pub async fn fetch_water_alerts() -> Result<Vec<UnifiedAlert>, String> {
    let client = build_client()?;
    let res = client
        .post(MPWIK_URL)
        .header(
            "content-type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        )
        .header("accept", "application/json")
        .header("x-requested-with", "XMLHttpRequest")
        .header("origin", "https://www.mpwik.wroc.pl")
        .header("referer", "https://www.mpwik.wroc.pl/")
        .body("action=all")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !res.status().is_success() {
        return Err(format!("MPWiK HTTP error: {}", res.status()));
    }

    let data: MpwikResponse = res.json().await.map_err(|e| e.to_string())?;
    let alerts: Vec<UnifiedAlert> = data
        .failures
        .unwrap_or_default()
        .iter()
        .map(|f| f.to_unified())
        .collect();

    Ok(alerts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mpwik_date() {
        let date = "12-03-2026 08:30";
        let parsed = parse_mpwik_date(date);
        assert_eq!(parsed, Some("2026-03-12T08:30:00".to_string()));

        let invalid = "invalid date";
        assert_eq!(parse_mpwik_date(invalid), None);
    }

    #[test]
    fn test_mpwik_to_unified() {
        let item = MpwikFailureItem {
            content: Some("Test water outage".to_string()),
            date_start: Some("12-03-2026 08:30".to_string()),
            date_end: Some("12-03-2026 16:00".to_string()),
        };
        let unified = item.to_unified();
        assert_eq!(unified.source, AlertSource::Water);
        assert_eq!(unified.message, Some("Test water outage".to_string()));
        assert_eq!(unified.startDate, Some("2026-03-12T08:30:00".to_string()));
        assert_eq!(unified.endDate, Some("2026-03-12T16:00:00".to_string()));
    }
}
