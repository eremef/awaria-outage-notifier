use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::ops::Range;
use std::sync::OnceLock;

pub const ECO_SA_FEED_URL: &str =
    "https://www.ecosa.pl/strefa-klienta/przerwy-w-dostawie-ciepla.feed?type=rss";

pub struct EcoSaProvider;

#[derive(Debug, Deserialize)]
struct RssFeed {
    channel: RssChannel,
}

#[derive(Debug, Deserialize)]
struct RssChannel {
    #[serde(rename = "item", default)]
    items: Vec<RssItem>,
}

#[derive(Debug, Deserialize)]
struct RssItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    guid: String,
}

#[derive(Clone, Debug)]
struct DateOccurrence {
    date: NaiveDate,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug)]
struct TimeOccurrence {
    time: NaiveTime,
    start: usize,
}

#[derive(Debug)]
struct NoticeGroup {
    text: String,
}

fn block_break_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?is)<br\s*/?>|</(?:p|div|li|ul|ol|tr|section|article|h[1-6])\s*>").unwrap()
    })
}

fn list_item_start_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<li\b[^>]*>").unwrap())
}

fn html_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<[^>]+>").unwrap())
}

fn partial_numeric_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"\b(\d{1,2})\s*[-–—]\s*(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap()
    })
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn iso_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{4})-(\d{1,2})-(\d{1,2})\b").unwrap())
}

fn named_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)\s+(\d{4})\b",
        )
        .unwrap()
    })
}

fn named_range_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(\d{1,2})\s*[-–—]\s*(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)\s+(\d{4})\b",
        )
        .unwrap()
    })
}

fn time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})[:.](\d{2})\b").unwrap())
}

fn inline_schedule_boundary_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\s+(oraz\s+w\s+dniach\b)").unwrap())
}

fn street_prefix_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?iu)(?:ul(?:ica)?\.?|aleja|al\.?|plac|pl\.?|osiedle|os\.?)\s+").unwrap()
    })
}

fn street_clause_stop_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?iu)\s+(?:wyst[aą]pi|nast[aą]pi|planowan|przerwa|awari|wstrzym|na potrzeby|dostaw|wszys|zasilan|w przypadku|za utrudnienia|do[lł]o[zż]ymy)\b|;",
        )
        .unwrap()
    })
}

fn street_clauses(line: &str) -> Vec<String> {
    let prefixes = street_prefix_regex().find_iter(line).collect::<Vec<_>>();
    prefixes
        .iter()
        .enumerate()
        .filter_map(|(index, prefix)| {
            let next_prefix = prefixes
                .get(index + 1)
                .map(|next| next.start())
                .unwrap_or(line.len());
            let clause = &line[prefix.end()..next_prefix];
            let end = street_clause_stop_regex()
                .find(clause)
                .map(|found| found.start())
                .unwrap_or(clause.len());
            let street = clause[..end].trim().trim_end_matches([',', ':']).trim();
            (!street.is_empty()).then(|| street.to_string())
        })
        .collect()
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

fn html_to_text(html: &str) -> String {
    let with_list_markers = list_item_start_regex().replace_all(html, "\n• ");
    let with_breaks = block_break_regex().replace_all(&with_list_markers, "\n");
    let without_tags = html_tag_regex().replace_all(&with_breaks, " ");
    let fragment = Html::parse_fragment(&without_tags);
    let decoded = fragment.root_element().text().collect::<Vec<_>>().join("");

    decoded
        .lines()
        .map(|line| {
            line.replace(['\u{a0}', '\u{202f}'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn month_number(month: &str) -> Option<u32> {
    Some(match normalize_polish(month).as_str() {
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

fn date_occurrences(text: &str) -> Vec<DateOccurrence> {
    let mut dates = Vec::new();
    let mut occupied = Vec::<Range<usize>>::new();

    for captures in partial_numeric_range_regex().captures_iter(text) {
        let whole = captures.get(0).unwrap();
        let start_day = captures[1].parse::<u32>().ok();
        let end_day = captures[2].parse::<u32>().ok();
        let month = captures[3].parse::<u32>().ok();
        let year = captures[4].parse::<i32>().ok();
        if let (Some(start_day), Some(end_day), Some(month), Some(year)) =
            (start_day, end_day, month, year)
        {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, start_day) {
                let part = captures.get(1).unwrap();
                dates.push(DateOccurrence {
                    date,
                    start: part.start(),
                    end: part.end(),
                });
            }
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, end_day) {
                let part = captures.get(2).unwrap();
                dates.push(DateOccurrence {
                    date,
                    start: part.start(),
                    end: part.end(),
                });
            }
        }
        occupied.push(whole.start()..whole.end());
    }

    for captures in named_range_regex().captures_iter(text) {
        let whole = captures.get(0).unwrap();
        let start_day = captures[1].parse::<u32>().ok();
        let end_day = captures[2].parse::<u32>().ok();
        let month = month_number(&captures[3]);
        let year = captures[4].parse::<i32>().ok();
        if let (Some(start_day), Some(end_day), Some(month), Some(year)) =
            (start_day, end_day, month, year)
        {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, start_day) {
                let part = captures.get(1).unwrap();
                dates.push(DateOccurrence {
                    date,
                    start: part.start(),
                    end: part.end(),
                });
            }
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, end_day) {
                let part = captures.get(2).unwrap();
                dates.push(DateOccurrence {
                    date,
                    start: part.start(),
                    end: part.end(),
                });
            }
        }
        occupied.push(whole.start()..whole.end());
    }

    for captures in numeric_date_regex().captures_iter(text) {
        let whole = captures.get(0).unwrap();
        if occupied
            .iter()
            .any(|range| range.start < whole.end() && range.end > whole.start())
        {
            continue;
        }
        if let (Ok(day), Ok(month), Ok(year)) = (
            captures[1].parse::<u32>(),
            captures[2].parse::<u32>(),
            captures[3].parse::<i32>(),
        ) && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            dates.push(DateOccurrence {
                date,
                start: whole.start(),
                end: whole.end(),
            });
        }
    }

    for captures in iso_date_regex().captures_iter(text) {
        let whole = captures.get(0).unwrap();
        if let (Ok(year), Ok(month), Ok(day)) = (
            captures[1].parse::<i32>(),
            captures[2].parse::<u32>(),
            captures[3].parse::<u32>(),
        ) && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            dates.push(DateOccurrence {
                date,
                start: whole.start(),
                end: whole.end(),
            });
        }
    }

    for captures in named_date_regex().captures_iter(text) {
        let whole = captures.get(0).unwrap();
        if occupied
            .iter()
            .any(|range| range.start < whole.end() && range.end > whole.start())
        {
            continue;
        }
        let day = captures[1].parse::<u32>().ok();
        let month = month_number(&captures[2]);
        let year = captures[3].parse::<i32>().ok();
        if let (Some(day), Some(month), Some(year)) = (day, month, year)
            && let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        {
            dates.push(DateOccurrence {
                date,
                start: whole.start(),
                end: whole.end(),
            });
        }
    }

    dates.sort_by_key(|occurrence| (occurrence.start, occurrence.end));
    dates
}

fn mask_dates(text: &str) -> String {
    let mut ranges = Vec::new();
    for regex in [
        partial_numeric_range_regex(),
        named_range_regex(),
        numeric_date_regex(),
        iso_date_regex(),
        named_date_regex(),
    ] {
        ranges.extend(
            regex
                .find_iter(text)
                .map(|found| found.start()..found.end()),
        );
    }
    ranges.sort_by_key(|range| range.start);
    ranges.dedup_by(|left, right| left.start == right.start && left.end == right.end);

    let mut masked = text.to_string();
    for range in ranges.into_iter().rev() {
        let byte_len = range.end - range.start;
        masked.replace_range(range, &" ".repeat(byte_len));
    }
    masked
}

fn parse_time(hour: &str, minute: &str) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn time_occurrences(text: &str) -> Vec<TimeOccurrence> {
    time_regex()
        .captures_iter(&mask_dates(text))
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            Some(TimeOccurrence {
                time: parse_time(&captures[1], &captures[2])?,
                start: whole.start(),
            })
        })
        .collect()
}

fn time_for_date(
    index: usize,
    dates: &[DateOccurrence],
    times: &[TimeOccurrence],
) -> Option<NaiveTime> {
    let current = dates.get(index)?;
    let next_date_position = dates
        .get(index + 1)
        .map(|next| next.start)
        .unwrap_or(usize::MAX);

    times
        .iter()
        .find(|time| time.start >= current.end && time.start < next_date_position)
        .map(|time| time.time)
}

fn format_schedule_date(date: NaiveDate, time: Option<NaiveTime>) -> String {
    match time {
        Some(time) => NaiveDateTime::new(date, time)
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string(),
        None => date.format("%Y-%m-%d").to_string(),
    }
}

fn is_expected_restoration(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "przewidywan",
        "oczekiwan",
        "spodziewan",
        "planowane wznowienie",
        "wznowienie dostaw",
        "wznowienie dostawy",
        "zostanie wznowion",
        "przywrocenie dostaw",
        "przywrocenie dostawy",
        "powrot dostaw",
        "termin usuniecia awarii",
        "usuniecie awarii",
        "zostanie przywrocon",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn parse_schedule(text: &str, fallback_text: &str) -> (Option<String>, Option<String>) {
    let mut dates = date_occurrences(text);
    let date_source = if dates.is_empty() {
        dates = date_occurrences(fallback_text);
        fallback_text
    } else {
        text
    };
    if dates.is_empty() {
        return (None, None);
    }

    let times = time_occurrences(date_source);
    if dates.len() == 1 && is_expected_restoration(date_source) {
        return (None, Some(format_schedule_date(dates[0].date, None)));
    }

    let start = format_schedule_date(dates[0].date, time_for_date(0, &dates, &times));
    let end = dates.last().filter(|_| dates.len() > 1).map(|last| {
        let last_index = dates.len() - 1;
        format_schedule_date(last.date, time_for_date(last_index, &dates, &times))
    });
    (Some(start), end)
}

fn has_schedule_context(line: &str) -> bool {
    let normalized = normalize_polish(line);
    [
        "w dniu",
        "w dniach",
        "od dnia",
        "przerwa",
        "awari",
        "wstrzym",
        "nastapi",
        "wystapi",
        "planowan",
        "obiektach",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn split_notice_groups(body: &str) -> Vec<NoticeGroup> {
    let body_with_schedule_breaks = inline_schedule_boundary_regex().replace_all(body, "\n$1");
    let lines = body_with_schedule_breaks
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Vec::new();
    }

    let boundaries = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            (!date_occurrences(line).is_empty()
                && has_schedule_context(line)
                && !is_expected_restoration(line))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    if boundaries.len() < 2 {
        return vec![NoticeGroup {
            text: lines.join("\n"),
        }];
    }

    let mut starts = vec![0];
    starts.extend(boundaries.into_iter().filter(|index| *index > 0));
    starts.sort_unstable();
    starts.dedup();
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(lines.len());
            NoticeGroup {
                text: lines[*start..end].join("\n"),
            }
        })
        .filter(|group| !date_occurrences(&group.text).is_empty())
        .collect()
}

fn outage_kind(title: &str, body: &str) -> Option<&'static str> {
    let combined = normalize_polish(&format!("{title}\n{body}"));
    let outage_context = ["przerwa", "awari", "wstrzym", "zaklocen", "brak dostaw"]
        .iter()
        .any(|marker| combined.contains(marker));
    if !outage_context {
        return None;
    }

    if combined.contains("planowan") {
        Some("Planowana przerwa w dostawie ciepła")
    } else if combined.contains("awari") {
        Some("Awaria ciepła")
    } else {
        Some("Przerwa w dostawie ciepła")
    }
}

fn phrase_matches(haystack: &str, needle: &str) -> bool {
    let haystack = normalize_polish(haystack);
    let needle = normalize_polish(needle);
    if needle.is_empty() {
        return false;
    }
    haystack
        .split(|character: char| !character.is_alphanumeric())
        .collect::<Vec<_>>()
        .windows(needle.split_whitespace().count().max(1))
        .any(|words| words.join(" ") == needle)
}

fn city_variants(city: &str) -> Vec<String> {
    let normalized = normalize_polish(city.trim());
    let mut variants = vec![normalized.clone()];
    let known_inflections = [
        ("krapkowice", "krapkowicach"),
        ("kety", "ketach"),
        ("lubliniec", "lublincu"),
        ("malbork", "malborku"),
        ("opole", "opolu"),
    ];
    if let Some((_, inflected)) = known_inflections
        .iter()
        .find(|(canonical, _)| *canonical == normalized)
    {
        variants.push((*inflected).to_string());
    }
    variants
}

fn known_city_voivodeship(city: &str) -> Option<&'static str> {
    match normalize_polish(city.trim()).as_str() {
        "krapkowice" => Some("opolskie"),
        "kety" => Some("malopolskie"),
        "lubliniec" => Some("slaskie"),
        "malbork" => Some("pomorskie"),
        _ => None,
    }
}

fn configured_city_in_text(text: &str, settings: &Settings) -> Option<String> {
    settings
        .addresses
        .iter()
        .filter(|address| address.is_active)
        .find(|address| {
            city_variants(&address.city_name)
                .iter()
                .any(|variant| phrase_matches(text, variant))
        })
        .map(|address| address.city_name.trim().to_string())
}

fn title_city_candidate(title: &str, link: &str) -> Option<String> {
    let title_without_parenthetical = title.split('(').next().unwrap_or(title).trim();
    let candidate = title_without_parenthetical
        .rsplit_once(" - ")
        .map(|(_, city)| city.trim())
        .or_else(|| {
            title_without_parenthetical
                .rsplit_once(" — ")
                .map(|(_, city)| city.trim())
        })
        .or_else(|| {
            title_without_parenthetical
                .rsplit_once(": ")
                .map(|(_, city)| city.trim())
        })
        .unwrap_or(title_without_parenthetical);
    if !candidate.is_empty()
        && candidate.split_whitespace().count() <= 4
        && ![
            "awaria",
            "przerwa",
            "dostawie",
            "ciepla",
            "ciepła",
            "planowana",
        ]
        .iter()
        .any(|word| normalize_polish(candidate).contains(word))
    {
        return Some(candidate.to_string());
    }

    let slug = link
        .split('?')
        .next()
        .unwrap_or(link)
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".html")
        .replace('-', " ");
    if slug.is_empty() {
        None
    } else {
        let mut chars = slug.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().to_string() + chars.as_str())
    }
}

fn extract_city(title: &str, link: &str, body: &str, settings: &Settings) -> String {
    let title_text = html_to_text(title);
    if let Some(city) = configured_city_in_text(&title_text, settings) {
        return city;
    }
    if let Some(city) = configured_city_in_text(body, settings) {
        return city;
    }
    title_city_candidate(&title_text, link).unwrap_or_else(|| "Nieustalona miejscowość".to_string())
}

fn is_citywide_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    [
        "wszystkich obiektow",
        "wszystkie obiekty",
        "wszystkich budynkow",
        "wszystkie budynki",
        "wszystkich odbiorcow",
        "calego miasta",
        "na terenie miasta",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn location_details(group: &str) -> String {
    let mut details = Vec::new();
    for line in group.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let clean = line.trim_start_matches('•').trim();
        if clean.is_empty() || !date_occurrences(clean).is_empty() {
            for street in street_clauses(clean) {
                if !details.contains(&street) {
                    details.push(street);
                }
            }
            continue;
        }

        if line.starts_with('•') {
            if !details.iter().any(|detail| detail == clean) {
                details.push(clean.to_string());
            }
            continue;
        }

        for street in street_clauses(clean) {
            if !details.iter().any(|detail| detail == &street) {
                details.push(street);
            }
        }
    }

    details.join("\n")
}

fn street_matches_address(text: &str, address: &AddressEntry) -> bool {
    let mut configured_streets = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    if let Some(second_street) = address.street_name_2.as_deref()
        && !second_street.trim().is_empty()
    {
        configured_streets.push(second_street.to_string());
    }

    if configured_streets.is_empty() {
        return true;
    }

    let notice_words = normalize_polish(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    configured_streets.into_iter().any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let tokens = normalize_polish(street)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect::<Vec<_>>();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem_length = token.chars().count().saturating_sub(2).max(3);
                let stem = token.chars().take(stem_length).collect::<String>();
                notice_words.iter().any(|word| word.starts_with(&stem))
            })
    })
}

/// Check whether an alert's location belongs to an address. The first line is
/// always the canonical city; optional later lines contain only the streets or
/// buildings affected by that schedule group.
pub(crate) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    let mut lines = location.lines();
    let locality = lines.next().unwrap_or_default().trim();
    let locality = locality
        .strip_prefix("Miejscowość:")
        .unwrap_or(locality)
        .trim();
    if normalize_polish(locality) != normalize_polish(address.city_name.trim()) {
        return false;
    }
    if let Some(expected_voivodeship) = known_city_voivodeship(locality)
        && !address.voivodeship.trim().is_empty()
        && normalize_polish(address.voivodeship.trim()) != expected_voivodeship
    {
        return false;
    }

    let affected_locations = lines.collect::<Vec<_>>().join(" ");
    affected_locations.trim().is_empty() || street_matches_address(&affected_locations, address)
}

/// Return only the affected location lines for this address's street. This
/// keeps central house-number filtering scoped to the schedule's own street
/// group. Citywide alerts have no street/number lines and return an empty spec.
pub(crate) fn number_spec_for_address(location: &str, address: &AddressEntry) -> String {
    if !location_matches_address(location, address) {
        return String::new();
    }
    location
        .lines()
        .skip(1)
        .filter(|line| street_matches_address(line, address))
        .collect::<Vec<_>>()
        .join("\n")
}

fn matching_address_index(location: &str, settings: &Settings) -> Option<usize> {
    settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| address.is_active && location_matches_address(location, address))
        .map(|(index, _)| index)
}

fn stable_alert_hash(
    identity: &str,
    city: &str,
    group_index: usize,
    start: Option<&str>,
    end: Option<&str>,
) -> String {
    let mut digest = Sha256::new();
    digest.update("eco_sa:");
    digest.update(identity);
    digest.update(":");
    digest.update(city);
    digest.update(":");
    digest.update(group_index.to_string());
    digest.update(":");
    digest.update(start.unwrap_or_default());
    digest.update(":");
    digest.update(end.unwrap_or_default());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_stale_boundary(value: &str) -> Option<NaiveDateTime> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        // Keep date-only starts and ends visible as dates. For expiry checks,
        // interpret them conservatively as the end of that Warsaw calendar day.
        return date.and_hms_opt(23, 59, 59);
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .or_else(|| {
            crate::utils::parse_date(value)
                .map(|date_time| date_time.with_timezone(&Warsaw).naive_local())
        })
}

fn is_stale_alert(start_date: Option<&str>, end_date: Option<&str>, now: NaiveDateTime) -> bool {
    if let Some(end) = end_date.and_then(parse_stale_boundary) {
        return end < now;
    }
    start_date
        .and_then(parse_stale_boundary)
        .is_some_and(|start| start + Duration::hours(72) < now)
}

fn item_alerts(item: RssItem, settings: &Settings) -> Vec<UnifiedAlert> {
    let title = html_to_text(&item.title);
    let body = if item.description.trim().is_empty() {
        title.clone()
    } else {
        html_to_text(&item.description)
    };
    let Some(kind) = outage_kind(&title, &body) else {
        return Vec::new();
    };

    let city = extract_city(&title, &item.link, &body, settings);
    let groups = split_notice_groups(&body);
    if groups.is_empty() {
        return Vec::new();
    }

    let full_text = [title.trim(), body.trim()]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let message = format!("{kind}: {full_text}");
    let identity = if !item.guid.trim().is_empty() {
        item.guid.trim()
    } else if !item.link.trim().is_empty() {
        item.link.trim()
    } else {
        title.trim()
    };
    let now = Utc::now().with_timezone(&Warsaw).naive_local();

    groups
        .into_iter()
        .enumerate()
        .filter_map(|(index, group)| {
            let (start_date, end_date) = parse_schedule(&group.text, &title);
            if start_date.is_none() && end_date.is_none() {
                return None;
            }
            if is_stale_alert(start_date.as_deref(), end_date.as_deref(), now) {
                return None;
            }

            let details = location_details(&group.text);
            let citywide = is_citywide_notice(&group.text);
            let city_location = format!("Miejscowość: {city}");
            let schedule_location = if details.is_empty() || citywide {
                city_location.clone()
            } else {
                format!("{city_location}\n{details}")
            };
            let address_index = if citywide || !details.is_empty() {
                matching_address_index(&schedule_location, settings)
            } else {
                None
            };
            let location = if address_index.is_some() {
                schedule_location
            } else {
                city_location
            };

            Some(UnifiedAlert {
                source: AlertSource::EcoSa,
                startDate: start_date.clone(),
                endDate: end_date.clone(),
                message: Some(message.clone()),
                location: Some(location),
                address_index,
                is_local: Some(address_index.is_some()),
                hash: Some(stable_alert_hash(
                    identity,
                    &city,
                    index,
                    start_date.as_deref(),
                    end_date.as_deref(),
                )),
            })
        })
        .collect()
}

pub(crate) fn parse_rss(xml: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    // RSS pubDate describes when the article was published. ECO has served old
    // pubDate values for current notices, so this parser deliberately ignores it.
    let feed: RssFeed = quick_xml::de::from_str(xml).map_err(|error| error.to_string())?;
    let alerts = feed
        .channel
        .items
        .into_iter()
        .flat_map(|item| item_alerts(item, settings))
        .collect();
    Ok(crate::api_logic::deduplicate_alerts(alerts))
}

#[async_trait]
impl AlertProvider for EcoSaProvider {
    fn id(&self) -> String {
        "eco_sa".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::EcoSa
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

        let response = match client
            .get(ECO_SA_FEED_URL)
            .header(
                reqwest::header::USER_AGENT,
                "Awaria/1.0 (+https://github.com/eremef/awaria)",
            )
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return (Vec::new(), vec![format!("ECO SA request error: {error}")]);
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("ECO SA HTTP error: {}", response.status())],
            );
        }

        match response.text().await {
            Ok(xml) => match parse_rss(&xml, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (Vec::new(), vec![format!("ECO SA RSS parse error: {error}")]),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("ECO SA response read error: {error}")],
            ),
        }
    }
}
