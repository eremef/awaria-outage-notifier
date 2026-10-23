use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use std::sync::OnceLock;

pub const PEC_GLIWICE_FEED_URL: &str = "https://pec.gliwice.pl/category/przerwy-w-dostawie/feed/";
const GLIWICE_SIMC: u64 = 940_000;

pub struct PecGliwiceProvider;

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
    link: String,
    #[serde(default)]
    guid: String,
    #[serde(rename = "pubDate", default)]
    pub_date: String,
    #[serde(default)]
    description: String,
    // quick-xml's serde deserializer maps prefixed XML element names to their
    // local name (`content:encoded` -> `encoded`).
    #[serde(rename = "encoded", default)]
    content: String,
}

#[derive(Clone, Copy, Debug)]
struct ParsedClock {
    value: NaiveTime,
    next_day: bool,
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(\d{1,2})[:.](\d{2})\b").unwrap())
}

fn block_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)</?(?:p|div|li|tr|h[1-6])\b[^>]*>|<br\s*/?>").unwrap())
}

fn street_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b(?:ul\.?|ulica|ulicy|al\.?|aleja)\s+").unwrap())
}

fn house_spec_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?P<start>\d+)(?P<start_suffix>[a-z]?)\s*(?:-\s*(?:(?P<end>\d+)(?P<end_suffix>[a-z]?)|(?P<end_suffix_only>[a-z]))(?:\s*-\s*(?P<tail_suffix>[a-z]))?)?",
        )
        .unwrap()
    })
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

fn html_text_once(html: &str) -> String {
    let with_breaks = block_tag_regex().replace_all(html, "\n");
    let document = Html::parse_fragment(&with_breaks);
    document.root_element().text().collect::<Vec<_>>().join("")
}

fn html_to_lines(html: &str) -> Vec<String> {
    let first_pass = html_text_once(html);
    // Some WordPress feeds put HTML-escaped markup inside content:encoded.
    let decoded = if first_pass.contains("&lt;")
        || first_pass.contains("<p")
        || first_pass.contains("<div")
        || first_pass.contains("<br")
    {
        html_text_once(&first_pass)
    } else {
        first_pass
    };

    decoded
        .lines()
        .map(|line| {
            line.replace('\u{a0}', " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|line| !line.is_empty())
        .collect()
}

fn parse_date(captures: &regex::Captures<'_>) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn parse_clock(hour: &str, minute: &str) -> Option<ParsedClock> {
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(ParsedClock {
            value: NaiveTime::from_hms_opt(0, 0, 0)?,
            next_day: true,
        });
    }
    Some(ParsedClock {
        value: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn format_date_time(date: NaiveDate, clock: Option<ParsedClock>) -> String {
    let date = if clock.is_some_and(|value| value.next_day) {
        date + Duration::days(1)
    } else {
        date
    };
    match clock {
        Some(clock) => format!(
            "{}T{}",
            date.format("%Y-%m-%d"),
            clock.value.format("%H:%M:%S")
        ),
        None => date.format("%Y-%m-%d").to_string(),
    }
}

/// Reads only dates and times written in the notice. RSS publication metadata is deliberately
/// excluded, and date-only values stay date-only instead of acquiring invented clock times.
fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let mut dates = Vec::new();
    for captures in numeric_date_regex().captures_iter(text) {
        if let Some(date) = parse_date(&captures)
            && !dates.contains(&date)
        {
            dates.push(date);
        }
    }
    let Some(start_date) = dates.first().copied() else {
        return (None, None);
    };

    // Strip dates before finding clock values so a date such as 26.10.2026 cannot be
    // mistaken for a clock time. Polish notice times are local civil time in Europe/Warsaw.
    let without_dates = numeric_date_regex().replace_all(text, " ");
    let clocks = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|captures| parse_clock(&captures[1], &captures[2]))
        .collect::<Vec<_>>();
    let start = format_date_time(start_date, clocks.first().copied());

    let end = if let Some(end_date) = dates.get(1).copied() {
        Some(format_date_time(end_date, clocks.get(1).copied()))
    } else if let Some(end_clock) = clocks.get(1).copied() {
        let start_clock = clocks.first().copied();
        let mut end_date = start_date;
        if end_clock.next_day
            || (!end_clock.next_day
                && start_clock.is_some_and(|start_clock| end_clock.value <= start_clock.value))
        {
            end_date += Duration::days(1);
        }
        Some(format_date_time(
            end_date,
            Some(ParsedClock {
                next_day: false,
                ..end_clock
            }),
        ))
    } else {
        None
    };

    (Some(start), end)
}

fn is_restoration_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("przywrocono dostawe")
        || normalized.contains("przywrocono dostawy")
        || normalized.contains("awaria zostala usunieta")
        || normalized.contains("awarie zostaly usuniete")
}

fn outage_kind(text: &str) -> Option<&'static str> {
    let normalized = normalize_polish(text);
    if is_restoration_notice(&normalized) {
        return None;
    }
    let has_outage_signal = normalized.contains("przer")
        || normalized.contains("awari")
        || normalized.contains("zaklocen")
        || normalized.contains("brak dostaw")
        || normalized.contains("wstrzyman");
    if !has_outage_signal {
        return None;
    }

    if normalized.contains("zaklocen") || normalized.contains("moga wystapic utrudnienia") {
        Some("Możliwe zakłócenia w dostawie ciepła")
    } else if normalized.contains("awaryjn")
        || (normalized.contains("awari") && !normalized.contains("planowan"))
    {
        Some("Awaria ciepła")
    } else if normalized.contains("planowan") {
        Some("Planowana przerwa w dostawie ciepła")
    } else {
        Some("Przerwa w dostawie ciepła")
    }
}

fn extract_affected_streets(body: &str) -> Vec<String> {
    let mut result = Vec::new();
    for line in html_to_lines(body) {
        let markers = street_marker_regex().find_iter(&line).collect::<Vec<_>>();
        if markers.is_empty() {
            // PEC often supplies street/address items as bare list entries (for example
            // `Andersena 2-48`) instead of repeating `ul.` on every line.
            if !numeric_date_regex().is_match(&line)
                && !time_regex().is_match(&line)
                && let Some(number) = house_spec_regex().find(&line)
                && line[..number.start()]
                    .chars()
                    .any(|character| character.is_alphabetic())
                && !result
                    .iter()
                    .any(|existing: &String| normalize_polish(existing) == normalize_polish(&line))
            {
                result.push(line);
            }
            continue;
        }
        for (index, marker) in markers.iter().enumerate() {
            let end = markers
                .get(index + 1)
                .map(|next| next.start())
                .unwrap_or(line.len());
            let segment = line[marker.start()..end]
                .trim_matches(|character: char| {
                    character.is_whitespace() || ",;:.-".contains(character)
                })
                .to_string();
            if !segment.is_empty()
                && !result.iter().any(|existing: &String| {
                    normalize_polish(existing) == normalize_polish(&segment)
                })
            {
                result.push(segment);
            }
        }
    }
    result
}

fn location_text(body: &str) -> String {
    let streets = extract_affected_streets(body);
    if streets.is_empty() {
        "Gliwice".to_string()
    } else {
        format!("Gliwice; {}", streets.join("; "))
    }
}

fn is_gliwice_address(address: &AddressEntry) -> bool {
    if let Some(city_id) = address.city_id {
        return city_id == GLIWICE_SIMC;
    }
    normalize_polish(address.city_name.trim()) == "gliwice"
}

/// Matches only addresses in Gliwice whose configured street is named in the notice location.
pub fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    if !is_gliwice_address(address) {
        return false;
    }

    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .into_iter()
    .filter(|street| !street.trim().is_empty())
    .any(|street| street_matches(location, &street))
}

fn street_matches(location: &str, street: &str) -> bool {
    let street = crate::utils::strip_street_prefixes(street.trim());
    let expected = normalize_polish(street)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() > 2)
        .map(|token| {
            let length = token.chars().count().saturating_sub(2).max(3);
            token.chars().take(length).collect::<String>()
        })
        .collect::<Vec<_>>();
    let normalized_location = normalize_polish(location);
    let actual = normalized_location
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    !expected.is_empty()
        && expected.iter().all(|expected_token| {
            actual
                .iter()
                .any(|actual_token| actual_token.starts_with(expected_token))
        })
}

fn house_point_matches_range(
    house_number: u32,
    house_suffix: &str,
    start_number: u32,
    start_suffix: &str,
    end_number: Option<u32>,
    end_suffix: &str,
) -> bool {
    let Some(end_number) = end_number else {
        if house_number != start_number {
            return false;
        }
        // A bare configured number also covers lettered buildings at the same number.
        return house_suffix.is_empty() || start_suffix.is_empty() || house_suffix == start_suffix;
    };

    if start_number != end_number {
        return house_number >= start_number.min(end_number)
            && house_number <= start_number.max(end_number);
    }

    let house = (house_number, house_suffix.to_ascii_lowercase());
    let start = (start_number, start_suffix.to_ascii_lowercase());
    let end = (end_number, end_suffix.to_ascii_lowercase());
    let (lower, upper) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    lower <= house && house <= upper
}

/// Applies the optional house-number check to parsed street locations. If the source has no
/// recognizable house number, keep the notice eligible as requested by the shared setting's
/// semantics; this helper is called by that shared path only when filtering is enabled.
pub fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }
    let street_candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let matching_segments = extract_affected_streets(location)
        .into_iter()
        .filter(|segment| {
            street_candidates
                .iter()
                .any(|street| !street.trim().is_empty() && street_matches(segment, street))
        })
        .collect::<Vec<_>>();
    let mut had_recognized_spec = false;

    for segment in matching_segments {
        for captures in house_spec_regex().captures_iter(&segment) {
            let Some(start_number) = captures
                .name("start")
                .and_then(|value| value.as_str().parse::<u32>().ok())
            else {
                continue;
            };
            had_recognized_spec = true;
            let start_suffix = captures
                .name("start_suffix")
                .map_or("", |value| value.as_str());
            let end_number = captures
                .name("end")
                .and_then(|value| value.as_str().parse::<u32>().ok())
                .or_else(|| captures.name("end_suffix_only").map(|_| start_number));
            let end_suffix = captures
                .name("tail_suffix")
                .or_else(|| captures.name("end_suffix"))
                .or_else(|| captures.name("end_suffix_only"))
                .map_or("", |value| value.as_str());

            for requested in house_spec_regex().captures_iter(&address.house_no) {
                let Some(house_number) = requested
                    .name("start")
                    .and_then(|value| value.as_str().parse::<u32>().ok())
                else {
                    continue;
                };
                let house_suffix = requested
                    .name("start_suffix")
                    .map_or("", |value| value.as_str());
                if house_point_matches_range(
                    house_number,
                    house_suffix,
                    start_number,
                    start_suffix,
                    end_number,
                    end_suffix,
                ) {
                    return true;
                }
            }
        }
    }

    // Unknown source formats are permissive; known number lists/ranges still enforce a match.
    !had_recognized_spec
}

fn parse_item(item: RssItem, settings: &Settings) -> Option<UnifiedAlert> {
    let title = item.title.trim();
    let content_html = if item.content.trim().is_empty() {
        item.description.as_str()
    } else {
        item.content.as_str()
    };
    let body = html_to_lines(content_html).join("\n");
    let combined = format!("{title}\n{body}");
    let kind = outage_kind(&combined)?;
    let (start_date, end_date) = parse_schedule(&combined);
    let location = location_text(content_html);
    let matched_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| address.is_active && location_matches_address(&location, address))
        .map(|(index, _)| index);
    let has_gliwice_address = settings
        .addresses
        .iter()
        .any(|address| address.is_active && is_gliwice_address(address));

    let mut message = format!("{kind}\n{title}");
    if !body.trim().is_empty() {
        message.push('\n');
        message.push_str(&body);
    }
    if !item.pub_date.trim().is_empty() {
        message.push_str("\nData publikacji RSS: ");
        message.push_str(item.pub_date.trim());
    }
    let source_link = if !item.link.trim().is_empty() {
        item.link.trim()
    } else {
        item.guid.trim()
    };
    if !source_link.is_empty() {
        message.push_str("\nŹródło: ");
        message.push_str(source_link);
    }

    Some(UnifiedAlert {
        source: AlertSource::PecGliwice,
        startDate: start_date,
        endDate: end_date,
        message: Some(message),
        location: Some(location),
        address_index: matched_address,
        // Keep Gliwice feed items as candidates so the central source matcher can re-evaluate
        // multi-street addresses and house-number preferences.
        is_local: Some(has_gliwice_address),
        hash: None,
    })
}

fn parse_rss(xml: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let feed: RssFeed = quick_xml::de::from_str(xml).map_err(|error| error.to_string())?;
    let alerts = feed
        .channel
        .items
        .into_iter()
        .filter_map(|item| parse_item(item, settings))
        .collect();
    Ok(crate::api_logic::deduplicate_alerts(alerts))
}

#[async_trait]
impl AlertProvider for PecGliwiceProvider {
    fn id(&self) -> String {
        "pec_gliwice".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PecGliwice
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
            .get(PEC_GLIWICE_FEED_URL)
            .header(
                "User-Agent",
                "Awaria/1.0 (+https://github.com/eremef/awaria)",
            )
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("PEC Gliwice request error: {error}")],
                );
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("PEC Gliwice HTTP error: {}", response.status())],
            );
        }

        match response.text().await {
            Ok(xml) => match parse_rss(&xml, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (
                    Vec::new(),
                    vec![format!("PEC Gliwice RSS parse error: {error}")],
                ),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("PEC Gliwice response read error: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/"><channel>
      <item>
        <title>Awaryjna przerwa w dostawie ciepła systemowego w dniu 26.10.2026 r.</title>
        <link>https://pec.gliwice.pl/przerwy-w-dostawie/old-slug-2024/</link>
        <guid>https://pec.gliwice.pl/?p=501</guid>
        <pubDate>Tue, 20 Oct 2026 06:00:00 +0000</pubDate>
        <description><![CDATA[Awaria sieci ciepłowniczej.]]></description>
        <content:encoded><![CDATA[<p>PEC Gliwice informuje, że w dniu <strong>26.10.2026 r.</strong> od godz. 07:00 ÷ 22:00 wystąpi awaryjna przerwa w dostawie ciepła.</p><p>Dotyczy budynków przy ul. Kozielskiej 24a-b oraz ul. Sowińskiego 34-34a-c.</p>]]></content:encoded>
      </item>
      <item>
        <title>Planowana przerwa w dostawie ciepła</title>
        <link>https://pec.gliwice.pl/przerwy-w-dostawie/planned/</link>
        <pubDate>Wed, 14 Oct 2026 10:15:00 +0000</pubDate>
        <content:encoded><![CDATA[<p>W dniu 05.11.2026 r. w godzinach 08:00-20:00 planowana przerwa w dostawie ciepła przy ul. Sowińskiego 10.</p>]]></content:encoded>
      </item>
    </channel></rss>"#;

    fn settings(street: &str, house_no: &str, city_id: u64) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".into(),
                city_name: "Gliwice".into(),
                voivodeship: "Śląskie".into(),
                district: String::new(),
                commune: String::new(),
                street_name: street.into(),
                street_name_1: street.into(),
                street_name_2: None,
                house_no: house_no.into(),
                city_id: Some(city_id),
                street_id: None,
                is_active: true,
            }],
            primary_address_index: Some(0),
            theme: None,
            language: Some("pl".into()),
            font_size: None,
            enabled_sources: None,
            notification_preferences: HashMap::new(),
            upcoming_notification_enabled: false,
            upcoming_notification_hours: 24,
            show_other_outages: true,
            filter_by_house_no: false,
        }
    }

    #[test]
    fn reads_full_encoded_notice_and_keeps_publication_separate_from_schedule() {
        let alerts = parse_rss(FEED, &settings("Kozielska", "24b", GLIWICE_SIMC)).unwrap();
        let emergency = alerts
            .iter()
            .find(|alert| {
                alert
                    .message
                    .as_deref()
                    .unwrap_or_default()
                    .contains("Awaryjna")
            })
            .unwrap();
        assert_eq!(emergency.startDate.as_deref(), Some("2026-10-26T07:00:00"));
        assert_eq!(emergency.endDate.as_deref(), Some("2026-10-26T22:00:00"));
        let message = emergency.message.as_deref().unwrap();
        assert!(message.contains("Data publikacji RSS: Tue, 20 Oct 2026"));
        assert!(message.contains("https://pec.gliwice.pl/przerwy-w-dostawie/old-slug-2024/"));
        assert!(message.contains("Kozielskiej 24a-b"));
        assert!(message.contains("Awaria ciepła"));
    }

    #[test]
    fn parses_planned_notice_using_body_date_not_pub_date() {
        let alerts = parse_rss(FEED, &settings("Sowińskiego", "10", GLIWICE_SIMC)).unwrap();
        let planned = alerts
            .iter()
            .find(|alert| {
                alert
                    .message
                    .as_deref()
                    .unwrap_or_default()
                    .contains("Planowana")
            })
            .unwrap();
        assert_eq!(planned.startDate.as_deref(), Some("2026-11-05T08:00:00"));
        assert_eq!(planned.endDate.as_deref(), Some("2026-11-05T20:00:00"));
        assert!(
            planned
                .message
                .as_deref()
                .unwrap()
                .contains("Planowana przerwa")
        );
    }

    #[test]
    fn keeps_partial_or_missing_schedule_without_using_rss_date() {
        let xml = r#"<rss version="2.0"><channel><item>
          <title>Przerwa w dostawie ciepła</title>
          <pubDate>Tue, 20 Oct 2026 06:00:00 +0000</pubDate>
          <content:encoded xmlns:content="http://purl.org/rss/1.0/modules/content/"><![CDATA[<p>Przerwa przy ul. Kozielskiej.</p>]]></content:encoded>
        </item></channel></rss>"#;
        let alerts = parse_rss(xml, &settings("Kozielska", "24", GLIWICE_SIMC)).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate, None);
        assert_eq!(alerts[0].endDate, None);
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("Data publikacji RSS")
        );

        assert_eq!(
            parse_schedule("w dniu 26.10.2026 r.").0.as_deref(),
            Some("2026-10-26")
        );
        assert_eq!(parse_schedule("bez daty").0, None);
        assert_eq!(parse_schedule("w dniu 32.13.2026 r.").0, None);
    }

    #[test]
    fn parses_multiday_bounds_and_midnight_end() {
        assert_eq!(
            parse_schedule("od 26.10.2026 r. godz. 22:00 do 27.10.2026 r. godz. 06:00"),
            (
                Some("2026-10-26T22:00:00".into()),
                Some("2026-10-27T06:00:00".into())
            )
        );
        assert_eq!(
            parse_schedule("w dniu 26.10.2026 r. od 22:00 do 24:00"),
            (
                Some("2026-10-26T22:00:00".into()),
                Some("2026-10-27T00:00:00".into())
            )
        );
    }

    #[test]
    fn extracts_multiple_streets_and_matches_gliwice_only() {
        let location = location_text("<p>ul. Kozielska 24a-b oraz ul. Sowińskiego 34-34a-c.</p>");
        assert!(location.contains("Kozielska"));
        assert!(location.contains("Sowińskiego"));
        assert!(location_matches_address(
            &location,
            &settings("Sowińskiego", "34a", GLIWICE_SIMC).addresses[0]
        ));
        assert!(!location_matches_address(
            &location,
            &settings("Sowińskiego", "34a", 940_001).addresses[0]
        ));
    }

    #[test]
    fn extracts_streets_from_bare_address_list_items() {
        let body = "<p>W Gliwicach w dniu 26.10.2026 r. od 07:00 do 22:00 wystąpi przerwa w dostawie ciepła.</p><ul><li>Andersena 2-48,</li><li>Brylantowa 2-12</li></ul>";
        let location = location_text(body);
        assert!(location.contains("Andersena 2-48"));
        assert!(location.contains("Brylantowa 2-12"));

        let address = settings("Andersena", "24", GLIWICE_SIMC)
            .addresses
            .remove(0);
        assert!(location_matches_address(&location, &address));
        assert!(house_number_matches_notice(&location, &address));

        let outside_range = settings("Andersena", "52", GLIWICE_SIMC)
            .addresses
            .remove(0);
        assert!(!house_number_matches_notice(&location, &outside_range));
    }

    #[test]
    fn house_matching_handles_ranges_suffixes_and_unknown_formats_permissively() {
        let kozielska = settings("Kozielska", "24b", GLIWICE_SIMC)
            .addresses
            .remove(0);
        assert!(house_number_matches_notice(
            "Gliwice; ul. Kozielska 24a-b",
            &kozielska
        ));
        let outside = settings("Kozielska", "25", GLIWICE_SIMC)
            .addresses
            .remove(0);
        assert!(!house_number_matches_notice(
            "Gliwice; ul. Kozielska 24a-b",
            &outside
        ));
        let sowinskiego = settings("Sowińskiego", "34c", GLIWICE_SIMC)
            .addresses
            .remove(0);
        assert!(house_number_matches_notice(
            "Gliwice; ul. Sowińskiego 34-34a-c",
            &sowinskiego
        ));
        assert!(house_number_matches_notice(
            "Gliwice; ul. Kozielska",
            &outside
        ));
    }

    #[test]
    fn house_setting_does_not_filter_inside_provider_and_duplicate_items_collapse() {
        let without_filter = settings("Kozielska", "999", GLIWICE_SIMC);
        let mut with_filter = without_filter.clone();
        with_filter.filter_by_house_no = true;
        assert_eq!(
            parse_rss(FEED, &without_filter).unwrap().len(),
            parse_rss(FEED, &with_filter).unwrap().len()
        );

        let duplicate = r#"<rss version="2.0"><channel>
          <item><title>Awaria ciepła 26.10.2026 r.</title><link>https://example.test/1</link>
            <description>Awaria ciepła przy ul. Kozielskiej 24.</description></item>
          <item><title>Awaria ciepła 26.10.2026 r.</title><link>https://example.test/1</link>
            <description>Awaria ciepła przy ul. Kozielskiej 24.</description></item>
        </channel></rss>"#;
        assert_eq!(parse_rss(duplicate, &without_filter).unwrap().len(), 1);
    }

    #[test]
    fn ignores_restoration_and_non_outage_items_and_accepts_empty_feed() {
        assert!(outage_kind("Awaria została usunięta, przywrócono dostawę ciepła").is_none());
        assert!(outage_kind("Informacje kontaktowe PEC Gliwice").is_none());
        let empty = r#"<rss version="2.0"><channel><title>Komunikaty</title></channel></rss>"#;
        assert!(
            parse_rss(empty, &settings("Kozielska", "24", GLIWICE_SIMC))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn decodes_entities_and_preserves_disturbance_language() {
        let lines = html_to_lines("<p>W Gliwicach&nbsp;mogą wystąpić zakłócenia.</p>");
        let text = lines.join(" ");
        assert!(text.contains("Gliwicach"));
        assert_eq!(
            outage_kind(&text),
            Some("Możliwe zakłócenia w dostawie ciepła")
        );
    }
}
