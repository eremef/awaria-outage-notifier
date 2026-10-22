use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use reqwest::{Client, StatusCode};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Timelike, Utc};
use chrono_tz::Europe::Warsaw;
use chrono_tz::Tz;
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::Duration as StdDuration;

const PGE_MAX_ATTEMPTS: usize = 4;
const PGE_RETRY_BACKOFF_MS: [u64; 3] = [250, 500, 1_000];
const PGE_REQUEST_TIMEOUT: StdDuration = StdDuration::from_secs(12);
const PGE_QUERY_DEADLINE: StdDuration = StdDuration::from_secs(30);
const PGE_FETCH_DEADLINE: StdDuration = StdDuration::from_secs(45);
const PGE_MAX_CONCURRENT_REQUESTS: usize = 4;
const PGE_MAX_CITY_SYM_SCOPES: usize = 8;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(non_snake_case)]
pub struct PgeTeryt {
    pub voivodeshipName: Option<String>,
    pub countyName: Option<String>,
    pub communeName: Option<String>,
    pub cityName: Option<String>,
    pub citySym: Option<String>,
    pub streetName: Option<String>,
    pub streetSymUl: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

#[cfg(test)]
pub async fn fetch_pge_outages(client: &Client) -> Result<Vec<PgeOutage>, String> {
    fetch_pge_outages_with_retry(client, &get_pge_url()).await
}

#[cfg(test)]
async fn fetch_pge_outages_with_retry(
    client: &Client,
    base_url: &str,
) -> Result<Vec<PgeOutage>, String> {
    let window = pge_date_window_now();
    fetch_pge_outages_with_retry_for_scope(
        client,
        base_url,
        &window,
        &PgeRequestScope::AllLocations,
    )
    .await
}

#[derive(Debug, Clone)]
struct PgeDateWindow {
    start_at_to: String,
    stop_at_from: String,
}

fn pge_date_window_now() -> PgeDateWindow {
    pge_date_window_at(Utc::now().with_timezone(&Warsaw))
}

fn pge_date_window_at(now: DateTime<Tz>) -> PgeDateWindow {
    let stop_at_from = round_down_to_quarter_hour(now);
    let start_at_to = round_up_to_quarter_hour(now + Duration::days(30));

    PgeDateWindow {
        start_at_to: start_at_to.format("%Y-%m-%d %H:%M:%S").to_string(),
        stop_at_from: stop_at_from.format("%Y-%m-%d %H:%M:%S").to_string(),
    }
}

fn round_down_to_quarter_hour(value: DateTime<Tz>) -> DateTime<Tz> {
    value
        .with_minute(value.minute() / 15 * 15)
        .expect("quarter-hour minute is valid")
        .with_second(0)
        .expect("zero seconds is valid")
        .with_nanosecond(0)
        .expect("zero nanoseconds is valid")
}

fn round_up_to_quarter_hour(value: DateTime<Tz>) -> DateTime<Tz> {
    let rounded_down = round_down_to_quarter_hour(value);
    if rounded_down < value {
        rounded_down + Duration::minutes(15)
    } else {
        rounded_down
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PgeRequestScope {
    CitySym(String),
    StreetSym(String),
    AllLocations,
}

impl PgeRequestScope {
    fn label(&self) -> &str {
        match self {
            Self::CitySym(city_sym) => city_sym,
            Self::StreetSym(street_sym) => street_sym,
            Self::AllLocations => "all locations",
        }
    }
}

fn build_pge_url(
    base_url: &str,
    window: &PgeDateWindow,
    scope: &PgeRequestScope,
) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(base_url).map_err(|error| error.to_string())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("startAtTo", &window.start_at_to)
            .append_pair("stopAtFrom", &window.stop_at_from);
        match scope {
            PgeRequestScope::CitySym(city_sym) => {
                query
                    .append_pair("type", "teryt")
                    .append_pair("citySym", city_sym);
            }
            PgeRequestScope::StreetSym(street_sym) => {
                query
                    .append_pair("type", "teryt")
                    .append_pair("streetSym", street_sym);
            }
            PgeRequestScope::AllLocations => {}
        }
    }
    Ok(url)
}

#[derive(Debug)]
enum PgeFetchError {
    Request(reqwest::Error),
    Http {
        status: StatusCode,
        retry_after: Option<StdDuration>,
    },
    Decode(reqwest::Error),
    Url(String),
}

impl PgeFetchError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Request(_) => true,
            Self::Http { status, .. } => {
                *status == StatusCode::NOT_FOUND
                    || *status == StatusCode::REQUEST_TIMEOUT
                    || *status == StatusCode::TOO_MANY_REQUESTS
                    || status.is_server_error()
            }
            Self::Decode(_) | Self::Url(_) => false,
        }
    }

    fn retry_delay(&self, attempt: usize) -> StdDuration {
        match self {
            Self::Http {
                status: StatusCode::TOO_MANY_REQUESTS,
                retry_after: Some(retry_after),
            } => *retry_after,
            _ => StdDuration::from_millis(PGE_RETRY_BACKOFF_MS[attempt]),
        }
    }
}

impl std::fmt::Display for PgeFetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) => write!(formatter, "PGE request error: {error}"),
            Self::Http { status, .. } => write!(formatter, "PGE HTTP error: {status}"),
            Self::Decode(error) => write!(formatter, "PGE JSON error: {error}"),
            Self::Url(error) => write!(formatter, "PGE URL error: {error}"),
        }
    }
}

async fn fetch_pge_outages_with_retry_for_scope(
    client: &Client,
    base_url: &str,
    window: &PgeDateWindow,
    scope: &PgeRequestScope,
) -> Result<Vec<PgeOutage>, String> {
    let attempts = async {
        for attempt in 0..PGE_MAX_ATTEMPTS {
            match fetch_pge_outages_once(client, base_url, window, scope).await {
                Ok(outages) => return Ok(outages),
                Err(error) if error.is_retryable() && attempt + 1 < PGE_MAX_ATTEMPTS => {
                    let delay = error.retry_delay(attempt);
                    log::warn!(
                        "PGE attempt {}/{} failed for {}: {}; retrying after {} ms",
                        attempt + 1,
                        PGE_MAX_ATTEMPTS,
                        scope.label(),
                        error,
                        delay.as_millis()
                    );
                    tokio::time::sleep(delay).await;
                }
                Err(error) => return Err(error.to_string()),
            }
        }

        unreachable!("the PGE retry loop always returns on its final attempt")
    };

    match tokio::time::timeout(PGE_QUERY_DEADLINE, attempts).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "PGE query timed out after {} seconds for {}",
            PGE_QUERY_DEADLINE.as_secs(),
            scope.label()
        )),
    }
}

async fn fetch_pge_outages_once(
    client: &Client,
    base_url: &str,
    window: &PgeDateWindow,
    scope: &PgeRequestScope,
) -> Result<Vec<PgeOutage>, PgeFetchError> {
    let url = build_pge_url(base_url, window, scope).map_err(PgeFetchError::Url)?;

    log::info!("PGE API: GET {}", url);

    let res = client
        .get(url)
        .timeout(PGE_REQUEST_TIMEOUT)
        .header("accept", "application/json")
        // Let each retry reach a fresh upstream connection instead of reusing
        // a connection that may be pinned to a failing PGE backend.
        .header("connection", "close")
        .send()
        .await
        .map_err(PgeFetchError::Request)?;

    if !res.status().is_success() {
        let status = res.status();
        let retry_after = res
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(StdDuration::from_secs);
        return Err(PgeFetchError::Http {
            status,
            retry_after,
        });
    }

    let mut data: Vec<PgeOutage> = res.json().await.map_err(PgeFetchError::Decode)?;
    data.retain(|outage| outage.revoked != Some(true));
    Ok(data)
}

fn pge_request_scopes(
    addresses: &[PgePreparedAddress<'_>],
    city_scope_ids: &HashMap<u64, Vec<u64>>,
) -> Vec<PgeRequestScope> {
    let mut scopes = BTreeSet::new();
    let mut needs_global_request = false;

    for address in addresses {
        if let Some(street_sym) = &address.street_sym {
            // PGE's street filter works without citySym, which avoids excluding
            // rows stored under a child SIMC code such as a city district.
            scopes.insert(PgeRequestScope::StreetSym(street_sym.clone()));
            continue;
        }

        let Some(city_id) = address.original.city_id.filter(|city_id| *city_id > 0) else {
            needs_global_request = true;
            continue;
        };

        match city_scope_ids.get(&city_id) {
            Some(city_ids) if !city_ids.is_empty() && city_ids.len() <= PGE_MAX_CITY_SYM_SCOPES => {
                for city_sym in city_ids {
                    scopes.insert(PgeRequestScope::CitySym(format!("{city_sym:07}")));
                }
            }
            _ => {
                // The autocomplete stores only root SIMC IDs. If we cannot
                // safely enumerate a small child set, keep coverage with the
                // broad endpoint request instead of guessing.
                needs_global_request = true;
            }
        }
    }

    if needs_global_request {
        scopes.insert(PgeRequestScope::AllLocations);
    }
    scopes.into_iter().collect()
}

fn load_pge_city_scope_ids(
    addresses: &[PgePreparedAddress<'_>],
    app_handle: Option<&tauri::AppHandle>,
) -> HashMap<u64, Vec<u64>> {
    let teryt_path = if app_handle.is_none() {
        resolve_pge_teryt_db_path()
    } else {
        None
    };
    if app_handle.is_none() && teryt_path.is_none() {
        log::warn!(
            "PGE: TERYT database unavailable without a Tauri handle; child-locality matching may be incomplete"
        );
    }

    let city_ids = addresses
        .iter()
        .filter_map(|address| address.original.city_id.filter(|city_id| *city_id > 0))
        .collect::<BTreeSet<_>>();
    let mut scopes = HashMap::new();
    for city_id in city_ids {
        let lookup = match (app_handle, teryt_path.as_deref()) {
            (Some(app_handle), _) => crate::teryt::lookup_city_scope_ids(app_handle, city_id),
            (None, Some(path)) => crate::teryt::lookup_city_scope_ids_by_path(path, city_id),
            (None, None) => continue,
        };
        match lookup {
            Ok(city_scope_ids) if !city_scope_ids.is_empty() => {
                scopes.insert(city_id, city_scope_ids);
            }
            Ok(_) => {
                log::warn!(
                    "PGE: TERYT returned no city scope for SIMC {city_id}; using broad fetch"
                );
            }
            Err(error) => {
                log::warn!("PGE: could not load TERYT city scope for SIMC {city_id}: {error}");
            }
        }
    }
    scopes
}

fn resolve_pge_teryt_db_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("AWARIA_TERYT_PATH") {
        candidates.push(PathBuf::from(path));
    }
    candidates.push(PathBuf::from("/usr/local/share/awaria/assets/teryt"));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("teryt"),
    );
    candidates.push(PathBuf::from("../src-tauri/assets/teryt"));

    candidates.into_iter().find(|path| path.is_file())
}

async fn fetch_pge_outages_for_addresses(
    client: &Client,
    base_url: &str,
    window: &PgeDateWindow,
    addresses: &[PgePreparedAddress<'_>],
    city_scope_ids: &HashMap<u64, Vec<u64>>,
) -> (Vec<PgeOutage>, Vec<String>) {
    let scopes = pge_request_scopes(addresses, city_scope_ids);
    if scopes.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let scope_count = scopes.len();
    let mut requests = stream::iter(scopes.into_iter().map(|scope| {
        let client = client.clone();
        let base_url = base_url.to_string();
        let window = window.clone();
        async move {
            let result =
                fetch_pge_outages_with_retry_for_scope(&client, &base_url, &window, &scope).await;
            (scope, result)
        }
    }))
    .buffer_unordered(PGE_MAX_CONCURRENT_REQUESTS);

    let mut completed_scopes = 0;
    let mut outage_batches = Vec::new();
    let mut errors = Vec::new();
    let fetch_results = async {
        while let Some((scope, result)) = requests.next().await {
            completed_scopes += 1;
            match result {
                Ok(outages) => outage_batches.push(outages),
                Err(error) => errors.push(format!("PGE {}: {error}", scope.label())),
            }
        }
    };

    if tokio::time::timeout(PGE_FETCH_DEADLINE, fetch_results)
        .await
        .is_err()
    {
        errors.push(format!(
            "PGE multi-location fetch timed out after {} seconds (completed {completed_scopes} of {scope_count} queries)",
            PGE_FETCH_DEADLINE.as_secs()
        ));
    }

    (merge_outages_by_id(outage_batches), errors)
}

fn merge_outages_by_id(outage_batches: Vec<Vec<PgeOutage>>) -> Vec<PgeOutage> {
    let mut merged: HashMap<i64, PgeOutage> = HashMap::new();
    for outage in outage_batches.into_iter().flatten() {
        if let Some(existing) = merged.get_mut(&outage.id) {
            for address in outage.addresses {
                if !existing.addresses.contains(&address) {
                    existing.addresses.push(address);
                }
            }
        } else {
            merged.insert(outage.id, outage);
        }
    }
    merged.into_values().collect()
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
    pub city_sym_candidates: Vec<String>,
    pub street_sym: Option<String>,
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
            city_sym_candidates: address
                .city_id
                .filter(|city_id| *city_id > 0)
                .map(|city_id| vec![format!("{city_id:07}")])
                .unwrap_or_default(),
            street_sym: address
                .street_id
                .filter(|street_id| *street_id > 0)
                .map(|street_id| format!("{street_id:05}")),
        }
    }

    fn set_city_scope_ids(&mut self, city_scope_ids: &[u64]) {
        self.city_sym_candidates = city_scope_ids
            .iter()
            .map(|city_id| format!("{city_id:07}"))
            .collect();
    }
}

pub fn matches_prepared_address(outage: &PgeOutage, prep: &PgePreparedAddress) -> bool {
    let outage_desc_lower = outage.description.as_ref().map(|d| d.to_lowercase());
    let city_match_desc = outage_desc_lower
        .as_ref()
        .map(|d| d.contains(&prep.city_name_lower))
        .unwrap_or(false);

    for addr in &outage.addresses {
        if let Some(teryt) = &addr.teryt {
            if let Some(v) = &teryt.voivodeshipName
                && v.to_uppercase() != prep.voivodeship_upper
            {
                continue;
            }
            if let Some(c) = &teryt.countyName {
                let c_lower = c.to_lowercase();
                if !c_lower.contains(&prep.district_lower)
                    && !prep.district_lower.contains(&c_lower)
                {
                    continue;
                }
            }
            if let Some(c) = &teryt.communeName {
                let c_lower = c.to_lowercase();
                if !c_lower.contains(&prep.commune_lower) && !prep.commune_lower.contains(&c_lower)
                {
                    continue;
                }
            }

            let city_code_match = teryt.citySym.as_ref().is_some_and(|city_sym| {
                prep.city_sym_candidates
                    .iter()
                    .any(|candidate| candidate == city_sym)
            });
            let city_name_match = teryt
                .cityName
                .as_ref()
                .map(|c| c.to_lowercase() == prep.city_name_lower)
                .unwrap_or(false);
            if !city_code_match && !city_name_match {
                continue;
            }

            // Check street. PGE streetName includes "ul. " etc.
            let street_match = if prep.street_name_candidates_lower.is_empty() {
                true
            } else {
                let street_code_match = teryt
                    .streetSymUl
                    .as_ref()
                    .is_some_and(|street_sym| prep.street_sym.as_ref() == Some(street_sym));
                street_code_match
                    || teryt.streetName.as_ref().is_some_and(|street| {
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
            let street_match = outage_desc_lower
                .as_ref()
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
        app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        // Prepare applicable addresses before the network call so the endpoint can
        // filter by street and an empty address list does not trigger a global fetch.
        let mut prepared_addresses: Vec<PgePreparedAddress> = settings
            .addresses
            .iter()
            .enumerate()
            .filter(|(_, address)| {
                address.is_active
                    && crate::api_logic::is_address_applicable_for_provider(
                        &AlertSource::Pge,
                        address,
                    )
            })
            .map(|(index, address)| PgePreparedAddress::new(address, index))
            .collect();
        if prepared_addresses.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let city_scope_ids = load_pge_city_scope_ids(&prepared_addresses, app_handle);
        for address in &mut prepared_addresses {
            if let Some(city_id) = address.original.city_id
                && let Some(city_ids) = city_scope_ids.get(&city_id)
            {
                address.set_city_scope_ids(city_ids);
            }
        }

        let window = pge_date_window_now();
        let (outages, errors) = fetch_pge_outages_for_addresses(
            client,
            &get_pge_url(),
            &window,
            &prepared_addresses,
            &city_scope_ids,
        )
        .await;

        let mut alerts = Vec::new();
        for prep in &prepared_addresses {
            let local_outages: Vec<UnifiedAlert> = outages
                .iter()
                .filter(|outage| matches_prepared_address(outage, prep))
                .map(|outage| {
                    let mut alert = outage.to_unified();
                    alert.address_index = Some(prep.index);
                    alert.is_local = Some(true);
                    alert.location = Some(format!("Miejscowość: {}", prep.original.city_name));
                    alert
                })
                .collect();
            alerts.extend(local_outages);
        }

        (alerts, errors)
    }
}

#[allow(dead_code)]
pub fn matches_address(outage: &PgeOutage, address: &AddressEntry) -> bool {
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
    fn test_pge_date_window_uses_stable_quarter_hour_bounds() {
        let first = DateTime::parse_from_rfc3339("2026-10-22T12:37:45+02:00")
            .unwrap()
            .with_timezone(&Warsaw);
        let second = DateTime::parse_from_rfc3339("2026-10-22T12:44:59+02:00")
            .unwrap()
            .with_timezone(&Warsaw);

        let first_window = pge_date_window_at(first);
        let second_window = pge_date_window_at(second);

        assert_eq!(first_window.stop_at_from, "2026-10-22 12:30:00");
        assert_eq!(first_window.start_at_to, "2026-11-21 11:45:00");
        assert_eq!(first_window.start_at_to, second_window.start_at_to);
        assert_eq!(first_window.stop_at_from, second_window.stop_at_from);
    }

    #[test]
    fn test_pge_url_includes_street_filter_and_date_bounds() {
        let window = PgeDateWindow {
            start_at_to: "2026-11-21 11:45:00".to_string(),
            stop_at_from: "2026-10-22 12:30:00".to_string(),
        };

        let url = build_pge_url(
            "https://example.test/api/power-outage",
            &window,
            &PgeRequestScope::StreetSym("026544".to_string()),
        )
        .unwrap();
        let query = url.query_pairs().into_owned().collect::<HashMap<_, _>>();

        assert_eq!(query.get("startAtTo").unwrap(), "2026-11-21 11:45:00");
        assert_eq!(query.get("stopAtFrom").unwrap(), "2026-10-22 12:30:00");
        assert_eq!(query.get("type").unwrap(), "teryt");
        assert_eq!(query.get("streetSym").unwrap(), "026544");
        assert!(!query.contains_key("citySym"));
    }

    #[test]
    fn test_pge_request_scopes_use_streets_and_include_city_children() {
        let addresses = vec![
            AddressEntry {
                city_id: Some(977120),
                street_id: Some(24170),
                ..Default::default()
            },
            AddressEntry {
                city_id: Some(977120),
                street_id: Some(24170),
                ..Default::default()
            },
            AddressEntry {
                city_id: Some(923526),
                ..Default::default()
            },
            AddressEntry {
                city_id: None,
                ..Default::default()
            },
        ];
        let prepared = addresses
            .iter()
            .enumerate()
            .map(|(index, address)| PgePreparedAddress::new(address, index))
            .collect::<Vec<_>>();
        let city_scope_ids =
            HashMap::from([(923526, vec![923526, 923532]), (977120, vec![977120])]);

        assert_eq!(
            vec![
                PgeRequestScope::CitySym("0923526".to_string()),
                PgeRequestScope::CitySym("0923532".to_string()),
                PgeRequestScope::StreetSym("24170".to_string()),
                PgeRequestScope::AllLocations,
            ],
            pge_request_scopes(&prepared, &city_scope_ids)
        );

        assert_eq!(
            pge_request_scopes(&prepared, &HashMap::new()),
            vec![
                PgeRequestScope::StreetSym("24170".to_string()),
                PgeRequestScope::AllLocations,
            ]
        );
    }

    #[test]
    fn test_pge_large_city_subtree_uses_broad_fallback() {
        let address = AddressEntry {
            city_id: Some(954700),
            ..Default::default()
        };
        let prepared = [PgePreparedAddress::new(&address, 0)];
        let children = (954700..954710).collect::<Vec<_>>();
        let city_scope_ids = HashMap::from([(954700, children)]);

        assert_eq!(
            pge_request_scopes(&prepared, &city_scope_ids),
            vec![PgeRequestScope::AllLocations]
        );
    }

    #[test]
    fn test_pge_merge_deduplicates_outages_and_unions_address_groups() {
        let shared_address = PgeAddress {
            teryt: Some(PgeTeryt {
                voivodeshipName: Some("MAZOWIECKIE".to_string()),
                countyName: Some("sochaczewski".to_string()),
                communeName: Some("Sochaczew".to_string()),
                cityName: Some("Sochaczew".to_string()),
                citySym: None,
                streetName: Some("ul. Wierzbowa".to_string()),
                streetSymUl: None,
            }),
            numbers: Some("11".to_string()),
        };
        let other_address = PgeAddress {
            teryt: Some(PgeTeryt {
                voivodeshipName: Some("MAZOWIECKIE".to_string()),
                countyName: Some("sochaczewski".to_string()),
                communeName: Some("Sochaczew".to_string()),
                cityName: Some("Sochaczew".to_string()),
                citySym: None,
                streetName: Some("ul. Pionierów".to_string()),
                streetSymUl: None,
            }),
            numbers: Some("10, 12".to_string()),
        };
        let make_outage = |addresses| PgeOutage {
            id: 7,
            startAt: "2026-10-22 13:00:00".to_string(),
            stopAt: "2026-10-22 14:00:00".to_string(),
            description: Some("Sochaczew".to_string()),
            regionName: Some("Rejon Sochaczew".to_string()),
            revoked: Some(false),
            addresses,
        };

        let merged = merge_outages_by_id(vec![
            vec![make_outage(vec![shared_address.clone()])],
            vec![make_outage(vec![
                shared_address.clone(),
                other_address.clone(),
            ])],
        ]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].addresses, vec![shared_address, other_address]);
    }

    #[tokio::test]
    async fn test_fetch_pge_queries_each_unique_street_and_merges_repeated_outages() {
        let mut server = mockito::Server::new_async().await;
        let first_street = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Regex(
                ".*type=teryt.*streetSym=24170.*".to_string(),
            ))
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":9,"startAt":"2026-10-22 13:00:00","stopAt":"2026-10-22 14:00:00","addresses":[{"numbers":"1","teryt":{"cityName":"Wasilków","citySym":"0923526","streetName":"ul. Lipowa","streetSymUl":"24170"}}]}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let second_street = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Regex(
                ".*type=teryt.*streetSym=26544.*".to_string(),
            ))
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":9,"startAt":"2026-10-22 13:00:00","stopAt":"2026-10-22 14:00:00","addresses":[{"numbers":"11","teryt":{"cityName":"Sochaczew","citySym":"0977120","streetName":"ul. Wierzbowa","streetSymUl":"26544"}}]}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let addresses = [
            AddressEntry {
                city_id: Some(923526),
                street_id: Some(24170),
                ..Default::default()
            },
            AddressEntry {
                city_id: Some(923526),
                street_id: Some(24170),
                ..Default::default()
            },
            AddressEntry {
                city_id: Some(977120),
                street_id: Some(26544),
                ..Default::default()
            },
        ];
        let prepared = addresses
            .iter()
            .enumerate()
            .map(|(index, address)| PgePreparedAddress::new(address, index))
            .collect::<Vec<_>>();
        let window = PgeDateWindow {
            start_at_to: "2026-11-21 11:45:00".to_string(),
            stop_at_from: "2026-10-22 12:30:00".to_string(),
        };
        let city_scope_ids = HashMap::new();

        let (outages, errors) = fetch_pge_outages_for_addresses(
            &Client::new(),
            &format!("{}/api/power-outage", server.url()),
            &window,
            &prepared,
            &city_scope_ids,
        )
        .await;

        assert!(errors.is_empty(), "unexpected PGE errors: {errors:?}");
        assert_eq!(outages.len(), 1);
        assert_eq!(outages[0].id, 9);
        assert_eq!(outages[0].addresses.len(), 2);
        first_street.assert_async().await;
        second_street.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_pge_keeps_successful_city_data_when_another_city_fails() {
        let mut server = mockito::Server::new_async().await;
        let successful = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Regex(
                ".*type=teryt.*streetSym=24170.*".to_string(),
            ))
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":10,"startAt":"2026-10-22 13:00:00","stopAt":"2026-10-22 14:00:00","addresses":[]}]"#,
            )
            .expect(1)
            .create_async()
            .await;
        let failed = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Regex(
                ".*type=teryt.*streetSym=26544.*".to_string(),
            ))
            .match_header("connection", "close")
            .with_status(400)
            .expect(1)
            .create_async()
            .await;
        let addresses = [
            AddressEntry {
                city_id: Some(977120),
                street_id: Some(24170),
                ..Default::default()
            },
            AddressEntry {
                city_id: Some(923526),
                street_id: Some(26544),
                ..Default::default()
            },
        ];
        let prepared = addresses
            .iter()
            .enumerate()
            .map(|(index, address)| PgePreparedAddress::new(address, index))
            .collect::<Vec<_>>();
        let window = PgeDateWindow {
            start_at_to: "2026-11-21 11:45:00".to_string(),
            stop_at_from: "2026-10-22 12:30:00".to_string(),
        };
        let city_scope_ids = HashMap::new();

        let (outages, errors) = fetch_pge_outages_for_addresses(
            &Client::new(),
            &format!("{}/api/power-outage", server.url()),
            &window,
            &prepared,
            &city_scope_ids,
        )
        .await;

        assert_eq!(outages.len(), 1);
        assert_eq!(outages[0].id, 10);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("26544"));
        successful.assert_async().await;
        failed.assert_async().await;
    }

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
                        citySym: None,
                        streetName: Some("ul. Wiejska".to_string()),
                        streetSymUl: None,
                    }),
                    numbers: Some("1, 2, 3".to_string()),
                },
                PgeAddress {
                    teryt: Some(PgeTeryt {
                        voivodeshipName: None,
                        countyName: None,
                        communeName: None,
                        cityName: None,
                        citySym: None,
                        streetName: Some("ul. Polna".to_string()),
                        streetSymUl: None,
                    }),
                    numbers: Some("10-20".to_string()),
                },
            ],
        };

        let unified = outage.to_unified();
        assert_eq!(unified.source, AlertSource::Pge);
        assert_eq!(
            unified.message,
            Some(
                "ul. Wiejska, Numery: 1, 2, 3; ul. Polna, Numery: 10-20 (Rejon Gliwice)"
                    .to_string()
            )
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
                    citySym: Some("0969400".to_string()),
                    streetName: Some("ul. Kuźnicza".to_string()),
                    streetSymUl: Some("13900".to_string()),
                }),
                numbers: Some("12-25".to_string()),
            }],
        };

        assert!(matches_address(&outage, &addr));

        let mut addr_wrong = addr.clone();
        addr_wrong.city_name = "Warszawa".to_string();
        addr_wrong.city_id = None;
        assert!(!matches_address(&outage, &addr_wrong));
    }

    #[test]
    fn test_pge_matches_child_simc_locality_for_selected_parent_city() {
        let address = AddressEntry {
            name: "Home".to_string(),
            city_name: "Łódź".to_string(),
            voivodeship: "ŁÓDZKIE".to_string(),
            district: "Łódź".to_string(),
            commune: "Łódź".to_string(),
            street_name: "ul. Żubardzka".to_string(),
            street_name_1: "Żubardzka".to_string(),
            house_no: "12".to_string(),
            city_id: Some(957650),
            street_id: Some(26544),
            is_active: true,
            ..Default::default()
        };
        let mut prepared = PgePreparedAddress::new(&address, 0);
        prepared.set_city_scope_ids(&[957650, 957666]);
        let outage = PgeOutage {
            id: 591296,
            startAt: "2026-10-22 13:00:00".to_string(),
            stopAt: "2026-10-22 14:00:00".to_string(),
            description: Some("Łódź-Bałuty: ul. Żubardzka".to_string()),
            regionName: Some("Łódź".to_string()),
            revoked: Some(false),
            addresses: vec![PgeAddress {
                teryt: Some(PgeTeryt {
                    voivodeshipName: Some("ŁÓDZKIE".to_string()),
                    countyName: Some("Łódź".to_string()),
                    communeName: Some("Łódź".to_string()),
                    cityName: Some("Łódź-Bałuty".to_string()),
                    citySym: Some("0957666".to_string()),
                    streetName: Some("ul. Żubardzka".to_string()),
                    streetSymUl: Some("26544".to_string()),
                }),
                numbers: Some("2-24".to_string()),
            }],
        };

        assert!(matches_prepared_address(&outage, &prepared));

        let mut unrelated_child = outage.clone();
        let teryt = unrelated_child.addresses[0].teryt.as_mut().unwrap();
        teryt.cityName = Some("Łódź-Górna".to_string());
        teryt.citySym = Some("0957985".to_string());
        assert!(!matches_prepared_address(&unrelated_child, &prepared));
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
        assert_eq!(data[2 - 1].id, 3);
    }

    #[tokio::test]
    async fn test_fetch_pge_recovers_after_404() {
        let mut server = mockito::Server::new_async().await;
        let unavailable = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(404)
            .with_body("404 page not found\n")
            .expect(3)
            .create_async()
            .await;
        let available = server.mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":1,"startAt":"2026-10-16 10:00:00","stopAt":"2026-10-16 14:00:00","addresses":[]}]"#)
            .expect(1)
            .create_async().await;

        let outages = fetch_pge_outages_with_retry(
            &Client::new(),
            &format!("{}/api/power-outage", server.url()),
        )
        .await
        .expect("should recover after a transient 404");

        assert_eq!(outages.len(), 1);
        assert_eq!(outages[0].id, 1);
        unavailable.assert_async().await;
        available.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_pge_reports_persistent_404() {
        let mut server = mockito::Server::new_async().await;
        let unavailable = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(404)
            .expect(4)
            .create_async()
            .await;

        let error = fetch_pge_outages_with_retry(
            &Client::new(),
            &format!("{}/api/power-outage", server.url()),
        )
        .await
        .unwrap_err();

        assert_eq!(error, "PGE HTTP error: 404 Not Found");
        unavailable.assert_async().await;
    }

    #[test]
    fn test_pge_retry_policy_only_retries_transient_statuses() {
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(
                PgeFetchError::Http {
                    status,
                    retry_after: None,
                }
                .is_retryable(),
                "{status} should retry"
            );
        }

        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::UNPROCESSABLE_ENTITY,
        ] {
            assert!(
                !PgeFetchError::Http {
                    status,
                    retry_after: None,
                }
                .is_retryable(),
                "{status} should not retry"
            );
        }

        let retry_after = StdDuration::from_secs(2);
        assert_eq!(
            PgeFetchError::Http {
                status: StatusCode::TOO_MANY_REQUESTS,
                retry_after: Some(retry_after),
            }
            .retry_delay(0),
            retry_after
        );
    }

    #[tokio::test]
    async fn test_fetch_pge_retries_server_error_then_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let unavailable = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(503)
            .expect(1)
            .create_async()
            .await;
        let available = server.mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":2,"startAt":"2026-10-16 10:00:00","stopAt":"2026-10-16 14:00:00","addresses":[]}]"#)
            .expect(1)
            .create_async().await;

        let outages = fetch_pge_outages_with_retry(
            &Client::new(),
            &format!("{}/api/power-outage", server.url()),
        )
        .await
        .expect("should recover after a transient 503");

        assert_eq!(outages.len(), 1);
        assert_eq!(outages[0].id, 2);
        unavailable.assert_async().await;
        available.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_pge_does_not_retry_permanent_http_or_json_errors() {
        let mut server = mockito::Server::new_async().await;
        let bad_request = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(400)
            .expect(1)
            .create_async()
            .await;
        let client = Client::new();
        let error =
            fetch_pge_outages_with_retry(&client, &format!("{}/api/power-outage", server.url()))
                .await
                .unwrap_err();
        assert_eq!(error, "PGE HTTP error: 400 Bad Request");
        bad_request.assert_async().await;

        let malformed_json = server
            .mock("GET", "/api/power-outage")
            .match_query(mockito::Matcher::Any)
            .match_header("connection", "close")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not json")
            .expect(1)
            .create_async()
            .await;
        let error =
            fetch_pge_outages_with_retry(&client, &format!("{}/api/power-outage", server.url()))
                .await
                .unwrap_err();
        assert!(error.starts_with("PGE JSON error:"));
        malformed_json.assert_async().await;
    }

    #[tokio::test]
    #[ignore = "requires the live PGE API"]
    async fn test_fetch_pge_real() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client().unwrap();
        let outages = fetch_pge_outages(&client)
            .await
            .expect("PGE live API failed after retries");
        println!("Fetched {} PGE outages", outages.len());
    }
}
