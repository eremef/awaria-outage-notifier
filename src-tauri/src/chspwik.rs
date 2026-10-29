use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const CHSPWIK_OUTAGE_PAGE_URL: &str = "https://www.chspwik.pl/o-firmie/wylaczenia";
const OFFICIAL_HOST_PREFIX: &str = "https://www.chspwik.pl/";

pub struct ChspwikProvider;

#[derive(Debug, Clone)]
struct NoticeLink {
    title: String,
    url: String,
}

#[derive(Debug, Clone)]
struct Notice {
    title: String,
    body: String,
    city: Option<&'static str>,
    street_entries: Vec<String>,
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

#[derive(Debug, Clone)]
struct TextToken {
    value: String,
    end: usize,
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize(text: &str) -> String {
    crate::utils::clean_text(text)
        .replace(['–', '—', '−'], "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn event_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:w\s+dniu|dnia)\s+(\d{1,2})\s+([\p{L}]+)\s+(\d{4})\b")
            .expect("valid event date regex")
    })
}

fn numeric_event_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:w\s+dniu|dnia)\s+(\d{1,2})[./-](\d{1,2})[./-](\d{4})\b")
            .expect("valid numeric event date regex")
    })
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(2[0-4]|[01]?\d)[:.]([0-5]\d)\b").expect("valid time regex"))
}

fn house_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,4}\s*[a-z]?)[ \t]*(?:-|\bdo\b)[ \t]*(?:nr\.?\s*)?(\d{1,4}\s*[a-z]?)\b")
            .expect("valid house range regex")
    })
}

fn house_number_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,4}\s*[a-z]?)\b").expect("valid house number regex"))
}

fn polish_month_number(month: &str) -> Option<u32> {
    match normalize(month).as_str() {
        "styczen" | "stycznia" => Some(1),
        "luty" | "lutego" => Some(2),
        "marzec" | "marca" => Some(3),
        "kwiecien" | "kwietnia" => Some(4),
        "maj" | "maja" => Some(5),
        "czerwiec" | "czerwca" => Some(6),
        "lipiec" | "lipca" => Some(7),
        "sierpien" | "sierpnia" => Some(8),
        "wrzesien" | "wrzesnia" => Some(9),
        "pazdziernik" | "pazdziernika" => Some(10),
        "listopad" | "listopada" | "lisopad" | "lisopada" => Some(11),
        "grudzien" | "grudnia" => Some(12),
        _ => None,
    }
}

fn parse_event_date(text: &str) -> Option<NaiveDate> {
    if let Some(captures) = event_date_regex().captures(text) {
        let day = captures.get(1)?.as_str().parse::<u32>().ok()?;
        let month = polish_month_number(captures.get(2)?.as_str())?;
        let year = captures.get(3)?.as_str().parse::<i32>().ok()?;
        return NaiveDate::from_ymd_opt(year, month, day);
    }

    let captures = numeric_event_date_regex().captures(text)?;
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn parse_time(value: &str) -> Option<(NaiveTime, bool)> {
    let captures = time_regex().captures(value)?;
    let hour = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some((NaiveTime::from_hms_opt(0, 0, 0)?, true));
    }
    Some((NaiveTime::from_hms_opt(hour, minute, 0)?, false))
}

fn parse_event_range(text: &str) -> (Option<String>, Option<String>) {
    let Some(date) = parse_event_date(text) else {
        return (None, None);
    };
    let times = time_regex()
        .captures_iter(text)
        .filter_map(|capture| parse_time(capture.get(0)?.as_str()))
        .collect::<Vec<_>>();

    let Some((start_time, start_next_day)) = times.first() else {
        return (Some(date.format("%Y-%m-%d").to_string()), None);
    };
    let start_date = if *start_next_day {
        date + Duration::days(1)
    } else {
        date
    };
    let start = start_date.and_time(*start_time);

    let Some((end_time, end_next_day)) = times.last() else {
        return (Some(format_datetime(start)), None);
    };
    if times.len() < 2 {
        return (Some(format_datetime(start)), None);
    }
    let mut end_date = if *end_next_day {
        date + Duration::days(1)
    } else {
        date
    };
    let mut end = end_date.and_time(*end_time);
    if end <= start && !*end_next_day {
        end_date += Duration::days(1);
        end = end_date.and_time(*end_time);
    }
    (Some(format_datetime(start)), Some(format_datetime(end)))
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn official_article_url(value: &str) -> Option<String> {
    let value = value.trim();
    let absolute = if value.starts_with('/') && !value.starts_with("//") {
        format!("https://www.chspwik.pl{value}")
    } else {
        value.to_string()
    };
    if !absolute.starts_with(OFFICIAL_HOST_PREFIX)
        || !absolute.contains("/o-firmie/wylaczenia/")
        || absolute.contains('@')
        || absolute.contains('#')
        || absolute.contains('?')
    {
        return None;
    }
    Some(absolute)
}

fn parse_notice_links(html: &str) -> Vec<NoticeLink> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("a[href*='/o-firmie/wylaczenia/']")
        .expect("valid ChŚPWiK archive link selector");
    let mut seen = HashSet::new();
    let mut links = Vec::new();

    for anchor in document.select(&selector) {
        let title = collapse_whitespace(&anchor.text().collect::<Vec<_>>().join(" "));
        let Some(url) = anchor.value().attr("href").and_then(official_article_url) else {
            continue;
        };
        if title.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        links.push(NoticeLink { title, url });
    }
    links
}

fn element_text(element: ElementRef<'_>) -> String {
    collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn city_from_notice(title: &str, body: &str) -> Option<&'static str> {
    let normalized_body = normalize(body);
    if normalized_body.contains("w chorzowie") {
        return Some("Chorzów");
    }
    if normalized_body.contains("w swietochlowicach") {
        return Some("Świętochłowice");
    }

    let normalized_title = normalize(title);
    if normalized_title.contains("chorzow") {
        Some("Chorzów")
    } else if normalized_title.contains("swietochlowice") {
        Some("Świętochłowice")
    } else {
        None
    }
}

fn is_water_supply_notice(body: &str) -> bool {
    let normalized = normalize(body);
    [
        "dostawie wody",
        "dostawy wody",
        "brak wody",
        "przerwa w dostawie wody",
        "przerwy w dostawie wody",
        "ograniczenia w dostawie wody",
        "zaklocenia w dostawie wody",
    ]
    .iter()
    .any(|term| normalized.contains(*term))
}

fn parse_notice(html: &str, fallback_title: &str) -> Option<Notice> {
    let document = Html::parse_document(html);
    let article_selector = Selector::parse("article").expect("valid article selector");
    let article = document.select(&article_selector).next()?;
    let title_selector = Selector::parse("h1.aktualnosci__tytul")
        .expect("valid ChŚPWiK title selector");
    let body_selector = Selector::parse(".aktualnosc--tresc").expect("valid notice body selector");
    let list_item_selector = Selector::parse("li").expect("valid list item selector");

    let title = article
        .select(&title_selector)
        .next()
        .map(element_text)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_title.to_string());
    let content = article.select(&body_selector).next()?;
    let body = element_text(content);
    if !is_water_supply_notice(&body) {
        return None;
    }

    let street_entries = content
        .select(&list_item_selector)
        .map(element_text)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    Some(Notice {
        city: city_from_notice(&title, &body),
        title,
        body,
        street_entries,
    })
}

fn format_location(notice: &Notice) -> String {
    let locality = notice
        .city
        .map(|city| format!("Miejscowość: {city}"))
        .unwrap_or_else(|| "Miejscowość: Chorzów / Świętochłowice".to_string());
    let mut parts = vec![locality];
    parts.extend(
        notice
            .street_entries
            .iter()
            .map(|entry| format!("ul. {entry}")),
    );
    parts.join("; ")
}

fn tokens_with_spans(value: &str) -> Vec<TextToken> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, character) in value.char_indices() {
        if character.is_alphanumeric() {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            let token = &value[begin..index];
            if token.chars().any(char::is_alphabetic) {
                tokens.push(TextToken {
                    value: normalize_street_token(token),
                    end: index,
                });
            }
        }
    }
    if let Some(begin) = start {
        let token = &value[begin..];
        if token.chars().any(char::is_alphabetic) {
            tokens.push(TextToken {
                value: normalize_street_token(token),
                end: value.len(),
            });
        }
    }
    tokens
}

fn normalize_street_token(value: &str) -> String {
    let normalized = normalize(value);
    for suffix in [
        "skiego", "ckiego", "skich", "ckich", "skiej", "ckiej", "owej", "owym", "owych",
        "ami", "ach", "ego", "emu", "iej", "ich", "ych", "owa", "owe", "owej", "ska",
        "skie", "ski", "ej", "owi", "ow", "em", "ie", "a", "e", "y", "u",
    ] {
        if normalized.len() > suffix.len() + 2 && normalized.ends_with(suffix) {
            return normalized[..normalized.len() - suffix.len()].to_string();
        }
    }
    normalized
}

fn street_token_matches(actual: &str, expected: &str) -> bool {
    actual == expected || actual.starts_with(expected) || expected.starts_with(actual)
}

fn street_tail<'a>(segment: &'a str, candidate: &str) -> Option<&'a str> {
    let expected = tokens_with_spans(crate::utils::strip_street_prefixes(candidate.trim()));
    let actual = tokens_with_spans(segment);
    if expected.is_empty() || actual.len() < expected.len() {
        return None;
    }
    for start in 0..=actual.len() - expected.len() {
        if actual[start..start + expected.len()]
            .iter()
            .zip(&expected)
            .all(|(actual, expected)| street_token_matches(&actual.value, &expected.value))
        {
            return Some(&segment[actual[start + expected.len() - 1].end..]);
        }
    }
    None
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

fn location_city(location: &str) -> Option<&'static str> {
    let locality = location.split(';').next()?.trim();
    let normalized = normalize(locality);
    if normalized == "miejscowosc: chorzow" {
        Some("chorzow")
    } else if normalized == "miejscowosc: swietochlowice" {
        Some("swietochlowice")
    } else {
        None
    }
}

fn matching_street_tails(location: &str, address: &AddressEntry) -> Vec<String> {
    let streets = configured_streets(address);
    if streets.is_empty() {
        return Vec::new();
    }
    location
        .split(';')
        .skip(1)
        .filter_map(|segment| {
            streets
                .iter()
                .find_map(|street| street_tail(segment, street))
                .map(str::trim)
                .map(str::to_string)
        })
        .collect()
}

fn address_matches_city(city: &str, address: &AddressEntry) -> bool {
    let normalized_city = normalize(city);
    let normalize_municipality = |value: &str| {
        let mut normalized = normalize(value);
        for prefix in ["gmina miejska ", "gmina ", "gm ", "miasto ", "m "] {
            if let Some(value) = normalized.strip_prefix(prefix) {
                normalized = value.to_string();
                break;
            }
        }
        normalized
    };
    let address_city = normalize_municipality(&address.city_name);
    let address_commune = normalize_municipality(&address.commune);
    match normalized_city.as_str() {
        "chorzow" => address_city == "chorzow" || address_commune == "chorzow",
        "swietochlowice" => {
            address_city == "swietochlowice" || address_commune == "swietochlowice"
        }
        _ => false,
    }
}

/// Matches the article's explicit locality and any listed affected street to a configured address.
/// A city-wide notice without street entries matches all addresses in that locality.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    if !crate::api_logic::is_address_applicable_for_provider(&AlertSource::Chspwik, address) {
        return false;
    }
    if let Some(city) = location_city(location)
        && !address_matches_city(city, address)
    {
        return false;
    }

    let street_entries = location.split(';').skip(1).collect::<Vec<_>>();
    if street_entries.is_empty() {
        return true;
    }
    !matching_street_tails(location, address).is_empty()
}

fn parse_house_number(value: &str) -> Option<HouseNumber> {
    let value = value.trim().to_ascii_lowercase().replace(' ', "");
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    let number = digits.parse().ok()?;
    let suffix = value[digits.len()..]
        .chars()
        .next()
        .filter(|character| character.is_ascii_alphabetic());
    Some(HouseNumber { number, suffix })
}

fn house_number_in_tail(tail: &str, requested: &str) -> Option<bool> {
    let requested = parse_house_number(requested)?;
    let ranges = house_range_regex()
        .captures_iter(tail)
        .filter_map(|capture| {
            Some((
                parse_house_number(capture.get(1)?.as_str())?,
                parse_house_number(capture.get(2)?.as_str())?,
            ))
        })
        .collect::<Vec<_>>();
    let listed = house_number_regex()
        .captures_iter(tail)
        .filter_map(|capture| parse_house_number(capture.get(1)?.as_str()))
        .collect::<Vec<_>>();
    if ranges.is_empty() && listed.is_empty() {
        return None;
    }

    let in_range = ranges.into_iter().any(|(start, end)| {
        let (low, high) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        requested >= low && requested <= high
    });
    Some(in_range || listed.contains(&requested))
}

/// Applies house-number matching only when the caller's shared setting enables it.
/// Missing or unparseable notice ranges do not discard a street-matched alert.
pub fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }
    let tails = matching_street_tails(location, address);
    if tails.is_empty() {
        return location.split(';').count() <= 1;
    }
    let matches = tails
        .iter()
        .filter_map(|tail| house_number_in_tail(tail, &address.house_no))
        .collect::<Vec<_>>();
    matches.is_empty() || matches.into_iter().any(|matches| matches)
}

fn notice_identity(notice: &Notice) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(&notice.title));
    hasher.update(normalize(&notice.body));
    let digest = hasher.finalize();
    format!(
        "chspwik_{}",
        digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn parse_notice_html(html: &str, fallback_title: &str, settings: &Settings) -> Option<UnifiedAlert> {
    let notice = parse_notice(html, fallback_title)?;
    let (start, end) = parse_event_range(&notice.body);
    let location = format_location(&notice);
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| address.is_active && location_matches_address(&location, address))
        .map(|(index, _)| index);

    Some(UnifiedAlert {
        source: AlertSource::Chspwik,
        startDate: start,
        endDate: end,
        message: Some(format!("{} — {}", notice.title, notice.body)),
        location: Some(location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(notice_identity(&notice)),
    })
}

async fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    response.text().await.map_err(|error| error.to_string())
}

#[async_trait]
impl AlertProvider for ChspwikProvider {
    fn id(&self) -> String {
        "chspwik".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Chspwik
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

        let archive = match fetch_text(client, CHSPWIK_OUTAGE_PAGE_URL).await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("ChŚPWiK outage list request failed: {error}")],
                );
            }
        };
        let links = parse_notice_links(&archive);
        let mut alerts = Vec::new();
        let mut errors = Vec::new();

        for link in links {
            match fetch_text(client, &link.url).await {
                Ok(html) => {
                    if let Some(alert) = parse_notice_html(&html, &link.title, settings) {
                        alerts.push(alert);
                    }
                }
                Err(error) => errors.push(format!(
                    "ChŚPWiK notice fetch failed for {}: {error}",
                    link.url
                )),
            }
        }

        (alerts, errors)
    }
}
