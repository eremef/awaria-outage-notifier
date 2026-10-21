use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

pub const WIK_OPOLE_POSTS_URL: &str = "https://wikopole.com.pl/wp-json/wp/v2/posts?categories=11&per_page=100&_fields=id,date,date_gmt,link,title,content,categories";

const WIK_OPOLE_CATEGORY_ID: u64 = 11;
const MAX_POST_PAGES: usize = 10;
const UNKNOWN_END_RETENTION_HOURS: i64 = 72;

pub struct WikOpoleProvider;

#[derive(Debug, Deserialize, Default)]
struct RenderedField {
    #[serde(default)]
    rendered: String,
}

#[derive(Debug, Deserialize, Default)]
struct WikPost {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    date: String,
    #[serde(default)]
    date_gmt: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    title: RenderedField,
    #[serde(default)]
    content: RenderedField,
    #[serde(default)]
    categories: Vec<u64>,
}

#[derive(Clone, Copy)]
struct ServiceLocality {
    key: &'static str,
    name: &'static str,
    city_id: Option<u64>,
    aliases: &'static [&'static str],
}

const SERVICE_LOCALITIES: &[ServiceLocality] = &[
    ServiceLocality {
        key: "opole",
        name: "Opole",
        city_id: Some(965016),
        aliases: &["opole", "opolu", "opola", "opolowi", "opolskiego"],
    },
    ServiceLocality {
        key: "komprachcice",
        name: "Komprachcice",
        city_id: None,
        aliases: &["komprachcice", "komprachcicach", "komprachcic"],
    },
    ServiceLocality {
        key: "proszkow",
        name: "Prószków",
        city_id: None,
        aliases: &["proszkow", "proszkowie", "proszkowa"],
    },
    ServiceLocality {
        key: "turawa",
        name: "Turawa",
        city_id: None,
        aliases: &["turawa", "turawie", "turawy"],
    },
];

#[derive(Clone)]
struct AffectedLocality {
    locality: &'static ServiceLocality,
    street_lines: Vec<String>,
}

#[derive(Clone, Copy)]
enum NoticeKind {
    Emergency,
    Planned,
    Interruption,
}

impl NoticeKind {
    fn from_text(title: &str, body: &str) -> Option<Self> {
        let normalized = normalize_polish(&format!("{title} {body}"));

        if [
            "brak awarii",
            "brak przerw",
            "nie bedzie przerwy",
            "nie bedzie wylaczenia",
            "nie nastapi przerwa",
            "nie zostana pozbawieni wody",
            "nie beda pozbawieni wody",
            "woda nie bedzie wstrzymana",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase))
        {
            return None;
        }

        // Category 11 also contains roadworks notices. Require clear wording about
        // interrupted water service instead of treating category membership as enough.
        let water_interruption = [
            "przerwa w dostawie wody",
            "przerwy w dostawie wody",
            "przerwa w dostarczaniu wody",
            "wylaczenie wody",
            "wylaczenia wody",
            "wylaczenie dostawy wody",
            "brak wody",
            "nie bedzie wody",
            "wody nie bedzie",
            "pozbawieni wody",
            "wstrzymanie dostawy wody",
            "wstrzymana dostawa wody",
            "wstrzymana bedzie dostawa wody",
            "wstrzymana zostanie dostawa wody",
            "wstrzymany doplyw wody",
            "dostawa wody bedzie wstrzymana",
            "dostawa wody zostanie wstrzymana",
            "przerwana dostawa wody",
            "awaria sieci wodociagowej",
            "awaria na sieci wodociagowej",
            "awaria w sieci wodociagowej",
            "awaria wodociagu",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase));

        if !water_interruption {
            return None;
        }

        if normalized.contains("awari") {
            Some(Self::Emergency)
        } else if ["planowan", "robot", "prac", "modernizac", "konserwac"]
            .iter()
            .any(|word| normalized.contains(word))
        {
            Some(Self::Planned)
        } else {
            Some(Self::Interruption)
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Emergency => "Awaria wodociągowa",
            Self::Planned => "Planowana przerwa w dostawie wody",
            Self::Interruption => "Przerwa w dostawie wody",
        }
    }
}

#[derive(Clone, Default)]
struct ParsedSchedule {
    start: Option<String>,
    end: Option<String>,
    expires_at: Option<NaiveDateTime>,
}

fn normalize_polish(value: &str) -> String {
    crate::utils::clean_text(value)
}

fn normalize_whitespace(value: &str) -> String {
    value
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn html_block_end_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)</(?:p|li|div|h[1-6]|tr|section|article|ul|ol)>\s*").unwrap()
    })
}

fn html_break_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<br\s*/?>").unwrap())
}

fn html_to_text(value: &str) -> String {
    let with_lines = html_block_end_regex().replace_all(value, "\n");
    let with_lines = html_break_regex().replace_all(&with_lines, "\n");
    let fragment = Html::parse_fragment(&with_lines);
    fragment
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("")
        .lines()
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn street_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:ul(?:ica|icy|ic)?|al(?:eja|ei)?|pl(?:ac|acu)?|os(?:iedle|iedlu)?|rondo|rynek)\b\.?\s+\p{L}")
            .unwrap()
    })
}

fn schedule_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:w\s+dni(?:u|ach)|dnia|od\s+dnia|w\s+terminie|w\s+okresie|w\s+godzinach|w\s+godz\.?|od\s+godz(?:iny)?\.?|do\s+godz(?:iny)?\.?|godz(?:ina)?\.?|od\s+\d{1,2}[:.]\d{2})\b",
        )
        .unwrap()
    })
}

fn extract_street_clause(line: &str) -> Option<String> {
    let street_start = street_line_regex().find(line)?.start();
    let after_street_prefix = &line[street_start..];
    let clause_end = schedule_marker_regex()
        .find(after_street_prefix)
        .map(|marker| marker.start())
        .unwrap_or(after_street_prefix.len());
    let clause = after_street_prefix[..clause_end]
        .trim()
        .trim_end_matches([',', ';', ':']);
    let clause = normalize_whitespace(clause);
    (!clause.is_empty()).then_some(clause)
}

fn month_number(value: &str) -> Option<u32> {
    match normalize_polish(value).as_str() {
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
        "listopad" | "listopada" => Some(11),
        "grudzien" | "grudnia" => Some(12),
        _ => None,
    }
}

fn numeric_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})[./-](\d{1,2})[./-](\d{4})(?:\s*r\.?)?\b").unwrap())
}

fn numeric_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(\d{1,2})\s*(?:-|–|—)\s*(\d{1,2})[./](\d{1,2})[./](\d{4})(?:\s*r\.?)?\b")
            .unwrap()
    })
}

fn written_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)\b(\d{1,2})\s*(?:-|–|—|do)\s*(\d{1,2})\s+(stycznia|styczen|lutego|luty|marca|marzec|kwietnia|kwiecien|maja|maj|czerwca|czerwiec|lipca|lipiec|sierpnia|sierpien|wrzesnia|wrzesien|pazdziernika|pazdziernik|listopada|listopad|grudnia|grudzien)\s+(\d{4})(?:\s*r\.?)?\b")
            .unwrap()
    })
}

fn written_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)\b(\d{1,2})\s+(stycznia|styczen|lutego|luty|marca|marzec|kwietnia|kwiecien|maja|maj|czerwca|czerwiec|lipca|lipiec|sierpnia|sierpien|wrzesnia|wrzesien|pazdziernika|pazdziernik|listopada|listopad|grudnia|grudzien)\s+(\d{4})(?:\s*r\.?)?\b")
            .unwrap()
    })
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(?:godz(?:ina)?\.?\s*)?(\d{1,2}[:.]\d{2})\s*(?:-|–|—|do)\s*(?:godz(?:ina)?\.?\s*)?(\d{1,2}[:.]\d{2})")
            .unwrap()
    })
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}[:.]\d{2})\b").unwrap())
}

fn parse_numeric_date(day: &str, month: &str, year: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
}

fn date_mentions(text: &str) -> Vec<NaiveDate> {
    let mut mentions = Vec::<(usize, Vec<NaiveDate>)>::new();
    let mut word_range_spans = Vec::new();

    for captures in written_range_regex().captures_iter(text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let (Ok(first_day), Ok(last_day), Ok(year)) = (
            captures[1].parse::<u32>(),
            captures[2].parse::<u32>(),
            captures[4].parse::<i32>(),
        ) else {
            continue;
        };
        let Some(month) = month_number(&captures[3]) else {
            continue;
        };
        let (Some(first), Some(last)) = (
            NaiveDate::from_ymd_opt(year, month, first_day),
            NaiveDate::from_ymd_opt(year, month, last_day),
        ) else {
            continue;
        };
        if last < first {
            continue;
        }
        word_range_spans.push((whole.start(), whole.end()));
        mentions.push((whole.start(), vec![first, last]));
    }

    for captures in numeric_range_regex().captures_iter(text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let (Ok(first_day), Ok(last_day)) =
            (captures[1].parse::<u32>(), captures[2].parse::<u32>())
        else {
            continue;
        };
        let Some(first) = parse_numeric_date(&first_day.to_string(), &captures[3], &captures[4])
        else {
            continue;
        };
        let Some(last) = parse_numeric_date(&last_day.to_string(), &captures[3], &captures[4])
        else {
            continue;
        };
        if last < first {
            continue;
        }
        word_range_spans.push((whole.start(), whole.end()));
        mentions.push((whole.start(), vec![first, last]));
    }

    for captures in numeric_date_regex().captures_iter(text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        if word_range_spans
            .iter()
            .any(|(start, end)| whole.start() >= *start && whole.end() <= *end)
        {
            continue;
        }
        let Some(date) = parse_numeric_date(&captures[1], &captures[2], &captures[3]) else {
            continue;
        };
        mentions.push((whole.start(), vec![date]));
    }

    for captures in written_date_regex().captures_iter(text) {
        let Some(whole) = captures.get(0) else {
            continue;
        };
        if word_range_spans
            .iter()
            .any(|(start, end)| whole.start() >= *start && whole.end() <= *end)
        {
            continue;
        }
        let (Ok(day), Ok(year)) = (captures[1].parse::<u32>(), captures[3].parse::<i32>()) else {
            continue;
        };
        let Some(month) = month_number(&captures[2]) else {
            continue;
        };
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            mentions.push((whole.start(), vec![date]));
        }
    }

    mentions.sort_by_key(|(position, _)| *position);
    mentions.into_iter().flat_map(|(_, dates)| dates).collect()
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(&value.replace('.', ":"), "%H:%M").ok()
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn end_of_day(date: NaiveDate) -> NaiveDateTime {
    date.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
}

fn extract_schedule(text: &str) -> ParsedSchedule {
    let dates = date_mentions(text);
    let mut schedule_text = written_range_regex().replace_all(text, " ").into_owned();
    schedule_text = numeric_range_regex()
        .replace_all(&schedule_text, " ")
        .into_owned();
    schedule_text = numeric_date_regex()
        .replace_all(&schedule_text, " ")
        .into_owned();
    schedule_text = written_date_regex()
        .replace_all(&schedule_text, " ")
        .into_owned();

    let time_ranges: Vec<_> = time_range_regex()
        .captures_iter(&schedule_text)
        .filter_map(|captures| Some((parse_time(&captures[1])?, parse_time(&captures[2])?)))
        .collect();
    let mut times: Vec<_> = time_regex()
        .captures_iter(&schedule_text)
        .filter_map(|captures| parse_time(&captures[1]))
        .collect();
    times.sort();

    let mut unique_dates = dates;
    unique_dates.sort();
    unique_dates.dedup();
    let (Some(first_date), Some(last_date)) =
        (unique_dates.first().copied(), unique_dates.last().copied())
    else {
        return ParsedSchedule::default();
    };

    // A single shared time window over multiple dates does not say whether the
    // outage repeats daily or continues across the whole span. Keep the prose as
    // the source of truth and avoid manufacturing either interpretation.
    if unique_dates.len() > 1 {
        if !time_ranges.is_empty() || !times.is_empty() {
            return ParsedSchedule {
                start: Some(first_date.format("%Y-%m-%d").to_string()),
                end: Some(last_date.format("%Y-%m-%d").to_string()),
                expires_at: Some(end_of_day(last_date)),
            };
        }
        return ParsedSchedule {
            start: Some(first_date.format("%Y-%m-%d").to_string()),
            end: Some(last_date.format("%Y-%m-%d").to_string()),
            expires_at: Some(end_of_day(last_date)),
        };
    }

    let schedule_date = first_date;
    if time_ranges.len() == 1 && times.len() == 2 {
        let (start_time, end_time) = time_ranges[0];
        let start = schedule_date.and_time(start_time);
        let end_date = if end_time <= start_time {
            schedule_date + Duration::days(1)
        } else {
            schedule_date
        };
        let end = end_date.and_time(end_time);
        return ParsedSchedule {
            start: Some(format_datetime(start)),
            end: Some(format_datetime(end)),
            expires_at: Some(end),
        };
    }

    if time_ranges.len() > 1 || times.len() > 2 {
        return ParsedSchedule {
            start: Some(schedule_date.format("%Y-%m-%d").to_string()),
            end: None,
            expires_at: Some(end_of_day(schedule_date)),
        };
    }

    if let Some(time) = times.first().copied() {
        let start = schedule_date.and_time(time);
        return ParsedSchedule {
            start: Some(format_datetime(start)),
            end: None,
            expires_at: Some(start + Duration::hours(24)),
        };
    }

    ParsedSchedule {
        start: Some(schedule_date.format("%Y-%m-%d").to_string()),
        end: None,
        expires_at: Some(end_of_day(schedule_date)),
    }
}

fn parse_publication_time(value: &str) -> Option<NaiveDateTime> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|datetime| datetime.with_timezone(&Warsaw).naive_local())
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .or_else(|| {
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .ok()?
                        .and_hms_opt(0, 0, 0)
                })
        })
}

fn is_stale(
    schedule: &ParsedSchedule,
    publication_time: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> bool {
    if schedule
        .expires_at
        .is_some_and(|expires_at| expires_at < now)
    {
        return true;
    }
    if schedule.expires_at.is_none()
        && publication_time
            .is_some_and(|published| published + Duration::hours(UNKNOWN_END_RETENTION_HOURS) < now)
    {
        return true;
    }
    false
}

fn locality_mentions(text: &str, locality: &ServiceLocality) -> bool {
    let normalized = normalize_polish(text);
    let tokens: HashSet<_> = normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    locality.aliases.iter().any(|alias| tokens.contains(alias))
}

fn find_localities(text: &str) -> Vec<&'static ServiceLocality> {
    SERVICE_LOCALITIES
        .iter()
        .filter(|locality| locality_mentions(text, locality))
        .collect()
}

fn locality_is_company_boilerplate(line: &str, locality: &ServiceLocality) -> bool {
    if locality.key != "opole" {
        return false;
    }
    let normalized = normalize_polish(line);
    normalized.contains("wik opole")
        || normalized.contains("wik w opolu")
        || normalized.contains("wodociagi i kanalizacja w opolu")
        || normalized.contains("wodociagi kanalizacja opole")
}

fn affected_localities(text: &str) -> Vec<AffectedLocality> {
    let mut groups = BTreeMap::<&'static str, AffectedLocality>::new();
    let mut current_locality: Option<&'static ServiceLocality> = None;
    let mut has_supported_locality_mention = false;

    for line in text
        .lines()
        .flat_map(|line| line.split(';'))
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
    {
        let mentioned: Vec<_> = find_localities(&line)
            .into_iter()
            .filter(|locality| !locality_is_company_boilerplate(&line, locality))
            .collect();
        let street_clause = extract_street_clause(&line);

        if !mentioned.is_empty() {
            has_supported_locality_mention = true;
            current_locality = mentioned.first().copied();
            for locality in mentioned {
                let group = groups
                    .entry(locality.key)
                    .or_insert_with(|| AffectedLocality {
                        locality,
                        street_lines: Vec::new(),
                    });
                if let Some(street_clause) = &street_clause
                    && !group.street_lines.contains(street_clause)
                {
                    group.street_lines.push(street_clause.clone());
                }
            }
        } else if let Some(street_clause) = street_clause {
            let locality = current_locality.unwrap_or(&SERVICE_LOCALITIES[0]);
            let group = groups
                .entry(locality.key)
                .or_insert_with(|| AffectedLocality {
                    locality,
                    street_lines: Vec::new(),
                });
            if !group.street_lines.contains(&street_clause) {
                group.street_lines.push(street_clause);
            }
        }
    }

    let normalized_text = normalize_polish(text);
    let has_explicit_locality_context = ["miejscowosc", "miejscowosci", "gminie", "gminy"]
        .iter()
        .any(|phrase| normalized_text.contains(phrase));
    if !has_supported_locality_mention && has_explicit_locality_context {
        // Avoid assigning an explicitly named but unsupported town to Opole.
        return Vec::new();
    }

    if groups.is_empty() {
        // WiK's water interruption notices are published for its Opole service
        // area and the sampled street notices omit the city name in the body.
        groups.insert(
            SERVICE_LOCALITIES[0].key,
            AffectedLocality {
                locality: &SERVICE_LOCALITIES[0],
                street_lines: Vec::new(),
            },
        );
    }

    groups.into_values().collect()
}

fn address_matches_locality(address: &AddressEntry, locality: &ServiceLocality) -> bool {
    if locality
        .city_id
        .is_some_and(|city_id| address.city_id == Some(city_id))
    {
        return true;
    }

    let aliases: HashSet<_> = locality.aliases.iter().copied().collect();
    [address.city_name.as_str(), address.commune.as_str()]
        .into_iter()
        .map(normalize_polish)
        .any(|value| {
            let tokens: Vec<_> = value
                .split(|character: char| !character.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect();
            !tokens.is_empty() && tokens.iter().any(|token| aliases.contains(token))
        })
}

fn street_matches_address(street_text: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let segments: Vec<_> = street_text
        .split(';')
        .map(normalize_polish)
        .filter(|segment| !segment.is_empty())
        .collect();

    candidates.into_iter().any(|candidate| {
        let candidate = crate::utils::strip_street_prefixes(candidate.trim());
        let candidate_tokens: Vec<_> = normalize_polish(candidate)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(|token| {
                if token.ends_with('a') && token.chars().count() > 5 {
                    token[..token.len() - 1].to_string()
                } else {
                    token.to_string()
                }
            })
            .collect();
        !candidate_tokens.is_empty()
            && segments.iter().any(|segment| {
                let notice_tokens: Vec<_> = segment
                    .split(|character: char| !character.is_alphanumeric())
                    .filter(|token| !token.is_empty())
                    .collect();
                candidate_tokens.iter().all(|candidate_token| {
                    notice_tokens
                        .iter()
                        .any(|notice_token| notice_token.starts_with(candidate_token.as_str()))
                })
            })
    })
}

fn address_matches_group(address: &AddressEntry, group: &AffectedLocality) -> bool {
    if !address_matches_locality(address, group.locality) {
        return false;
    }
    let has_street = !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty());
    !has_street
        || (!group.street_lines.is_empty()
            && street_matches_address(&group.street_lines.join("; "), address))
}

/// Checks that a rendered WiK location belongs to the configured locality and street.
/// House-number matching remains centralized in `Settings.filter_by_house_no`.
pub(crate) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    let (locality_text, street_text) = location
        .split_once(';')
        .map(|(locality, details)| (locality, details.trim()))
        .unwrap_or((location, ""));
    let locality_label = locality_text
        .split_once(':')
        .map(|(_, value)| value.trim())
        .unwrap_or(locality_text.trim());
    let Some(locality) = find_localities(locality_label).first().copied() else {
        return false;
    };
    if !address_matches_locality(address, locality) {
        return false;
    }
    let has_street = !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty());
    !has_street || (!street_text.is_empty() && street_matches_address(street_text, address))
}

fn stable_alert_hash(post: &WikPost, locality: &ServiceLocality, message: &str) -> String {
    let mut digest = Sha256::new();
    digest.update("wik_opole:");
    if post.id != 0 {
        digest.update(post.id.to_string());
    } else if !post.link.trim().is_empty() {
        digest.update(post.link.trim());
    } else {
        digest.update(message);
    }
    digest.update(":");
    digest.update(locality.key);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_post(post: &WikPost, settings: &Settings, now: NaiveDateTime) -> Vec<UnifiedAlert> {
    if !post.categories.contains(&WIK_OPOLE_CATEGORY_ID) {
        return Vec::new();
    }

    let title = html_to_text(&post.title.rendered);
    let body = html_to_text(&post.content.rendered);
    let Some(kind) = NoticeKind::from_text(&title, &body) else {
        return Vec::new();
    };
    let full_message = [title.trim(), body.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let schedule = extract_schedule(&full_message);
    let publication_time =
        parse_publication_time(&post.date).or_else(|| parse_publication_time(&post.date_gmt));
    if is_stale(&schedule, publication_time, now) {
        return Vec::new();
    }

    let active_addresses: Vec<_> = settings
        .addresses
        .iter()
        .enumerate()
        .filter(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::WikOpole,
                    address,
                )
        })
        .collect();

    let mut alerts = Vec::new();
    for group in affected_localities(&full_message) {
        let address_index = active_addresses
            .iter()
            .find(|(_, address)| address_matches_group(address, &group))
            .map(|(index, _)| *index);
        let street_details = group.street_lines.join("; ");
        let location = if address_index.is_some() && !street_details.is_empty() {
            format!("Miejscowość: {}; {street_details}", group.locality.name)
        } else {
            format!("Miejscowość: {}", group.locality.name)
        };
        let message = if full_message.is_empty() {
            kind.label().to_string()
        } else {
            // Keep the complete title and body intact, including all locations,
            // dates, causes, and provider instructions.
            full_message.clone()
        };

        alerts.push(UnifiedAlert {
            source: AlertSource::WikOpole,
            startDate: schedule.start.clone(),
            endDate: schedule.end.clone(),
            message: Some(message),
            location: Some(location),
            address_index,
            is_local: Some(address_index.is_some()),
            hash: Some(stable_alert_hash(post, group.locality, &full_message)),
        });
    }
    alerts
}

async fn fetch_posts_page(client: &Client, page: usize) -> Result<(Vec<WikPost>, usize), String> {
    let response = client
        .get(format!("{WIK_OPOLE_POSTS_URL}&page={page}"))
        .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let total_pages = response
        .headers()
        .get("X-WP-TotalPages")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let body = response.text().await.map_err(|error| error.to_string())?;
    let posts = serde_json::from_str(&body)
        .map_err(|error| format!("WIK Opole response JSON error: {error}"))?;
    Ok((posts, total_pages))
}

async fn fetch_posts(client: &Client) -> Result<(Vec<WikPost>, Vec<String>), String> {
    let (mut posts, total_pages) =
        crate::utils::retry(|| async { fetch_posts_page(client, 1).await }, 3)
            .await
            .map_err(|error| error.to_string())?;
    let page_limit = total_pages.min(MAX_POST_PAGES);
    let mut warnings = Vec::new();

    if total_pages > MAX_POST_PAGES {
        warnings.push(format!(
            "WIK Opole has {total_pages} REST pages; capped this fetch at {MAX_POST_PAGES}"
        ));
    }

    for page in 2..=page_limit {
        let (mut page_posts, _) =
            crate::utils::retry(|| async { fetch_posts_page(client, page).await }, 3)
                .await
                .map_err(|error| format!("page {page}: {error}"))?;
        posts.append(&mut page_posts);
    }

    Ok((posts, warnings))
}

#[async_trait]
impl AlertProvider for WikOpoleProvider {
    fn id(&self) -> String {
        "wik_opole".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::WikOpole
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

        let (posts, errors) = match fetch_posts(client).await {
            Ok(result) => result,
            Err(error) => {
                let error = format!("WIK Opole fetch error: {error}");
                log::error!("{error}");
                return (Vec::new(), vec![error]);
            }
        };
        let now = Utc::now().with_timezone(&Warsaw).naive_local();
        let mut alerts = Vec::new();
        let mut seen_ids = HashSet::new();

        for post in posts {
            if post.id != 0 && !seen_ids.insert(post.id) {
                continue;
            }
            alerts.extend(parse_post(&post, settings, now));
        }

        if !errors.is_empty() {
            for error in &errors {
                log::warn!("{error}");
            }
        }
        (alerts, errors)
    }
}
