use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use sha2::Digest;
use std::collections::HashSet;
use std::sync::OnceLock;

pub const LPWIK_LEGNICA_LISTING_URL: &str = "https://www.lpwiksa.pl/lpwik/awarie-i-remonty";

const ORIGIN: &str = "https://www.lpwiksa.pl";
const NOTICE_PATH: &str = "/lpwik/awarie-i-remonty/";
const LEGNICA_CITY_ID: u64 = 954_047;

pub struct LpwikLegnicaProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct NoticeLink {
    url: String,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotice {
    url: String,
    title: String,
    body: String,
    start_date: Option<NaiveDateTime>,
    end_date: Option<NaiveDateTime>,
    location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetMention {
    name: String,
    house_specs: Vec<String>,
}

fn listing_link_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("ul.aktualnosci-lista a[href]").unwrap())
}

fn any_link_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("a[href]").unwrap())
}

fn normalize_notice_url(href: &str) -> Option<String> {
    let href = href.trim();
    let path_and_query = if let Some(rest) = href.strip_prefix(ORIGIN) {
        rest
    } else if href.starts_with("https://") || href.starts_with("http://") {
        return None;
    } else if href.starts_with('/') {
        href
    } else {
        return None;
    };

    let path = path_and_query.split(['?', '#']).next().unwrap_or_default();
    let lower_path = path.to_ascii_lowercase();
    if !path.starts_with(NOTICE_PATH)
        || path.len() <= NOTICE_PATH.len()
        || lower_path.contains("%2e")
        || lower_path.contains("%2f")
        || path.contains('\\')
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return None;
    }

    Some(format!("{ORIGIN}{path}"))
}

fn text_content(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_listing(html: &str) -> Vec<NoticeLink> {
    let document = Html::parse_document(html);
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    let category_links: Vec<_> = document.select(listing_link_selector()).collect();
    let candidates = if category_links.is_empty() {
        document.select(any_link_selector()).collect::<Vec<_>>()
    } else {
        category_links
    };

    for anchor in candidates {
        let Some(url) = anchor.value().attr("href").and_then(normalize_notice_url) else {
            continue;
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        let title = text_content(anchor);
        if title.is_empty() {
            continue;
        }
        links.push(NoticeLink { url, title });
    }

    links
}

fn article_title_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("article h1, main h1, h1").unwrap())
}

fn article_container_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("article").unwrap())
}

fn article_body_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| {
        Selector::parse(
            "[itemprop='articleBody'], .aktualnosc, .art-content, .article-body, .article-content, .tresc",
        )
        .unwrap()
    })
}

fn article_paragraph_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("p, li").unwrap())
}

#[cfg(test)]
fn article_publication_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| {
        Selector::parse(
            "time[datetime], article .data, article .date, article .aktualnosci__data, article time",
        )
        .unwrap()
    })
}

fn extract_article_body(document: &Html) -> String {
    if let Some(body) = document.select(article_body_selector()).next() {
        let value = text_content(body);
        if !value.is_empty() {
            return value;
        }
    }

    if let Some(article) = document.select(article_container_selector()).next() {
        let paragraphs = article
            .select(article_paragraph_selector())
            .map(text_content)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>();
        if !paragraphs.is_empty() {
            return paragraphs.join("\n");
        }
    }

    String::new()
}

fn normalize_polish(value: &str) -> String {
    value
        .to_lowercase()
        .replace('ą', "a")
        .replace('ć', "c")
        .replace('ę', "e")
        .replace('ł', "l")
        .replace('ń', "n")
        .replace('ó', "o")
        .replace('ś', "s")
        .replace(['ź', 'ż'], "z")
}

#[cfg(test)]
fn parse_polish_publication_date(value: &str) -> Option<NaiveDate> {
    let normalized = normalize_polish(value);
    if let Ok(date) = NaiveDate::parse_from_str(normalized.trim(), "%Y-%m-%d") {
        return Some(date);
    }
    let date_re = Regex::new(r"(?i)(\d{1,2})\s+([a-z]+)\s+(\d{4})").ok()?;
    let captures = date_re.captures(&normalized)?;
    let day = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let month_word = captures.get(2)?.as_str();
    let year = captures.get(3)?.as_str().parse::<i32>().ok()?;
    let month = match month_word {
        "stycznia" | "styczen" => 1,
        "lutego" | "luty" => 2,
        "marca" | "marzec" => 3,
        "kwietnia" | "kwiecien" => 4,
        "maja" | "maj" => 5,
        "czerwca" | "czerwiec" => 6,
        "lipca" | "lipiec" => 7,
        "sierpnia" | "sierpien" => 8,
        "wrzesnia" | "wrzesien" => 9,
        "pazdziernika" | "pazdziernik" => 10,
        "listopada" | "listopad" => 11,
        "grudnia" | "grudzien" => 12,
        _ => return None,
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

#[cfg(test)]
fn parse_publication_date(document: &Html) -> Option<NaiveDate> {
    for element in document.select(article_publication_selector()) {
        if let Some(datetime) = element.value().attr("datetime")
            && let Some(date) = parse_polish_publication_date(datetime)
        {
            return Some(date);
        }
        if let Some(date) = parse_polish_publication_date(&text_content(element)) {
            return Some(date);
        }
    }
    None
}

fn event_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)(\d{1,2})\.(\d{1,2})\.(\d{4})").unwrap())
}

fn time_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:od\s+(?:godz(?:iny)?\.?\s*)?)?(\d{1,2}[:.]\d{2})\s*(?:-|–|—|do)\s*(\d{1,2}[:.]\d{2})",
        )
        .unwrap()
    })
}

fn explicit_start_time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\bod\s+(?:godz(?:iny)?\.?\s*)?(\d{1,2}[:.]\d{2})").unwrap()
    })
}

fn explicit_end_time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\bdo\s+(?:godz(?:iny)?\.?\s*)?(\d{1,2}[:.]\d{2})").unwrap()
    })
}

fn parse_event_date(value: &str) -> Option<NaiveDate> {
    let captures = event_date_regex().captures(value)?;
    let day = captures.get(1)?.as_str().parse::<u32>().ok()?;
    let month = captures.get(2)?.as_str().parse::<u32>().ok()?;
    let year = captures.get(3)?.as_str().parse::<i32>().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn parse_event_time(value: &str) -> Option<NaiveTime> {
    let normalized = value.replace('.', ":");
    NaiveTime::parse_from_str(&normalized, "%H:%M").ok()
}

fn extract_schedule(body: &str) -> (Option<NaiveDateTime>, Option<NaiveDateTime>) {
    let Some(date) = parse_event_date(body) else {
        return (None, None);
    };

    let range = time_range_regex().captures(body);
    let (start_time, end_time) = if let Some(captures) = range {
        (
            captures
                .get(1)
                .and_then(|value| parse_event_time(value.as_str())),
            captures
                .get(2)
                .and_then(|value| parse_event_time(value.as_str())),
        )
    } else {
        (
            explicit_start_time_regex()
                .captures(body)
                .and_then(|captures| captures.get(1))
                .and_then(|value| parse_event_time(value.as_str())),
            explicit_end_time_regex()
                .captures(body)
                .and_then(|captures| captures.get(1))
                .and_then(|value| parse_event_time(value.as_str())),
        )
    };

    let start = start_time.map(|time| date.and_time(time));
    let mut end = end_time.map(|time| date.and_time(time));
    if let (Some(start), Some(end_value)) = (start, end)
        && end_value < start
    {
        end = Some(end_value + Duration::days(1));
    }
    (start, end)
}

fn has_customer_impact(
    title: &str,
    body: &str,
    schedule: (Option<NaiveDateTime>, Option<NaiveDateTime>),
) -> bool {
    let normalized = normalize_polish(&format!("{title} {body}"));
    let interruption = normalized.contains("przerwe w dostawach wody")
        || normalized.contains("przerwa w dostawach wody")
        || normalized.contains("przerwy w dostawach wody")
        || normalized.contains("przerwe w dostawie wody")
        || normalized.contains("przerwa w dostawie wody")
        || normalized.contains("brak dostaw wody")
        || normalized.contains("brak wody")
        || normalized.contains("braku wody")
        || normalized.contains("wstrzyman")
            && normalized.contains("dostaw")
            && normalized.contains("wod");
    let quality_impact = [
        "zabarwienie wody",
        "zabarwiona woda",
        "przebarwienia wody",
        "przebarwienie wody",
        "zmetnienie wody",
        "metna woda",
        "zwiazki zelaza",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));

    interruption || (quality_impact && (schedule.0.is_some() || schedule.1.is_some()))
}

fn strip_house_spec(chunk: &str) -> (String, Option<String>) {
    static HOUSE_SPEC: OnceLock<Regex> = OnceLock::new();
    let regex = HOUSE_SPEC.get_or_init(|| {
        Regex::new(
            r"(?i)^(.*?)(?:\s+)(?:nr\.?\s*)?(\d+[a-z]?(?:/\d+[a-z]?)?(?:\s*[-–—]\s*\d+[a-z]?(?:/\d+[a-z]?)?)?)\s*$",
        )
        .unwrap()
    });
    if let Some(captures) = regex.captures(chunk.trim()) {
        let name = captures
            .get(1)
            .map(|value| value.as_str().trim().to_string())
            .unwrap_or_default();
        let spec = captures
            .get(2)
            .map(|value| value.as_str().trim().to_string());
        (name, spec)
    } else {
        (chunk.trim().to_string(), None)
    }
}

fn standalone_house_spec(chunk: &str) -> Option<String> {
    static HOUSE_SPEC: OnceLock<Regex> = OnceLock::new();
    let regex = HOUSE_SPEC.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:nr\.?\s*)?(\d+[a-z]?(?:/\d+[a-z]?)?(?:\s*[-–—]\s*\d+[a-z]?(?:/\d+[a-z]?)?)?)\s*$",
        )
        .unwrap()
    });
    regex
        .captures(chunk)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().trim().to_string())
}

fn street_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b(?:ul\.?|ulicy)\s+([^.;\n]+)").unwrap())
}

fn extract_street_mentions(text: &str) -> Vec<StreetMention> {
    let mut streets: Vec<StreetMention> = Vec::new();

    for capture in street_marker_regex().captures_iter(text) {
        let Some(list) = capture.get(1).map(|value| value.as_str()) else {
            continue;
        };
        for raw_chunk in list.split(',') {
            let chunk = raw_chunk
                .trim()
                .trim_matches(|character: char| character.is_whitespace() || character == '.')
                .trim_start_matches([';', ':'])
                .trim();
            if chunk.is_empty() {
                continue;
            }

            if let Some(spec) = standalone_house_spec(chunk) {
                if let Some(previous) = streets.last_mut() {
                    previous.house_specs.push(spec);
                }
                continue;
            }

            let (name, house_spec) = strip_house_spec(chunk);
            let name = name
                .trim_start_matches(|character: char| character.is_whitespace())
                .trim_start_matches("oraz ")
                .trim();
            if name.is_empty() {
                continue;
            }
            let mut mention = StreetMention {
                name: name.to_string(),
                house_specs: Vec::new(),
            };
            if let Some(spec) = house_spec {
                mention.house_specs.push(spec);
            }
            streets.push(mention);
        }
    }

    streets
}

fn unique_street_mentions(text: &str) -> Vec<StreetMention> {
    let mut result: Vec<StreetMention> = Vec::new();
    for mention in extract_street_mentions(text) {
        if let Some(existing) = result
            .iter_mut()
            .find(|existing| street_names_match(&existing.name, &mention.name))
        {
            for spec in mention.house_specs {
                if !existing.house_specs.contains(&spec) {
                    existing.house_specs.push(spec);
                }
            }
        } else {
            result.push(mention);
        }
    }
    result
}

fn street_stem(token: &str) -> String {
    for suffix in ["iego", "iej", "ego", "ej", "a", "i", "y"] {
        if token.len() > suffix.len() + 2 && token.ends_with(suffix) {
            return token[..token.len() - suffix.len()].to_string();
        }
    }
    token.to_string()
}

fn street_tokens(value: &str) -> Vec<String> {
    let stripped = crate::utils::strip_street_prefixes(value.trim());
    normalize_polish(stripped)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() > 1)
        .map(street_stem)
        .collect()
}

fn street_names_match(configured: &str, mentioned: &str) -> bool {
    let configured_tokens = street_tokens(configured);
    let mentioned_tokens = street_tokens(mentioned);
    !configured_tokens.is_empty()
        && configured_tokens.iter().all(|candidate| {
            mentioned_tokens.iter().any(|mentioned| {
                candidate == mentioned
                    || candidate.starts_with(mentioned)
                    || mentioned.starts_with(candidate)
            })
        })
}

fn address_has_street(address: &AddressEntry) -> bool {
    !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty())
}

fn address_is_in_legnica(address: &AddressEntry) -> bool {
    let voivodeship = normalize_polish(&address.voivodeship);
    if !voivodeship.trim().is_empty() && !voivodeship.trim().starts_with("dolnoslaskie") {
        return false;
    }

    if let Some(city_id) = address.city_id {
        return city_id == LEGNICA_CITY_ID;
    }

    let city = normalize_polish(&address.city_name);
    city.trim().starts_with("legnica")
}

fn address_street_candidates(address: &AddressEntry) -> Vec<String> {
    let mut candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    if let Some(second) = address.street_name_2.as_deref()
        && !second.trim().is_empty()
        && !candidates.contains(&second.to_string())
    {
        candidates.push(second.to_string());
    }
    candidates
}

fn notice_streets_match_address(streets: &[StreetMention], address: &AddressEntry) -> bool {
    if !address_has_street(address) {
        return true;
    }
    let candidates = address_street_candidates(address);
    streets.iter().any(|notice_street| {
        candidates
            .iter()
            .any(|candidate| street_names_match(candidate, &notice_street.name))
    })
}

fn is_clearly_citywide_notice(message: &str, location: &str) -> bool {
    let normalized = normalize_polish(&format!("{message} {location}"));
    [
        "w calej legnicy",
        "na terenie legnicy",
        "na terenie miasta legnica",
        "w calym miescie",
        "wszyscy odbiorcy w legnicy",
        "mieszkancow legnicy",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

/// Matches a Legnica notice to the configured city and street without applying house-number filtering.
pub fn address_matches_notice(message: &str, location: &str, address: &AddressEntry) -> bool {
    if !address_is_in_legnica(address) {
        return false;
    }
    if !address_has_street(address) {
        return true;
    }

    let combined = format!("{message}\n{location}");
    let streets = unique_street_mentions(&combined);
    if streets.is_empty() {
        return is_clearly_citywide_notice(message, location);
    }
    notice_streets_match_address(&streets, address)
}

fn parse_house_value(value: &str) -> Option<(u32, String, Option<String>)> {
    static NUMBER: OnceLock<Regex> = OnceLock::new();
    let regex = NUMBER.get_or_init(|| {
        Regex::new(r"(?i)^\s*(\d+)\s*([a-z]?)\s*(?:/\s*(\d+)\s*([a-z]?))?\s*$").unwrap()
    });
    let captures = regex.captures(value.trim())?;
    Some((
        captures.get(1)?.as_str().parse().ok()?,
        captures[2].to_ascii_lowercase(),
        captures
            .get(3)
            .map(|value| format!("{}{}", value.as_str(), captures[4].to_ascii_lowercase())),
    ))
}

fn house_number_matches_spec(house_no: &str, spec: &str) -> bool {
    let Some((house_number, house_suffix, house_unit)) = parse_house_value(house_no) else {
        return false;
    };
    let parts = spec
        .split(['-', '–', '—'])
        .map(str::trim)
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [single] => parse_house_value(single).is_some_and(|(number, suffix, unit)| {
            number == house_number
                && (suffix.is_empty() || suffix == house_suffix)
                && (unit.is_none() || unit == house_unit)
        }),
        [first, last] => {
            let (Some((first, _, _)), Some((last, _, _))) =
                (parse_house_value(first), parse_house_value(last))
            else {
                return false;
            };
            house_number >= first.min(last) && house_number <= first.max(last)
        }
        _ => false,
    }
}

/// Applies a notice's street-scoped house-number range to a Legnica address.
pub fn house_number_matches_notice(message: &str, location: &str, address: &AddressEntry) -> bool {
    if !address_is_in_legnica(address) {
        return false;
    }
    if !address_has_street(address) {
        return true;
    }
    if !address_matches_notice(message, location, address) {
        return false;
    }

    let combined = format!("{message}\n{location}");
    let streets = unique_street_mentions(&combined);
    if streets.is_empty() {
        return is_clearly_citywide_notice(message, location);
    }

    let matching = streets
        .iter()
        .filter(|notice_street| {
            address_street_candidates(address)
                .iter()
                .any(|candidate| street_names_match(candidate, &notice_street.name))
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return false;
    }

    // If a street has explicit ranges, those ranges take precedence over an unscoped title mention.
    let explicit_specs = matching
        .iter()
        .flat_map(|street| street.house_specs.iter())
        .collect::<Vec<_>>();
    if explicit_specs.is_empty() {
        return true;
    }
    explicit_specs
        .iter()
        .any(|spec| house_number_matches_spec(&address.house_no, spec))
}

fn format_location(message: &str) -> String {
    let streets = unique_street_mentions(message);
    if streets.is_empty() {
        return "Miejscowość: Legnica".to_string();
    }
    let street_details = streets
        .iter()
        .map(|street| {
            if street.house_specs.is_empty() {
                format!("ul. {}", street.name)
            } else {
                format!("ul. {} {}", street.name, street.house_specs.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("Miejscowość: Legnica; {street_details}")
}

fn parse_article(html: &str, link: &NoticeLink) -> Option<ParsedNotice> {
    let document = Html::parse_document(html);
    let body = extract_article_body(&document);
    if body.is_empty() {
        return None;
    }
    let title = document
        .select(article_title_selector())
        .next()
        .map(text_content)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| link.title.clone());
    if title.is_empty() {
        return None;
    }

    let schedule = extract_schedule(&body);
    if !has_customer_impact(&title, &body, schedule) {
        return None;
    }
    let full_message = format!("{title}\n{body}");
    let (start_date, end_date) = schedule;
    Some(ParsedNotice {
        url: link.url.clone(),
        title,
        body,
        start_date,
        end_date,
        location: format_location(&full_message),
    })
}

fn format_datetime(value: Option<NaiveDateTime>) -> Option<String> {
    value.map(|date| date.format("%Y-%m-%dT%H:%M:%S").to_string())
}

fn alert_from_notice(notice: ParsedNotice, settings: &Settings) -> UnifiedAlert {
    let message = format!("{}\n{}", notice.title.trim(), notice.body.trim());
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::LpwikLegnica,
                    address,
                )
                && address_matches_notice(&message, &notice.location, address)
        })
        .map(|(index, _)| index);
    let digest = sha2::Sha256::digest(notice.url.as_bytes());
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    UnifiedAlert {
        source: AlertSource::LpwikLegnica,
        startDate: format_datetime(notice.start_date),
        endDate: format_datetime(notice.end_date),
        message: Some(message),
        location: Some(notice.location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(hash),
    }
}

#[async_trait]
impl AlertProvider for LpwikLegnicaProvider {
    fn id(&self) -> String {
        "lpwik_legnica".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::LpwikLegnica
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

        let mut errors = Vec::new();
        let listing_response = match client.get(LPWIK_LEGNICA_LISTING_URL).send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                return (
                    Vec::new(),
                    vec![format!(
                        "LPWiK Legnica listing HTTP error: {}",
                        response.status()
                    )],
                );
            }
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("LPWiK Legnica listing request error: {error}")],
                );
            }
        };
        let listing_html = match listing_response.text().await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("LPWiK Legnica listing read error: {error}")],
                );
            }
        };

        let links = parse_listing(&listing_html);
        let mut alerts = Vec::new();
        let mut seen_hashes = HashSet::new();
        for link in links {
            let response = match client.get(&link.url).send().await {
                Ok(response) if response.status().is_success() => response,
                Ok(response) => {
                    errors.push(format!(
                        "LPWiK Legnica article HTTP error for {}: {}",
                        link.url,
                        response.status()
                    ));
                    continue;
                }
                Err(error) => {
                    errors.push(format!(
                        "LPWiK Legnica article request error for {}: {error}",
                        link.url
                    ));
                    continue;
                }
            };
            let html = match response.text().await {
                Ok(html) => html,
                Err(error) => {
                    errors.push(format!(
                        "LPWiK Legnica article read error for {}: {error}",
                        link.url
                    ));
                    continue;
                }
            };
            let Some(notice) = parse_article(&html, &link) else {
                continue;
            };
            let alert = alert_from_notice(notice, settings);
            if alert
                .hash
                .as_ref()
                .is_some_and(|hash| seen_hashes.insert(hash.clone()))
            {
                alerts.push(alert);
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING_FIXTURE: &str = r#"
        <ul class="aktualnosci-lista">
          <li><a href="/lpwik/awarie-i-remonty/naprawa-sokolska/idn:914">Prace naprawcze przy ul. Sokolskiej</a><time>2026-10-22</time></li>
          <li><a href="https://www.lpwiksa.pl/lpwik/awarie-i-remonty/diagnostyka-grabskiego/idn:913">Prace diagnostyczne przy ul. Grabskiego</a></li>
          <li><a href="/lpwik/awarie-i-remonty/rolnicza/idn:912">Prace przy ul. Rolniczej</a></li>
          <li><a href="/lpwik/awarie-i-remonty/fieldorfa/idn:911">Prace przy ul. Fieldorfa</a></li>
          <li><a href="/lpwik/awarie-i-remonty/powstancow/idn:910">Prace przy ul. Powstańców Śl.</a></li>
          <li><a href="https://attacker.example/lpwik/awarie-i-remonty/fake/idn:999">External</a></li>
          <li><a href="/lpwik/inwestycje/fake/idn:998">Other section</a></li>
        </ul>
    "#;

    const SOKOLSKA_ARTICLE: &str = r#"
        <article>
          <div class="aktualnosc-naglowek"><h1>Prace naprawcze na urządzeniach wodociągowych przy ul. Sokolskiej.</h1>
            <span class="aktualnosci__data">22 Października 2026</span></div>
          <div class="aktualnosc">
            <p>LPWiK S.A. informuje, że w dniu 26.10.2026 r. będą miały miejsce prace naprawcze przy ul. Sokolskiej, Jaskółczej.</p>
            <p>Przewidywany czas prac: 8:00 – 14:00.</p>
            <p>Zakres prac spowoduje przerwę w dostawach wody do obiektów przy ul. Sokolskiej 1 - 37, Jaskółczej 1 - 3.</p>
            <p>Po zakończeniu prac mogą wystąpić przebarwienia wody związkami żelaza.</p>
          </div>
        </article>
    "#;

    const GRABSKIEGO_ARTICLE: &str = r#"
        <article>
          <header><h1>Prace diagnostyczne na sieci wodociągowej przy ul. Grabskiego.</h1>
            <time datetime="2026-10-22">22 Października 2026</time></header>
          <div class="aktualnosc">
            <p>W dniu 27.10.2026r., w godz. od 08:00 do 14:00 może wystąpić zabarwienie wody związkami żelaza podczas prac przy ul. Grabskiego, Słowackiego, Złotoryjskiej, Mieszka I, Sienkiewicza.</p>
            <p>W celu szybkiego usunięcia zabarwienia będzie wykonywane płukanie sieci wodociągowej.</p>
          </div>
        </article>
    "#;

    fn address(city: &str, street: &str, house_no: &str) -> AddressEntry {
        AddressEntry {
            city_name: city.to_string(),
            street_name: street.to_string(),
            house_no: house_no.to_string(),
            city_id: (city == "Legnica").then_some(LEGNICA_CITY_ID),
            ..Default::default()
        }
    }

    #[test]
    fn listing_only_returns_valid_notice_path_links_and_handles_empty_list() {
        let links = parse_listing(LISTING_FIXTURE);
        assert_eq!(links.len(), 5);
        assert!(links[0].url.ends_with("/naprawa-sokolska/idn:914"));
        assert!(parse_listing("<ul class='aktualnosci-lista'></ul>").is_empty());
    }

    #[test]
    fn url_validation_rejects_other_hosts_and_path_traversal() {
        assert!(
            normalize_notice_url("https://attacker.example/lpwik/awarie-i-remonty/fake").is_none()
        );
        assert!(normalize_notice_url("/lpwik/awarie-i-remonty/../inwestycje").is_none());
        assert!(normalize_notice_url("/lpwik/awarie-i-remonty/%2e%2e/inwestycje").is_none());
        assert!(normalize_notice_url("/lpwik/inwestycje/idn:1").is_none());
    }

    #[test]
    fn article_publication_date_is_separate_from_notice_event_schedule() {
        let link = NoticeLink {
            url: format!("{ORIGIN}{NOTICE_PATH}naprawa-sokolska/idn:914"),
            title: "Prace naprawcze przy ul. Sokolskiej".to_string(),
        };
        let notice = parse_article(SOKOLSKA_ARTICLE, &link).unwrap();
        assert_eq!(
            parse_publication_date(&Html::parse_document(SOKOLSKA_ARTICLE)),
            NaiveDate::from_ymd_opt(2026, 10, 22)
        );
        assert_eq!(
            notice
                .start_date
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-26 08:00"
        );
        assert_eq!(
            notice
                .end_date
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-26 14:00"
        );
        assert!(notice.body.contains("przerwę w dostawach wody"));
    }

    #[test]
    fn diagnostics_warning_is_included_without_claiming_a_water_cut() {
        let link = NoticeLink {
            url: format!("{ORIGIN}{NOTICE_PATH}diagnostyka-grabskiego/idn:913"),
            title: "Prace diagnostyczne przy ul. Grabskiego".to_string(),
        };
        let notice = parse_article(GRABSKIEGO_ARTICLE, &link).unwrap();
        assert_eq!(
            notice
                .start_date
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-27 08:00"
        );
        assert_eq!(
            notice
                .end_date
                .unwrap()
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            "2026-10-27 14:00"
        );
        assert!(notice.body.contains("może wystąpić zabarwienie wody"));
        assert!(!normalize_polish(&notice.body).contains("przerwe w dostawach wody"));
    }

    #[test]
    fn missing_event_bounds_stay_unknown_and_general_news_is_excluded() {
        assert_eq!(
            extract_schedule("Prace przy ul. Sokolskiej 26.10.2026 r."),
            (None, None)
        );
        let (start, end) = extract_schedule("Prace 26.10.2026 r. od godz. 08:00.");
        assert_eq!(
            start.unwrap().format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-26 08:00"
        );
        assert!(end.is_none());

        let generic = r#"<article><h1>Nowy samochód dla LPWiK</h1><div class="art-content"><p>Spółka zakupiła nowy pojazd.</p></div></article>"#;
        let link = NoticeLink {
            url: format!("{ORIGIN}{NOTICE_PATH}news/idn:1"),
            title: "Nowy samochód".to_string(),
        };
        assert!(parse_article(generic, &link).is_none());
    }

    #[test]
    fn notice_matches_inflected_street_names_without_filtering_house_numbers() {
        let message = "Przerwa w dostawach wody. ul. Sokolskiej 1 - 37, Jaskółczej 1 - 3.";
        let location = "Miejscowość: Legnica; ul. Sokolskiej 1 - 37; ul. Jaskółczej 1 - 3";
        assert!(address_matches_notice(
            message,
            location,
            &address("Legnica", "Sokolska", "40")
        ));
        assert!(!house_number_matches_notice(
            message,
            location,
            &address("Legnica", "Sokolska", "40")
        ));
        assert!(house_number_matches_notice(
            message,
            location,
            &address("Legnica", "Sokolska", "20")
        ));
        assert!(house_number_matches_notice(
            message,
            location,
            &address("Legnica", "Jaskółcza", "2")
        ));
        assert!(!house_number_matches_notice(
            message,
            location,
            &address("Legnica", "Jaskółcza", "20")
        ));
    }

    #[test]
    fn diagnostic_article_street_inflections_match_configured_streets() {
        let link = NoticeLink {
            url: format!("{ORIGIN}{NOTICE_PATH}diagnostyka-grabskiego/idn:913"),
            title: "Prace diagnostyczne przy ul. Grabskiego".to_string(),
        };
        let notice = parse_article(GRABSKIEGO_ARTICLE, &link).unwrap();
        let message = format!("{}\n{}", notice.title, notice.body);
        assert!(address_matches_notice(
            &message,
            &notice.location,
            &address("Legnica", "Grabski", "4")
        ));
        assert!(address_matches_notice(
            &message,
            &notice.location,
            &address("Legnica", "Złotoryjska", "12")
        ));
        assert!(!address_matches_notice(
            &message,
            &notice.location,
            &address("Wrocław", "Grabski", "4")
        ));
    }

    #[test]
    fn house_number_lists_support_units_and_nr_prefix_without_cross_street_leakage() {
        let message = "Przerwa w dostawie wody przy ul. Katowickiej 37/1, 37/2.";
        let location = format_location(message);
        assert!(house_number_matches_notice(
            message,
            &location,
            &address("Legnica", "Katowicka", "37/1")
        ));
        assert!(house_number_matches_notice(
            message,
            &location,
            &address("Legnica", "Katowicka", "37/2")
        ));
        assert!(!house_number_matches_notice(
            message,
            &location,
            &address("Legnica", "Katowicka", "37/3")
        ));

        let numbered_message = "Przerwa w dostawie wody przy ul. Katowickiej nr 27, 29, 33.";
        let numbered_location = format_location(numbered_message);
        assert!(house_number_matches_notice(
            numbered_message,
            &numbered_location,
            &address("Legnica", "Katowicka", "29")
        ));
        assert!(!house_number_matches_notice(
            numbered_message,
            &numbered_location,
            &address("Legnica", "Katowicka", "28")
        ));
        assert!(!house_number_matches_notice(
            numbered_message,
            &numbered_location,
            &address("Legnica", "Sokolska", "29")
        ));
    }

    #[test]
    fn matching_rejects_conflicting_city_ids_and_non_dolnoslaskie_addresses() {
        let message = "Przerwa w dostawie wody przy ul. Sokolskiej 1 - 37.";
        let location = format_location(message);
        let mut wrong_id = address("Legnica", "Sokolska", "5");
        wrong_id.city_id = Some(954_071);
        let mut wrong_voivodeship = address("Legnica", "Sokolska", "5");
        wrong_voivodeship.voivodeship = "Lubuskie".to_string();
        let mut correct_voivodeship = address("Legnica", "Sokolska", "5");
        correct_voivodeship.voivodeship = "Dolnośląskie".to_string();

        assert!(!address_matches_notice(message, &location, &wrong_id));
        assert!(!address_matches_notice(
            message,
            &location,
            &wrong_voivodeship
        ));
        assert!(address_matches_notice(
            message,
            &location,
            &correct_voivodeship
        ));
    }
}
