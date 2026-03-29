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

pub fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())
}
