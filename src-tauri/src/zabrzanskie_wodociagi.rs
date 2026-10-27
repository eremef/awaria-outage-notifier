use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use futures::{StreamExt, stream};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const ZABRZANSKIE_WODOCIAGI_XML_URL: &str = "https://wodociagi.zabrze.pl/artykuly/xml/401/1/1";
const MAX_ARTICLES_PER_REFRESH: usize = 24;
const ARTICLE_CONCURRENCY: usize = 4;
const UNKNOWN_END_RETENTION_HOURS: i64 = 72;
const OFFICIAL_HOST: &str = "wodociagi.zabrze.pl";

pub struct ZabrzanskieWodociagiProvider;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FeedItem {
    title: String,
    link: String,
    published: String,
    summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NoticeKind {
    Failure,
    Planned,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Failure => "Awaria wodociągowa",
            Self::Planned => "Prace planowe na sieci wodociągowej",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Schedule {
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize(value: &str) -> String {
    crate::utils::clean_text(value)
}

fn node_text(element: ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

#[derive(Debug, Default)]
struct XmlNode {
    name: String,
    attributes: HashMap<String, String>,
    text: String,
    children: Vec<XmlNode>,
}

fn xml_node_from_start(start: &quick_xml::events::BytesStart<'_>) -> Result<XmlNode, String> {
    let name = start.local_name().as_ref().to_ascii_lowercase();
    let mut node = XmlNode {
        name,
        ..XmlNode::default()
    };
    for attribute in start.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| error.to_string())?;
        let key = attribute.key.local_name().as_ref().to_ascii_lowercase();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?
            .into_owned();
        node.attributes.insert(key, value);
    }
    Ok(node)
}

fn xml_node_text(node: &XmlNode) -> String {
    let mut parts = Vec::new();
    if !node.text.trim().is_empty() {
        parts.push(node.text.trim().to_string());
    }
    parts.extend(
        node.children
            .iter()
            .map(xml_node_text)
            .filter(|text| !text.is_empty()),
    );
    normalize_whitespace(&parts.join(" "))
}

fn find_xml_field<'a>(node: &'a XmlNode, names: &[&str]) -> Option<&'a XmlNode> {
    node.children.iter().find_map(|child| {
        if names.contains(&child.name.as_str()) {
            Some(child)
        } else {
            find_xml_field(child, names)
        }
    })
}

fn xml_field_value(node: &XmlNode, names: &[&str], attribute_names: &[&str]) -> String {
    find_xml_field(node, names)
        .map(xml_node_text)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            attribute_names
                .iter()
                .find_map(|name| node.attributes.get(*name).map(String::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn collect_feed_nodes<'a>(node: &'a XmlNode, names: &[&str], result: &mut Vec<&'a XmlNode>) {
    if names.contains(&node.name.as_str()) {
        result.push(node);
        return;
    }
    for child in &node.children {
        collect_feed_nodes(child, names, result);
    }
}

fn parse_feed(xml: &str) -> Result<Vec<FeedItem>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<XmlNode> = Vec::new();
    let mut roots = Vec::new();
    loop {
        match reader.read_event().map_err(|error| error.to_string())? {
            Event::Start(start) => stack.push(xml_node_from_start(&start)?),
            Event::Empty(start) => {
                let node = xml_node_from_start(&start)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            Event::Text(text) => {
                let content = text.xml_content(XmlVersion::Implicit1_0);
                let decoded = quick_xml::escape::unescape(content.as_ref())
                    .map_err(|error| error.to_string())?;
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(decoded.as_ref());
                }
            }
            Event::CData(text) => {
                if let Some(node) = stack.last_mut() {
                    node.text
                        .push_str(text.xml_content(XmlVersion::Implicit1_0).as_ref());
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(node) = stack.last_mut() {
                    if let Some(character) = reference
                        .resolve_char_ref()
                        .map_err(|error| error.to_string())?
                    {
                        node.text.push(character);
                    } else {
                        let replacement = match reference.as_ref() {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "apos" => "'",
                            "quot" => "\"",
                            _ => "",
                        };
                        node.text.push_str(replacement);
                    }
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| "unexpected XML closing tag".to_string())?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() || roots.is_empty() {
        return Err("ZPWiK Zabrze category XML is malformed or has no root element".to_string());
    }
    let item_names = [
        "item",
        "entry",
        "artykul",
        "article",
        "record",
        "rekord",
        "news",
        "komunikat",
    ];
    let mut records = Vec::new();
    for root in &roots {
        collect_feed_nodes(root, &item_names, &mut records);
    }
    let mut items = Vec::new();
    for node in records {
        let title = xml_field_value(
            node,
            &["title", "tytul", "nazwa", "headline"],
            &["title", "tytul", "nazwa", "headline"],
        );
        let link = xml_field_value(
            node,
            &["link", "url", "adres", "href"],
            &["link", "url", "adres", "href"],
        );
        let published = xml_field_value(
            node,
            &[
                "pubdate",
                "published",
                "date",
                "data",
                "data_publikacji",
                "data-dodania",
                "datetime",
                "modified",
                "created",
            ],
            &[
                "pubdate",
                "published",
                "date",
                "data",
                "datetime",
                "modified",
            ],
        );
        let summary = xml_field_value(
            node,
            &[
                "summary",
                "description",
                "opis",
                "skrot",
                "streszczenie",
                "zajawka",
                "lead",
                "tresc",
                "teaser",
            ],
            &["summary", "description", "opis", "skrot", "streszczenie"],
        );
        if !title.is_empty() || !link.is_empty() {
            items.push(FeedItem {
                title,
                link,
                published,
                summary,
            });
        }
    }
    Ok(items)
}

fn is_feed_candidate(item: &FeedItem) -> bool {
    let text = normalize(&format!("{} {}", item.title, item.summary));
    [
        "awaria",
        "wodociag",
        "dostawie wody",
        "przerwa w dostawie",
        "prace na sieci",
    ]
    .iter()
    .any(|term| text.contains(term))
}

fn safe_article_url(raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let base = Url::parse("https://wodociagi.zabrze.pl/").ok()?;
    let url = base.join(raw.trim()).ok()?;
    (url.scheme() == "https" && url.host_str() == Some(OFFICIAL_HOST)).then(|| url.to_string())
}

async fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    response.text().await.map_err(|error| error.to_string())
}

fn page_text(html: &str) -> String {
    let document = Html::parse_document(html);
    let main_selector =
        Selector::parse("main, article, .article-content, .artykul-tresc, #content").unwrap();
    let content = document.select(&main_selector).next().or_else(|| {
        Selector::parse("body")
            .ok()
            .and_then(|selector| document.select(&selector).next())
    });
    content.map(node_text).unwrap_or_else(|| {
        normalize_whitespace(&document.root_element().text().collect::<Vec<_>>().join(" "))
    })
}

fn is_completed_notice(text: &str) -> bool {
    let text = normalize(text);
    [
        "awaria zostala usunieta",
        "awaria zostala zlikwidowana",
        "awaria usunieta",
        "awaria zlikwidowana",
        "dostawa wody zostala przywrocona",
        "woda zostala przywrocona",
        "przywrocono dostawe wody",
        "przywrocono dostawy wody",
        "przerwa zostala odwolana",
        "przerwa zostaje odwolana",
        "przerwa odwolana",
        "nie bedzie przerwy w dostawie wody",
        "nie wystapi przerwa w dostawie wody",
        "nie wystapia zaklocenia w dostawie wody",
        "nie bedzie przerwy",
        "nie beda wystepowac zaklocenia",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn has_water_service_impact(text: &str) -> bool {
    let text = normalize(text);
    [
        "przerwa w dostawie wody",
        "przerwy w dostawie wody",
        "zaklocenia w dostawie wody",
        "zaklocenia w dostawach wody",
        "zaklocen w dostawie wody",
        "braki w dostawie wody",
        "brak wody",
        "braki wody",
        "wylaczenie wody",
        "wylaczenia wody",
        "wstrzymanie dostawy wody",
        "dostawa wody bedzie wstrzymana",
        "awaria na sieci wodociagowej",
        "awarii na sieci wodociagowej",
        "awaria wodociagowa",
        "spadek cisnienia wody",
        "obnizenie cisnienia wody",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn classify_notice(title: &str, body: &str) -> Option<NoticeKind> {
    let combined = format!("{title} {body}");
    if is_completed_notice(&combined) || !has_water_service_impact(&combined) {
        return None;
    }
    let normalized = normalize(&combined);
    if [
        "prace na sieci",
        "planowan",
        "planowana przerwa",
        "roboty na sieci",
    ]
    .iter()
    .any(|term| normalized.contains(term))
    {
        Some(NoticeKind::Planned)
    } else if normalized.contains("awaria") {
        Some(NoticeKind::Failure)
    } else {
        None
    }
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})[./-](\d{1,2})[./-](\d{2,4})(?:\s*r\.?)?\b").unwrap())
}

fn named_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(\d{1,2})\s+(stycznia|lutego|marca|kwietnia|maja|czerwca|lipca|sierpnia|września|wrzesnia|października|pazdziernika|listopada|grudnia)\s+(\d{4})\b").unwrap()
    })
}

fn overnight_date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})\.(\d{1,2})\.(\d{4})(?:r\.?)?").unwrap())
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(\d{1,2})[:.](\d{2})\s*(?:-|–|—|do)\s*(?:godz\.?\s*)?(\d{1,2})[:.](\d{2})")
            .unwrap()
    })
}

fn single_time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(\d{1,2})[:.](\d{2})").unwrap())
}

fn expand_year(year: u32) -> i32 {
    if year < 100 {
        2000 + year as i32
    } else {
        year as i32
    }
}

fn parse_date(day: &str, month: &str, year: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        expand_year(year.parse().ok()?),
        month.parse().ok()?,
        day.parse().ok()?,
    )
}

fn parse_polish_month(month: &str) -> Option<u32> {
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

fn parse_time(hour: &str, minute: &str) -> Option<NaiveTime> {
    let hour: u32 = hour.parse().ok()?;
    let minute: u32 = minute.parse().ok()?;
    if hour == 24 && minute == 0 {
        NaiveTime::from_hms_opt(0, 0, 0)
    } else {
        NaiveTime::from_hms_opt(hour, minute, 0)
    }
}

fn parse_publication_date(value: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Warsaw).date_naive())
        .or_else(|| {
            DateTime::parse_from_rfc2822(value)
                .ok()
                .map(|date| date.with_timezone(&Warsaw).date_naive())
        })
        .or_else(|| {
            NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
                .ok()
                .or_else(|| NaiveDate::parse_from_str(value.trim(), "%d.%m.%Y").ok())
                .or_else(|| NaiveDate::parse_from_str(value.trim(), "%d.%m.%y").ok())
                .or_else(|| {
                    date_regex().captures(value).and_then(|captures| {
                        parse_date(
                            captures.get(1)?.as_str(),
                            captures.get(2)?.as_str(),
                            captures.get(3)?.as_str(),
                        )
                    })
                })
        })
}

fn parse_schedule(text: &str, published: Option<NaiveDate>) -> Schedule {
    let overnight = overnight_date_regex().captures(text).and_then(|captures| {
        Some((
            parse_date(
                captures.get(1)?.as_str(),
                captures.get(3)?.as_str(),
                captures.get(4)?.as_str(),
            )?,
            parse_date(
                captures.get(2)?.as_str(),
                captures.get(3)?.as_str(),
                captures.get(4)?.as_str(),
            )?,
        ))
    });
    let mut dates: Vec<NaiveDate> = date_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            parse_date(
                captures.get(1)?.as_str(),
                captures.get(2)?.as_str(),
                captures.get(3)?.as_str(),
            )
        })
        .collect();
    dates.extend(
        named_date_regex()
            .captures_iter(text)
            .filter_map(|captures| {
                let day = captures.get(1)?.as_str().parse().ok()?;
                let month = parse_polish_month(captures.get(2)?.as_str())?;
                let year = captures.get(3)?.as_str().parse().ok()?;
                NaiveDate::from_ymd_opt(year, month, day)
            }),
    );
    if let Some((first, second)) = overnight {
        dates.insert(0, first);
        if second != first {
            dates.insert(1, second);
        }
    }
    dates.dedup();
    let event_date = dates
        .iter()
        .copied()
        .find(|date| Some(*date) != published)
        .or_else(|| dates.first().copied());
    let Some(date) = event_date else {
        return Schedule {
            start: None,
            end: None,
        };
    };

    if let Some(captures) = time_range_regex().captures(text) {
        let start_time = captures.get(1).and_then(|hour| {
            captures
                .get(2)
                .and_then(|minute| parse_time(hour.as_str(), minute.as_str()))
        });
        let end_time = captures.get(3).and_then(|hour| {
            captures
                .get(4)
                .and_then(|minute| parse_time(hour.as_str(), minute.as_str()))
        });
        if let (Some(start_time), Some(end_time)) = (start_time, end_time) {
            let start = date.and_time(start_time);
            let mut end_date = if dates.len() > 1 && dates[0] == date {
                dates[1]
            } else {
                date
            };
            let mut end = end_date.and_time(end_time);
            if end <= start {
                end_date = end_date.succ_opt().unwrap_or(end_date);
                end = end_date.and_time(end_time);
            }
            return Schedule {
                start: Some(start),
                end: Some(end),
            };
        }
    }

    let times: Vec<_> = single_time_regex()
        .captures_iter(text)
        .filter_map(|captures| parse_time(captures.get(1)?.as_str(), captures.get(2)?.as_str()))
        .collect();
    let normalized_text = normalize(text);
    if times.len() == 1
        && (normalized_text.contains("do godz") || normalized_text.contains("do g."))
        && !normalized_text.contains("od godz")
        && !normalized_text.contains("od g.")
    {
        return Schedule {
            start: None,
            end: Some(date.and_time(times[0])),
        };
    }
    let start = times.first().copied().map(|time| date.and_time(time));
    let end = times.get(1).copied().map(|time| date.and_time(time));
    Schedule { start, end }
}

fn schedule_stale(schedule: &Schedule, published: Option<NaiveDate>, now: NaiveDateTime) -> bool {
    if let Some(end) = schedule.end {
        return end < now;
    }
    if let Some(start) = schedule.start {
        return start + Duration::hours(UNKNOWN_END_RETENTION_HOURS) < now;
    }
    published.is_some_and(|date| {
        date.and_hms_opt(23, 59, 59)
            .is_some_and(|published| published + Duration::hours(UNKNOWN_END_RETENTION_HOURS) < now)
    })
}

fn extract_location_detail(text: &str) -> String {
    let normalized = normalize(text);
    let preferred_starts = [
        "obejmujace ulice:",
        "obejmujace ulice",
        "obejmujaca ulice:",
        "przy ulicach:",
    ];
    let fallback_starts = ["przy ulicy", "przy ul.", "w rejonie ul.", "w rejonie ulicy"];
    let start = preferred_starts
        .iter()
        .filter_map(|needle| normalized.find(needle))
        .min()
        .or_else(|| {
            fallback_starts
                .iter()
                .filter_map(|needle| normalized.find(needle))
                .min()
        });
    let Some(start) = start else {
        return normalize_whitespace(text);
    };
    let start_original = text
        .char_indices()
        .find(|(index, _)| normalize(&text[..*index]).len() >= start)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let remainder = &text[start_original..];
    let lower = remainder.to_lowercase();
    let stops = [
        "czas wyłączenia",
        "czas wylaczenia",
        "przepraszamy",
        "za utrudnienia",
        "bieżących informacji",
        "biezacych informacji",
    ];
    let end = stops
        .iter()
        .filter_map(|stop| lower.find(stop))
        .min()
        .unwrap_or(remainder.len());
    normalize_whitespace(&remainder[..end])
        .trim_matches([' ', ':', '-', '–'])
        .to_string()
}

fn address_is_zabrze(address: &AddressEntry) -> bool {
    crate::api_logic::is_address_applicable_for_provider(
        &AlertSource::ZabrzanskieWodociagi,
        address,
    )
}

pub(crate) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    if !address_is_zabrze(address) || !normalize(location).contains("zabrze") {
        return false;
    }
    let street_candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    if street_candidates.is_empty() {
        return true;
    }
    let notice = normalize(location);
    street_candidates.into_iter().any(|candidate| {
        let candidate = normalize(crate::utils::strip_street_prefixes(candidate.trim()));
        !candidate.is_empty() && notice.contains(&candidate)
    })
}

fn parse_article(item: &FeedItem, html: &str, now: NaiveDateTime) -> Option<UnifiedAlert> {
    let body = page_text(html);
    let kind = classify_notice(&item.title, &body)?;
    let published = parse_publication_date(&item.published);
    let schedule = parse_schedule(&body, published);
    if schedule_stale(&schedule, published, now) {
        return None;
    }
    let location_detail = extract_location_detail(&body);
    let location = format!("Zabrze; {location_detail}");
    let message = [item.title.trim(), body.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    item.link.hash(&mut hasher);
    kind.hash(&mut hasher);
    location_detail.hash(&mut hasher);
    schedule.start.hash(&mut hasher);
    schedule.end.hash(&mut hasher);
    Some(UnifiedAlert {
        source: AlertSource::ZabrzanskieWodociagi,
        startDate: schedule
            .start
            .map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string()),
        endDate: schedule
            .end
            .map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string()),
        message: Some(format!("{}: {}", kind.label(), message)),
        location: Some(location),
        address_index: None,
        is_local: None,
        hash: Some(format!("zabrzanskie_wodociagi_{:x}", hasher.finish())),
    })
}

async fn fetch_candidate(
    client: &Client,
    item: FeedItem,
    now: NaiveDateTime,
) -> (FeedItem, Option<UnifiedAlert>, Option<String>) {
    let Some(url) = safe_article_url(&item.link) else {
        return (
            item,
            None,
            Some("article URL is missing or outside the official host".to_string()),
        );
    };
    match fetch_text(client, &url).await {
        Ok(html) => {
            let alert = parse_article(&item, &html, now);
            (item, alert, None)
        }
        Err(error) => (item, None, Some(error)),
    }
}

#[async_trait]
impl AlertProvider for ZabrzanskieWodociagiProvider {
    fn id(&self) -> String {
        "zabrzanskie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::ZabrzanskieWodociagi
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
        let xml = match crate::utils::retry(|| fetch_text(client, ZABRZANSKIE_WODOCIAGI_XML_URL), 2)
            .await
        {
            Ok(xml) => xml,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("ZPWiK Zabrze XML fetch error: {error}")],
                );
            }
        };
        let items = match parse_feed(&xml) {
            Ok(items) => items,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("ZPWiK Zabrze XML parse error: {error}")],
                );
            }
        };
        let mut candidates: Vec<_> = items.into_iter().filter(is_feed_candidate).collect();
        let mut warnings = Vec::new();
        if candidates.len() > MAX_ARTICLES_PER_REFRESH {
            warnings.push(format!("ZPWiK Zabrze: limiting full article fetch to the first {MAX_ARTICLES_PER_REFRESH} recent outage candidates"));
            candidates.truncate(MAX_ARTICLES_PER_REFRESH);
        }
        let now = Utc::now().with_timezone(&Warsaw).naive_local();
        let results = stream::iter(
            candidates
                .into_iter()
                .map(|item| fetch_candidate(client, item, now)),
        )
        .buffer_unordered(ARTICLE_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        let mut alerts = Vec::new();
        for (item, alert, error) in results {
            if let Some(error) = error {
                warnings.push(format!(
                    "ZPWiK Zabrze article error for {}: {error}",
                    item.link
                ));
            }
            if let Some(mut alert) = alert {
                let location = alert.location.as_deref().unwrap_or_default();
                let address_index = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .find(|(_, address)| {
                        address.is_active && location_matches_address(location, address)
                    })
                    .map(|(index, _)| index);
                alert.address_index = address_index;
                alert.is_local = Some(address_index.is_some());
                alerts.push(alert);
            }
        }
        (alerts, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML_WITH_PROCESSING_INSTRUCTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
      <?xml-stylesheet type="text/xsl" href="/xml.xsl"?>
      <articles>
        <item><title>Awaria</title><link>/artykuly/555/awaria</link><pubDate>27.10.2026</pubDate><description>awaria sieci wodociągowej przy ul. Śląskiej</description></item>
        <item><title>Postępowanie przetargowe</title><link>/artykuly/556/przetarg</link><pubDate>26.10.2026</pubDate><description>Zaproszenie do składania ofert na dostawę materiałów.</description></item>
      </articles>"#;

    const FAILURE_ARTICLE: &str = r#"
      <html><body><main>
        <h1>Awaria</h1>
        <p>Informujemy, że w związku z awarią na sieci wodociągowej przy ul. Śląskiej, w dniu 28.10.2026 r. od godz. 9:00 do godz. 12:00 mogą wystąpić zakłócenia w dostawie wody obejmujące ulice: Plac Warszawski, Targowa i Wandy. Czas wyłączenia odcinka sieci wodociągowej może ulec zmianie. Przepraszamy za utrudnienia.</p>
      </main></body></html>
    "#;

    #[test]
    fn parses_category_xml_with_processing_instruction_and_skips_unrelated_records() {
        let items = parse_feed(XML_WITH_PROCESSING_INSTRUCTION).unwrap();
        assert_eq!(items.len(), 2);
        assert!(is_feed_candidate(&items[0]));
        assert!(!is_feed_candidate(&items[1]));
    }

    #[test]
    fn rejects_malformed_xml_instead_of_reporting_an_empty_feed() {
        assert!(parse_feed("<?xml version='1.0'?><articles><item>").is_err());
    }

    #[test]
    fn parses_event_date_independently_from_publication_date_and_extracts_multiple_streets() {
        let item = FeedItem {
            title: "Awaria".to_string(),
            link: "https://wodociagi.zabrze.pl/artykuly/555/awaria".to_string(),
            published: "27.10.2026".to_string(),
            summary: "awaria na sieci".to_string(),
        };
        let now = NaiveDate::from_ymd_opt(2026, 10, 27)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let alert = parse_article(&item, FAILURE_ARTICLE, now).unwrap();
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-28T09:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-28T12:00:00"));
        assert!(
            alert
                .location
                .as_deref()
                .unwrap()
                .contains("Plac Warszawski")
        );
        assert!(alert.location.as_deref().unwrap().contains("Wandy"));
    }

    #[test]
    fn skips_restored_notices_and_unrelated_road_or_sewer_content() {
        assert!(classify_notice("Awaria", "Przywrócono dostawy wody przy ul. Wolności.").is_none());
        assert!(
            classify_notice(
                "Awaria kanalizacji",
                "Prace kanalizacyjne przy ul. Wolności."
            )
            .is_none()
        );
    }

    #[test]
    fn only_accepts_official_article_links() {
        assert!(safe_article_url("/artykuly/123/awaria").is_some());
        assert!(safe_article_url("https://example.com/article").is_none());
        assert!(safe_article_url("http://wodociagi.zabrze.pl/article").is_none());
    }
}
