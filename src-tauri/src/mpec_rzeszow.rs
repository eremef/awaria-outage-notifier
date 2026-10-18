use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use std::sync::OnceLock;

pub const MPEC_RZESZOW_FEED_URL: &str = "https://mpecrzeszow.pl/komunikaty?format=feed&type=rss";

pub struct MpecRzeszowProvider;

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
}

#[derive(Debug)]
struct NoticeSegment {
    display_text: String,
    match_text: String,
}

#[derive(Clone, Copy)]
struct ParsedTime {
    value: NaiveTime,
    next_day: bool,
}

fn block_break_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<br\s*/?>|</(?:p|div|li|tr|h[1-6])\s*>").unwrap())
}

fn superscript_minute_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<sup>\s*(\d{2})\s*</sup>").unwrap())
}

fn html_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<[^>]+>").unwrap())
}

fn split_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(\d{1,2})\s+(\.\d{1,2}\.\d{4})").unwrap())
}

fn split_clock_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(\d{1,2})\s+:(\d{2})").unwrap())
}

fn numeric_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b\d{1,2}\.\d{1,2}\.\d{4}\b").unwrap())
}

fn named_month_date_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)\b\d{1,2}\s+(?:stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|wrzesnia|pazdziernika|listopada|grudnia)\s+\d{4}\b",
        )
        .unwrap()
    })
}

fn time_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)(?:godz(?:ina|iny)?\.?\s*|g\.?\s*)?(\d{1,2}[:.]\d{2})\b").unwrap()
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

fn html_to_lines(html: &str) -> Vec<String> {
    let with_clock_minutes = superscript_minute_regex().replace_all(html, ":$1");
    let with_breaks = block_break_regex().replace_all(&with_clock_minutes, "\n");
    let without_tags = html_tag_regex().replace_all(&with_breaks, " ");
    let document = Html::parse_fragment(&without_tags);
    let decoded = document.root_element().text().collect::<Vec<_>>().join("");
    let joined_dates = split_date_regex().replace_all(&decoded, "$1$2");
    let normalized_clock = split_clock_regex().replace_all(&joined_dates, "$1:$2");

    normalized_clock
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

fn month_number(name: &str) -> Option<u32> {
    Some(match name {
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

fn dates_in_text(text: &str) -> Vec<NaiveDate> {
    let mut dates = numeric_date_regex()
        .find_iter(text)
        .filter_map(|found| NaiveDate::parse_from_str(found.as_str(), "%d.%m.%Y").ok())
        .collect::<Vec<_>>();

    let normalized = normalize_polish(text);
    dates.extend(
        named_month_date_regex()
            .captures_iter(&normalized)
            .filter_map(|caps| {
                let words: Vec<_> = caps[0].split_whitespace().collect();
                let day = words.first()?.parse::<u32>().ok()?;
                let month = month_number(*words.get(1)?)?;
                let year = words.get(2)?.parse::<i32>().ok()?;
                NaiveDate::from_ymd_opt(year, month, day)
            }),
    );

    dates
}

fn parse_time(value: &str) -> Option<ParsedTime> {
    let (hour, minute) = value.split_once([':', '.'])?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(ParsedTime {
            value: NaiveTime::from_hms_opt(0, 0, 0)?,
            next_day: true,
        });
    }
    Some(ParsedTime {
        value: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let dates = dates_in_text(text);
    let Some(start_date) = dates.first().copied() else {
        return (None, None);
    };

    // Remove dates before scanning for clocks so values such as 16.10.2026 do
    // not look like a 16:10 time.
    let without_numeric_dates = numeric_date_regex().replace_all(text, " ");
    let without_dates = named_month_date_regex().replace_all(&without_numeric_dates, " ");
    let times = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|caps| parse_time(&caps[1]))
        .collect::<Vec<_>>();

    let start_time = times.first().copied();
    let start_date = if start_time.is_some_and(|time| time.next_day) {
        start_date + Duration::days(1)
    } else {
        start_date
    };
    let start = start_date.and_time(
        start_time
            .map(|time| time.value)
            .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap()),
    );

    let explicit_end = times.get(1).copied().and_then(|end_time| {
        let mut end_date = dates.get(1).copied().unwrap_or(start_date);
        if end_time.next_day {
            end_date += Duration::days(1);
        }
        let mut end = end_date.and_time(end_time.value);
        if dates.len() < 2 && !end_time.next_day && end <= start {
            end += Duration::days(1);
        }
        (end > start).then_some(end)
    });
    let end = explicit_end.or_else(|| {
        start_date
            .and_hms_opt(23, 59, 0)
            .filter(|end_of_day| *end_of_day >= start)
    });
    (Some(format_datetime(start)), end.map(format_datetime))
}

fn line_starts_schedule(line: &str) -> bool {
    if dates_in_text(line).is_empty() {
        return false;
    }
    let normalized = normalize_polish(line);
    normalized.contains("w dniu")
        || normalized.contains("w dniach")
        || normalized.contains("dniu dzisiejszym")
        || time_regex().is_match(line)
}

fn notice_segments(title: &str, description: &str) -> Vec<NoticeSegment> {
    let mut lines = html_to_lines(description);
    if lines.is_empty() {
        lines = html_to_lines(title);
    }
    if lines.is_empty() {
        return Vec::new();
    }

    let schedule_indices = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line_starts_schedule(line).then_some(index))
        .collect::<Vec<_>>();
    if schedule_indices.is_empty() {
        let text = lines.join("\n");
        return vec![NoticeSegment {
            display_text: text.clone(),
            match_text: text,
        }];
    }

    let common_prefix = lines[..schedule_indices[0]].join("\n");
    schedule_indices
        .iter()
        .enumerate()
        .map(|(position, start)| {
            let end = schedule_indices
                .get(position + 1)
                .copied()
                .unwrap_or(lines.len());
            let segment_lines = &lines[*start..end];
            let match_text = segment_lines.join("\n");
            let display_text = if common_prefix.is_empty() {
                match_text.clone()
            } else {
                format!("{common_prefix}\n{match_text}")
            };
            NoticeSegment {
                display_text,
                match_text,
            }
        })
        .collect()
}

fn is_restoration_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("przywrocono dostawe")
        || normalized.contains("przywrocono dostawy")
        || normalized.contains("zostala usunieta")
        || normalized.contains("zostal usuniety")
        || normalized.contains("awaria zostala usunieta")
}

fn outage_kind(text: &str) -> Option<&'static str> {
    let normalized = normalize_polish(text);
    if is_restoration_notice(&normalized) {
        return None;
    }
    let heat_context =
        normalized.contains("ciepl") || normalized.contains("c.w.u") || normalized.contains("cwu");
    let outage_context = normalized.contains("przer")
        || normalized.contains("awari")
        || normalized.contains("zaklocen");
    if !heat_context || !outage_context {
        return None;
    }

    Some(if normalized.contains("awari") {
        "Awaria ciepła"
    } else {
        "Przerwa w dostawie ciepła"
    })
}

fn street_matches_address(text: &str, address: &AddressEntry) -> bool {
    let notice = normalize_polish(text);
    let mut configured_streets = vec![address.street_name.as_str(), address.street_name_1.as_str()];
    if let Some(second) = address.street_name_2.as_deref() {
        configured_streets.push(second);
    }

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
                notice
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|word| word.starts_with(&stem))
            })
    })
}

fn matching_address_index(text: &str, settings: &Settings) -> Option<usize> {
    settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            if !address.is_active
                || !crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::MpecRzeszow,
                    address,
                )
            {
                return false;
            }

            let has_street = !address.street_name.trim().is_empty()
                || !address.street_name_1.trim().is_empty()
                || address
                    .street_name_2
                    .as_deref()
                    .is_some_and(|street| !street.trim().is_empty());
            !has_street || street_matches_address(text, address)
        })
        .map(|(index, _)| index)
}

fn parse_item(item: RssItem, settings: &Settings) -> Vec<UnifiedAlert> {
    let base_text = if item.description.trim().is_empty() {
        item.title.clone()
    } else {
        item.description.clone()
    };
    let Some(kind) = outage_kind(&format!("{}\n{base_text}", item.title)) else {
        return Vec::new();
    };

    notice_segments(&item.title, &base_text)
        .into_iter()
        .filter_map(|segment| {
            let (start_date, end_date) = parse_schedule(&segment.display_text);
            if start_date.is_none() {
                return None;
            }
            let address_index = matching_address_index(&segment.match_text, settings);
            Some(UnifiedAlert {
                source: AlertSource::MpecRzeszow,
                startDate: start_date,
                endDate: end_date,
                message: Some(format!("{kind}: {}", segment.display_text)),
                location: Some("Miejscowość: Rzeszów".to_string()),
                address_index,
                is_local: Some(address_index.is_some()),
                hash: None,
            })
        })
        .collect()
}

fn parse_rss(xml: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let feed: RssFeed = quick_xml::de::from_str(xml).map_err(|error| error.to_string())?;
    let alerts = feed
        .channel
        .items
        .into_iter()
        .flat_map(|item| parse_item(item, settings))
        .collect();
    Ok(crate::api_logic::deduplicate_alerts(alerts))
}

#[async_trait]
impl AlertProvider for MpecRzeszowProvider {
    fn id(&self) -> String {
        "mpec_rzeszow".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MpecRzeszow
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
            .get(MPEC_RZESZOW_FEED_URL)
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
                    vec![format!("MPEC Rzeszów request error: {error}")],
                );
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("MPEC Rzeszów HTTP error: {}", response.status())],
            );
        }

        match response.text().await {
            Ok(xml) => match parse_rss(&xml, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (
                    Vec::new(),
                    vec![format!("MPEC Rzeszów RSS parse error: {error}")],
                ),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("MPEC Rzeszów response read error: {error}")],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <rss version="2.0"><channel>
      <item>
        <title>Przerwa w dostawie ciepła dla celów przygotowania c.w.u. ul. Architektów 3, 5</title>
        <link>https://mpecrzeszow.pl/komunikaty/1462</link>
        <pubDate>Tue, 13 Oct 2026 06:19:50 +0000</pubDate>
        <description>&lt;p&gt;Z powodu prowadzenia prac konserwacyjnych w węzłach cieplnych wystąpi przerwa w dostawie ciepła do podgrzewu zimnej wody wodociągowej.&lt;/p&gt;
        &lt;div&gt;w dniu &lt;strong&gt;16&lt;/strong&gt;&lt;strong&gt;.10.2026 r. od godz. 9&lt;sup&gt;00&lt;/sup&gt; do godz. 14&lt;sup&gt;00&lt;/sup&gt;&lt;/strong&gt; w budynku przy ul. Architektów 3&lt;/div&gt;
        &lt;div&gt;w dniu &lt;strong&gt;19.10.2026 r. od godz. 9:00 do godz. 14:00&lt;/strong&gt; w budynku przy ul. Architektów 5.&lt;/div&gt;
        &lt;p&gt;Dołożymy wszelkich starań. Za utrudnienia przepraszamy.&lt;/p&gt;</description>
      </item>
      <item>
        <title>Przerwa w dostawie ciepła - budynki przy ul. Paderewskiego</title>
        <description>&lt;div&gt;W związku z modernizacją sieci nastąpi przerwa w dostawie ciepła&lt;/div&gt;
        &lt;div&gt;w dniu 23.10.2026 r. od godz. 8:00 do godz. 24:00 dla budynków:&lt;/div&gt;
        &lt;div&gt;Paderewskiego 43, 43A, 47A, Hubala 1.&lt;/div&gt;</description>
      </item>
      <item>
        <title>Awaria sieci ciepłowniczej, 8 października</title>
        <description>&lt;div&gt;Z powodu awarii sieci ciepłowniczej w dniu dzisiejszym, tj. 8 października 2026 r. od godz. 15:00 do późnych godzin popołudniowych wystąpi przerwa w dostawie ciepła do budynków przy ul. Cegielnianej 1.&lt;/div&gt;</description>
      </item>
      <item>
        <title>Aktualizacja dot. przerwy w dostawie ciepła</title>
        <description>&lt;p&gt;Awaria sieci ciepłowniczej została usunięta. Przywrócono dostawę ciepła do budynku przy Pl. Ofiar Getta 6.&lt;/p&gt;</description>
      </item>
      <item>
        <title>Brak komunikatów</title>
        <description>&lt;p&gt;Informacja o taryfie i kontakt do pogotowia ciepłowniczego.&lt;/p&gt;</description>
      </item>
    </channel></rss>"#;

    fn settings(street: &str, house_no: &str, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".into(),
                city_name: "Rzeszów".into(),
                voivodeship: "Podkarpackie".into(),
                district: String::new(),
                commune: String::new(),
                street_name: street.into(),
                street_name_1: street.into(),
                street_name_2: None,
                house_no: house_no.into(),
                city_id: Some(974133),
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
            filter_by_house_no,
        }
    }

    fn alert_containing<'a>(alerts: &'a [UnifiedAlert], text: &str) -> &'a UnifiedAlert {
        alerts
            .iter()
            .find(|alert| alert.message.as_deref().unwrap_or_default().contains(text))
            .unwrap_or_else(|| panic!("missing alert containing {text:?}"))
    }

    #[test]
    fn splits_multiple_building_schedules_and_normalizes_superscript_minutes() {
        let alerts = parse_rss(FEED, &settings("Architektów", "3", false)).unwrap();
        assert_eq!(alerts.len(), 4);
        let architektow_3 = alert_containing(&alerts, "Architektów 3");
        assert_eq!(
            architektow_3.startDate.as_deref(),
            Some("2026-10-16T09:00:00")
        );
        assert_eq!(
            architektow_3.endDate.as_deref(),
            Some("2026-10-16T14:00:00")
        );
        assert_eq!(architektow_3.is_local, Some(true));
        let architektow_5 = alert_containing(&alerts, "Architektów 5");
        assert_eq!(
            architektow_5.startDate.as_deref(),
            Some("2026-10-19T09:00:00")
        );
        assert_eq!(architektow_5.is_local, Some(true));
    }

    #[test]
    fn handles_24_clock_vague_ends_and_named_month_dates() {
        let alerts = parse_rss(FEED, &settings("Paderewskiego", "43A", false)).unwrap();
        let paderewskiego = alert_containing(&alerts, "Paderewskiego 43, 43A");
        assert_eq!(
            paderewskiego.startDate.as_deref(),
            Some("2026-10-23T08:00:00")
        );
        assert_eq!(
            paderewskiego.endDate.as_deref(),
            Some("2026-10-24T00:00:00")
        );
        assert_eq!(paderewskiego.is_local, Some(true));
        let cegielniana = alert_containing(&alerts, "Cegielnianej 1");
        assert_eq!(
            cegielniana.startDate.as_deref(),
            Some("2026-10-08T15:00:00")
        );
        assert_eq!(cegielniana.endDate.as_deref(), Some("2026-10-08T23:59:00"));
        assert!(
            cegielniana
                .message
                .as_deref()
                .unwrap()
                .contains("Awaria ciepła")
        );
    }

    #[test]
    fn ignores_restoration_and_non_outage_items() {
        let alerts = parse_rss(FEED, &settings("Pl. Ofiar Getta", "6", false)).unwrap();
        assert!(
            alerts
                .iter()
                .all(|alert| !alert.message.as_deref().unwrap().contains("Przywrócono"))
        );
        assert_eq!(alerts.len(), 4);
    }

    #[test]
    fn house_number_preference_stays_in_shared_filter() {
        let without_filter = parse_rss(FEED, &settings("Architektów", "3", false)).unwrap();
        let with_filter = parse_rss(FEED, &settings("Architektów", "3", true)).unwrap();
        assert_eq!(
            alert_containing(&without_filter, "Architektów 3").is_local,
            Some(true)
        );
        assert_eq!(
            alert_containing(&with_filter, "Architektów 3").is_local,
            Some(true)
        );
    }

    #[test]
    fn empty_feed_returns_no_alerts() {
        let xml = r#"<rss version="2.0"><channel><title>Communications</title></channel></rss>"#;
        assert!(
            parse_rss(xml, &settings("Architektów", "3", false))
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    #[ignore = "requires live access to MPEC Rzeszów RSS"]
    async fn live_feed_parses_with_app_http_client() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let response = client.get(MPEC_RZESZOW_FEED_URL).send().await.unwrap();
        assert!(response.status().is_success());
        let xml = response.text().await.unwrap();
        let alerts = parse_rss(&xml, &settings("Architektów", "3", false)).unwrap();
        assert!(!alerts.is_empty());
    }
}
