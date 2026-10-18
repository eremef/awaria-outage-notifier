use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use std::sync::OnceLock;

pub const MPWIK_RZESZOW_FEED_URL: &str =
    "https://mpwik.rzeszow.pl/category/przerwy-w-dostawie-wody/feed/";

pub struct MpwikRzeszowProvider;

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

fn normalize_polish(text: &str) -> String {
    text.to_lowercase()
        .replace('ą', "a")
        .replace('ć', "c")
        .replace('ę', "e")
        .replace('ł', "l")
        .replace('ń', "n")
        .replace('ó', "o")
        .replace('ś', "s")
        .replace(['ź', 'ż'], "z")
}

fn date_regex() -> &'static Regex {
    static DATE_RE: OnceLock<Regex> = OnceLock::new();
    DATE_RE.get_or_init(|| Regex::new(r"\b(\d{1,2}\.\d{1,2}\.\d{4})\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static TIME_RE: OnceLock<Regex> = OnceLock::new();
    TIME_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:\b(?:od|do)\s+(?:godz(?:iny)?\.?\s*)?|\bgodz(?:iny)?\.?\s*|[-–]\s*)(\d{1,2}[:.]\d{2})\b",
        )
        .unwrap()
    })
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    let normalized = value.replace('.', ":");
    NaiveTime::parse_from_str(&normalized, "%H:%M").ok()
}

fn extract_schedule(text: &str) -> (Option<String>, Option<String>) {
    let dates: Vec<_> = date_regex()
        .captures_iter(text)
        .filter_map(|caps| NaiveDate::parse_from_str(&caps[1], "%d.%m.%Y").ok())
        .fold(Vec::new(), |mut acc, date| {
            if acc.last() != Some(&date) {
                acc.push(date);
            }
            acc
        });
    let without_dates = date_regex().replace_all(text, " ");
    let times: Vec<_> = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|caps| parse_time(&caps[1]))
        .collect();

    let Some(start_date) = dates.first().copied() else {
        return (None, None);
    };
    let start_time = times
        .first()
        .copied()
        .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let start = start_date.and_time(start_time);

    let end = if dates.len() > 1 {
        let end_date = dates[1];
        if let Some(end_time) = times.get(1).copied() {
            end_date.and_time(end_time)
        } else {
            end_date.and_hms_opt(23, 59, 59).unwrap()
        }
    } else if let Some(end_time) = times.get(1).copied() {
        let end_date = if end_time <= start_time {
            start_date + Duration::days(1)
        } else {
            start_date
        };
        end_date.and_time(end_time)
    } else {
        return (Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()), None);
    };

    (
        Some(start.format("%Y-%m-%dT%H:%M:%S").to_string()),
        Some(end.format("%Y-%m-%dT%H:%M:%S").to_string()),
    )
}

fn html_to_text(value: &str) -> String {
    let text = if value.contains('<') {
        let document = Html::parse_fragment(value);
        document.root_element().text().collect::<Vec<_>>().join(" ")
    } else {
        value.to_string()
    };
    text.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn outage_kind(title: &str, text: &str) -> Option<&'static str> {
    let normalized = normalize_polish(&format!("{title} {text}"));
    if !(normalized.contains("przerwa w dostawie wody")
        || (normalized.contains("przer") && normalized.contains("dostawie wody")))
        && !normalized.contains("wylaczenie wody")
        && !normalized.contains("brak wody")
    {
        return None;
    }

    Some(if normalized.contains("awari") {
        "Awaria wodociągowa"
    } else if normalized.contains("planowan")
        || normalized.contains("prace")
        || normalized.contains("robot")
    {
        "Planowana przerwa w dostawie wody"
    } else {
        "Przerwa w dostawie wody"
    })
}

fn mentioned_locality(text: &str) -> &'static str {
    let normalized = normalize_polish(text);
    if normalized.contains("boguchwala") {
        "Boguchwała"
    } else if normalized.contains("tyczyn") {
        "Tyczyn"
    } else if normalized.contains("krasne") {
        "Krasne"
    } else {
        "Rzeszów"
    }
}

fn strip_street_prefix(value: &str) -> String {
    crate::utils::strip_street_prefixes(value.trim()).to_string()
}

fn street_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let notice = normalize_polish(text);
    let mut candidates = vec![address.street_name.as_str(), address.street_name_1.as_str()];
    if let Some(second) = address.street_name_2.as_deref() {
        candidates.push(second);
    }

    candidates.into_iter().any(|candidate| {
        let candidate = strip_street_prefix(candidate);
        let tokens: Vec<_> = normalize_polish(&candidate)
            .split(|ch: char| !ch.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_string)
            .collect();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem = if token.ends_with('a') && token.chars().count() > 5 {
                    &token[..token.len() - 1]
                } else {
                    token.as_str()
                };
                notice
                    .split(|ch: char| !ch.is_alphanumeric())
                    .any(|word| word.starts_with(stem))
            })
    })
}

fn locality_matches(text: &str, address: &AddressEntry) -> bool {
    let notice = normalize_polish(text);
    let city = normalize_polish(&address.city_name);
    if notice.contains("boguchwala") {
        city.contains("boguchwala")
    } else if notice.contains("tyczyn") {
        city.contains("tyczyn")
    } else if notice.contains("krasne") {
        city.contains("krasne")
    } else {
        city.contains("rzeszow")
    }
}

fn address_matches_notice(text: &str, address: &AddressEntry) -> bool {
    let has_street = !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty());

    if !has_street {
        locality_matches(text, address)
    } else {
        locality_matches(text, address) && street_matches_notice(text, address)
    }
}

fn parse_rss(xml: &str, settings: &Settings) -> Result<Vec<UnifiedAlert>, String> {
    let rss: RssFeed = quick_xml::de::from_str(xml).map_err(|error| error.to_string())?;
    let mut alerts = Vec::new();

    for item in rss.channel.items {
        let title = html_to_text(&item.title);
        let text = html_to_text(&item.description);
        let Some(kind) = outage_kind(&title, &text) else {
            continue;
        };
        let (start_date, end_date) = extract_schedule(&format!("{title} {text}"));
        let locality = mentioned_locality(&format!("{title} {text}"));
        let address_index = settings
            .addresses
            .iter()
            .enumerate()
            .find(|(_, address)| {
                address.is_active
                    && crate::api_logic::is_address_applicable_for_provider(
                        &AlertSource::MpwikRzeszow,
                        address,
                    )
                    && address_matches_notice(&format!("{title} {text}"), address)
            })
            .map(|(index, _)| index);

        alerts.push(UnifiedAlert {
            source: AlertSource::MpwikRzeszow,
            startDate: start_date,
            endDate: end_date,
            message: Some(format!("{kind}: {text}")),
            location: Some(format!("Miejscowość: {locality}")),
            address_index,
            is_local: Some(address_index.is_some()),
            hash: None,
        });
    }

    Ok(alerts)
}

#[async_trait]
impl AlertProvider for MpwikRzeszowProvider {
    fn id(&self) -> String {
        "mpwik_rzeszow".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MpwikRzeszow
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

        let response = match client.get(MPWIK_RZESZOW_FEED_URL).send().await {
            Ok(response) => response,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MPWiK Rzeszów request error: {error}")],
                );
            }
        };
        if !response.status().is_success() {
            return (
                Vec::new(),
                vec![format!("MPWiK Rzeszów HTTP error: {}", response.status())],
            );
        }

        match response.text().await {
            Ok(xml) => match parse_rss(&xml, settings) {
                Ok(alerts) => (alerts, Vec::new()),
                Err(error) => (
                    Vec::new(),
                    vec![format!("MPWiK Rzeszów RSS parse error: {error}")],
                ),
            },
            Err(error) => (
                Vec::new(),
                vec![format!("MPWiK Rzeszów response read error: {error}")],
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
        <title>Przerwa w dostawie wody w dniu 16.10.2026 r. od godziny 8:30 do godziny 15:00.</title>
        <link>https://mpwik.rzeszow.pl/przerwa/</link>
        <pubDate>Fri, 16 Oct 2026 06:44:11 +0000</pubDate>
        <description><![CDATA[MPWiK informuje, w dniu 16.10.2026 r. od godziny 8:30 do godziny 15:00 z powodu awarii nastąpi przerwa w dostawie wody przy ul. Nowosądeckiej.]]></description>
      </item>
      <item>
        <title>Przerwa w dostawie wody w dniu 13.05.2026 r. do 14.05.2026 r.</title>
        <link>https://mpwik.rzeszow.pl/przerwa-2/</link>
        <pubDate>Wed, 13 May 2026 08:00:00 +0000</pubDate>
        <description><![CDATA[Od dnia 13.05.2026 r. do dnia 14.05.2026 r. będzie występować przerwa w dostawie wody na ul. Siemiradzkiego, ul. Orzeszkowej, Piłsudskiego i boczne.]]></description>
      </item>
      <item>
        <title>Planowane prace na sieci wodociągowej w dniu 15.05.2026 r.</title>
        <description><![CDATA[W dniu 15.05.2026 r. planowane prace spowodują przerwę w dostawie wody na ul. Kwarcowej.]]></description>
      </item>
      <item>
        <title>Przerwa w dostawie wody w dniu 7.10.2026 r. od godziny 8:00 do godziny 12:00.</title>
        <description><![CDATA[W dniu 7.10.2026 r. nastąpi przerwa w dostawie wody przy ul. Zawiszy Czarnego 66, 68a-b,73, 75, 76, 78, 79, 80, 81, 82.]]></description>
      </item>
      <item>
        <title>Przerwa w dostawie wody 22.04.2026</title>
        <pubDate>Wed, 22 Apr 2026 08:00:00 +0000</pubDate>
        <description><![CDATA[W dniu 22.04.2026 r. od godz. 11:30 do godz. 17:00 nastąpi przerwa w dostawie wody dla gm. Boguchwała.]]></description>
      </item>
      <item>
        <title>Utrudnienia w ruchu drogowym w dniach 8.10.2026–30.11.2026</title>
        <pubDate>Thu, 8 Oct 2026 08:00:00 +0000</pubDate>
        <description><![CDATA[Budowa sieci wodociągowej przy ul. Jana Wąsacza może powodować utrudnienia w ruchu.]]></description>
      </item>
      <item>
        <title>Przerwa w dostawie wody w dniu 16.10.2026 r. od godziny 8:30 do godziny 15:00.</title>
        <description><![CDATA[MPWiK informuje, w dniu 16.10.2026 r. od godziny 8:30 do godziny 15:00 z powodu awarii nastąpi przerwa w dostawie wody przy ul. Nowosądeckiej.]]></description>
      </item>
    </channel></rss>"#;

    fn settings(city: &str, street: &str, city_id: u64, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Home".into(),
                city_name: city.into(),
                voivodeship: "Podkarpackie".into(),
                district: String::new(),
                commune: String::new(),
                street_name: street.into(),
                street_name_1: street.into(),
                street_name_2: None,
                house_no: "12".into(),
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
            filter_by_house_no,
        }
    }

    #[test]
    fn parses_feed_schedules_and_ignores_unrelated_posts() {
        let alerts = parse_rss(FEED, &settings("Rzeszów", "Nowosądecka", 974133, false)).unwrap();
        assert_eq!(alerts.len(), 6);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-16T08:30:00"));
        assert_eq!(alerts[0].endDate.as_deref(), Some("2026-10-16T15:00:00"));
        assert!(
            alerts[0]
                .message
                .as_deref()
                .unwrap()
                .contains("Awaria wodociągowa")
        );
        assert_eq!(alerts[1].startDate.as_deref(), Some("2026-05-13T00:00:00"));
        assert_eq!(alerts[1].endDate.as_deref(), Some("2026-05-14T23:59:59"));
        assert!(alerts[1].message.as_deref().unwrap().contains("i boczne"));
        assert!(
            alerts[2]
                .message
                .as_deref()
                .unwrap()
                .contains("Planowana przerwa")
        );
        assert!(alerts[3].message.as_deref().unwrap().contains("68a-b"));
        assert!(alerts[4].message.as_deref().unwrap().contains("Boguchwała"));
        assert_eq!(
            crate::api_logic::deduplicate_alerts(alerts.clone()).len(),
            alerts.len() - 1
        );
    }

    #[test]
    fn matches_city_and_street_and_respects_locality() {
        let rzeszow = parse_rss(FEED, &settings("Rzeszów", "Nowosądecka", 974133, false)).unwrap();
        assert_eq!(rzeszow[0].is_local, Some(true));
        let other_city =
            parse_rss(FEED, &settings("Lublin", "Nowosądecka", 959423, false)).unwrap();
        assert_eq!(other_city[0].is_local, Some(false));
        assert!(!crate::api_logic::is_address_applicable_for_provider(
            &AlertSource::MpwikRzeszow,
            &settings("Lublin", "Nowosądecka", 959423, false).addresses[0]
        ));
        let boguchwala = parse_rss(FEED, &settings("Boguchwała", "", 644750, false)).unwrap();
        assert_eq!(boguchwala[4].is_local, Some(true));
        let numbered_street = parse_rss(
            FEED,
            &settings("Rzeszów", "Zawiszy Czarnego", 974133, false),
        )
        .unwrap();
        assert_eq!(numbered_street[3].is_local, Some(true));
        assert!(
            numbered_street[3]
                .message
                .as_deref()
                .unwrap()
                .contains("68a-b")
        );
    }

    #[test]
    fn house_number_preference_does_not_change_provider_street_matching() {
        let without_filter =
            parse_rss(FEED, &settings("Rzeszów", "Nowosądecka", 974133, false)).unwrap();
        let with_filter =
            parse_rss(FEED, &settings("Rzeszów", "Nowosądecka", 974133, true)).unwrap();
        assert_eq!(without_filter[0].is_local, Some(true));
        assert_eq!(with_filter[0].is_local, Some(true));
        assert_eq!(with_filter[0].address_index, Some(0));
    }

    #[test]
    fn missing_or_invalid_schedules_stay_unknown() {
        assert_eq!(extract_schedule("brak daty i godziny"), (None, None));
        assert_eq!(extract_schedule("99.99.2026 r. godz. 25:70"), (None, None));
        let (start, end) = extract_schedule("dnia 17.09.2026 od godz. 1:30");
        assert_eq!(start.as_deref(), Some("2026-09-17T01:30:00"));
        assert_eq!(end, None);
        let (start, end) = extract_schedule("dnia 17.09.2026 od godz. 08:00 do 13:00");
        assert_eq!(start.as_deref(), Some("2026-09-17T08:00:00"));
        assert_eq!(end.as_deref(), Some("2026-09-17T13:00:00"));
        let (start, end) = extract_schedule("dnia 17.09.2026 godz. 08:00 - 13:00");
        assert_eq!(start.as_deref(), Some("2026-09-17T08:00:00"));
        assert_eq!(end.as_deref(), Some("2026-09-17T13:00:00"));
    }
}
