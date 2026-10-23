use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Datelike, NaiveDate, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const DABROWSKIE_WODOCIAGI_RSS_URL: &str =
    "https://dabrowskie-wodociagi.pl/category/komunikaty/feed/";
pub const DABROWSKIE_WODOCIAGI_CATEGORY_URL: &str =
    "https://dabrowskie-wodociagi.pl/category/komunikaty/";
const OFFICIAL_HOST_PREFIX: &str = "https://dabrowskie-wodociagi.pl/";

pub struct DabrowskieWodociagiProvider;

#[derive(Debug, Deserialize)]
struct RssDocument {
    channel: RssChannel,
}

#[derive(Debug, Deserialize)]
struct RssChannel {
    #[serde(default, rename = "item")]
    items: Vec<RssItem>,
}

#[derive(Debug, Deserialize)]
struct RssItem {
    title: Option<String>,
    link: Option<String>,
    #[serde(rename = "pubDate")]
    published: Option<String>,
}

#[derive(Debug, Clone)]
struct NoticeLink {
    title: String,
    url: String,
    published: Option<String>,
}

#[derive(Debug, Default)]
struct NoticeSections {
    start: String,
    end: String,
    place: String,
    cause: String,
    effects: String,
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

fn street_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(?:ul\.|ulica)\s*").expect("valid street marker regex"))
}

fn street_number_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s+(?:nr\.?|numery|numer)\s*").expect("valid house-number marker regex")
    })
}

fn house_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,4}\s*[a-z]?)[ \t]*(?:-|–|—|−|\bdo\b)[ \t]*(?:nr\.?\s*)?(\d{1,4}\s*[a-z]?)\b",
        )
        .expect("valid house-number range regex")
    })
}

fn house_number_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(\d{1,4}\s*[a-z]?)\b").expect("valid house-number regex"))
}

fn outage_terms() -> &'static [&'static str] {
    &[
        "przerwa w dostawie wody",
        "przerwy w dostawie wody",
        "awaria wodociagowa",
        "awarie wodociagowe",
        "zaklocenia w dostawie wody",
        "spadki cisnienia",
        "pogorszenie jakosci wody",
        "brak wody",
    ]
}

fn is_water_disruption(text: &str) -> bool {
    let normalized = normalize(text);
    if normalized.contains("aktualnie brak przerw w dostawie wody")
        || normalized.contains("obecnie brak przerw w dostawie wody")
    {
        return false;
    }
    outage_terms().iter().any(|term| normalized.contains(term))
}

fn stem_token(token: &str) -> String {
    for suffix in [
        "skiego", "ckiego", "skich", "ckich", "skiej", "ckiej", "owej", "owym", "owych", "ami",
        "ach", "ego", "emu", "iej", "ich", "ych", "owa", "owe", "owej", "ska", "skie", "ski", "ej",
        "owi", "ow", "em", "ie", "a", "e", "y", "u",
    ] {
        if token.len() > suffix.len() + 2 && token.ends_with(suffix) {
            return token[..token.len() - suffix.len()].to_string();
        }
    }
    token.to_string()
}

fn tokens(value: &str) -> Vec<String> {
    normalize(value)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().any(char::is_alphabetic))
        .map(stem_token)
        .collect()
}

fn contains_phrase(text: &str, candidate: &str) -> bool {
    let expected = tokens(candidate);
    let actual = tokens(text);
    !expected.is_empty()
        && actual.windows(expected.len()).any(|words| {
            words.iter().zip(&expected).all(|(word, expected)| {
                word == expected || word.starts_with(expected) || expected.starts_with(word)
            })
        })
}

fn street_matches(actual: &str, candidate: &str) -> bool {
    contains_phrase(
        actual,
        crate::utils::strip_street_prefixes(candidate.trim()),
    )
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

fn street_segments(location: &str) -> Vec<(String, String)> {
    let mut segments = Vec::new();
    for marker in street_marker_regex().find_iter(location) {
        let rest = &location[marker.end()..];
        let end = street_marker_regex()
            .find(rest)
            .map(|next| next.start())
            .unwrap_or(rest.len());
        let street_info = rest[..end].trim().trim_start_matches(':').trim();
        let (name, numbers) = street_number_marker_regex()
            .find(street_info)
            .map(|number_marker| {
                (
                    &street_info[..number_marker.start()],
                    street_info[number_marker.end()..].trim(),
                )
            })
            .unwrap_or((street_info, street_info));
        if !name.trim().is_empty() {
            segments.push((name.trim().to_string(), numbers.to_string()));
        }
    }
    segments
}

fn location_lines(location: &str) -> Vec<&str> {
    location
        .split(['\n', '\r', ';'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn matching_location_segments(location: &str, address: &AddressEntry) -> Vec<String> {
    let streets = configured_streets(address);
    let explicit_segments = street_segments(location);
    if !explicit_segments.is_empty() {
        return explicit_segments
            .into_iter()
            .filter(|(name, _)| streets.iter().any(|street| street_matches(name, street)))
            .map(|(name, numbers)| format!("ul. {name} {numbers}"))
            .collect();
    }

    let locality_candidates = [&address.city_name, &address.district]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    location_lines(location)
        .into_iter()
        .filter(|line| {
            streets.iter().any(|street| contains_phrase(line, street))
                || locality_candidates
                    .iter()
                    .any(|locality| contains_phrase(line, locality))
        })
        .map(str::to_string)
        .collect()
}

/// Matches an affected street or named locality to one configured Dąbrowa address.
/// The caller must first enforce the provider's Dąbrowa Górnicza service area.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    !matching_location_segments(location, address).is_empty()
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

fn house_number_in_segment(segment: &str, requested: &str) -> Option<bool> {
    let requested = parse_house_number(requested)?;
    if !street_marker_regex().is_match(segment) && !street_number_marker_regex().is_match(segment) {
        return None;
    }
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
        .filter_map(|capture| {
            Some((
                parse_house_number(capture.get(1)?.as_str())?,
                parse_house_number(capture.get(2)?.as_str())?,
            ))
        })
        .collect::<Vec<_>>();
    let in_range = ranges.into_iter().any(|(start, end)| {
        let (low, high) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        requested >= low && requested <= high
    });

    let listed = house_number_regex()
        .captures_iter(segment)
        .filter_map(|capture| parse_house_number(capture.get(1)?.as_str()))
        .any(|number| number == requested);
    let found_numbers = house_number_regex().is_match(segment);
    if found_numbers {
        Some(in_range || listed)
    } else {
        None
    }
}

/// Applies the saved house number only when the shared settings enable that filter.
/// Unparseable or absent notice numbers remain non-restrictive.
pub fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }
    let results = matching_location_segments(location, address)
        .into_iter()
        .filter_map(|segment| house_number_in_segment(&segment, &address.house_no))
        .collect::<Vec<_>>();
    results.is_empty() || results.into_iter().any(|matches| matches)
}

fn official_article_url(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.starts_with(OFFICIAL_HOST_PREFIX)
        || value.contains('@')
        || value.contains('#')
        || value.contains('?')
    {
        return None;
    }
    Some(value.to_string())
}

fn parse_rss(xml: &str) -> Result<Vec<NoticeLink>, String> {
    let feed: RssDocument = quick_xml::de::from_str(xml)
        .map_err(|error| format!("Could not parse Dąbrowskie Wodociągi RSS: {error}"))?;

    let mut seen = HashSet::new();
    let mut notices = Vec::new();
    for item in feed.channel.items {
        let title = item
            .title
            .as_deref()
            .map(collapse_whitespace)
            .unwrap_or_default();
        let Some(url) = item.link.as_deref().and_then(official_article_url) else {
            continue;
        };
        if title.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        notices.push(NoticeLink {
            title,
            url,
            published: item.published,
        });
    }
    Ok(notices)
}

fn parse_category_html(html: &str) -> Vec<NoticeLink> {
    let document = Html::parse_document(html);
    let selector =
        Selector::parse("h1 a, h2 a, h3 a, .entry-title a").expect("valid notice-link selector");
    let mut seen = HashSet::new();
    let mut notices = Vec::new();

    for anchor in document.select(&selector) {
        let title = collapse_whitespace(&anchor.text().collect::<Vec<_>>().join(" "));
        let Some(url) = anchor.value().attr("href").and_then(official_article_url) else {
            continue;
        };
        if title.is_empty() || !seen.insert(url.clone()) {
            continue;
        }
        notices.push(NoticeLink {
            title,
            url,
            published: None,
        });
    }
    notices
}

async fn fetch_notice_links(client: &Client) -> Result<Vec<NoticeLink>, String> {
    let rss_response = client.get(DABROWSKIE_WODOCIAGI_RSS_URL).send().await;
    if let Ok(response) = rss_response
        && let Ok(response) = response.error_for_status()
        && let Ok(body) = response.text().await
        && let Ok(notices) = parse_rss(&body)
        && !notices.is_empty()
    {
        return Ok(notices);
    }

    let response = client
        .get(DABROWSKIE_WODOCIAGI_CATEGORY_URL)
        .send()
        .await
        .map_err(|error| format!("Could not fetch Dąbrowskie Wodociągi notices: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Dąbrowskie Wodociągi archive returned an error: {error}"))?;
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read Dąbrowskie Wodociągi archive: {error}"))?;
    Ok(parse_category_html(&body))
}

fn month_number(month: &str) -> Option<u32> {
    match normalize(month).as_str() {
        "stycznia" => Some(1),
        "lutego" => Some(2),
        "marca" => Some(3),
        "kwietnia" => Some(4),
        "maja" => Some(5),
        "czerwca" => Some(6),
        "lipca" => Some(7),
        "sierpnia" => Some(8),
        "wrzesnia" => Some(9),
        "pazdziernika" => Some(10),
        "listopada" => Some(11),
        "grudnia" => Some(12),
        _ => None,
    }
}

fn year_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(20\d{2})\b").expect("valid year regex"))
}

fn explicit_start_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\bod\s+(?:dnia\s+)?\d{1,2}\b").expect("valid explicit start marker regex")
    })
}

fn numeric_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(\d{1,2})[./-](\d{1,2})[./-](20\d{2})\b").expect("valid numeric date regex")
    })
}

fn month_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})(?:\s*(?:-|–|—|\bdo\b)\s*(\d{1,2}))?\s*(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|września|października|listopada|grudnia)\b(?:\s+(20\d{2}))?",
        )
        .expect("valid Polish month date regex")
    })
}

fn parse_date_from_text(
    text: &str,
    use_range_end: bool,
    fallback_year: Option<i32>,
) -> Option<NaiveDate> {
    let explicit_year = year_regex()
        .captures(text)
        .and_then(|capture| capture.get(1))
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .or(fallback_year)?;

    let numeric_dates = numeric_date_regex()
        .captures_iter(text)
        .filter_map(|capture| {
            let day = capture.get(1)?.as_str().parse::<u32>().ok()?;
            let month = capture.get(2)?.as_str().parse::<u32>().ok()?;
            let year = capture.get(3)?.as_str().parse::<i32>().ok()?;
            NaiveDate::from_ymd_opt(year, month, day)
        })
        .collect::<Vec<_>>();
    if !numeric_dates.is_empty() {
        return if use_range_end {
            numeric_dates.last().copied()
        } else {
            numeric_dates.first().copied()
        };
    }

    let month_dates = month_date_regex()
        .captures_iter(text)
        .filter_map(|capture| {
            let first_day = capture.get(1)?.as_str().parse::<u32>().ok()?;
            let last_day = capture
                .get(2)
                .and_then(|value| value.as_str().parse::<u32>().ok());
            let day = if use_range_end {
                last_day.unwrap_or(first_day)
            } else {
                first_day
            };
            let month = month_number(capture.get(3)?.as_str())?;
            let year = capture
                .get(4)
                .and_then(|value| value.as_str().parse::<i32>().ok())
                .unwrap_or(explicit_year);
            NaiveDate::from_ymd_opt(year, month, day)
        })
        .collect::<Vec<_>>();

    if use_range_end {
        month_dates.last().copied()
    } else {
        month_dates.first().copied()
    }
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b([01]?\d|2[0-3])[:.]([0-5]\d)\b").expect("valid notice time regex")
    })
}

fn parse_time_from_text(text: &str, use_last: bool) -> Option<NaiveTime> {
    let times = time_regex()
        .captures_iter(text)
        .filter_map(|capture| {
            let hour = capture.get(1)?.as_str().parse::<u32>().ok()?;
            let minute = capture.get(2)?.as_str().parse::<u32>().ok()?;
            NaiveTime::from_hms_opt(hour, minute, 0)
        })
        .collect::<Vec<_>>();
    if use_last {
        times.last().copied()
    } else {
        times.first().copied()
    }
}

fn title_has_schedule_range(title: &str) -> bool {
    month_date_regex()
        .captures_iter(title)
        .any(|capture| capture.get(2).is_some())
        || numeric_date_regex().captures_iter(title).count() > 1
        || time_regex().captures_iter(title).count() > 1
}

fn section_value(line: &str, label: &str) -> Option<String> {
    let normalized_line = normalize(line);
    let normalized_label = normalize(label);
    let remainder = normalized_line.strip_prefix(&normalized_label)?;
    if let Some((_, original_remainder)) = line.split_once(':') {
        return Some(original_remainder.trim().to_string());
    }
    Some(
        remainder
            .trim_start_matches([':', ' ', '-'])
            .to_string(),
    )
}

fn is_section_label(line: &str) -> bool {
    [
        "szacunkowa data i godzina przewidywanego rozpoczęcia przerwy w dostawie wody",
        "szacunkowa data i godzina przewidywanego zakończenia przerwy w dostawie wody",
        "miejsce prac",
        "przyczyna prac",
        "skutki prac",
    ]
    .iter()
    .any(|label| section_value(line, label).is_some())
}

fn collect_section(lines: &[String], label: &str) -> String {
    let Some(index) = lines
        .iter()
        .position(|line| section_value(line, label).is_some())
    else {
        return String::new();
    };

    let mut values = Vec::new();
    if let Some(value) = section_value(&lines[index], label)
        && !value.is_empty()
    {
        values.push(value);
    }
    for line in lines.iter().skip(index + 1) {
        if is_section_label(line) || normalize(line) == "polecane artykuly" {
            break;
        }
        if !line.trim().is_empty() {
            values.push(collapse_whitespace(line));
        }
    }
    values.join("\n")
}

fn article_lines(html: &str) -> Vec<String> {
    let document = Html::parse_document(html);
    let blocks_selector =
        Selector::parse("h1, h2, h3, p, li, td, th").expect("valid article text selector");
    let mut root = None;
    for selector_text in [".entry-content", ".post-content", "article", "main"] {
        let selector = Selector::parse(selector_text).expect("valid article content selector");
        if let Some(element) = document.select(&selector).next() {
            root = Some(element);
            break;
        }
    }
    if root.is_none() {
        let body_selector = Selector::parse("body").expect("valid body selector");
        root = document.select(&body_selector).next();
    }

    let Some(root) = root else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for element in root.select(&blocks_selector) {
        let line = collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "));
        if !line.is_empty() && lines.last() != Some(&line) {
            lines.push(line);
        }
    }
    lines
}

fn published_year(value: Option<&str>) -> Option<i32> {
    let value = value?;
    chrono::DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|date| date.year())
        .or_else(|| {
            numeric_date_regex()
                .captures(value)
                .and_then(|capture| capture.get(3))
                .and_then(|year| year.as_str().parse::<i32>().ok())
        })
}

fn format_boundary(date: Option<NaiveDate>, time: Option<NaiveTime>) -> Option<String> {
    date.map(|date| match time {
        Some(time) => date.and_time(time).format("%Y-%m-%dT%H:%M:%S").to_string(),
        None => date.format("%Y-%m-%d").to_string(),
    })
}

fn notice_hash(url: &str, title: &str, schedule: &str, location: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(url.as_bytes());
    digest.update(normalize(title).as_bytes());
    digest.update(normalize(schedule).as_bytes());
    digest.update(normalize(location).as_bytes());
    let bytes = digest.finalize();
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("dabrowskie_wodociagi_{hex}")
}

fn parse_notice(notice: &NoticeLink, html: &str) -> Option<UnifiedAlert> {
    let lines = article_lines(html);
    if lines.is_empty() {
        return None;
    }
    let title = lines
        .iter()
        .find(|line| line.starts_with("Przewidywana przerwa") || line.starts_with("KOMUNIKAT"))
        .cloned()
        .unwrap_or_else(|| notice.title.clone());
    let text = lines.join("\n");
    if !is_water_disruption(&format!("{title}\n{text}")) {
        return None;
    }

    let sections = NoticeSections {
        start: collect_section(
            &lines,
            "Szacunkowa data i godzina przewidywanego rozpoczęcia przerwy w dostawie wody",
        ),
        end: collect_section(
            &lines,
            "Szacunkowa data i godzina przewidywanego zakończenia przerwy w dostawie wody",
        ),
        place: collect_section(&lines, "Miejsce prac"),
        cause: collect_section(&lines, "Przyczyna prac"),
        effects: collect_section(&lines, "Skutki prac"),
    };

    let fallback_year = year_regex()
        .captures(&sections.start)
        .and_then(|capture| capture.get(1))
        .and_then(|value| value.as_str().parse::<i32>().ok())
        .or_else(|| {
            year_regex()
                .captures(&sections.end)
                .and_then(|capture| capture.get(1))
                .and_then(|value| value.as_str().parse::<i32>().ok())
        })
        .or_else(|| {
            year_regex()
                .captures(&title)
                .and_then(|capture| capture.get(1))
                .and_then(|value| value.as_str().parse::<i32>().ok())
        })
        .or_else(|| published_year(notice.published.as_deref()));

    let start_date = parse_date_from_text(&sections.start, false, fallback_year)
        .or_else(|| parse_date_from_text(&title, false, fallback_year))
        .or_else(|| {
            lines.iter().find_map(|line| {
                explicit_start_marker_regex()
                    .is_match(line)
                    .then(|| parse_date_from_text(line, false, fallback_year))
                    .flatten()
            })
        });
    let end_date = parse_date_from_text(&sections.end, true, fallback_year).or_else(|| {
        if !sections.end.is_empty() || title_has_schedule_range(&title) {
            parse_date_from_text(&title, true, fallback_year)
        } else {
            None
        }
    });
    let start_time = parse_time_from_text(&sections.start, false)
        .or_else(|| parse_time_from_text(&title, false));
    let end_time =
        parse_time_from_text(&sections.end, true).or_else(|| parse_time_from_text(&title, true));

    let mut end_date = end_date;
    if let (Some(start), Some(end), Some(start_time), Some(end_time)) =
        (start_date, end_date, start_time, end_time)
        && end == start
        && end_time <= start_time
    {
        end_date = end.succ_opt();
    }

    let schedule = [sections.start.as_str(), sections.end.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    let mut message = if text.starts_with(&title) {
        text.clone()
    } else {
        format!("{title}\n{text}")
    };
    for section in [
        &sections.start,
        &sections.end,
        &sections.place,
        &sections.cause,
        &sections.effects,
    ] {
        if !section.is_empty() && !message.contains(section) {
            message.push('\n');
            message.push_str(section);
        }
    }

    let mut location = [sections.place.as_str(), sections.effects.as_str()]
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if location.is_empty() {
        location = format!("{title}\n{text}");
    }
    let hash = notice_hash(&notice.url, &title, &schedule, &location);

    Some(UnifiedAlert {
        source: AlertSource::DabrowskieWodociagi,
        startDate: format_boundary(start_date, start_time),
        endDate: format_boundary(end_date, end_time),
        message: Some(message),
        location: Some(location),
        address_index: None,
        is_local: None,
        hash: Some(hash),
    })
}

fn mark_locality(alert: &mut UnifiedAlert, settings: &Settings) {
    let location = alert.location.as_deref().unwrap_or_default();
    let matching_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::DabrowskieWodociagi,
                    address,
                )
                && location_matches_address(location, address)
        })
        .map(|(index, _)| index);
    alert.address_index = matching_address;
    alert.is_local = Some(matching_address.is_some());
}

#[async_trait]
impl AlertProvider for DabrowskieWodociagiProvider {
    fn id(&self) -> String {
        "dabrowskie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::DabrowskieWodociagi
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let notices = match fetch_notice_links(client).await {
            Ok(notices) => notices,
            Err(error) => return (Vec::new(), vec![error]),
        };

        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        for notice in notices {
            let response = match client.get(&notice.url).send().await {
                Ok(response) => response,
                Err(error) => {
                    errors.push(format!(
                        "Could not fetch Dąbrowskie Wodociągi notice {}: {error}",
                        notice.url
                    ));
                    continue;
                }
            };
            let response = match response.error_for_status() {
                Ok(response) => response,
                Err(error) => {
                    errors.push(format!(
                        "Dąbrowskie Wodociągi notice {} returned an error: {error}",
                        notice.url
                    ));
                    continue;
                }
            };
            let body = match response.text().await {
                Ok(body) => body,
                Err(error) => {
                    errors.push(format!(
                        "Could not read Dąbrowskie Wodociągi notice {}: {error}",
                        notice.url
                    ));
                    continue;
                }
            };
            if let Some(mut alert) = parse_notice(&notice, &body) {
                mark_locality(&mut alert, settings);
                alerts.push(alert);
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(street: &str, house_no: &str) -> AddressEntry {
        AddressEntry {
            name: "Dom".to_string(),
            city_name: "Dąbrowa Górnicza".to_string(),
            voivodeship: "Śląskie".to_string(),
            district: String::new(),
            commune: "Dąbrowa Górnicza".to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            street_name_2: None,
            house_no: house_no.to_string(),
            city_id: Some(939_473),
            street_id: None,
            is_active: true,
        }
    }

    fn notice(title: &str) -> NoticeLink {
        NoticeLink {
            title: title.to_string(),
            url: format!("{OFFICIAL_HOST_PREFIX}test-notice/"),
            published: Some("Fri, 23 Oct 2026 14:00:00 +0200".to_string()),
        }
    }

    #[test]
    fn parses_rss_links_and_rejects_non_official_article_hosts() {
        let feed = r#"<?xml version="1.0"?><rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/"><channel><title>Komunikaty</title><item><title>Przewidywana przerwa w dostawie wody – ul. Działki</title><link>https://dabrowskie-wodociagi.pl/notice-one/</link><pubDate>Fri, 23 Oct 2026 14:00:00 +0200</pubDate><content:encoded><![CDATA[<p>Brak wody</p>]]></content:encoded></item><item><title>Przerwa w dostawie wody</title><link>https://example.com/not-official/</link></item></channel></rss>"#;
        let parsed = parse_rss(feed).expect("valid RSS feed");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].url, "https://dabrowskie-wodociagi.pl/notice-one/");
        assert_eq!(
            parsed[0].published.as_deref(),
            Some("Fri, 23 Oct 2026 14:00:00 +0200")
        );
    }

    #[test]
    fn empty_rss_is_a_successful_empty_result_and_archive_links_are_filtered() {
        let parsed = parse_rss("<rss version=\"2.0\"><channel/></rss>").unwrap();
        assert!(parsed.is_empty());

        let html = r#"<main><h2><a href="https://dabrowskie-wodociagi.pl/notice/">Przewidywana przerwa w dostawie wody</a></h2><h2><a href="https://example.com/notice/">Awaria wodociągowa</a></h2></main>"#;
        let links = parse_category_html(html);
        assert_eq!(links.len(), 1);
        assert!(links[0].url.starts_with(OFFICIAL_HOST_PREFIX));
    }

    #[test]
    fn parses_polish_date_ranges_and_time_punctuation() {
        assert_eq!(
            parse_date_from_text("19 – 31 października", false, Some(2026)),
            NaiveDate::from_ymd_opt(2026, 10, 19)
        );
        assert_eq!(
            parse_date_from_text("19 – 31października 2026 r.", true, None),
            NaiveDate::from_ymd_opt(2026, 10, 31)
        );
        assert_eq!(
            parse_time_from_text("od godz. 8.00 do godz. 14:30", false),
            NaiveTime::from_hms_opt(8, 0, 0)
        );
        assert_eq!(
            parse_time_from_text("od godz. 8.00 do godz. 14:30", true),
            NaiveTime::from_hms_opt(14, 30, 0)
        );
    }

    #[test]
    fn article_schedule_is_separate_from_publication_date_and_keeps_notice_details() {
        let html = r#"
            <article><div class="entry-content">
              <h1>Przewidywana przerwa w dostawie wody – ul. Działki – 28 października 2026</h1>
              <p>23.10.2026</p>
              <p>Szacunkowa data i godzina przewidywanego rozpoczęcia przerwy w dostawie wody:</p>
              <p>28 października od godz. 8.00</p>
              <p>Szacunkowa data i godzina przewidywanego zakończenia przerwy w dostawie wody:</p>
              <p>28 października do godz. 14.00</p>
              <p>Miejsce prac:</p><p>ul. Działki</p>
              <p>Przyczyna prac:</p><p>Prace eksploatacyjne na sieci wodociągowej</p>
              <p>Skutki prac:</p><p>Brak wody</p>
              <ul><li>ul. Działki 39, 39A, 39 B, 43A</li></ul>
            </div></article>"#;
        let parsed = parse_notice(
            &notice("Przewidywana przerwa w dostawie wody – ul. Działki"),
            html,
        )
        .expect("water interruption notice");

        assert_eq!(parsed.startDate.as_deref(), Some("2026-10-28T08:00:00"));
        assert_eq!(parsed.endDate.as_deref(), Some("2026-10-28T14:00:00"));
        let message = parsed.message.as_deref().unwrap();
        assert!(message.contains("Prace eksploatacyjne"));
        assert!(message.contains("Brak wody"));
        assert!(message.contains("39 B"));
        assert!(parsed.location.as_deref().unwrap().contains("ul. Działki"));
    }

    #[test]
    fn open_ended_disruption_uses_its_explicit_start_without_inventing_an_end() {
        let html = r#"
            <article><div class="entry-content">
              <h1>KOMUNIKAT – Gołonóg i Tworzeń</h1>
              <p>23.10.2026</p>
              <p>Od 5 maja 2026 roku mogą wystąpić zakłócenia w dostawie wody w rejonie Gołonoga i Tworzenia.</p>
            </div></article>"#;
        let parsed = parse_notice(&notice("KOMUNIKAT – Gołonóg i Tworzeń"), html)
            .expect("open-ended water disruption notice");
        assert_eq!(parsed.startDate.as_deref(), Some("2026-05-05"));
        assert_eq!(parsed.endDate, None);
        assert!(parsed.location.as_deref().unwrap().contains("Gołonóg"));
    }

    #[test]
    fn location_and_house_matching_are_scoped_to_the_affected_street_and_numbers() {
        let location = "ul. Działki\nBrak wody\nul. Działki 39, 39A, 39 B, 43A";
        assert!(location_matches_address(
            location,
            &address("Działki", "39A")
        ));
        assert!(house_number_matches_notice(
            location,
            &address("Działki", "39A")
        ));
        assert!(house_number_matches_notice(
            location,
            &address("Działki", "39 B")
        ));
        assert!(!house_number_matches_notice(
            location,
            &address("Działki", "40")
        ));
        assert!(!location_matches_address(
            location,
            &address("Parkowa", "39A")
        ));

        let range = "ul. Zagórze od nr 7 do nr 61";
        assert!(location_matches_address(range, &address("Zagórze", "14")));
        assert!(house_number_matches_notice(
            range,
            &address("Zagórze", "14")
        ));
        assert!(!house_number_matches_notice(
            range,
            &address("Zagórze", "62")
        ));
    }

    #[test]
    fn service_area_accepts_dabrowa_simc_and_commune_but_rejects_other_cities() {
        assert!(crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::DabrowskieWodociagi,
            &address("Działki", "1")
        ));

        let mut village = address("Polna", "1");
        village.city_name = "Gołonóg".to_string();
        village.city_id = Some(123_456);
        assert!(crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::DabrowskieWodociagi,
            &village
        ));

        let mut outside = address("Działki", "1");
        outside.city_name = "Katowice".to_string();
        outside.commune = "Katowice".to_string();
        outside.city_id = Some(937_474);
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::DabrowskieWodociagi,
            &outside
        ));
    }

    #[test]
    fn no_outage_notice_is_not_emitted_as_an_alert() {
        let html = r#"<article><div class="entry-content"><h1>Aktualnie brak przerw w dostawie wody</h1><p>Aktualnie brak przerw w dostawie wody.</p></div></article>"#;
        assert!(parse_notice(&notice("Aktualnie brak przerw w dostawie wody"), html).is_none());
    }
}
