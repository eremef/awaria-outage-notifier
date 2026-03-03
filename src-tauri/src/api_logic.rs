use serde::{Deserialize, Serialize};

pub const BASE_URL: &str = "https://www.tauron-dystrybucja.pl/waapi";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct GeoItem {
    pub GAID: u64,
    pub Name: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct Settings {
    pub cityName: String,
    pub streetName: String,
    pub houseNo: String,
    pub cityGAID: u64,
    pub streetGAID: u64,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
}

pub fn get_cities_query(city_name: &str, cache_bust: &str) -> Vec<(&'static str, String)> {
    vec![
        ("partName", city_name.to_string()),
        ("_", cache_bust.to_string()),
    ]
}

pub fn get_streets_query(street_name: &str, city_gaid: u64, cache_bust: &str) -> Vec<(&'static str, String)> {
    vec![
        ("partName", street_name.to_string()),
        ("ownerGAID", city_gaid.to_string()),
        ("_", cache_bust.to_string()),
    ]
}

pub fn get_outages_query(
    city_gaid: u64,
    street_gaid: u64,
    house_no: &str,
    from_date: &str,
    cache_bust: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("cityGAID", city_gaid.to_string()),
        ("streetGAID", street_gaid.to_string()),
        ("houseNo", house_no.to_string()),
        ("fromDate", from_date.to_string()),
        ("getLightingSupport", "false".to_string()),
        ("getServicedSwitchingoff", "true".to_string()),
        ("_", cache_bust.to_string()),
    ]
}

pub fn save_settings_to_path(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_settings_from_path(path: &std::path::Path) -> Result<Option<Settings>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let settings: Settings = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(Some(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            cityName: "Wrocław".to_string(),
            streetName: "Rozbrat".to_string(),
            houseNo: "1".to_string(),
            cityGAID: 123,
            streetGAID: 456,
            theme: Some("dark".to_string()),
            language: Some("pl".to_string()),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_cities_query() {
        let query = get_cities_query("Wro", "12345");
        assert_eq!(query.len(), 2);
        assert_eq!(query[0], ("partName", "Wro".to_string()));
        assert_eq!(query[1], ("_", "12345".to_string()));
    }

    #[test]
    fn test_streets_query() {
        let query = get_streets_query("Roz", 123, "12345");
        assert_eq!(query.len(), 3);
        assert_eq!(query[0], ("partName", "Roz".to_string()));
        assert_eq!(query[1], ("ownerGAID", "123".to_string()));
        assert_eq!(query[2], ("_", "12345".to_string()));
    }

    #[test]
    fn test_outages_query() {
        let query = get_outages_query(123, 456, "5", "2024-01-01", "12345");
        assert_eq!(query.len(), 7);
        assert_eq!(query[0], ("cityGAID", "123".to_string()));
        assert_eq!(query[1], ("streetGAID", "456".to_string()));
        assert_eq!(query[2], ("houseNo", "5".to_string()));
    }

    #[test]
    fn test_settings_persistence() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_settings.json");
        
        let settings = Settings {
            cityName: "TestCity".to_string(),
            streetName: "TestStreet".to_string(),
            houseNo: "10".to_string(),
            cityGAID: 111,
            streetGAID: 222,
            theme: Some("light".to_string()),
            language: Some("en".to_string()),
        };

        // Save
        save_settings_to_path(&test_path, &settings).expect("Failed to save settings");
        
        // Load
        let loaded = load_settings_from_path(&test_path).expect("Failed to load settings");
        assert_eq!(Some(settings), loaded);

        // Cleanup
        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_load_non_existent_settings() {
        let test_path = std::path::Path::new("non_existent_settings.json");
        let loaded = load_settings_from_path(test_path).expect("Failed to load settings");
        assert_eq!(None, loaded);
    }

    #[test]
    fn test_load_corrupt_settings() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("corrupt_settings.json");
        std::fs::write(&test_path, "{ invalid json }").unwrap();

        let result = load_settings_from_path(&test_path);
        assert!(result.is_err());
        // Just verify it's an error, exact message can vary

        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_load_legacy_settings_missing_fields() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("legacy_settings.json");
        // Theme is optional, but let's see if we missing other fields how it behaves
        let legacy_json = r#"{
            "cityName": "Legacy",
            "streetName": "Old St",
            "houseNo": "1",
            "cityGAID": 1,
            "streetGAID": 2
        }"#;
        std::fs::write(&test_path, legacy_json).unwrap();

        let loaded = load_settings_from_path(&test_path).expect("Should handle missing optional fields");
        assert!(loaded.is_some());
        let s = loaded.unwrap();
        assert_eq!(s.cityName, "Legacy");
        assert_eq!(s.theme, None); // Should default to None

        std::fs::remove_file(test_path).ok();
    }

    #[test]
    fn test_parse_outage_response() {
        let json = r#"{
            "OutageItems": [
                {
                    "GAID": 100,
                    "Message": "Outage at St.",
                    "StartDate": "2024-01-01T10:00:00",
                    "EndDate": "2024-01-01T12:00:00",
                    "Description": "Testing"
                }
            ]
        }"#;
        let response: OutageResponse = serde_json::from_str(json).expect("Failed to parse OutageResponse");
        let items = response.OutageItems.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].GAID, Some(100));
        assert_eq!(items[0].Message.as_deref(), Some("Outage at St."));
    }

    #[test]
    fn test_parse_incomplete_outage_response() {
        // Test that we handle missing optional fields gracefully
        let json = r#"{
            "OutageItems": [
                {
                    "GAID": 101
                }
            ]
        }"#;
        let response: OutageResponse = serde_json::from_str(json).expect("Failed to parse OutageResponse");
        let items = response.OutageItems.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].GAID, Some(101));
        assert!(items[0].Message.is_none());
    }
}
