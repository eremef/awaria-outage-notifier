use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const BYTOMSKIE_WODOCIAGI_PAGE_URL: &str =
    "https://bytomskiewodociagi.pl/wp-json/wp/v2/pages/2";

#[cfg(test)]
const BYTOM_CITY_ID: u64 = 938_670;

pub struct BytomskieWodociagiProvider;

#[derive(Debug, Deserialize)]
struct RestPage {
    content: RestContent,
}

#[derive(Debug, Deserialize)]
struct RestContent {
    rendered: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    day_text: String,
    time_text: String,
    location_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HouseNumber {
    number: u32,
    suffix: Option<char>,
}

impl Ord for HouseNumber {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.number
            .cmp(&other.number)
            .then_with(|| self.suffix.cmp(&other.suffix))
    }
}

impl PartialOrd for HouseNumber {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{2}|\d{4})\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(2[0-4]|[01]?\d)[:.]([0-5]\d)\b").unwrap())
}

fn house_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,4}[a-z]?)[ \t]*(?:-|–|—|−|\bdo\b)[ \t]*(\d{1,4}[a-z]?)\b").unwrap()
    })
}

fn house_number_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,4}[a-z]?)\b").unwrap())
}

fn normalize(text: &str) -> String {
    crate::utils::clean_text(text)
        .replace(['–', '—', '−'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    let captures = date_regex().captures(text)?;
    let day = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let raw_year = captures.get(3)?.as_str();
    let year = raw_year.parse::<i32>().ok()?;
    let year = if raw_year.len() == 2 {
        2000 + year
    } else {
        year
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

fn parse_time(text: &str) -> Option<(NaiveTime, bool)> {
    let captures = time_regex().captures(text)?;
    let hour = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some((NaiveTime::from_hms_opt(0, 0, 0)?, true));
    }
    Some((NaiveTime::from_hms_opt(hour, minute, 0)?, false))
}

fn parse_time_range(text: &str, date: NaiveDate) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let times = time_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let value = captures.get(0)?.as_str();
            parse_time(value)
        })
        .collect::<Vec<_>>();

    let (Some((start_time, start_next_day)), Some((end_time, end_next_day))) =
        (times.first(), times.last())
    else {
        return (None, None);
    };
    if times.len() < 2 {
        return (None, None);
    }

    let start_date = if *start_next_day {
        date + Duration::days(1)
    } else {
        date
    };
    let mut end_date = if *end_next_day {
        date + Duration::days(1)
    } else {
        date
    };
    let start = start_date.and_time(*start_time);
    let mut end = end_date.and_time(*end_time);
    if end <= start && !*end_next_day {
        end_date += Duration::days(1);
        end = end_date.and_time(*end_time);
    }
    (Some(start), Some(end))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn report_element_text(element: scraper::ElementRef<'_>, selector: &Selector) -> Option<String> {
    element
        .select(selector)
        .next()
        .map(|node| collapse_whitespace(&node.text().collect::<Vec<_>>().join(" ")))
        .filter(|text| !text.is_empty())
}

fn parse_notices(html: &str) -> Vec<Notice> {
    let document = Html::parse_fragment(html);
    let (Ok(report_selector), Ok(day_selector), Ok(time_selector), Ok(where_selector)) = (
        Selector::parse(".reportElement"),
        Selector::parse(".day"),
        Selector::parse(".time"),
        Selector::parse(".where"),
    ) else {
        return Vec::new();
    };

    document
        .select(&report_selector)
        .filter_map(|element| {
            let notice = Notice {
                day_text: report_element_text(element, &day_selector)?,
                time_text: report_element_text(element, &time_selector)?,
                location_text: report_element_text(element, &where_selector)?,
            };
            Some(notice)
        })
        .collect()
}

fn notice_identity(notice: &Notice) -> String {
    let day = parse_date(&notice.day_text)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| normalize(&notice.day_text));
    let mut hasher = DefaultHasher::new();
    day.hash(&mut hasher);
    normalize(&notice.time_text).hash(&mut hasher);
    normalize(&notice.location_text).hash(&mut hasher);
    format!("bytomskie_wodociagi_{:016x}", hasher.finish())
}

fn normalize_street_token(token: &str) -> String {
    let normalized = normalize(token);
    let endings = [
        "skiego", "skich", "skiej", "ckiego", "ckich", "ckiej", "owego", "owych", "owej", "owym",
    ];
    for ending in endings {
        if normalized.len() > ending.len() + 2 && normalized.ends_with(ending) {
            return normalized[..normalized.len() - ending.len()].to_string();
        }
    }
    for ending in [
        "iej", "ich", "ych", "ego", "emu", "imi", "ami", "ach", "owa", "owe", "owej", "ska",
        "skie", "ski", "ej", "owi", "ow", "em", "ie", "a", "u",
    ] {
        if normalized.len() > ending.len() + 2 && normalized.ends_with(ending) {
            return normalized[..normalized.len() - ending.len()].to_string();
        }
    }
    normalized
}

fn street_tokens_match(candidate: &str, segment: &str) -> bool {
    let candidate = crate::utils::strip_street_prefixes(candidate.trim());
    let candidate_tokens = normalize(candidate)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(normalize_street_token)
        .collect::<Vec<_>>();
    let segment_tokens = normalize(segment)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(normalize_street_token)
        .collect::<Vec<_>>();

    !candidate_tokens.is_empty()
        && candidate_tokens.iter().all(|candidate_token| {
            segment_tokens.iter().any(|segment_token| {
                candidate_token == segment_token
                    || candidate_token.starts_with(segment_token)
                    || segment_token.starts_with(candidate_token)
            })
        })
}

fn configured_streets(address: &AddressEntry) -> Vec<String> {
    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .into_iter()
    .filter(|street| !street.trim().is_empty())
    .collect()
}

fn location_segments(location: &str) -> Vec<&str> {
    location
        .split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn matching_location_segments<'a>(location: &'a str, address: &AddressEntry) -> Vec<&'a str> {
    let streets = configured_streets(address);
    if streets.is_empty() {
        return Vec::new();
    }
    location_segments(location)
        .into_iter()
        .filter(|segment| {
            streets
                .iter()
                .any(|street| street_tokens_match(street, segment))
        })
        .collect()
}

/// Matches a configured street against an affected-address segment.
///
/// This intentionally does not inspect house numbers; number enforcement is
/// owned by the central `Settings.filter_by_house_no` path.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    !matching_location_segments(location, address).is_empty()
}

fn parse_house_number(value: &str) -> Option<HouseNumber> {
    let value = value.trim().to_ascii_lowercase();
    let mut chars = value.chars();
    let digits = chars
        .clone()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    let suffix = chars
        .nth(digits.chars().count())
        .filter(|character| character.is_ascii_alphabetic());
    Some(HouseNumber {
        number: digits.parse().ok()?,
        suffix,
    })
}

fn house_number_in_segment(segment: &str, requested: &str) -> Option<bool> {
    let requested = parse_house_number(requested)?;
    let normalized = normalize(segment);
    let odd_only = normalized.contains("nieparzyste") || normalized.contains("nieparz");
    let even_only = !odd_only && (normalized.contains("parzyste") || normalized.contains("parz"));
    if (odd_only && requested.number.is_multiple_of(2))
        || (even_only && !requested.number.is_multiple_of(2))
    {
        return Some(false);
    }

    let ranges = house_range_regex()
        .captures_iter(segment)
        .filter_map(|captures| {
            Some((
                parse_house_number(captures.get(1)?.as_str())?,
                parse_house_number(captures.get(2)?.as_str())?,
            ))
        })
        .collect::<Vec<_>>();
    if !ranges.is_empty() {
        return Some(ranges.into_iter().any(|(start, end)| {
            let (low, high) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            requested >= low && requested <= high
        }));
    }

    let numbers = house_number_regex()
        .captures_iter(segment)
        .filter_map(|captures| parse_house_number(captures.get(1)?.as_str()))
        .collect::<Vec<_>>();
    if numbers.is_empty() {
        None
    } else {
        Some(numbers.contains(&requested))
    }
}

/// Checks the saved house number against the range in the matching street
/// segment. If the notice lists no parseable numbers, the unknown range is
/// treated as non-restrictive so callers can keep the existing shared policy.
pub fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    let segments = matching_location_segments(location, address);
    if segments.is_empty() {
        return false;
    }
    if address.house_no.trim().is_empty() {
        return true;
    }

    let matches = segments
        .into_iter()
        .filter_map(|segment| house_number_in_segment(segment, &address.house_no))
        .collect::<Vec<_>>();
    matches.is_empty() || matches.into_iter().any(|matches| matches)
}

fn parse_notice(notice: &Notice, settings: &Settings) -> UnifiedAlert {
    let date = parse_date(&notice.day_text);
    let (start, end) = date
        .map(|date| parse_time_range(&notice.time_text, date))
        .unwrap_or((None, None));
    let location = format!("Miejscowość: Bytom; {}", notice.location_text);

    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::BytomskieWodociagi,
                    address,
                )
                && location_matches_address(&notice.location_text, address)
        })
        .map(|(index, _)| index);

    UnifiedAlert {
        source: AlertSource::BytomskieWodociagi,
        startDate: start.map(format_datetime),
        endDate: end.map(format_datetime),
        message: Some(format!(
            "{} — {}. {}",
            notice.day_text, notice.time_text, notice.location_text
        )),
        location: Some(location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(notice_identity(notice)),
    }
}

/// Parses the rendered WordPress REST page response into Awaria alerts.
pub fn parse_bytomskie_wodociagi_page(
    json: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    let page: RestPage = serde_json::from_str(json)
        .map_err(|error| format!("Bytomskie Wodociągi REST response parse error: {error}"))?;
    Ok(parse_bytomskie_wodociagi_html(
        &page.content.rendered,
        settings,
    ))
}

/// Parses the notice markup from the `content.rendered` HTML field.
pub fn parse_bytomskie_wodociagi_html(html: &str, settings: &Settings) -> Vec<UnifiedAlert> {
    let mut seen = HashSet::new();
    parse_notices(html)
        .into_iter()
        .filter(|notice| seen.insert(notice_identity(notice)))
        .map(|notice| parse_notice(&notice, settings))
        .collect()
}

async fn fetch_page(client: &Client) -> Result<String, String> {
    crate::utils::retry(
        || async {
            let response = client
                .get(BYTOMSKIE_WODOCIAGI_PAGE_URL)
                .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
                .send()
                .await
                .map_err(|error| error.to_string())?;
            if !response.status().is_success() {
                return Err(format!("HTTP {}", response.status()));
            }
            response.text().await.map_err(|error| error.to_string())
        },
        2,
    )
    .await
}

#[async_trait]
impl AlertProvider for BytomskieWodociagiProvider {
    fn id(&self) -> String {
        "bytomskie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::BytomskieWodociagi
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !crate::api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }

        match fetch_page(client).await {
            Ok(json) => match parse_bytomskie_wodociagi_page(&json, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (Vec::new(), vec![error]),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("Bytomskie Wodociągi REST request error: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(addresses: Vec<AddressEntry>) -> Settings {
        let mut settings = Settings::default();
        settings.addresses = addresses;
        settings
    }

    fn address(street: &str, house_no: &str) -> AddressEntry {
        AddressEntry {
            city_name: "Bytom".to_string(),
            city_id: Some(BYTOM_CITY_ID),
            street_name: street.to_string(),
            street_name_1: String::new(),
            street_name_2: None,
            house_no: house_no.to_string(),
            ..AddressEntry::default()
        }
    }

    fn sample_html() -> String {
        r#"
        <div class="reportElement">
          <p class="day">23.10.2026</p>
          <p class="time">Planowana przerwa w dostawie wody w godz. 08:00 - 16:00</p>
          <p class="where">Brak wody w budynkach: ul. Orląt Lwowskich 7 -15, Oświęcimska 2 – 10a, Prusa 22 - 32</p>
        </div>
        <div class="reportElement">
          <p class="day">22.10.26</p>
          <p class="time">Przerwa w dostawie wody w godz. 8:30 - 15:50</p>
          <p class="where">ul. Mickiewicza 1 - 9 (numery nieparzyste)</p>
        </div>
        "#
        .to_string()
    }

    #[test]
    fn parses_rest_page_and_both_observed_date_formats() {
        let json = serde_json::json!({"content": {"rendered": sample_html()}}).to_string();
        let alerts = parse_bytomskie_wodociagi_page(&json, &settings_with(Vec::new())).unwrap();

        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-23T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-23T16:00:00"));
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-10-22T08:30:00"));
        assert_eq!(alerts[1].endDate.as_deref(), Some("2026-10-22T15:50:00"));
    }

    #[test]
    fn parses_time_punctuation_and_midnight_rollover() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let (start, end) = parse_time_range("Przerwa 23:30 – 24.00", date);
        assert_eq!(start, Some(date.and_hms_opt(23, 30, 0).unwrap()));
        assert_eq!(
            end,
            Some((date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
        );
    }

    #[test]
    fn date_and_times_remain_unknown_when_source_values_are_malformed_or_incomplete() {
        let html = r#"
          <div class="reportElement"><p class="day">31.02.2026</p><p class="time">Awaria od 8:00</p><p class="where">ul. Prusa 1</p></div>
          <div class="reportElement"><p class="day">23.10.2026</p><p class="time">Awaria od 8:00</p><p class="where">ul. Prusa 1</p></div>
        "#;
        let alerts = parse_bytomskie_wodociagi_html(html, &settings_with(Vec::new()));
        assert_eq!(alerts.len(), 2);
        assert!(
            alerts
                .iter()
                .all(|alert| alert.startDate.is_none() && alert.endDate.is_none())
        );
    }

    #[test]
    fn preserves_complete_polish_wording_and_distinct_same_day_notices() {
        let html = r#"
          <div class="reportElement"><p class="day">22.10.26</p><p class="time">Przerwa w dostawie wody w godz. 8:30 - 15:50</p><p class="where">ul. Prusa 1 - 9</p></div>
          <div class="reportElement"><p class="day">22.10.26</p><p class="time">Planowana przerwa w dostawie wody w godz. 08:00 - 16:00</p><p class="where">ul. Prusa 1 - 9</p></div>
          <div class="reportElement"><p class="day">22.10.26</p><p class="time">Planowana przerwa w dostawie wody w godz. 08:00 - 16:00</p><p class="where">ul. Prusa 1 - 9</p></div>
        "#;
        let alerts = parse_bytomskie_wodociagi_html(html, &settings_with(Vec::new()));
        assert_eq!(alerts.len(), 2);
        assert!(alerts.iter().any(|alert| {
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("Planowana przerwa")
        }));
        assert!(
            alerts[0]
                .location
                .as_deref()
                .unwrap()
                .contains("ul. Prusa 1 - 9")
        );
        assert_ne!(alerts[0].hash, alerts[1].hash);
    }

    #[test]
    fn location_matching_is_street_scoped_and_handles_polish_inflection() {
        let location =
            "Miejscowość: Bytom; ul. Orląt Lwowskich 7 -15, Oświęcimskiej 2 – 10a, Prusa 22 - 32";
        assert!(location_matches_address(
            location,
            &address("ul. Orląt Lwowskiego", "12")
        ));
        assert!(location_matches_address(
            location,
            &address("Oświęcimska", "5")
        ));
        assert!(location_matches_address(
            location,
            &address("ul. Prusa", "25")
        ));
        assert!(!location_matches_address(
            location,
            &address("Mickiewicza", "5")
        ));
    }

    #[test]
    fn house_number_matching_uses_only_the_matching_street_range() {
        let location =
            "Miejscowość: Bytom; ul. Orląt Lwowskich 7 -15, Oświęcimska 2 – 10a, Prusa 22 - 32";
        assert!(house_number_matches_notice(
            location,
            &address("Oświęcimska", "10a")
        ));
        assert!(!house_number_matches_notice(
            location,
            &address("Oświęcimska", "15")
        ));
        assert!(!house_number_matches_notice(
            location,
            &address("Prusa", "10")
        ));
        assert!(house_number_matches_notice(
            location,
            &address("Prusa", "25")
        ));
    }

    #[test]
    fn house_number_matching_unions_repeated_street_segments() {
        let location = "Oświęcimska 2 – 10a, Prusa 1 - 3, Oświęcimska 20 - 30";
        assert!(house_number_matches_notice(
            location,
            &address("Oświęcimska", "25")
        ));
        assert!(!house_number_matches_notice(
            location,
            &address("Oświęcimska", "15")
        ));
        assert!(house_number_matches_notice(
            location,
            &address("Oświęcimska", "10a")
        ));
    }

    #[test]
    fn house_number_matching_respects_suffixes_lists_and_parity() {
        assert!(house_number_matches_notice(
            "ul. Oświęcimska 2 – 10a",
            &address("Oświęcimska", "10a")
        ));
        assert!(!house_number_matches_notice(
            "ul. Oświęcimska 2 – 10a",
            &address("Oświęcimska", "10b")
        ));
        assert!(house_number_matches_notice(
            "ul. Mickiewicza 1 - 9 (numery nieparzyste)",
            &address("Mickiewicza", "7")
        ));
        assert!(!house_number_matches_notice(
            "ul. Mickiewicza 1 - 9 (numery nieparzyste)",
            &address("Mickiewicza", "8")
        ));
    }

    #[test]
    fn malformed_rest_page_and_empty_or_unrelated_markup_are_safe() {
        assert!(parse_bytomskie_wodociagi_page("{}", &settings_with(Vec::new())).is_err());
        assert!(
            parse_bytomskie_wodociagi_html("<p>brak komunikatów</p>", &settings_with(Vec::new()))
                .is_empty()
        );
        assert!(parse_bytomskie_wodociagi_html(
            "<div class=\"reportElement\"><p class=\"day\">23.10.2026</p><p class=\"time\">Awaria</p></div>",
            &settings_with(Vec::new())
        )
        .is_empty());
    }
}
