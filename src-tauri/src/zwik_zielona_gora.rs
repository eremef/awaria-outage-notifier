use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const ZWIK_ZIELONA_GORA_POSTS_URL: &str = "https://www.zwik.zgora.pl/wp-json/wp/v2/posts?categories=28&per_page=100&_fields=id,date,date_gmt,link,title,content,categories";

const ZWIK_ZIELONA_GORA_CATEGORY_ID: u64 = 28;
const MAX_POST_PAGES: usize = 2;
const UNKNOWN_END_RETENTION_HOURS: i64 = 72;

pub struct ZwikZielonaGoraProvider;

#[derive(Debug, Deserialize, Default, Clone)]
struct RenderedField {
    #[serde(default)]
    rendered: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct ZwikPost {
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
enum NoticeKind {
    Failure,
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
            "przerwa nie bedzie miala miejsca",
            "przerwa zostala odwolana",
            "przerwa zostaje odwolana",
            "awaria zostala usunieta",
            "awaria zostala zlikwidowana",
            "awaria usunieta",
            "awaria zlikwidowana",
            "dostawa wody zostala przywrocona",
            "woda zostala przywrocona",
        ]
        .iter()
        .any(|phrase| normalized.contains(phrase))
        {
            return None;
        }

        // The archive also includes road and sewer work. Category membership or
        // road-closure wording alone is not enough to create a water alert.
        let water_interruption = [
            "przerwa w dostawie wody",
            "przerwy w dostawie wody",
            "przerwa w dostarczaniu wody",
            "przerwy w dostarczaniu wody",
            "wylaczenie wody",
            "wylaczenia wody",
            "wylaczenie dostawy wody",
            "brak wody",
            "nie bedzie wody",
            "wody nie bedzie",
            "pozbawieni wody",
            "pozbawione wody",
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
            Some(Self::Failure)
        } else if ["planowan", "remont", "robot", "modernizac", "konserwac"]
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
            Self::Failure => "Awaria sieci wodociągowej",
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
        Regex::new(r"(?i)</(?:p|li|div|h[1-6]|tr|section|article|ul|ol|blockquote)>\s*").unwrap()
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

fn street_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?iu)\b(?:ul(?:ica|icy|ic)?|al(?:eja|ei)?|os(?:iedle|iedlu)?|pl(?:ac|acu)?|rondo|rynek|przy\s+gazowni)\b\.?\s+(?:\p{L}|\d)",
        )
        .unwrap()
    })
}

fn outage_event_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)(?:przerw(?:a|y)\s+w\s+dostawie\s+wody|przerw(?:a|y)\s+w\s+dostarczaniu\s+wody|brak\s+wody|awaria\s+(?:na\s+)?sieci\s+wodoci(?:a|ą)gowej|awaria\s+wodoci(?:a|ą)gu)")
            .unwrap()
    })
}

fn affected_street_section_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)(?:przy\s+ulicach|na\s+ulicach|obejmuje\s+ulic(?:e|ę|ach)|dotyczy\s+ulic(?:y|ach)?|przy\s+ul(?:ica|icy|ic)?\.?\s+)\s*:?\s*")
            .unwrap()
    })
}

fn where_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?iu)\bgdzie\s*\?\s*").unwrap())
}

fn section_terminator_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)^(?:jednocześnie|za\s+powstałe|za\s+utrudnienia|przewidywany|godziny\s+(?:wieczorne|popołudniowe|poranne|południowe)|beczk[aię]|uwaga|informujemy|po\s+ponownym|przepraszamy|⏰)")
            .unwrap()
    })
}

fn schedule_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?iu)\b(?:w\s+dni(?:u|ach)|dnia|od\s+dnia|w\s+terminie|w\s+okresie|w\s+godzinach|w\s+godz\.?|od\s+godz(?:ina|iny|in)?\.?|do\s+godz(?:ina|iny|in)?\.?|godz(?:ina|iny|in)?\.?|od\s+\d{1,2}[:.]\d{2})\b",
        )
        .unwrap()
    })
}

fn street_scope_sentence_end_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)\.\s+(?:przewidujemy|jednocześnie|za\s+powstałe|za\s+utrudnienia|informujemy|przepraszamy)\b")
            .unwrap()
    })
}

fn clean_street_scope(value: &str) -> String {
    let value = normalize_whitespace(value.trim());
    let cutoff = [
        street_scope_sentence_end_regex()
            .find(&value)
            .map(|marker| marker.start()),
        schedule_marker_regex()
            .find(&value)
            .filter(|marker| marker.start() > 0)
            .map(|marker| marker.start()),
    ]
    .into_iter()
    .flatten()
    .min();
    let value = cutoff.map(|position| &value[..position]).unwrap_or(&value);
    let value = value
        .trim()
        .trim_matches(|character: char| character.is_whitespace() || ",;:.–—-".contains(character));
    normalize_whitespace(value)
}

fn trim_bullet(value: &str) -> &str {
    value.trim_start_matches(|character: char| {
        character.is_whitespace() || "–—-•*➡→►".contains(character)
    })
}

fn looks_like_bare_street_scope(value: &str) -> bool {
    let normalized = normalize_polish(value);
    value.chars().any(char::is_numeric)
        || ["wszystkie numery", "wszystkie adresy", "cala ulica"]
            .iter()
            .any(|needle| normalized.contains(needle))
        || [" nr ", "numery:", "od numeru", "od nr", "od\u{a0}nr"]
            .iter()
            .any(|needle| normalized.contains(needle))
}

fn split_street_scopes_from_line(line: &str) -> Vec<String> {
    let line = clean_street_scope(trim_bullet(line));
    if line.is_empty() {
        return Vec::new();
    }

    let matches: Vec<_> = street_prefix_regex().find_iter(&line).collect();
    if matches.is_empty() {
        if !looks_like_bare_street_scope(&line) {
            return Vec::new();
        }
        let normalized = normalize_polish(&line);
        let name_is_prepositional = normalized.starts_with("przy ");
        return vec![if name_is_prepositional {
            line
        } else {
            format!("ul. {line}")
        }];
    }

    let mut scopes = Vec::new();
    if matches[0].start() > 0 {
        let bare_prefix = clean_street_scope(&line[..matches[0].start()]);
        if looks_like_bare_street_scope(&bare_prefix) {
            scopes.push(format!("ul. {bare_prefix}"));
        }
    }
    for (index, street_match) in matches.iter().enumerate() {
        let start = street_match.start();
        let end = matches
            .get(index + 1)
            .map(|next| next.start())
            .unwrap_or(line.len());
        let scope = clean_street_scope(&line[start..end]);
        if !scope.is_empty() {
            scopes.push(scope);
        }
    }
    scopes
}

fn section_lines(section: &str, stop_after_first_scope: bool) -> Vec<String> {
    let mut scopes = Vec::new();
    for line in section.lines() {
        let line = normalize_whitespace(line);
        let line = trim_bullet(&line).trim();
        if line.is_empty() {
            continue;
        }
        if section_terminator_regex().is_match(line)
            || (!scopes.is_empty() && normalize_polish(line).starts_with("przerwa w dostawie wody"))
        {
            break;
        }
        let mut line_scopes = split_street_scopes_from_line(line);
        if !line_scopes.is_empty() {
            scopes.append(&mut line_scopes);
            if stop_after_first_scope {
                break;
            }
        }
    }
    scopes
}

/// Finds the affected-street list after the water-outage wording. This avoids
/// treating the repair location in the introduction as an affected address.
fn affected_street_section(text: &str) -> Option<&str> {
    let events: Vec<_> = outage_event_regex().find_iter(text).collect();
    for event in events.into_iter().rev() {
        let search = &text[event.end()..];
        if let Some(marker) = affected_street_section_marker_regex().find(search) {
            return Some(&text[event.end() + marker.end()..]);
        }
    }
    None
}

/// Extracts one location scope per affected street. It supports the site's
/// prefixed inline addresses and its bare inflected bullet list entries.
/// House-number wording stays attached to the corresponding street.
fn extract_street_clauses(text: &str) -> Vec<String> {
    let scopes = if let Some(section) = affected_street_section(text) {
        section_lines(section, false)
    } else if let Some(marker) = where_marker_regex().find(text) {
        section_lines(&text[marker.end()..], true)
    } else if let Some(event) = outage_event_regex().find(text) {
        // Some notices put a single affected address directly after the outage
        // wording without an "obejmuje ulice" section or a "Gdzie?" line.
        section_lines(&text[event.end()..], true)
    } else {
        Vec::new()
    };

    let mut unique = Vec::new();
    for scope in scopes {
        if !unique
            .iter()
            .any(|existing: &String| normalize_polish(existing) == normalize_polish(&scope))
        {
            unique.push(scope);
        }
    }
    unique
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

fn yearless_numeric_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\b").unwrap())
}

fn relative_tomorrow_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?iu)\b(?:jutro|jutrzejsz\p{L}*)\b").unwrap())
}

fn explicit_night_crossing_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?iu)\bw\s+nocy\s+z\s+\d{1,2}/\d{1,2}\.\d{1,2}\b").unwrap())
}

fn numeric_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(\d{1,2})\s*(?:-|–|—|do)\s*(\d{1,2})[./](\d{1,2})[./](\d{4})(?:\s*r\.?)?\b")
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
        Regex::new(r"(?iu)(?:od\s*)?(?:godz(?:ina|iny|in)?\.?\s*)?(\d{1,2}[:.]\d{2})\s*(?:-|–|—|do)\s*(?:do\s*)?(?:godz(?:ina|iny|in)?\.?\s*)?(\d{1,2}[:.]\d{2})")
            .unwrap()
    })
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}[:.]\d{2})\b").unwrap())
}

#[derive(Clone, Copy)]
struct ParsedTime {
    time: NaiveTime,
    day_offset: i64,
}

fn explicit_start_time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?iu)\bod\s*(?:godz(?:ina|iny|in)?\.?\s*)?\d{1,2}[:.]\d{2}\b").unwrap()
    })
}

fn parse_numeric_date(day: &str, month: &str, year: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
}

fn date_mentions(text: &str, publication_time: Option<NaiveDateTime>) -> Vec<NaiveDate> {
    let mut mentions = Vec::<(usize, Vec<NaiveDate>)>::new();
    let mut range_spans = Vec::new();
    let mut explicit_date_spans = Vec::new();

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
        if last >= first {
            range_spans.push((whole.start(), whole.end()));
            mentions.push((whole.start(), vec![first, last]));
        }
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
        let (Some(first), Some(last)) = (
            parse_numeric_date(&first_day.to_string(), &captures[3], &captures[4]),
            parse_numeric_date(&last_day.to_string(), &captures[3], &captures[4]),
        ) else {
            continue;
        };
        if last >= first {
            range_spans.push((whole.start(), whole.end()));
            mentions.push((whole.start(), vec![first, last]));
        }
    }

    for regex in [numeric_date_regex(), written_date_regex()] {
        for captures in regex.captures_iter(text) {
            let Some(whole) = captures.get(0) else {
                continue;
            };
            if range_spans
                .iter()
                .any(|(start, end)| whole.start() >= *start && whole.end() <= *end)
            {
                continue;
            }
            let date = if std::ptr::eq(regex, numeric_date_regex()) {
                parse_numeric_date(&captures[1], &captures[2], &captures[3])
            } else {
                let (Ok(day), Ok(year)) = (captures[1].parse::<u32>(), captures[3].parse::<i32>())
                else {
                    continue;
                };
                month_number(&captures[2])
                    .and_then(|month| NaiveDate::from_ymd_opt(year, month, day))
            };
            if let Some(date) = date {
                explicit_date_spans.push((whole.start(), whole.end()));
                mentions.push((whole.start(), vec![date]));
            }
        }
    }

    if let Some(publication_time) = publication_time {
        for captures in yearless_numeric_date_regex().captures_iter(text) {
            let Some(whole) = captures.get(0) else {
                continue;
            };
            if explicit_date_spans
                .iter()
                .chain(range_spans.iter())
                .any(|(start, end)| whole.start() < *end && whole.end() > *start)
            {
                continue;
            }
            let (Ok(day), Ok(month)) = (captures[1].parse::<u32>(), captures[2].parse::<u32>())
            else {
                continue;
            };
            if let Some(mut date) = NaiveDate::from_ymd_opt(publication_time.year(), month, day) {
                if publication_time.date().signed_duration_since(date) > Duration::days(180) {
                    date = NaiveDate::from_ymd_opt(publication_time.year() + 1, month, day)
                        .unwrap_or(date);
                }
                mentions.push((whole.start(), vec![date]));
            }
        }

        if let Some(relative_day) = relative_tomorrow_regex().find(text) {
            mentions.push((
                relative_day.start(),
                vec![publication_time.date() + Duration::days(1)],
            ));
        }
    }

    mentions.sort_by_key(|(position, _)| *position);
    mentions.into_iter().flat_map(|(_, dates)| dates).collect()
}

fn parse_time(value: &str) -> Option<ParsedTime> {
    let normalized = value.replace('.', ":");
    if normalized == "24:00" {
        return Some(ParsedTime {
            time: NaiveTime::from_hms_opt(0, 0, 0)?,
            day_offset: 1,
        });
    }
    Some(ParsedTime {
        time: NaiveTime::parse_from_str(&normalized, "%H:%M").ok()?,
        day_offset: 0,
    })
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn end_of_day(date: NaiveDate) -> NaiveDateTime {
    date.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
}

fn extract_schedule(text: &str, publication_time: Option<NaiveDateTime>) -> ParsedSchedule {
    let mut dates = date_mentions(text, publication_time);
    dates.sort();
    dates.dedup();
    let (Some(first_date), Some(last_date)) = (dates.first().copied(), dates.last().copied())
    else {
        // Do not infer an outage date from the WordPress publication timestamp.
        return ParsedSchedule::default();
    };

    if dates.len() > 1 {
        // Multiple dates can describe independent daily windows. Keep only the
        // outer date bounds and preserve the detailed schedule in the message.
        return ParsedSchedule {
            start: Some(first_date.format("%Y-%m-%d").to_string()),
            end: Some(last_date.format("%Y-%m-%d").to_string()),
            expires_at: Some(end_of_day(last_date)),
        };
    }

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
    schedule_text = yearless_numeric_date_regex()
        .replace_all(&schedule_text, " ")
        .into_owned();

    let ranges: Vec<_> = time_range_regex()
        .captures_iter(&schedule_text)
        .filter_map(|captures| Some((parse_time(&captures[1])?, parse_time(&captures[2])?)))
        .collect();
    let times: Vec<_> = time_regex()
        .captures_iter(&schedule_text)
        .filter_map(|captures| parse_time(&captures[1]))
        .collect();

    if ranges.len() == 1 && times.len() == 2 {
        let (start_time, end_time) = ranges[0];
        let start = first_date.and_time(start_time.time) + Duration::days(start_time.day_offset);
        let mut end = first_date.and_time(end_time.time) + Duration::days(end_time.day_offset);
        if end <= start {
            end += Duration::days(1);
        }
        return ParsedSchedule {
            start: Some(format_datetime(start)),
            end: Some(format_datetime(end)),
            expires_at: Some(end),
        };
    }

    if ranges.len() > 1 || times.len() > 2 {
        return ParsedSchedule {
            start: Some(first_date.format("%Y-%m-%d").to_string()),
            end: Some(first_date.format("%Y-%m-%d").to_string()),
            expires_at: Some(end_of_day(first_date)),
        };
    }

    if times.len() == 1 {
        let time = times[0];
        let night_crossing_24 =
            time.day_offset == 1 && explicit_night_crossing_regex().is_match(text);
        let has_explicit_start = if time.day_offset == 1 {
            night_crossing_24
        } else {
            explicit_start_time_regex().is_match(&schedule_text)
        };
        if !has_explicit_start {
            return ParsedSchedule {
                start: Some(first_date.format("%Y-%m-%d").to_string()),
                end: None,
                expires_at: Some(end_of_day(first_date)),
            };
        }
        let day_offset = if night_crossing_24 {
            0
        } else {
            time.day_offset
        };
        let start = first_date.and_time(time.time) + Duration::days(day_offset);
        return ParsedSchedule {
            start: Some(format_datetime(start)),
            end: None,
            expires_at: Some(start + Duration::hours(24)),
        };
    }

    ParsedSchedule {
        start: Some(first_date.format("%Y-%m-%d").to_string()),
        end: None,
        expires_at: Some(end_of_day(first_date)),
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
    schedule.expires_at.is_none()
        && publication_time
            .is_some_and(|published| published + Duration::hours(UNKNOWN_END_RETENTION_HOURS) < now)
}

fn address_is_in_service_area(address: &AddressEntry) -> bool {
    crate::api_logic::is_address_applicable_for_provider(&AlertSource::ZwikZielonaGora, address)
}

fn street_matches_address(street_text: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let normalized_notice = normalize_polish(street_text);
    let notice_tokens: HashSet<_> = normalized_notice
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();

    candidates.into_iter().any(|candidate| {
        let candidate = crate::utils::strip_street_prefixes(candidate.trim());
        let tokens: Vec<_> = normalize_polish(candidate)
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
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                notice_tokens
                    .iter()
                    .any(|notice_token| notice_token.starts_with(token.as_str()))
            })
    })
}

fn address_matches_scope(address: &AddressEntry, street_scope: Option<&str>) -> bool {
    if !address_is_in_service_area(address) {
        return false;
    }
    match street_scope {
        Some(scope) => street_matches_address(scope, address),
        None => {
            address.street_name.trim().is_empty()
                && address.street_name_1.trim().is_empty()
                && address
                    .street_name_2
                    .as_deref()
                    .is_none_or(|street| street.trim().is_empty())
        }
    }
}

/// Checks a street-scoped location emitted by this provider against an address.
/// The service-area guard is also enforced centrally in `api_logic.rs`.
#[cfg(test)]
pub(crate) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    if !address_is_in_service_area(address) {
        return false;
    }
    let Some((_, street_text)) = location.split_once(';') else {
        return address_matches_scope(address, None);
    };
    street_matches_address(street_text.trim(), address)
}

fn stable_alert_hash(post: &ZwikPost, scope: &str) -> String {
    let mut digest = Sha256::new();
    digest.update("zwik_zielona_gora:");
    if post.id != 0 {
        digest.update(post.id.to_string());
    } else if !post.link.trim().is_empty() {
        digest.update(post.link.trim());
    } else {
        digest.update(post.title.rendered.trim());
        digest.update(post.content.rendered.trim());
    }
    digest.update(":");
    digest.update(normalize_polish(scope));
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_post(post: &ZwikPost, settings: &Settings, now: NaiveDateTime) -> Vec<UnifiedAlert> {
    if !post.categories.contains(&ZWIK_ZIELONA_GORA_CATEGORY_ID) {
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
    let publication_time =
        parse_publication_time(&post.date).or_else(|| parse_publication_time(&post.date_gmt));
    let schedule = extract_schedule(&full_message, publication_time);
    if is_stale(&schedule, publication_time, now) {
        return Vec::new();
    }

    let mut street_scopes = extract_street_clauses(&full_message);
    if street_scopes.is_empty() {
        street_scopes.push(String::new());
    }

    let active_addresses: Vec<_> = settings
        .addresses
        .iter()
        .enumerate()
        .filter(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::ZwikZielonaGora,
                    address,
                )
        })
        .collect();

    street_scopes
        .into_iter()
        .map(|street_scope| {
            let scope = (!street_scope.trim().is_empty()).then_some(street_scope.as_str());
            let address_index = active_addresses
                .iter()
                .find(|(_, address)| address_matches_scope(address, scope))
                .map(|(index, _)| *index);
            let location = if let Some(street) = scope {
                format!("Zielona Góra; {street}")
            } else {
                "Zielona Góra".to_string()
            };
            UnifiedAlert {
                source: AlertSource::ZwikZielonaGora,
                startDate: schedule.start.clone(),
                endDate: schedule.end.clone(),
                message: Some(if full_message.is_empty() {
                    kind.label().to_string()
                } else {
                    // Retain the complete upstream notice on every street-scoped
                    // alert, including all other streets, dates and causes.
                    full_message.clone()
                }),
                location: Some(location),
                address_index,
                is_local: Some(address_index.is_some()),
                hash: Some(stable_alert_hash(post, scope.unwrap_or("all streets"))),
            }
        })
        .collect()
}

fn parse_posts(posts: Vec<ZwikPost>, settings: &Settings, now: NaiveDateTime) -> Vec<UnifiedAlert> {
    let mut seen_ids = HashSet::new();
    posts
        .iter()
        .filter(|post| post.id == 0 || seen_ids.insert(post.id))
        .flat_map(|post| parse_post(post, settings, now))
        .collect()
}

async fn fetch_posts_page(client: &Client, page: usize) -> Result<(Vec<ZwikPost>, usize), String> {
    let response = client
        .get(format!("{ZWIK_ZIELONA_GORA_POSTS_URL}&page={page}"))
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
        .map_err(|error| format!("ZWiK Zielona Góra response JSON error: {error}"))?;
    Ok((posts, total_pages))
}

async fn fetch_posts(client: &Client) -> Result<(Vec<ZwikPost>, Vec<String>), String> {
    let (mut posts, total_pages) =
        crate::utils::retry(|| async { fetch_posts_page(client, 1).await }, 3)
            .await
            .map_err(|error| error.to_string())?;
    let page_limit = total_pages.min(MAX_POST_PAGES);
    let mut warnings = Vec::new();

    if total_pages > MAX_POST_PAGES {
        warnings.push(format!(
            "ZWiK Zielona Góra has {total_pages} REST pages; capped this fetch at {MAX_POST_PAGES}"
        ));
    }

    for page in 2..=page_limit {
        let (mut page_posts, _) =
            crate::utils::retry(|| async { fetch_posts_page(client, page).await }, 3)
                .await
                .map_err(|error| format!("page {page}: {error}"))?;
        if page_posts.is_empty() {
            break;
        }
        posts.append(&mut page_posts);
    }

    Ok((posts, warnings))
}

#[async_trait]
impl AlertProvider for ZwikZielonaGoraProvider {
    fn id(&self) -> String {
        "zwik_zielona_gora".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::ZwikZielonaGora
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

        let (posts, warnings) = match fetch_posts(client).await {
            Ok(result) => result,
            Err(error) => {
                let error = format!("ZWiK Zielona Góra fetch error: {error}");
                log::error!("{error}");
                return (Vec::new(), vec![error]);
            }
        };
        let now = Utc::now().with_timezone(&Warsaw).naive_local();
        for warning in &warnings {
            log::warn!("{warning}");
        }
        (parse_posts(posts, settings, now), warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: &str = "2026-10-22T12:00:00";

    fn now() -> NaiveDateTime {
        NaiveDateTime::parse_from_str(NOW, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    fn post(id: u64, published: &str, title: &str, content: &str) -> ZwikPost {
        ZwikPost {
            id,
            date: published.to_string(),
            date_gmt: String::new(),
            link: format!("https://www.zwik.zgora.pl/?p={id}"),
            title: RenderedField {
                rendered: title.to_string(),
            },
            content: RenderedField {
                rendered: content.to_string(),
            },
            categories: vec![ZWIK_ZIELONA_GORA_CATEGORY_ID],
        }
    }

    fn service_address(street: &str, number: &str) -> AddressEntry {
        AddressEntry {
            city_name: "Zielona Góra".to_string(),
            voivodeship: "lubuskie".to_string(),
            street_name: street.to_string(),
            house_no: number.to_string(),
            city_id: Some(917313),
            is_active: true,
            ..AddressEntry::default()
        }
    }

    #[test]
    fn planned_notice_parses_outage_window_and_scopes_street() {
        let notice = post(
            31320,
            "2026-10-22T11:02:36",
            "Prace remontowe na sieci wodociągowej przy ul. Truskawkowej w obrębie skrzyżowania z ul. Jodłową w Zielonej Górze w dniu 23.10.2026 r.",
            r#"<p>„Zielonogórskie Wodociągi i Kanalizacja” Spółka z o. o. informują, że w związku z koniecznością wykonania pilnych prac remontowych na sieci wodociągowej przy ul. Truskawkowej w obrębie skrzyżowania z ul. Jodłową w Zielonej Górze, w dniu 23.10.2026 r. od godziny 08:00<sup> </sup>do godziny 13:00,<sup> </sup>nastąpi przerwa w dostawie wody przy ulicach:<br />
&#8211; Poznańskiej nr 1;<br />
&#8211; Truskawkowej numery: 1A, 1B, 2A;<br />
&#8211; Jodłowej &#8211; wszystkie numery;<br />
&#8211; Jastrzębiej &#8211; wszystkie numery;<br />
&#8211; Drzymały nr 4 i numery nieparzyste od 5 do 11, 11A.</p>
<p>Jednocześnie informujemy, że po ponownym włączeniu może nastąpić chwilowe pogorszenie jakości wody, spowodowane płukaniem sieci po przeprowadzonych pracach.</p>
<p>Za powstałe utrudnienia przepraszamy!</p>"#,
        );
        let address = service_address("Poznańska", "1");
        assert!(address_is_in_service_area(&address));
        assert!(location_matches_address(
            "Zielona Góra; ul. Poznańskiej nr 1",
            &address
        ));
        let settings = Settings {
            addresses: vec![address],
            ..Settings::default()
        };

        let alerts = parse_post(&notice, &settings, now());
        assert_eq!(alerts.len(), 5);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-23T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-23T13:00:00"));
        assert_eq!(
            alerts[0].location.as_deref(),
            Some("Zielona Góra; ul. Poznańskiej nr 1")
        );
        assert_eq!(alerts[0].address_index, Some(0));
        assert_eq!(
            alerts[1].location.as_deref(),
            Some("Zielona Góra; ul. Truskawkowej numery: 1A, 1B, 2A")
        );
        assert_eq!(
            alerts[2].location.as_deref(),
            Some("Zielona Góra; ul. Jodłowej – wszystkie numery")
        );
        assert_eq!(
            alerts[3].location.as_deref(),
            Some("Zielona Góra; ul. Jastrzębiej – wszystkie numery")
        );
        assert_eq!(
            alerts[4].location.as_deref(),
            Some("Zielona Góra; ul. Drzymały nr 4 i numery nieparzyste od 5 do 11, 11A")
        );
        assert!(alerts.iter().all(|alert| {
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("pogorszenie jakości wody")
        }));
        assert!(!alerts.iter().any(|alert| {
            alert
                .location
                .as_deref()
                .unwrap()
                .contains("Truskawkowej w obrębie skrzyżowania")
        }));
    }

    #[test]
    fn multi_street_notice_keeps_each_range_and_suffix_with_its_street() {
        let text = "Prace przy ul. Robotniczej 1; nastąpi przerwa w dostawie wody przy ulicach:\n– ul. Chmielna 1–23, 23a;\n– ul. Zacisze 76–76a-b;\n– cała ul. Zielona";
        let clauses = extract_street_clauses(text);
        assert_eq!(clauses.len(), 3);
        assert!(clauses[0].contains("1–23, 23a"));
        assert!(clauses[1].contains("76–76a-b"));
        assert!(clauses[2].contains("cała ul. Zielona") || clauses[2].contains("ul. Zielona"));

        let notice = post(
            31321,
            "2026-10-22T09:00:00",
            "Przerwy w dostawie wody",
            "Prace przy ul. Robotniczej 1. W dniu 23.10.2026 w godz. 08:00–13:00 nastąpi przerwa w dostawie wody przy ulicach: ul. Chmielna 1–23, 23a oraz ul. Zacisze 76–76a-b; cała ul. Zielona.",
        );
        let alerts = parse_post(&notice, &Settings::default(), now());
        assert_eq!(alerts.len(), 3);
        assert!(
            alerts[0]
                .location
                .as_deref()
                .unwrap()
                .contains("Chmielna 1–23, 23a")
        );
        assert!(
            alerts[1]
                .location
                .as_deref()
                .unwrap()
                .contains("Zacisze 76–76a-b")
        );
        assert!(alerts[2].location.as_deref().unwrap().contains("Zielona"));
        assert!(
            alerts
                .iter()
                .all(|alert| alert.message.as_deref().unwrap().contains("ul. Chmielna"))
        );
    }

    #[test]
    fn failure_with_vague_repair_time_has_no_invented_schedule() {
        let notice = post(
            31322,
            "2026-10-22T10:00:00",
            "Awaria sieci wodociągowej",
            "Wystąpiła awaria wodociągu. Prace naprawcze trwają, przewidywany czas usunięcia awarii: godziny popołudniowe. ul. Chmielna 1–5.",
        );
        let alerts = parse_post(&notice, &Settings::default(), now());
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate, None);
        assert_eq!(alerts[0].endDate, None);
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("godziny popołudniowe")
        );
    }

    #[test]
    fn live_failure_notice_resolves_yearless_night_boundary_without_guessing_end() {
        let notice = post(
            31252,
            "2026-10-02T20:56:44",
            "Awaria sieci wodociągowej Ul. Cyryla i Metodego 3",
            r#"<p>Awaria sieci wodociągowej 🚱❗</p>
<p>🏠 Gdzie? Ul. Cyryla i Metodego 3</p>
<p>➡ Przerwa w dostawie wody obejmuje ulice:</p>
<p>ul. Cyryla i Metodego 1, 3 i 7.</p>
<p>⏰ Przewidywany czas usunięcia awarii to godziny południowe 03.10.</p>
<p>Sieć wodociągowa zostanie zamknięta w nocy z 02/03.10 o godz. 24:00. Awaria będzie usuwana w dniu jutrzejszym (03.10) od godzin porannych. Beczki z wodą zostaną postawione między budynkami 1-3-7.</p>
<p>🙏 Za powstałe utrudnienia przepraszamy ❗</p>"#,
        );
        let fixture_now =
            NaiveDateTime::parse_from_str("2026-10-02T21:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();

        let alerts = parse_post(&notice, &Settings::default(), fixture_now);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-03T00:00:00"));
        assert_eq!(alerts[0].endDate, None);
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("godziny południowe 03.10")
        );
    }

    #[test]
    fn inline_live_address_list_keeps_each_street_number_scope_separate() {
        let notice = post(
            31250,
            "2026-10-02T09:13:30",
            "Awaria na przyłączu wodociągowym ul. Świętojańska w Zielonej Górze",
            r#"<p>„Zielonogórskie Wodociągi i Kanalizacja” Sp. z o. o. informują, że w związku z wystąpieniem awarii na przyłączu wodociągowym przy ul. Świętojańskiej nastąpi przerwa w dostawie wody przy ulicach:  Świętojańskiej od numeru 1 do 11, Al. Wojska Polskiego 9, Plac Powstańców Wielkopolskich 4, Przy Gazowni od numeru 29 do 41. Przewidujemy, że przerwa w dostawie wody potrwa do godziny 11:00.</p>
<p>Za powstałe utrudnienia przepraszamy.</p>"#,
        );
        let fixture_now =
            NaiveDateTime::parse_from_str("2026-10-02T09:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();

        let alerts = parse_post(&notice, &Settings::default(), fixture_now);
        assert_eq!(alerts.len(), 4);
        assert_eq!(
            alerts[0].location.as_deref(),
            Some("Zielona Góra; ul. Świętojańskiej od numeru 1 do 11")
        );
        assert_eq!(
            alerts[1].location.as_deref(),
            Some("Zielona Góra; Al. Wojska Polskiego 9")
        );
        assert_eq!(
            alerts[2].location.as_deref(),
            Some("Zielona Góra; Plac Powstańców Wielkopolskich 4")
        );
        assert_eq!(
            alerts[3].location.as_deref(),
            Some("Zielona Góra; Przy Gazowni od numeru 29 do 41")
        );
        assert!(!alerts[2].location.as_deref().unwrap().contains("29"));
        assert!(
            !alerts[3]
                .location
                .as_deref()
                .unwrap()
                .contains("Powstańców Wielkopolskich 4")
        );
    }

    #[test]
    fn yearless_january_date_rolls_forward_near_new_year() {
        let publication_time =
            NaiveDateTime::parse_from_str("2026-12-31T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let schedule =
            extract_schedule("Przerwa w dostawie wody dnia 01.01", Some(publication_time));

        assert_eq!(schedule.start.as_deref(), Some("2027-01-01"));
    }

    #[test]
    fn roadwork_without_water_interruption_is_excluded() {
        let notice = post(
            31323,
            "2026-10-22T10:00:00",
            "Utrudnienia w ruchu na ul. Zacisze",
            "W związku z pracami drogowymi wystąpią utrudnienia w ruchu. Nie będzie przerwy w dostawie wody.",
        );
        assert!(parse_post(&notice, &Settings::default(), now()).is_empty());
    }

    #[test]
    fn cancelled_or_restored_notices_are_ignored() {
        let cancelled = post(
            31329,
            "2026-10-22T10:00:00",
            "Prace remontowe na sieci wodociągowej",
            "Planowana przerwa w dostawie wody została odwołana. Nie będzie przerwy w dostawie wody.",
        );
        let restored = post(
            31330,
            "2026-10-22T10:00:00",
            "Awaria sieci wodociągowej",
            "Awaria została usunięta, a dostawa wody została przywrócona.",
        );

        assert!(parse_post(&cancelled, &Settings::default(), now()).is_empty());
        assert!(parse_post(&restored, &Settings::default(), now()).is_empty());
    }

    #[test]
    fn empty_feed_and_duplicate_posts_are_handled() {
        let settings = Settings::default();
        let empty: Vec<ZwikPost> = serde_json::from_str("[]").unwrap();
        assert!(parse_posts(empty, &settings, now()).is_empty());

        let notice = post(
            31324,
            "2026-10-22T09:00:00",
            "Przerwa w dostawie wody",
            "W dniu 23.10.2026 nastąpi przerwa w dostawie wody przy ul. Chmielnej 1–3.",
        );
        let alerts = parse_posts(vec![notice.clone(), notice], &settings, now());
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn outage_date_comes_from_notice_not_wordpress_publication_date() {
        let notice = post(
            31325,
            "2026-10-22T09:00:00",
            "Przerwa w dostawie wody 23.10.2026",
            "W dniu 23.10.2026 w godz. 08:00–13:00 nastąpi przerwa w dostawie wody.",
        );
        let alerts = parse_post(&notice, &Settings::default(), now());
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-23T08:00:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-23T13:00:00"));
    }

    #[test]
    fn publication_timestamp_only_controls_freshness_when_schedule_is_absent() {
        let recent = post(
            31326,
            "2026-10-21T20:00:00",
            "Awaria sieci wodociągowej",
            "Trwają prace naprawcze przy ul. Zacisze.",
        );
        let stale = post(
            31327,
            "2026-10-18T10:00:00",
            "Awaria sieci wodociągowej",
            "Trwają prace naprawcze przy ul. Zacisze.",
        );
        assert_eq!(parse_post(&recent, &Settings::default(), now()).len(), 1);
        assert!(parse_post(&stale, &Settings::default(), now()).is_empty());

        // An older publication date does not expire an explicitly future outage.
        let planned = post(
            31328,
            "2026-10-18T10:00:00",
            "Przerwa w dostawie wody",
            "W dniu 23.10.2026 w godz. 08:00–13:00 nastąpi przerwa w dostawie wody.",
        );
        assert_eq!(parse_post(&planned, &Settings::default(), now()).len(), 1);
    }

    #[test]
    fn address_matching_is_street_and_area_scoped_without_house_number_filtering() {
        let zacisze = service_address("Zacisze", "80");
        let outside = AddressEntry {
            city_name: "Gorzów Wielkopolski".to_string(),
            voivodeship: "lubuskie".to_string(),
            street_name: "Chmielna".to_string(),
            city_id: Some(999999),
            ..AddressEntry::default()
        };
        assert!(location_matches_address(
            "Zielona Góra; ul. Zacisze 76–76a-b",
            &zacisze
        ));
        assert!(!location_matches_address(
            "Zielona Góra; ul. Chmielna 1–23",
            &zacisze
        ));
        assert!(!location_matches_address(
            "Zielona Góra; ul. Chmielna 1–23",
            &outside
        ));
    }
}
