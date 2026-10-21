use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::Html;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub const PWIK_OLSZTYN_EMERGENCY_FEED_URL: &str =
    "https://pwik.olsztyn.pl/mapa-awarii/awarie/feed/";
pub const PWIK_OLSZTYN_PLANNED_FEED_URL: &str =
    "https://pwik.olsztyn.pl/mapa-awarii/planowane-przerwy-w-dostawie/feed/";

const UNKNOWN_END_RETENTION_HOURS: i64 = 72;
const SCHEDULED_START_RETENTION_HOURS: i64 = 24;

pub struct PwikOlsztynProvider;

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
    #[serde(default)]
    description: String,
    #[serde(rename = "content:encoded", default)]
    content: String,
    #[serde(rename = "pubDate", default)]
    pub_date: String,
}

#[derive(Clone, Copy)]
struct ServiceLocality {
    name: &'static str,
    city_id: u64,
    aliases: &'static [&'static str],
}

const SERVICE_LOCALITIES: &[ServiceLocality] = &[
    ServiceLocality {
        name: "Olsztyn",
        city_id: 964465,
        aliases: &["olsztyn", "olsztynie", "olsztyna"],
    },
    ServiceLocality {
        name: "Nikielkowo",
        city_id: 470036,
        aliases: &["nikielkowo", "nikielkowie"],
    },
    ServiceLocality {
        name: "Jonkowo",
        city_id: 477080,
        aliases: &["jonkowo", "jonkowie", "jonkowa"],
    },
    ServiceLocality {
        name: "Ostrzeszewo",
        city_id: 486698,
        aliases: &["ostrzeszewo", "ostrzeszewie"],
    },
    ServiceLocality {
        name: "Zalbki",
        city_id: 473135,
        aliases: &["zalbki", "zalbkach"],
    },
    ServiceLocality {
        name: "Myki",
        city_id: 473098,
        aliases: &["myki", "mykach"],
    },
    ServiceLocality {
        name: "Wadąg",
        city_id: 473129,
        aliases: &["wadag", "wadagu"],
    },
    ServiceLocality {
        name: "Gietrzwałd",
        city_id: 473589,
        aliases: &["gietrzwald", "gietrzwaldzie"],
    },
    ServiceLocality {
        name: "Bartąg",
        city_id: 488830,
        aliases: &["bartag", "bartagu"],
    },
];

#[derive(Clone)]
struct AffectedLocality {
    locality: &'static ServiceLocality,
    address_lines: Vec<String>,
}

#[derive(Clone, Copy)]
enum NoticeKind {
    Emergency,
    Planned,
    WaterInterruption,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Emergency => "Awaria wodociągowa",
            Self::Planned => "Planowana przerwa w dostawie wody",
            Self::WaterInterruption => "Przerwa w dostawie wody",
        }
    }
}

fn normalize_polish(value: &str) -> String {
    crate::utils::clean_text(value)
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn block_end_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)</(?:p|li|div|h[1-6]|tr|section|article)>\s*").unwrap())
}

fn break_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<br\s*/?>").unwrap())
}

fn html_to_text(value: &str) -> String {
    let with_lines = block_end_regex().replace_all(value, "\n");
    let with_lines = break_regex().replace_all(&with_lines, "\n");
    let document = Html::parse_fragment(&with_lines);
    let decoded = document.root_element().text().collect::<Vec<_>>().join("");

    decoded
        .replace('\u{a0}', " ")
        .lines()
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_truncation_marker(value: &str) -> bool {
    value.contains("[...]") || value.contains("[…]")
}

fn notice_kind(title: &str, body: &str) -> Option<NoticeKind> {
    let normalized = normalize_polish(&format!("{title} {body}"));
    if normalized.contains("brak awarii")
        || normalized.contains("brak przerw")
        || normalized.contains("brak planowanych")
    {
        return None;
    }

    let mentions_water_interruption = [
        "przerwa w dostawie wody",
        "przerwy w dostawie wody",
        "wylaczenie wody",
        "wylaczenia wody",
        "brak wody",
        "nie bedzie wody",
        "wody nie bedzie",
        "pozbawieni wody",
        "wstrzymana dostawa wody",
        "wstrzymana bedzie dostawa wody",
        "wstrzymanie dostawy wody",
        "dostawa wody bedzie wstrzymana",
        "dystrybucja wody bedzie wstrzymana",
        "dystrybucja wody zostanie wstrzymana",
        "wstrzymany doplyw wody",
        "nie bedzie dostepu do wody",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));
    if !mentions_water_interruption {
        return None;
    }

    if normalized.contains("awari") {
        Some(NoticeKind::Emergency)
    } else if ["planowan", "modernizac", "prace", "robot", "konserwac"]
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        Some(NoticeKind::Planned)
    } else {
        Some(NoticeKind::WaterInterruption)
    }
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})(?:\s*r\.?)?\b").unwrap())
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(\d{1,2}[:.]\d{2})\s*(?:-|–|—|do)\s*(\d{1,2}[:.]\d{2})").unwrap()
    })
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2}[:.]\d{2})\b").unwrap())
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    let normalized = value.replace('/', ".");
    NaiveDate::parse_from_str(&normalized, "%d.%m.%Y").ok()
}

fn parse_time(value: &str) -> Option<NaiveTime> {
    let normalized = value.replace('.', ":");
    NaiveTime::parse_from_str(&normalized, "%H:%M").ok()
}

fn format_datetime(datetime: NaiveDateTime) -> String {
    datetime.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn extract_schedule(text: &str) -> (Option<String>, Option<String>) {
    let dates: Vec<_> = date_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let value = format!("{}.{}.{}", &captures[1], &captures[2], &captures[3]);
            parse_date(&value)
        })
        .fold(Vec::new(), |mut dates, date| {
            if dates.last() != Some(&date) {
                dates.push(date);
            }
            dates
        });
    let Some(start_date) = dates.first().copied() else {
        return (None, None);
    };

    let without_dates = date_regex().replace_all(text, " ");
    if let Some(captures) = time_range_regex().captures(&without_dates) {
        let Some(start_time) = parse_time(&captures[1]) else {
            return (None, None);
        };
        let Some(end_time) = parse_time(&captures[2]) else {
            return (None, None);
        };
        let start = start_date.and_time(start_time);
        let end_date = if let Some(end_date) = dates.get(1).copied() {
            end_date
        } else if end_time <= start_time {
            start_date + Duration::days(1)
        } else {
            start_date
        };
        return (
            Some(format_datetime(start)),
            Some(format_datetime(end_date.and_time(end_time))),
        );
    }

    if let Some(capture) = time_regex().captures(&without_dates)
        && let Some(time) = parse_time(&capture[1])
    {
        return (Some(format_datetime(start_date.and_time(time))), None);
    }

    (None, None)
}

fn latest_event_date(text: &str) -> Option<NaiveDate> {
    date_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let value = format!("{}.{}.{}", &captures[1], &captures[2], &captures[3]);
            parse_date(&value)
        })
        .max()
}

fn address_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:ul|ulica|al|aleja|pl|plac|os|osiedle)\b\.?\s+[^\n]*\d").unwrap()
    })
}

fn number_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{1,4}[a-zA-Z]?\b").unwrap())
}

fn is_address_line(line: &str) -> bool {
    let without_dates = date_regex().replace_all(line, " ");
    let without_times = time_regex().replace_all(&without_dates, " ");
    address_line_regex().is_match(&without_times)
        || (find_localities(&without_times)
            .iter()
            .any(|_| number_regex().is_match(&without_times)))
}

fn locality_mentions(text: &str, locality: &ServiceLocality) -> bool {
    let normalized = normalize_polish(text);
    locality.aliases.iter().any(|alias| {
        normalized
            .split(|character: char| !character.is_alphanumeric())
            .any(|token| token == *alias)
    })
}

fn find_localities(text: &str) -> Vec<&'static ServiceLocality> {
    SERVICE_LOCALITIES
        .iter()
        .filter(|locality| locality_mentions(text, locality))
        .collect()
}

fn affected_localities(text: &str) -> Vec<AffectedLocality> {
    let mut groups = BTreeMap::<u64, AffectedLocality>::new();
    for line in text
        .lines()
        .flat_map(|line| line.split(';'))
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
    {
        let mentioned = find_localities(&line);
        if !mentioned.is_empty() {
            for locality in mentioned {
                let group = groups
                    .entry(locality.city_id)
                    .or_insert_with(|| AffectedLocality {
                        locality,
                        address_lines: Vec::new(),
                    });
                if is_address_line(&line) && !group.address_lines.contains(&line) {
                    group.address_lines.push(line.clone());
                }
            }
        } else if is_address_line(&line) {
            let locality = &SERVICE_LOCALITIES[0];
            let group = groups
                .entry(locality.city_id)
                .or_insert_with(|| AffectedLocality {
                    locality,
                    address_lines: Vec::new(),
                });
            if !group.address_lines.contains(&line) {
                group.address_lines.push(line);
            }
        }
    }

    if groups.is_empty() {
        for locality in find_localities(text) {
            groups.insert(
                locality.city_id,
                AffectedLocality {
                    locality,
                    address_lines: Vec::new(),
                },
            );
        }
    }
    if groups.is_empty() {
        let locality = &SERVICE_LOCALITIES[0];
        groups.insert(
            locality.city_id,
            AffectedLocality {
                locality,
                address_lines: Vec::new(),
            },
        );
    }

    groups.into_values().collect()
}

fn address_matches_locality(address: &AddressEntry, locality: &ServiceLocality) -> bool {
    if let Some(city_id) = address.city_id {
        return city_id == locality.city_id;
    }
    let city = normalize_polish(&address.city_name);
    let commune = normalize_polish(&address.commune);
    city == normalize_polish(locality.name) || commune == normalize_polish(locality.name)
}

fn street_matches_address(location_text: &str, address: &AddressEntry) -> bool {
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let notice_tokens: Vec<_> = normalize_polish(location_text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect();

    candidates.into_iter().any(|candidate| {
        let candidate = crate::utils::strip_street_prefixes(candidate.trim());
        let tokens: Vec<_> = normalize_polish(candidate)
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| token.chars().count() > 2)
            .map(str::to_owned)
            .collect();
        !tokens.is_empty()
            && tokens.iter().all(|token| {
                let stem = if token.ends_with('a') && token.chars().count() > 5 {
                    &token[..token.len() - 1]
                } else {
                    token.as_str()
                };
                notice_tokens
                    .iter()
                    .any(|notice_token| notice_token.starts_with(stem))
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
        || group.address_lines.is_empty()
        || street_matches_address(&group.address_lines.join(" "), address)
}

pub(crate) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    let (locality_text, address_text) = location
        .split_once(';')
        .map(|(locality, details)| (locality, details.trim()))
        .unwrap_or((location, ""));
    let locality_name = locality_text
        .split_once(':')
        .map(|(_, value)| value.trim())
        .unwrap_or(locality_text.trim());
    let Some(locality) = find_localities(locality_name).first().copied() else {
        return false;
    };
    address_matches_group(
        address,
        &AffectedLocality {
            locality,
            address_lines: if address_text.is_empty() {
                Vec::new()
            } else {
                vec![address_text.to_string()]
            },
        },
    )
}

fn parse_publication_time(value: &str) -> Option<NaiveDateTime> {
    chrono::DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|datetime| datetime.with_timezone(&Warsaw).naive_local())
}

fn is_stale(
    start: Option<&str>,
    end: Option<&str>,
    scheduled_date: Option<NaiveDate>,
    published_at: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> bool {
    let parse_datetime =
        |value: &str| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok();
    if let Some(end) = end.and_then(parse_datetime) {
        return end < now;
    }
    if let Some(start) = start.and_then(parse_datetime) {
        return start < now - Duration::hours(SCHEDULED_START_RETENTION_HOURS);
    }
    if let Some(scheduled_date) = scheduled_date {
        return scheduled_date < (now - Duration::hours(SCHEDULED_START_RETENTION_HOURS)).date();
    }
    if let Some(published_at) = published_at {
        return published_at < now - Duration::hours(UNKNOWN_END_RETENTION_HOURS);
    }
    true
}

fn stable_alert_hash(identity: &str, locality: &ServiceLocality) -> String {
    let mut digest = Sha256::new();
    digest.update("pwik_olsztyn:");
    digest.update(identity);
    digest.update(":");
    digest.update(locality.city_id.to_string());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_rss(
    xml: &str,
    settings: &Settings,
    now: NaiveDateTime,
) -> Result<Vec<UnifiedAlert>, String> {
    let rss: RssFeed = quick_xml::de::from_str(xml).map_err(|error| error.to_string())?;
    let active_addresses: Vec<_> = settings
        .addresses
        .iter()
        .enumerate()
        .filter(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::PwikOlsztyn,
                    address,
                )
        })
        .collect();
    let mut alerts = Vec::new();

    for item in rss.channel.items {
        let title = html_to_text(&item.title);
        let body = if !item.content.trim().is_empty() {
            html_to_text(&item.content)
        } else if !has_truncation_marker(&html_to_text(&item.description)) {
            html_to_text(&item.description)
        } else {
            continue;
        };
        let Some(kind) = notice_kind(&title, &body) else {
            continue;
        };
        let full_text = [title.trim(), body.trim()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let (start_date, end_date) = extract_schedule(&full_text);
        let scheduled_date = latest_event_date(&full_text);
        let published_at = parse_publication_time(&item.pub_date);
        if is_stale(
            start_date.as_deref(),
            end_date.as_deref(),
            scheduled_date,
            published_at,
            now,
        ) {
            continue;
        }

        let identity = if !item.guid.trim().is_empty() {
            item.guid.trim()
        } else if !item.link.trim().is_empty() {
            item.link.trim()
        } else {
            &full_text
        };

        for group in affected_localities(&full_text) {
            let group_location = group.address_lines.join("; ");
            let address_index = active_addresses
                .iter()
                .find(|(_, address)| address_matches_group(address, &group))
                .map(|(index, _)| *index);
            let location = if address_index.is_some() && !group_location.is_empty() {
                format!("Miejscowość: {}; {}", group.locality.name, group_location)
            } else {
                format!("Miejscowość: {}", group.locality.name)
            };

            alerts.push(UnifiedAlert {
                source: AlertSource::PwikOlsztyn,
                startDate: start_date.clone(),
                endDate: end_date.clone(),
                message: Some(format!("{}: {}", kind.label(), full_text)),
                location: Some(location),
                address_index,
                is_local: Some(address_index.is_some()),
                hash: Some(stable_alert_hash(identity, group.locality)),
            });
        }
    }

    Ok(alerts)
}

async fn fetch_feed(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.text().await.map_err(|error| error.to_string())
}

#[async_trait]
impl AlertProvider for PwikOlsztynProvider {
    fn id(&self) -> String {
        "pwik_olsztyn".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PwikOlsztyn
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

        let (emergency, planned) = tokio::join!(
            fetch_feed(client, PWIK_OLSZTYN_EMERGENCY_FEED_URL),
            fetch_feed(client, PWIK_OLSZTYN_PLANNED_FEED_URL),
        );
        let now = Utc::now().with_timezone(&Warsaw).naive_local();
        let mut alerts = Vec::new();
        let mut errors = Vec::new();

        for (label, result) in [("awarie", emergency), ("planowane przerwy", planned)] {
            match result {
                Ok(xml) => match parse_rss(&xml, settings, now) {
                    Ok(feed_alerts) => alerts.extend(feed_alerts),
                    Err(error) => {
                        errors.push(format!("PWiK Olsztyn {label} RSS parse error: {error}"))
                    }
                },
                Err(error) => errors.push(format!("PWiK Olsztyn {label} request error: {error}")),
            }
        }

        let mut seen = std::collections::HashSet::new();
        alerts.retain(|alert| {
            alert
                .hash
                .as_ref()
                .is_some_and(|hash| seen.insert(hash.clone()))
        });
        (alerts, errors)
    }
}
