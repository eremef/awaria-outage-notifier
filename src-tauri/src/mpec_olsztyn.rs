use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const MPEC_OLSZTYN_OUTAGES_URL: &str = "https://mpec.olsztyn.pl/awarie-i-wylaczenia/";
const SITE_HOST: &str = "mpec.olsztyn.pl";

pub struct MpecOlsztynProvider;

#[derive(Debug, Clone)]
struct NoticeLink {
    url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetNotice {
    name: String,
    house_numbers: String,
}

#[derive(Debug, Clone)]
struct ParsedNotice {
    title: String,
    body: String,
    start_date: Option<String>,
    end_date: Option<String>,
    streets: Vec<StreetNotice>,
    city_wide: bool,
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

fn clean_text(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn element_text(element: ElementRef<'_>) -> String {
    clean_text(&element.text().collect::<Vec<_>>().join(" "))
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"\b(\d{1,2})\s*[./-]\s*(\d{1,2})\s*[./-]\s*(20\d{2})\b").unwrap()
    })
}

fn named_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)\s+(20\d{2})\b",
        )
        .unwrap()
    })
}

fn clock_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b([01]?\d|2[0-3]):([0-5]\d)\b").unwrap())
}

fn house_number_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b\d+[a-z]?(?:/\d+[a-z]?)?\b").unwrap())
}

fn house_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b(\d+)[a-z]?\s*[-–—]\s*(\d+)[a-z]?\b").unwrap())
}

fn first_house_number_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b\d+[a-z]?(?:/\d+[a-z]?)?(?:\s*[-–—]\s*\d+[a-z]?(?:/\d+[a-z]?)?)?\b")
            .unwrap()
    })
}

fn month_number(month: &str) -> Option<u32> {
    Some(match month {
        "stycznia" => 1,
        "lutego" => 2,
        "marca" => 3,
        "kwietnia" => 4,
        "maja" => 5,
        "czerwca" => 6,
        "lipca" => 7,
        "sierpnia" => 8,
        "wrzesnia" => 9,
        "pazdziernika" => 10,
        "listopada" => 11,
        "grudnia" => 12,
        _ => return None,
    })
}

fn date_occurrences(text: &str) -> Vec<(usize, NaiveDate)> {
    let normalized = normalize_polish(text);
    let mut dates = Vec::new();

    for captures in numeric_date_regex().captures_iter(&normalized) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        let parsed = NaiveDate::from_ymd_opt(
            captures
                .get(3)
                .and_then(|value| value.as_str().parse().ok())
                .unwrap_or(0),
            captures
                .get(2)
                .and_then(|value| value.as_str().parse().ok())
                .unwrap_or(0),
            captures
                .get(1)
                .and_then(|value| value.as_str().parse().ok())
                .unwrap_or(0),
        );
        if let Some(date) = parsed {
            dates.push((full.start(), date));
        }
    }

    for captures in named_date_regex().captures_iter(&normalized) {
        let Some(full) = captures.get(0) else {
            continue;
        };
        let parsed = captures
            .get(1)
            .and_then(|value| value.as_str().parse::<u32>().ok())
            .zip(
                captures
                    .get(2)
                    .and_then(|value| month_number(value.as_str())),
            )
            .zip(
                captures
                    .get(3)
                    .and_then(|value| value.as_str().parse::<i32>().ok()),
            )
            .and_then(|((day, month), year)| NaiveDate::from_ymd_opt(year, month, day));
        if let Some(date) = parsed {
            dates.push((full.start(), date));
        }
    }

    dates.sort_by_key(|(position, _)| *position);
    dates.dedup_by_key(|(position, _)| *position);
    dates
}

fn clock_occurrences(text: &str) -> Vec<NaiveTime> {
    let normalized = normalize_polish(text);
    clock_regex()
        .captures_iter(&normalized)
        .filter_map(|captures| {
            NaiveTime::from_hms_opt(
                captures.get(1)?.as_str().parse().ok()?,
                captures.get(2)?.as_str().parse().ok()?,
                0,
            )
        })
        .collect()
}

fn datetime_string(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let dates = date_occurrences(text);
    let Some((_, start_date)) = dates.first().copied() else {
        return (None, None);
    };
    let end_date = dates.last().map(|(_, date)| *date).unwrap_or(start_date);
    let clocks = clock_occurrences(text);

    if clocks.len() >= 2 {
        let start = start_date.and_time(clocks[0]);
        let mut end = end_date.and_time(*clocks.last().unwrap());
        if end <= start && start_date == end_date {
            end += Duration::days(1);
        }
        return (Some(datetime_string(start)), Some(datetime_string(end)));
    }

    if clocks.len() == 1 {
        return (Some(datetime_string(start_date.and_time(clocks[0]))), None);
    }

    (
        Some(start_date.format("%Y-%m-%d").to_string()),
        Some(end_date.format("%Y-%m-%d").to_string()),
    )
}

fn parse_datetime(value: &str, date_only_end: bool) -> Option<NaiveDateTime> {
    if let Ok(datetime) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Some(datetime);
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    Some(date.and_time(if date_only_end {
        NaiveTime::from_hms_opt(23, 59, 59)?
    } else {
        NaiveTime::MIN
    }))
}

fn is_expired(end_date: Option<&str>, now: NaiveDateTime) -> bool {
    end_date
        .and_then(|value| parse_datetime(value, true))
        .is_some_and(|end| end < now)
}

fn normalize_article_url(href: &str) -> Option<String> {
    let base = Url::parse(MPEC_OLSZTYN_OUTAGES_URL).ok()?;
    let url = base.join(href.trim()).ok()?;
    if url.scheme() != "https" || url.host_str() != Some(SITE_HOST) {
        return None;
    }
    let path = url.path().trim_end_matches('/');
    if path.is_empty() || path == "/awarie-i-wylaczenia" {
        return None;
    }
    Some(url.to_string())
}

fn is_heat_outage_candidate(text: &str) -> bool {
    let normalized = normalize_polish(text);
    let heat_context = normalized.contains("ciepl") || normalized.contains("ogrzewan");
    let outage_context = ["przerw", "awari", "wylacz", "ograniczen"]
        .iter()
        .any(|needle| normalized.contains(needle));
    heat_context && outage_context
}

fn is_explicit_empty_listing(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "brak aktualnych awarii",
        "brak aktualnych wylaczen",
        "brak biezacych komunikatow",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn parse_listing(html: &str) -> Result<Vec<NoticeLink>, String> {
    let document = Html::parse_document(html);
    let page_text = clean_text(&document.root_element().text().collect::<Vec<_>>().join(" "));
    let h1_selector = Selector::parse("h1").map_err(|error| error.to_string())?;
    let expected_heading = document
        .select(&h1_selector)
        .any(|heading| normalize_polish(&element_text(heading)).contains("awarie i wylaczenia"));
    if !expected_heading {
        return Err("MPEC Olsztyn listing heading was not found".to_string());
    }

    let card_selector = Selector::parse(".aktualnosc").map_err(|error| error.to_string())?;
    let cards = document.select(&card_selector).collect::<Vec<_>>();
    if cards.is_empty() {
        return if is_explicit_empty_listing(&page_text) {
            Ok(Vec::new())
        } else {
            Err(
                "MPEC Olsztyn listing has no notice cards or recognized empty-state marker"
                    .to_string(),
            )
        };
    }

    let link_selector = Selector::parse("a[href]").map_err(|error| error.to_string())?;
    let mut seen = HashSet::new();
    let mut notices = Vec::new();
    for card in cards {
        let card_text = element_text(card);
        let title = Selector::parse("h3")
            .ok()
            .and_then(|selector| card.select(&selector).next())
            .map(element_text)
            .unwrap_or_default();
        if !is_heat_outage_candidate(&format!("{title} {card_text}")) {
            continue;
        }

        let href = card
            .select(&link_selector)
            .filter_map(|link| link.value().attr("href"))
            .find_map(normalize_article_url)
            .ok_or_else(|| format!("MPEC Olsztyn notice has no same-site article link: {title}"))?;
        if seen.insert(href.clone()) {
            notices.push(NoticeLink { url: href });
        }
    }

    if notices.is_empty() {
        return Err("MPEC Olsztyn listing cards contained no recognized heat notice".to_string());
    }
    Ok(notices)
}

fn article_content<'a>(document: &'a Html) -> Option<ElementRef<'a>> {
    for selector_text in [
        "div.desc_content",
        ".entry-content",
        ".post-content",
        ".aktualnosc-tresc",
        "article",
        "main",
        "div.container-fluid",
    ] {
        let Ok(selector) = Selector::parse(selector_text) else {
            continue;
        };
        if let Some(content) = document.select(&selector).find(|content| {
            let text = element_text(*content);
            is_heat_outage_candidate(&text)
                && (is_affected_addresses_marker(&text)
                    || is_citywide_notice(&text)
                    || is_cancelled_or_restored(&text))
        }) {
            return Some(content);
        }
    }
    None
}

fn is_cancelled_or_restored(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "awaria usunieta",
        "awaria ciepla usunieta",
        "awaria ciepla zostala usunieta",
        "awaria zostala usunieta",
        "przerwa odwolana",
        "przerwa anulowana",
        "dostawa ciepla zostala przywrocona",
        "dostawy ciepla zostaly przywrocone",
        "dostawa ciepla przywrocono",
        "dostawy ciepla przywrocono",
        "dostawa cieplej wody zostala przywrocona",
        "dostawy cieplej wody zostaly przywrocone",
        "wznowiono dostawy ciepla",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn is_affected_addresses_marker(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "przerwa obejmie nastepujace budynki",
        "do nastepujacych budynkow",
        "do budynkow o nastepujacych adresach",
        "do wymienionych budynkow",
        "budynkow o nastepujacych adresach",
        "nastepujacych adresach",
        "do nizej wymienionych budynkow",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn is_affected_section_end(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "przerwa wynika",
        "utrudnienia zwiazane",
        "utrudnienia wynikaja",
        "bieżących informacji",
        "biezacych informacji",
        "administrac",
        "dziękujemy",
        "dziekujemy",
        "przepraszamy",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn parse_street_row(value: &str) -> Option<StreetNotice> {
    let row = value
        .trim()
        .trim_start_matches(['➡', '➜', '→', '•', '▪', '◦', '▶'])
        .trim_start_matches('\u{fe0f}')
        .trim();
    let number = first_house_number_regex().find(row)?;
    let mut name = row[..number.start()]
        .trim()
        .trim_end_matches([':', ',', ';', '.'])
        .trim()
        .to_string();
    let normalized_name = normalize_polish(&name);
    for prefix in ["ulicy ", "ulica ", "ul. "] {
        if normalized_name.starts_with(prefix) {
            name = name[prefix.len()..].trim().to_string();
            break;
        }
    }
    if name.is_empty() || !name.chars().any(char::is_alphabetic) {
        return None;
    }
    let house_numbers = row[number.start()..]
        .trim()
        .trim_end_matches(['.', ';', ',', ':'])
        .trim()
        .to_string();
    if house_numbers.is_empty() {
        return None;
    }
    Some(StreetNotice {
        name,
        house_numbers,
    })
}

fn extract_affected_streets(content: ElementRef<'_>) -> Vec<StreetNotice> {
    let Ok(block_selector) = Selector::parse("p, li, div[dir='auto'], div:not([class])") else {
        return Vec::new();
    };
    let blocks = content
        .select(&block_selector)
        .map(element_text)
        .collect::<Vec<_>>();
    let Some(marker_index) = blocks
        .iter()
        .position(|block| is_affected_addresses_marker(block))
    else {
        return Vec::new();
    };

    let mut streets = Vec::new();
    for block in blocks.iter().skip(marker_index + 1) {
        if is_affected_section_end(block) {
            break;
        }
        if let Some(street) = parse_street_row(block)
            && !streets.iter().any(|existing: &StreetNotice| {
                normalize_polish(&existing.name) == normalize_polish(&street.name)
            })
        {
            streets.push(street);
        }
    }
    streets
}

fn is_citywide_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "w calym miescie",
        "na terenie calego miasta",
        "w calym olsztynie",
        "na terenie calego olsztyna",
        "wszyscy odbiorcy w olsztynie",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn parse_article(html: &str) -> Result<Option<ParsedNotice>, String> {
    let document = Html::parse_document(html);
    let heading_selector = Selector::parse("h1, h2").map_err(|error| error.to_string())?;
    let title = document
        .select(&heading_selector)
        .find(|heading| is_heat_outage_candidate(&element_text(*heading)))
        .map(element_text)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| "MPEC Olsztyn article heading was not found".to_string())?;
    let content = article_content(&document)
        .ok_or_else(|| "MPEC Olsztyn article content was not recognized".to_string())?;
    let body = element_text(content);
    let combined = format!("{title} {body}");
    if !is_heat_outage_candidate(&combined) || is_cancelled_or_restored(&combined) {
        return Ok(None);
    }

    let streets = extract_affected_streets(content);
    let city_wide = is_citywide_notice(&combined);
    if streets.is_empty() && !city_wide {
        return Err("MPEC Olsztyn article has no recognized affected-address section".to_string());
    }
    let (start_date, end_date) = parse_schedule(&body);
    Ok(Some(ParsedNotice {
        title,
        body,
        start_date,
        end_date,
        streets,
        city_wide,
    }))
}

fn location_text(streets: &[StreetNotice], city_wide: bool) -> String {
    let mut parts = vec!["Olsztyn".to_string()];
    if city_wide {
        parts.push("cały Olsztyn".to_string());
    }
    parts.extend(
        streets
            .iter()
            .map(|street| format!("{} ({})", street.name, street.house_numbers)),
    );
    parts.join("; ")
}

fn tokens(value: &str) -> Vec<String> {
    normalize_polish(value)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn street_matches_segment(segment: &str, configured_street: &str) -> bool {
    let street = crate::utils::strip_street_prefixes(configured_street.trim());
    let expected = tokens(street)
        .into_iter()
        .filter(|token| token.chars().count() > 2)
        .map(|token| {
            let stem_length = token.chars().count().saturating_sub(2).max(3);
            token.chars().take(stem_length).collect::<String>()
        })
        .collect::<Vec<_>>();
    let street_name = segment.split_once('(').map_or(segment, |(name, _)| name);
    let actual = tokens(street_name);
    !expected.is_empty()
        && expected
            .iter()
            .all(|expected| actual.iter().any(|actual| actual.starts_with(expected)))
}

pub(super) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    if !crate::api_logic::is_address_applicable_for_provider(&AlertSource::MpecOlsztyn, address) {
        return false;
    }
    let mut parts = location.split(';').map(str::trim);
    if !parts
        .next()
        .is_some_and(|locality| normalize_polish(locality) == "olsztyn")
    {
        return false;
    }
    let street_candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    // TERYT's `street_name_2` is the prefix and `street_name_1` the primary
    // name; the shared helper tries both the compound and primary-name forms.
    let segments = parts.collect::<Vec<_>>();
    if segments
        .iter()
        .any(|segment| normalize_polish(segment) == "caly olsztyn")
    {
        return true;
    }
    segments.iter().any(|segment| {
        street_candidates
            .iter()
            .any(|street| !street.trim().is_empty() && street_matches_segment(segment, street))
    })
}

fn normalize_house_number(value: &str) -> String {
    normalize_polish(value).replace([' ', '\t'], "")
}

pub(super) fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }

    let street_candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let matching_segments = location
        .split(';')
        .skip(1)
        .filter(|segment| {
            normalize_polish(segment) != "caly olsztyn"
                && street_candidates
                    .iter()
                    .any(|street| street_matches_segment(segment, street))
        })
        .collect::<Vec<_>>();
    if matching_segments.is_empty() {
        return location
            .split(';')
            .skip(1)
            .any(|segment| normalize_polish(segment) == "caly olsztyn");
    }

    let requested = normalize_house_number(&address.house_no);
    let mut had_known_numbers = false;
    for segment in matching_segments {
        let Some((_, numbers)) = segment.split_once('(') else {
            continue;
        };
        let numbers = numbers.trim_end_matches(')').trim();
        let normalized_numbers = normalize_house_number(numbers);
        let ranges = house_range_regex()
            .captures_iter(&normalized_numbers)
            .filter_map(|captures| {
                Some((
                    captures.get(1)?.as_str().parse::<u32>().ok()?,
                    captures.get(2)?.as_str().parse::<u32>().ok()?,
                ))
            })
            .collect::<Vec<_>>();
        let listed = house_number_regex()
            .find_iter(&normalized_numbers)
            .map(|number| normalize_house_number(number.as_str()))
            .collect::<Vec<_>>();
        if ranges.is_empty() && listed.is_empty() {
            continue;
        }
        had_known_numbers = true;

        if listed.iter().any(|number| number == &requested) {
            return true;
        }
        let requested_numeric = requested
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .ok();
        if let Some(requested_numeric) = requested_numeric
            && ranges.iter().any(|(start, end)| {
                requested_numeric >= (*start).min(*end) && requested_numeric <= (*start).max(*end)
            })
        {
            return true;
        }
    }

    !had_known_numbers
}

fn incident_kind(title: &str) -> &'static str {
    if normalize_polish(title).contains("awari") {
        "Awaria ciepła"
    } else {
        "Przerwa w dostawie ciepła"
    }
}

fn build_alert(url: &str, notice: ParsedNotice, settings: &Settings) -> UnifiedAlert {
    let location = location_text(&notice.streets, notice.city_wide);
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| address.is_active && location_matches_address(&location, address))
        .map(|(index, _)| index);
    let has_covered_address = settings.addresses.iter().any(|address| {
        address.is_active
            && crate::api_logic::is_address_applicable_for_provider(
                &AlertSource::MpecOlsztyn,
                address,
            )
    });
    let mut alert = UnifiedAlert {
        source: AlertSource::MpecOlsztyn,
        startDate: notice.start_date,
        endDate: notice.end_date,
        message: Some(format!(
            "{}: {}\n{}",
            incident_kind(&notice.title),
            notice.title,
            notice.body
        )),
        location: Some(location),
        address_index,
        is_local: Some(has_covered_address),
        hash: None,
    };
    let slug = Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
                .map(str::to_string)
        })
        .unwrap_or_else(|| "notice".to_string());
    alert.hash = Some(format!("mpec_olsztyn_{slug}_{}", alert.to_hash()));
    alert
}

async fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header(
            "User-Agent",
            "Awaria/1.0 (+https://github.com/eremef/awaria)",
        )
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP status {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| format!("response read failed: {error}"))
}

#[async_trait]
impl AlertProvider for MpecOlsztynProvider {
    fn id(&self) -> String {
        "mpec_olsztyn".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MpecOlsztyn
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

        let listing_html =
            match crate::utils::retry(|| fetch_text(client, MPEC_OLSZTYN_OUTAGES_URL), 3).await {
                Ok(html) => html,
                Err(error) => {
                    return (
                        Vec::new(),
                        vec![format!("MPEC Olsztyn listing request error: {error}")],
                    );
                }
            };
        let notices = match parse_listing(&listing_html) {
            Ok(notices) => notices,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MPEC Olsztyn listing parse error: {error}")],
                );
            }
        };

        let now = Utc::now().with_timezone(&Warsaw).naive_local();
        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        for link in notices {
            let article_html = match crate::utils::retry(|| fetch_text(client, &link.url), 3).await
            {
                Ok(html) => html,
                Err(error) => {
                    errors.push(format!(
                        "MPEC Olsztyn article request error for {}: {error}",
                        link.url
                    ));
                    continue;
                }
            };
            match parse_article(&article_html) {
                Ok(Some(notice)) if !is_expired(notice.end_date.as_deref(), now) => {
                    alerts.push(build_alert(&link.url, notice, settings));
                }
                Ok(Some(_)) | Ok(None) => {}
                Err(error) => errors.push(format!(
                    "MPEC Olsztyn article parse error for {}: {error}",
                    link.url
                )),
            }
        }
        (crate::api_logic::deduplicate_alerts(alerts), errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Official live article, checked 2026-11-18. Publication date is on the
    // listing card; the event date and affected buildings are in the article.
    const LIVE_ARTICLE: &str = r#"
        <html><h2>Przerwa w dostawie ciepła – Herdera/Kanta/Wilczyńskiego/Wiecherta</h2>
        <div class="container-fluid"><div class="row"><div class="col-md-12">
          <p class="x12ofw9d">MPEC informuje, że w związku z koniecznością usunięcia awarii na miejskiej sieci ciepłowniczej, w dniu 19 listopada 2026 r. (czwartek), w godzinach od 7:00 do 20:00, nastąpi przerwa w dostawie ciepła na potrzeby przygotowania ciepłej wody użytkowej oraz centralnego ogrzewania.</p>
          <p class="x12ofw9d">Przerwa obejmie następujące budynki:</p>
          <div>Herdera 2, 3, 4, 6, 7, 7A, 7B, 8, 9, 10, 11, 13, 14, 15, 16A, 16B, 17, 18, 18A, 19, 20, 21, 22, 24, 26.</div>
          <div>Kanta 1A, 1B, 3, 5A, 5B, 7A, 7B, 9.</div>
          <div>Wilczyńskiego 7, 11, 13, 15, 17, 17A, 19.</div>
          <div>Wiecherta 12, 14, 16, 18A, 18B, 20.</div>
          <p>Za powstałe utrudnienia przepraszamy.</p>
        </div></div></div></html>
    "#;

    // Official archived failure notice, checked 2026-11-18. The repair point
    // follows the affected-address list and is not an affected building row.
    const ARCHIVED_FAILURE: &str = r#"
        <html><h1>Przerwa w dostawie ciepłej wody – ul. Żurawskiego i Stramkowskiej</h1>
        <div class="desc_content">
          <p>MPEC informuje, że w związku z koniecznością usunięcia awarii, w dn. 23 września 2026 r., w godz. 07:00-20:00 wystąpi przerwa w dostawie ciepłej wody do budynków o następujących adresach:</p>
          <ul><li>ul. Żurawskiego 2, 4, 4a, 6, 8, 10, 12, 14, 16</li><li>ul. Stramkowskiej: 1, 3, 5</li></ul>
          <p>Przerwa wynika z konieczności usunięcia awarii przy ul. Żurawskiego 4A.</p>
          <p>Dziękujemy za wyrozumiałość.</p>
        </div></html>
    "#;

    // Official archived planned-work notice, checked 2026-11-18.
    const ARCHIVED_MULTI_DAY: &str = r#"
        <html><h1>Przerwa w dostawie ciepłej wody – rejon ul. Lubelskiej</h1>
        <div class="desc_content">
          <p>W związku z realizacją przebudowy sieci ciepłowniczej, w terminie od 10 czerwca 2024, od godz. 01:00 do 12 czerwca 2024, do godz. 23:00 wystąpi przerwa w dostawie ciepłej wody do następujących budynków:</p>
          <ul><li>Budowlana 2b, 3, 3a</li><li>Lubelska 35, 43c, 43d</li><li>Piłsudskiego 69A</li><li>Stalowa 3, 4</li><li>Towarowa 11, 18, 18c, 18e, 18g, 18h</li><li>Żelazna 1, 4</li></ul>
        </div></html>
    "#;

    const LIVE_LISTING: &str = r#"
        <html><h1>Awarie i wyłączenia</h1>
        <div class="aktualnosc"><span>18-11-2026</span>
          <h3>Przerwa w dostawie ciepła – Herdera/Kanta/Wilczyńskiego/Wiecherta</h3>
          <div class="krotki">W dniu 19 listopada wystąpi przerwa w dostawie ciepła.</div>
          <a href="/aktualnosci/przerwa-w-dostawie-ciepla-herdera-kanta-wilczynskiego/">Więcej &gt;</a>
        </div></html>
    "#;

    fn address(city: &str, city_id: Option<u64>, street: &str, house_no: &str) -> AddressEntry {
        address_with_teryt_street(city, city_id, street, street, None, house_no)
    }

    fn address_with_teryt_street(
        city: &str,
        city_id: Option<u64>,
        full_street: &str,
        street_name_1: &str,
        street_name_2: Option<&str>,
        house_no: &str,
    ) -> AddressEntry {
        AddressEntry {
            name: "Test".to_string(),
            city_name: city.to_string(),
            voivodeship: "Warmińsko-Mazurskie".to_string(),
            district: String::new(),
            commune: city.to_string(),
            street_name: full_street.to_string(),
            street_name_1: street_name_1.to_string(),
            street_name_2: street_name_2.map(str::to_string),
            house_no: house_no.to_string(),
            city_id,
            street_id: None,
            is_active: true,
        }
    }

    #[test]
    fn parses_live_listing_and_deduplicates_article_links() {
        let listing = LIVE_LISTING.replace(
            "</div></html>",
            "</div><div class=\"aktualnosc\"><h3>Przerwa w dostawie ciepła</h3><a href=\"/aktualnosci/przerwa-w-dostawie-ciepla-herdera-kanta-wilczynskiego/\">Więcej</a></div></html>",
        );
        let links = parse_listing(&listing).unwrap();
        assert_eq!(links.len(), 1);
        assert!(
            links[0]
                .url
                .contains("przerwa-w-dostawie-ciepla-herdera-kanta-wilczynskiego")
        );
    }

    #[test]
    fn explicit_empty_listing_is_distinct_from_unknown_markup() {
        let empty =
            r#"<html><h1>Awarie i wyłączenia</h1><p>Brak aktualnych awarii i wyłączeń.</p></html>"#;
        assert!(parse_listing(empty).unwrap().is_empty());

        let unknown = r#"<html><h1>Awarie i wyłączenia</h1><div class="feed"></div></html>"#;
        assert!(parse_listing(unknown).is_err());
        assert!(parse_listing("<html><h1>Aktualności</h1></html>").is_err());
    }

    #[test]
    fn malformed_articles_fail_and_restored_notices_are_skipped() {
        let malformed = r#"<html><h1>Przerwa w dostawie ciepła</h1></html>"#;
        assert!(parse_article(malformed).is_err());

        let restored = r#"<html><h1>Awaria ciepła usunięta – Olsztyn</h1>
            <div class="desc_content"><p>Awaria ciepła została usunięta. Dostawy ciepła przywrócono.</p></div></html>"#;
        assert!(parse_article(restored).unwrap().is_none());

        assert_eq!(
            parse_schedule("Awaria ciepła bez podanej daty"),
            (None, None)
        );
    }

    #[test]
    fn parses_live_event_window_and_keeps_street_house_groups_attached() {
        let notice = parse_article(LIVE_ARTICLE).unwrap().unwrap();
        assert_eq!(notice.start_date.as_deref(), Some("2026-11-19T07:00:00"));
        assert_eq!(notice.end_date.as_deref(), Some("2026-11-19T20:00:00"));
        assert_eq!(notice.streets.len(), 4);
        assert_eq!(notice.streets[0].name, "Herdera");
        assert!(notice.streets[0].house_numbers.contains("16A"));
        assert_eq!(notice.streets[1].name, "Kanta");
        assert!(notice.streets[1].house_numbers.contains("5B"));
        assert_eq!(notice.streets[2].name, "Wilczyńskiego");
        assert_eq!(notice.streets[3].name, "Wiecherta");
        assert!(!notice.city_wide);
    }

    #[test]
    fn historical_repair_site_is_not_added_to_affected_addresses() {
        let notice = parse_article(ARCHIVED_FAILURE).unwrap().unwrap();
        assert_eq!(notice.start_date.as_deref(), Some("2026-09-23T07:00:00"));
        assert_eq!(notice.streets.len(), 2);
        let location = location_text(&notice.streets, notice.city_wide);
        let repair_adjacent_four_b = address("Olsztyn", Some(964_465), "Żurawskiego", "4B");
        let affected_four = address("Olsztyn", Some(964_465), "Żurawskiego", "4");
        assert!(location_matches_address(&location, &repair_adjacent_four_b));
        assert!(!house_number_matches_notice(
            &location,
            &repair_adjacent_four_b
        ));
        assert!(house_number_matches_notice(&location, &affected_four));
        assert!(!location.contains("Żurawskiego 4A"));
    }

    #[test]
    fn parses_archived_multi_day_named_date_range() {
        let notice = parse_article(ARCHIVED_MULTI_DAY).unwrap().unwrap();
        assert_eq!(notice.start_date.as_deref(), Some("2024-06-10T01:00:00"));
        assert_eq!(notice.end_date.as_deref(), Some("2024-06-12T23:00:00"));
        assert_eq!(notice.streets.len(), 6);
        assert_eq!(notice.streets[0].name, "Budowlana");
        assert!(notice.streets[0].house_numbers.contains("2b"));
    }

    #[test]
    fn rejects_other_localities_and_respects_shared_house_number_setting() {
        let notice = parse_article(LIVE_ARTICLE).unwrap().unwrap();
        let location = location_text(&notice.streets, notice.city_wide);
        let olsztyn = address("Olsztyn", Some(964_465), "Herdera", "7B");
        let other_house = address("Olsztyn", Some(964_465), "Herdera", "7C");
        let formal_wilczynskiego = address_with_teryt_street(
            "Olsztyn",
            Some(964_465),
            "Biskupa Tomasza Wilczyńskiego",
            "Wilczyńskiego",
            Some("Biskupa Tomasza"),
            "1",
        );
        let outside = address("Nikielkowo", Some(470_036), "Herdera", "7B");
        let same_street_elsewhere = address("Gdańsk", None, "Herdera", "7B");

        assert!(location_matches_address(&location, &olsztyn));
        assert!(house_number_matches_notice(&location, &olsztyn));
        assert!(location_matches_address(&location, &formal_wilczynskiego));
        assert!(!house_number_matches_notice(
            &location,
            &formal_wilczynskiego
        ));
        assert!(location_matches_address(&location, &other_house));
        assert!(!house_number_matches_notice(&location, &other_house));
        assert!(!location_matches_address(&location, &outside));
        assert!(!location_matches_address(&location, &same_street_elsewhere));
        assert!(crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::MpecOlsztyn,
            &olsztyn
        ));
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::MpecOlsztyn,
            &outside
        ));
    }

    #[test]
    fn explicit_citywide_notice_matches_covered_locality_without_street_rows() {
        let html = r#"<html><h1>Awaria ciepła w całym Olsztynie</h1><div class="desc_content"><p>Awaria ciepła w całym Olsztynie. Dostawy zostaną wznowione po naprawie.</p></div></html>"#;
        let notice = parse_article(html).unwrap().unwrap();
        assert!(notice.city_wide);
        let location = location_text(&notice.streets, notice.city_wide);
        assert!(location_matches_address(
            &location,
            &address("Olsztyn", Some(964_465), "Herdera", "7")
        ));
    }

    #[test]
    fn expired_notices_are_ignored_by_warsaw_local_date_comparison() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert!(is_expired(Some("2024-06-12T23:00:00"), now));
        assert!(!is_expired(Some("2026-11-18"), now));
        assert!(!is_expired(None, now));
    }

    #[tokio::test]
    async fn fetch_text_reports_non_success_http_status() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).await;
            stream
                .write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let client = Client::builder().build().unwrap();
        let error = fetch_text(&client, &format!("http://{address}/"))
            .await
            .unwrap_err();
        assert!(error.contains("HTTP status 503"));
    }

    #[tokio::test]
    #[ignore = "live connectivity check for the official MPEC Olsztyn page and current linked notices"]
    async fn live_official_notice_fetches_and_matches_olsztyn() {
        let settings = Settings {
            addresses: vec![address_with_teryt_street(
                "Olsztyn",
                Some(964_465),
                "Biskupa Tomasza Wilczyńskiego",
                "Wilczyńskiego",
                Some("Biskupa Tomasza"),
                "1",
            )],
            ..Settings::default()
        };
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        let (alerts, errors) = MpecOlsztynProvider
            .fetch(&client, &client, &settings, None)
            .await;
        if !errors.is_empty() {
            let listing_html = fetch_text(&client, MPEC_OLSZTYN_OUTAGES_URL)
                .await
                .unwrap_or_default();
            let article_html = if let Some(link) = parse_listing(&listing_html)
                .ok()
                .and_then(|links| links.first().cloned())
            {
                fetch_text(&client, &link.url).await.unwrap_or_default()
            } else {
                String::new()
            };
            let document = Html::parse_document(&article_html);
            let diagnostic_selector =
                Selector::parse("h1, h2, article, main, p, li, div, section").unwrap();
            let structure = document
                .select(&diagnostic_selector)
                .filter_map(|element| {
                    let text = element_text(element);
                    let normalized = normalize_polish(&text);
                    (normalized.contains("herdera")
                        || normalized.contains("przerwa obejmie")
                        || normalized.contains("19 listopada"))
                    .then(|| {
                        format!(
                            "{} class={:?}: {}",
                            element.value().name(),
                            element.value().attr("class"),
                            text.chars().take(160).collect::<String>()
                        )
                    })
                })
                .take(30)
                .collect::<Vec<_>>();
            assert!(
                errors.is_empty(),
                "{errors:?}; live article structure: {structure:#?}"
            );
        }
        assert!(
            alerts.iter().any(|alert| {
                alert.source == AlertSource::MpecOlsztyn
                    && alert.is_local == Some(true)
                    && alert.startDate.is_some()
                    && alert.endDate.is_some()
            }),
            "no active MPEC Olsztyn notice matched the configured address"
        );
    }
}
