use crate::api_logic::{
    AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert,
    is_address_applicable_for_provider,
};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const EPWIK_ELBLAG_API_URL: &str = "https://www.epwik.com.pl/wp-json/wp/v2/awaria";
const PER_PAGE: usize = 100;
const MAX_PAGES: usize = 5;

pub struct EpwikElblagProvider;

#[derive(Debug, Deserialize)]
struct RestPost {
    id: u64,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    date_gmt: Option<String>,
    #[serde(default)]
    modified: Option<String>,
    #[serde(default)]
    modified_gmt: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    title: Option<RenderedField>,
    #[serde(default)]
    content: Option<RenderedField>,
}

#[derive(Debug, Deserialize, Default)]
struct RenderedField {
    #[serde(default)]
    rendered: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EventTime {
    time: NaiveTime,
    next_day: bool,
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})[./-](\d{1,2})[./-](20\d{2})\b").unwrap())
}

fn named_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|września|wrzesnia|października|pazdziernika|listopada|grudnia)\b",
        )
        .unwrap()
    })
}

fn numeric_time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(2[0-4]|[01]?\d)[:.]([0-5]\d)\b").unwrap())
}

fn house_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:nr\.?\s*)?(\d{1,4}[a-z]?)\s*(?:-|–|—|−|\bdo\b)\s*(?:nr\.?\s*)?(\d{1,4}[a-z]?)",
        )
        .unwrap()
    })
}

fn house_number_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b(\d{1,4}[a-z]?)\b").unwrap())
}

fn house_number_continuation_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^\s*(?:(?:od|nr\.?)\s*)?\d{1,4}[a-z]?(?:\s*(?:do|i|[-–—−/])\s*(?:nr\.?\s*)?\d{1,4}[a-z]?)*\s*[.;]?\s*$",
        )
        .unwrap()
    })
}

fn normalize_whitespace(value: &str) -> String {
    value
        .replace(['\u{00a0}', '\u{202f}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn html_to_blocks(html: &str) -> Vec<String> {
    let fragment = Html::parse_fragment(html);
    let Ok(paragraph_selector) = Selector::parse("p") else {
        return Vec::new();
    };

    let paragraphs = fragment
        .select(&paragraph_selector)
        .map(|paragraph| normalize_whitespace(&paragraph.text().collect::<Vec<_>>().join(" ")))
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>();
    if !paragraphs.is_empty() {
        return paragraphs;
    }

    let text = normalize_whitespace(&fragment.root_element().text().collect::<Vec<_>>().join(" "));
    if text.is_empty() {
        Vec::new()
    } else {
        vec![text]
    }
}

fn normalize_words(value: &str) -> String {
    crate::utils::clean_text(value)
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    let haystack = normalize_words(haystack);
    let phrase = normalize_words(phrase);
    !phrase.is_empty() && format!(" {haystack} ").contains(&format!(" {phrase} "))
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

fn parse_post_local_datetime(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f"))
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|value| value.with_timezone(&Warsaw).naive_local())
        })
}

fn publication_date(local: Option<&str>, utc: Option<&str>) -> Option<NaiveDate> {
    if let Some(value) = local.and_then(parse_post_local_datetime) {
        return Some(value.date());
    }
    let value = utc?;
    if let Some(value) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f").ok())
    {
        // WordPress returns date_gmt as an offset-free UTC timestamp.
        return Some(
            Utc.from_utc_datetime(&value)
                .with_timezone(&Warsaw)
                .date_naive(),
        );
    }
    DateTime::parse_from_rfc3339(value).ok().map(|value| {
        value
            .with_timezone(&Utc)
            .with_timezone(&Warsaw)
            .date_naive()
    })
}

fn resolve_yearless_date(day: u32, month: u32, publication: NaiveDate) -> Option<NaiveDate> {
    let mut candidate = NaiveDate::from_ymd_opt(publication.year(), month, day)?;
    let difference = (candidate - publication).num_days();
    if difference < -180 {
        candidate = NaiveDate::from_ymd_opt(publication.year() + 1, month, day)?;
    } else if difference > 180 {
        candidate = NaiveDate::from_ymd_opt(publication.year() - 1, month, day)?;
    }
    Some(candidate)
}

fn parse_event_dates(text: &str, publication: Option<NaiveDate>) -> Vec<NaiveDate> {
    let mut occurrences = Vec::new();
    for captures in numeric_date_regex().captures_iter(text) {
        let Some(date) = NaiveDate::from_ymd_opt(
            captures
                .get(3)
                .and_then(|value| value.as_str().parse().ok())
                .unwrap_or_default(),
            captures
                .get(2)
                .and_then(|value| value.as_str().parse().ok())
                .unwrap_or_default(),
            captures
                .get(1)
                .and_then(|value| value.as_str().parse().ok())
                .unwrap_or_default(),
        ) else {
            continue;
        };
        occurrences.push((
            captures
                .get(0)
                .map(|value| value.start())
                .unwrap_or_default(),
            date,
        ));
    }

    if let Some(publication) = publication {
        for captures in named_date_regex().captures_iter(text) {
            let Some(day) = captures
                .get(1)
                .and_then(|value| value.as_str().parse().ok())
            else {
                continue;
            };
            let Some(month) = captures
                .get(2)
                .and_then(|value| month_number(&crate::utils::clean_text(value.as_str())))
            else {
                continue;
            };
            let Some(date) = resolve_yearless_date(day, month, publication) else {
                continue;
            };
            occurrences.push((
                captures
                    .get(0)
                    .map(|value| value.start())
                    .unwrap_or_default(),
                date,
            ));
        }
    }

    occurrences.sort_by_key(|(position, _)| *position);
    let mut dates = Vec::new();
    for (_, date) in occurrences {
        if !dates.contains(&date) {
            dates.push(date);
        }
    }
    dates
}

fn parse_numeric_times(text: &str) -> Vec<EventTime> {
    numeric_time_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            let previous = text[..whole.start()].chars().next_back();
            let before_previous = text[..whole.start()].chars().rev().nth(1);
            let next = text[whole.end()..].chars().next();
            let after_next = text[whole.end()..].chars().nth(1);
            // Do not treat the first pair in a numeric date (10.12.2026) as 10:12.
            if matches!(previous, Some('0'..='9'))
                || (matches!(previous, Some('.' | '/'))
                    && matches!(before_previous, Some('0'..='9')))
                || matches!(next, Some('0'..='9'))
                || (matches!(next, Some('.' | '/')) && matches!(after_next, Some('0'..='9')))
            {
                return None;
            }

            let hour = captures.get(1)?.as_str().parse::<u32>().ok()?;
            let minute = captures.get(2)?.as_str().parse::<u32>().ok()?;
            if hour == 24 && minute == 0 {
                return Some(EventTime {
                    time: NaiveTime::from_hms_opt(0, 0, 0)?,
                    next_day: true,
                });
            }
            Some(EventTime {
                time: NaiveTime::from_hms_opt(hour, minute, 0)?,
                next_day: false,
            })
        })
        .collect()
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_event_interval(
    text: &str,
    publication: Option<NaiveDate>,
) -> (Option<String>, Option<String>) {
    let dates = parse_event_dates(text, publication);
    let Some(first_date) = dates.first().copied() else {
        return (None, None);
    };
    let times = parse_numeric_times(text);

    if times.len() >= 2 {
        let first = times[0];
        let last = *times.last().expect("at least two event times");
        let start_date = first_date + Duration::days(i64::from(first.next_day));
        let mut end_date =
            dates.last().copied().unwrap_or(first_date) + Duration::days(i64::from(last.next_day));
        let start = start_date.and_time(first.time);
        let mut end = end_date.and_time(last.time);
        if end <= start && !last.next_day {
            end_date += Duration::days(1);
            end = end_date.and_time(last.time);
        }
        return (Some(format_datetime(start)), Some(format_datetime(end)));
    }

    let start = first_date.format("%Y-%m-%d").to_string();
    let end = dates
        .last()
        .filter(|date| **date != first_date)
        .map(|date| date.format("%Y-%m-%d").to_string());
    (Some(start), end)
}

fn is_location_paragraph(text: &str) -> bool {
    let normalized = normalize_words(text);
    [
        "rejonie",
        "adres",
        "adresy",
        "odbiorcy",
        "ulica",
        "ulice",
        "ulicach",
        "miasto elblag",
        "gmina elblag",
        "gmina milejewo",
        "elblag",
        "milejewo",
    ]
    .iter()
    .any(|phrase| format!(" {normalized} ").contains(&format!(" {phrase} ")))
}

fn location_fragment(block: &str) -> String {
    let lower = block.to_lowercase();
    let after_markers = [
        "rejonie:",
        "ulicach:",
        "ulicach :",
        "ulice:",
        "ulice :",
        "ulic:",
        "ulic :",
        "adresy:",
        "adresy :",
        "adres:",
        "adres :",
    ];
    let start_after = after_markers
        .iter()
        .filter_map(|marker| lower.find(marker).map(|index| index + marker.len()))
        .min();
    if let Some(start) = start_after {
        return normalize_whitespace(block[start..].trim_matches([' ', ':', ';']));
    }

    let start_at = [
        "odbiorcy",
        "miasto ",
        "gmina ",
        "gm. ",
        "gm ",
        "m. elbląg",
        "elbląg:",
        "milejewo:",
        "ul.",
        "ulica ",
    ]
    .iter()
    .filter_map(|marker| lower.find(marker))
    .min();
    if let Some(start) = start_at {
        return normalize_whitespace(&block[start..]);
    }
    normalize_whitespace(block)
}

fn extract_location(html: &str) -> Option<String> {
    let locations = html_to_blocks(html)
        .into_iter()
        .filter(|block| is_location_paragraph(block))
        .map(|block| location_fragment(&block))
        .filter(|location| !location.is_empty())
        .collect::<Vec<_>>();
    (!locations.is_empty()).then(|| locations.join("; "))
}

fn normalize_municipality(value: &str) -> String {
    let normalized = normalize_words(value);
    let mut words = normalized.split_whitespace().collect::<Vec<_>>();
    for prefix in [
        &["gmina", "miejsko", "wiejska"][..],
        &["gmina", "miejska"][..],
        &["gmina", "wiejska"][..],
        &["gmina"],
        &["gm"],
        &["miasto"],
        &["m"],
    ] {
        if words.starts_with(prefix) {
            words.drain(..prefix.len());
            break;
        }
    }
    words.join(" ")
}

fn city_name_matches(address: &AddressEntry, name: &str) -> bool {
    let name = normalize_municipality(name);
    if name.is_empty() {
        return false;
    }
    if address.city_id.is_some() && name == "elblag" {
        return address.city_id == Some(932_703);
    }
    if address.city_id.is_some() && name == "milejewo" {
        return address.city_id == Some(152_572);
    }
    normalize_municipality(&address.city_name) == name
}

fn commune_name_matches(address: &AddressEntry, name: &str) -> bool {
    let name = normalize_municipality(name);
    if name.is_empty() {
        return false;
    }
    normalize_municipality(&address.commune) == name
        && !(name == "elblag" && address.city_id == Some(932_703))
}

/// Requires the locality named by the notice to match the configured address.
/// An explicit city label cannot accidentally match a same-named rural commune.
pub fn locality_matches_address(location: &str, address: &AddressEntry) -> bool {
    let normalized = normalize_words(location);
    if contains_phrase(&normalized, "miasto elblag") || contains_phrase(&normalized, "m elblag") {
        return city_name_matches(address, "elblag");
    }
    if contains_phrase(&normalized, "gmina elblag") || contains_phrase(&normalized, "gm elblag") {
        return commune_name_matches(address, "elblag");
    }
    if contains_phrase(&normalized, "gmina milejewo") || contains_phrase(&normalized, "gm milejewo")
    {
        return commune_name_matches(address, "milejewo");
    }

    // Unqualified town labels in notices (for example “Elbląg: ul. ...”) refer
    // to that locality, not every village in the similarly named commune.
    for name in ["elblag", "milejewo"] {
        if contains_phrase(&normalized, name) && city_name_matches(address, name) {
            return true;
        }
    }

    let city = normalize_municipality(&address.city_name);
    !city.is_empty() && contains_phrase(&normalized, &city)
}

fn street_tokens(value: &str) -> Vec<String> {
    let stripped = crate::utils::strip_street_prefixes(value);
    normalize_words(stripped)
        .split_whitespace()
        .filter(|token| {
            !matches!(
                *token,
                "ul" | "ulica"
                    | "ulice"
                    | "ulic"
                    | "ulicy"
                    | "ulicach"
                    | "aleja"
                    | "al"
                    | "plac"
                    | "pl"
                    | "nr"
                    | "numer"
            )
        })
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(str::to_string)
        .collect()
}

fn street_token_matches(left: &str, right: &str) -> bool {
    left == right
        || (left.chars().count() >= 5 && right.starts_with(left))
        || (right.chars().count() >= 5 && left.starts_with(right))
}

fn location_segments(location: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    for segment in location
        .split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
    {
        if house_number_continuation_regex().is_match(segment)
            && let Some(previous) = segments.last_mut()
        {
            previous.push_str(", ");
            previous.push_str(segment);
        } else {
            segments.push(segment.to_string());
        }
    }
    segments
}

fn matching_street_segments(location: &str, address: &AddressEntry) -> Vec<String> {
    let configured = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let candidates = configured
        .iter()
        .map(|street| street_tokens(street))
        .filter(|tokens| !tokens.is_empty())
        .collect::<Vec<_>>();

    location_segments(location)
        .into_iter()
        .filter(|segment| {
            let segment_tokens = street_tokens(segment);
            candidates.iter().any(|candidate| {
                candidate.iter().all(|candidate_token| {
                    segment_tokens
                        .iter()
                        .any(|segment_token| street_token_matches(candidate_token, segment_token))
                })
            })
        })
        .collect()
}

/// Matches a notice's named locality and street, without enforcing house number.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    is_address_applicable_for_provider(&AlertSource::EpwikElblag, address)
        && locality_matches_address(location, address)
        && !matching_street_segments(location, address).is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct HouseNumber {
    number: u32,
    suffix: Option<char>,
}

fn parse_house_number(text: &str) -> Option<HouseNumber> {
    let text = text.trim().to_ascii_lowercase();
    let mut chars = text.chars();
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
    let ranges = house_range_regex()
        .captures_iter(segment)
        .filter_map(|captures| {
            Some((
                parse_house_number(captures.get(1)?.as_str())?,
                parse_house_number(captures.get(2)?.as_str())?,
            ))
        })
        .collect::<Vec<_>>();
    let has_range = !ranges.is_empty();
    let in_range = ranges.into_iter().any(|(start, end)| {
        let (low, high) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        requested >= low && requested <= high
    });

    let numbers = house_number_regex()
        .captures_iter(segment)
        .filter_map(|captures| parse_house_number(captures.get(1)?.as_str()))
        .collect::<Vec<_>>();
    (!numbers.is_empty() || has_range).then(|| in_range || numbers.contains(&requested))
}

/// Checks the saved number only in the segment for the matching street.
/// A missing number range remains non-restrictive, as in the shared policy.
pub fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    let segments = matching_street_segments(location, address);
    if segments.is_empty() {
        return false;
    }
    if address.house_no.trim().is_empty() {
        return true;
    }
    let outcomes = segments
        .into_iter()
        .filter_map(|segment| house_number_in_segment(&segment, &address.house_no))
        .collect::<Vec<_>>();
    outcomes.is_empty() || outcomes.into_iter().any(|matches| matches)
}

fn notice_identity(post: &RestPost, message: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(post.id.to_be_bytes());
    hasher.update(post.link.as_deref().unwrap_or(""));
    hasher.update(
        post.modified_gmt
            .as_deref()
            .or(post.modified.as_deref())
            .unwrap_or(""),
    );
    hasher.update(message);
    let digest = hasher.finalize();
    format!(
        "epwik_elblag_{}_{}",
        post.id,
        digest[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn parse_post(post: RestPost, settings: &Settings) -> Option<UnifiedAlert> {
    if post.id == 0 {
        return None;
    }
    let title_html = post
        .title
        .as_ref()
        .map(|title| title.rendered.as_str())
        .unwrap_or_default();
    let content_html = post
        .content
        .as_ref()
        .map(|content| content.rendered.as_str())
        .unwrap_or_default();
    let title = html_to_blocks(title_html).join(" ");
    let blocks = html_to_blocks(content_html);
    let body = blocks.join("\n");
    if title.is_empty() && body.is_empty() {
        return None;
    }

    let message = if title.is_empty() {
        body.clone()
    } else if body.is_empty() {
        title.clone()
    } else {
        format!("{title}. {body}")
    };
    let published = publication_date(post.date.as_deref(), post.date_gmt.as_deref());
    let (start_date, end_date) = parse_event_interval(&body, published);
    let location = extract_location(content_html);

    let address_index = location.as_deref().and_then(|location| {
        settings
            .addresses
            .iter()
            .enumerate()
            .find(|(_, address)| address.is_active && location_matches_address(location, address))
            .map(|(index, _)| index)
    });

    let hash = notice_identity(&post, &message);
    Some(UnifiedAlert {
        source: AlertSource::EpwikElblag,
        startDate: start_date,
        endDate: end_date,
        message: Some(message),
        location,
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(hash),
    })
}

/// Parses the public EPWiK WordPress REST collection into one alert per post.
pub fn parse_epwik_elblag_posts(
    json: &str,
    settings: &Settings,
) -> Result<Vec<UnifiedAlert>, String> {
    let posts: Vec<RestPost> = serde_json::from_str(json)
        .map_err(|error| format!("EPWiK Elbląg REST response parse error: {error}"))?;
    Ok(parse_posts(posts, settings))
}

fn parse_posts(posts: Vec<RestPost>, settings: &Settings) -> Vec<UnifiedAlert> {
    let mut seen = HashSet::new();
    posts
        .into_iter()
        .filter_map(|post| parse_post(post, settings))
        .filter(|alert| {
            alert
                .hash
                .as_ref()
                .is_some_and(|hash| seen.insert(hash.clone()))
        })
        .collect()
}

async fn fetch_posts(client: &Client) -> Result<Vec<RestPost>, String> {
    let mut posts = Vec::new();
    let mut page = 1usize;
    let mut total_pages = 1usize;
    loop {
        let mut url = Url::parse(EPWIK_ELBLAG_API_URL)
            .map_err(|error| format!("EPWiK Elbląg REST endpoint URL error: {error}"))?;
        let per_page = PER_PAGE.to_string();
        let page_number = page.to_string();
        url.query_pairs_mut()
            .append_pair("per_page", &per_page)
            .append_pair("page", &page_number)
            .append_pair(
                "_fields",
                "id,date,date_gmt,modified,modified_gmt,link,title,content",
            );
        let response = client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;

        if page == 1 {
            total_pages = response
                .headers()
                .get("X-WP-TotalPages")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            if total_pages > MAX_PAGES {
                log::warn!(
                    "EPWiK Elbląg REST collection has {total_pages} pages; only the newest {MAX_PAGES} will be fetched"
                );
                total_pages = MAX_PAGES;
            }
        }

        let body = response.text().await.map_err(|error| error.to_string())?;
        let page_posts: Vec<RestPost> = serde_json::from_str(&body)
            .map_err(|error| format!("EPWiK Elbląg REST page {page} parse error: {error}"))?;
        posts.extend(page_posts);

        if page >= total_pages {
            break;
        }
        page += 1;
    }
    Ok(posts)
}

#[async_trait]
impl AlertProvider for EpwikElblagProvider {
    fn id(&self) -> String {
        "epwik_elblag".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::EpwikElblag
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

        match fetch_posts(client).await {
            Ok(posts) => (parse_posts(posts, settings), Vec::new()),
            Err(error) => (
                Vec::new(),
                vec![format!("EPWiK Elbląg REST request failed: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELBLAG_CITY_ID: u64 = 932_703;
    const ELBLAG_RURAL_CITY_ID: u64 = 149_481;

    fn address(
        city: &str,
        commune: &str,
        city_id: Option<u64>,
        street: &str,
        house: &str,
    ) -> AddressEntry {
        AddressEntry {
            name: "Dom".to_string(),
            city_name: city.to_string(),
            voivodeship: "Warmińsko-Mazurskie".to_string(),
            commune: commune.to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            house_no: house.to_string(),
            city_id,
            is_active: true,
            ..AddressEntry::default()
        }
    }

    fn settings(addresses: Vec<AddressEntry>) -> Settings {
        Settings {
            addresses,
            ..Settings::default()
        }
    }

    fn live_post(content: &str) -> String {
        serde_json::json!([{
            "id": 565149,
            "date": "2026-11-07T20:15:12",
            "date_gmt": "2026-11-07T19:15:12",
            "modified": "2026-11-07T20:15:12",
            "modified_gmt": "2026-11-07T19:15:12",
            "link": "https://www.epwik.com.pl/awaria/dotyczy-legionow-40-fromborska-31/",
            "title": { "rendered": "Dotyczy Legionów 40, Fromborska 31" },
            "content": { "rendered": content }
        }])
        .to_string()
    }

    #[test]
    fn parses_live_rest_shape_and_keeps_written_time_unparsed() {
        let settings = settings(vec![
            address(
                "Elbląg",
                "Gmina Miasto Elbląg",
                Some(ELBLAG_CITY_ID),
                "Legionów",
                "40",
            ),
            address(
                "Elbląg",
                "Gmina Miasto Elbląg",
                Some(ELBLAG_CITY_ID),
                "Fromborska",
                "31",
            ),
        ]);
        let json = live_post(
            "<p>Informujemy, że&nbsp;w związku z awarią na sieci wodociągowej występuje brak wody w rejonie:<br>miasto Elbląg, ulice : Legionów 40, Fromborska 31.</p><p>Awaria będzie usuwana w dniu 8 listopada.<br>Przewidywany czas pracy w godzinach siódma piętnasta.</p>",
        );

        let alerts = parse_epwik_elblag_posts(&json, &settings).unwrap();
        assert_eq!(alerts.len(), 1);
        let alert = &alerts[0];
        assert_eq!(alert.source, AlertSource::EpwikElblag);
        assert_eq!(alert.startDate.as_deref(), Some("2026-11-08"));
        assert_eq!(alert.endDate, None);
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("siódma piętnasta")
        );
        assert!(alert.location.as_deref().unwrap().contains("miasto Elbląg"));
        assert_eq!(alert.is_local, Some(true));
        assert!(
            alert
                .hash
                .as_deref()
                .unwrap()
                .starts_with("epwik_elblag_565149_")
        );
    }

    #[test]
    fn parses_numeric_event_dates_times_and_house_ranges() {
        let settings = settings(vec![address(
            "Elbląg",
            "Gmina Miasto Elbląg",
            Some(ELBLAG_CITY_ID),
            "Szkolna",
            "4",
        )]);
        let json = live_post(
            "<p>EPWiK informuje, że odbiorcy m. Elbląg: ul. Szkolna od nr 1 do nr 6.</p><p>Przewidywany czas pracy w dniu 10.12.2025 w godz. 8.00 - 16.00.</p>",
        );

        let alerts = parse_epwik_elblag_posts(&json, &settings).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2025-12-10T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2025-12-10T16:00:00"));
        assert!(house_number_matches_notice(
            alerts[0].location.as_deref().unwrap(),
            &settings.addresses[0]
        ));
    }

    #[test]
    fn resolves_yearless_dates_across_new_year_from_publication_date() {
        let publication = NaiveDate::from_ymd_opt(2026, 12, 30).unwrap();
        assert_eq!(
            resolve_yearless_date(2, 1, publication),
            NaiveDate::from_ymd_opt(2027, 1, 2)
        );
    }

    #[test]
    fn associates_house_numbers_with_their_own_street_segment() {
        let location = "miasto Elbląg, ulice: Legionów 40, Fromborska 31";
        let legionow_40 = address(
            "Elbląg",
            "Gmina Miasto Elbląg",
            Some(ELBLAG_CITY_ID),
            "Legionów",
            "40",
        );
        let legionow_31 = address(
            "Elbląg",
            "Gmina Miasto Elbląg",
            Some(ELBLAG_CITY_ID),
            "Legionów",
            "31",
        );
        let fromborska_31 = address(
            "Elbląg",
            "Gmina Miasto Elbląg",
            Some(ELBLAG_CITY_ID),
            "Fromborska",
            "31",
        );

        assert!(location_matches_address(location, &legionow_40));
        assert!(house_number_matches_notice(location, &legionow_40));
        assert!(location_matches_address(location, &fromborska_31));
        assert!(house_number_matches_notice(location, &fromborska_31));
        assert!(!house_number_matches_notice(location, &legionow_31));
    }

    #[test]
    fn keeps_comma_separated_house_lists_and_ranges_with_their_street() {
        let location = "miasto Elbląg, ul. Żurawia nr 7, 9, 11, 17, 21";
        for house in ["7", "9", "11", "17", "21"] {
            let configured = address(
                "Elbląg",
                "Gmina Miasto Elbląg",
                Some(ELBLAG_CITY_ID),
                "Żurawia",
                house,
            );
            assert!(
                house_number_matches_notice(location, &configured),
                "house {house}"
            );
        }

        let mixed = "miasto Elbląg, ul. Szkolna 1, 3, 5-9";
        for house in ["1", "3", "6", "9"] {
            let configured = address(
                "Elbląg",
                "Gmina Miasto Elbląg",
                Some(ELBLAG_CITY_ID),
                "Szkolna",
                house,
            );
            assert!(
                house_number_matches_notice(mixed, &configured),
                "house {house}"
            );
        }
        let outside = address(
            "Elbląg",
            "Gmina Miasto Elbląg",
            Some(ELBLAG_CITY_ID),
            "Szkolna",
            "4",
        );
        assert!(!house_number_matches_notice(mixed, &outside));
    }

    #[test]
    fn explicit_city_and_commune_localities_do_not_cross_match() {
        let city = address(
            "Elbląg",
            "Gmina Miasto Elbląg",
            Some(ELBLAG_CITY_ID),
            "Legionów",
            "40",
        );
        let village = address(
            "Jagodnik",
            "Gmina Elbląg",
            Some(ELBLAG_RURAL_CITY_ID),
            "Legionów",
            "40",
        );
        let city_location = "miasto Elbląg, ul. Legionów 40";
        let rural_location = "Gmina Elbląg: ul. Legionów 40";

        assert!(location_matches_address(city_location, &city));
        assert!(!location_matches_address(city_location, &village));
        assert!(location_matches_address(rural_location, &village));
        assert!(!location_matches_address(rural_location, &city));
    }

    #[test]
    fn supports_milejewo_commune_and_rejects_out_of_area_addresses() {
        let milejewo = address("Jagodnik", "Gmina Milejewo", Some(152_537), "Polna", "2");
        let out_of_area = address("Olsztyn", "Olsztyn", Some(966_000), "Polna", "2");
        let notice = "Gmina Milejewo: ul. Polna 1-4";

        assert!(is_address_applicable_for_provider(
            &AlertSource::EpwikElblag,
            &milejewo
        ));
        assert!(location_matches_address(notice, &milejewo));
        assert!(!is_address_applicable_for_provider(
            &AlertSource::EpwikElblag,
            &out_of_area
        ));
        assert!(!location_matches_address(notice, &out_of_area));
    }

    #[test]
    fn accepts_empty_rest_collections_and_rejects_malformed_json() {
        assert!(
            parse_epwik_elblag_posts("[]", &Settings::default())
                .unwrap()
                .is_empty()
        );
        assert!(parse_epwik_elblag_posts("not-json", &Settings::default()).is_err());
        assert!(parse_epwik_elblag_posts("{}", &Settings::default()).is_err());
    }
}
