use crate::api_logic::{self, AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::sync::OnceLock;

pub const OPEC_GDYNIA_URL: &str = "https://opecgdy.com.pl/wp-json/wp/v2/posts?categories=8&per_page=100&orderby=date&order=desc&_fields=id,date,modified,title,content,categories";
const OPEC_CATEGORY_ID: u64 = 8;

pub struct OpecGdyniaProvider;

#[derive(Debug, Deserialize)]
struct RenderedField {
    rendered: String,
}

#[derive(Debug, Deserialize)]
struct OpecPost {
    id: u64,
    #[allow(dead_code)]
    date: String,
    #[allow(dead_code)]
    modified: String,
    title: RenderedField,
    content: RenderedField,
    categories: Vec<u64>,
}

struct ParsedSchedule {
    start_date: String,
    end_date: Option<String>,
    start: Option<NaiveDateTime>,
    end: Option<NaiveDateTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Locality {
    Gdynia,
    Rumia,
    Reda,
    Kosakowo,
    Wejherowo,
}

impl Locality {
    fn label(self) -> &'static str {
        match self {
            Self::Gdynia => "Gdynia",
            Self::Rumia => "Rumia",
            Self::Reda => "Reda",
            Self::Kosakowo => "Kosakowo",
            Self::Wejherowo => "Wejherowo",
        }
    }

    fn stem(self) -> &'static str {
        match self {
            Self::Gdynia => "gdyn",
            Self::Rumia => "rumi",
            Self::Reda => "red",
            Self::Kosakowo => "kosakow",
            Self::Wejherowo => "wejherow",
        }
    }

    fn matches_address(self, address: &AddressEntry) -> bool {
        let city = crate::utils::clean_text(&address.city_name);
        let commune = crate::utils::clean_text(&address.commune);
        match self {
            Self::Kosakowo => city.starts_with(self.stem()) || commune.starts_with(self.stem()),
            _ => city.starts_with(self.stem()),
        }
    }
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})[.:](\d{2})\b").unwrap())
}

fn parse_date(captures: &regex::Captures<'_>) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn parse_time(captures: &regex::Captures<'_>) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        0,
    )
}

fn is_update_date(text: &str, date_start: usize) -> bool {
    let prefix = crate::utils::clean_text(&text[..date_start]);
    let prefix = prefix.trim_end();
    prefix.ends_with("aktualizacja z dnia") || prefix.ends_with("aktualizacja dnia")
}

fn date_is_schedule_start(text: &str, date_start: usize) -> bool {
    let prefix = crate::utils::clean_text(&text[..date_start]);
    let prefix = prefix.trim_end();
    prefix.ends_with("w dniu")
        || prefix.ends_with("od")
        || prefix.ends_with("od dnia")
        || prefix.ends_with("w terminie")
}

fn parse_schedule(text: &str) -> Option<ParsedSchedule> {
    let date_matches: Vec<_> = date_regex()
        .captures_iter(text)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            if is_update_date(text, whole.start()) {
                return None;
            }
            Some((whole.start(), whole.end(), parse_date(&captures)?))
        })
        .collect();

    let (start_date_index, start_date) = date_matches
        .iter()
        .enumerate()
        .find(|(_, (start, _, _))| date_is_schedule_start(text, *start))
        .map(|(index, (_, _, date))| (index, *date))
        .or_else(|| date_matches.first().map(|(_, _, date)| (0, *date)))?;
    let (_, start_date_end, _) = date_matches.get(start_date_index)?;
    let remaining = &text[*start_date_end..];
    let without_dates = date_regex().replace_all(remaining, " ");
    let times: Vec<NaiveTime> = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|captures| parse_time(&captures))
        .collect();

    let start_time = times.first().copied();
    let start = start_time.map(|time| start_date.and_time(time));
    let mut end = times.get(1).copied().map(|time| {
        let explicit_end_date = date_matches
            .iter()
            .skip(start_date_index + 1)
            .find(|(_, _, date)| *date >= start_date)
            .map(|(_, _, date)| *date)
            .unwrap_or(start_date);
        explicit_end_date.and_time(time)
    });

    if let (Some(start), Some(end_value)) = (start, end)
        && end_value <= start
        && date_matches
            .iter()
            .skip(start_date_index + 1)
            .all(|(_, _, date)| *date == start_date)
    {
        end = Some(end_value + Duration::days(1));
    }

    let start_string = start
        .map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_else(|| start_date.format("%Y-%m-%d").to_string());
    let end_string = end.map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string());
    Some(ParsedSchedule {
        start_date: start_string,
        end_date: end_string,
        start,
        end,
    })
}

fn content_parts(content_html: &str) -> (String, Vec<String>) {
    let fragment = Html::parse_fragment(content_html);
    let block_selector = Selector::parse("p, li").unwrap();
    let list_selector = Selector::parse("li").unwrap();
    let mut parts = Vec::new();
    for element in fragment.select(&block_selector) {
        let value = normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "));
        if !value.is_empty() && parts.last() != Some(&value) {
            parts.push(value);
        }
    }
    if parts.is_empty() {
        parts.push(normalize_whitespace(
            &fragment.root_element().text().collect::<Vec<_>>().join(" "),
        ));
    }

    let streets = fragment
        .select(&list_selector)
        .map(|element| normalize_whitespace(&element.text().collect::<Vec<_>>().join(" ")))
        .filter(|value| !value.is_empty())
        .collect();
    (parts.join("\n"), streets)
}

fn mentioned_localities(text: &str) -> Vec<Locality> {
    let normalized = crate::utils::clean_text(text);
    [
        Locality::Gdynia,
        Locality::Kosakowo,
        Locality::Reda,
        Locality::Rumia,
        Locality::Wejherowo,
    ]
    .into_iter()
    .filter(|locality| {
        normalized
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| word.starts_with(locality.stem()))
    })
    .collect()
}

fn contains_word_sequence(text: &str, phrase: &str) -> bool {
    let text: Vec<String> = crate::utils::clean_text(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    let phrase: Vec<String> = crate::utils::clean_text(phrase)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    !phrase.is_empty()
        && text
            .windows(phrase.len())
            .any(|candidate| candidate == phrase.as_slice())
}

fn address_matches_notice(
    address: &AddressEntry,
    localities: &[Locality],
    streets_text: &str,
) -> bool {
    localities
        .iter()
        .any(|locality| locality.matches_address(address))
        && crate::utils::configured_street_name_candidates(
            &address.street_name,
            &address.street_name_1,
            address.street_name_2.as_deref(),
        )
        .iter()
        .any(|street| contains_word_sequence(streets_text, street))
}

fn location_street_entry(value: &str) -> String {
    let normalized = crate::utils::clean_text(value);
    if [
        "ul.", "ul ", "ulica ", "al.", "al ", "aleja ", "pl.", "pl ", "plac ",
    ]
    .iter()
    .any(|prefix| normalized.trim_start().starts_with(prefix))
    {
        value.to_string()
    } else {
        format!("ul. {value}")
    }
}

fn parse_post(
    post: &OpecPost,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Option<UnifiedAlert> {
    if !post.categories.contains(&OPEC_CATEGORY_ID) {
        return None;
    }

    let (full_text, listed_streets) = content_parts(&post.content.rendered);
    let title = normalize_whitespace(&post.title.rendered);
    let text = if title.is_empty() {
        full_text.clone()
    } else {
        format!("{title}\n{full_text}")
    };
    let normalized = crate::utils::clean_text(&text);
    let is_heat_interruption = normalized.contains("przerwa")
        && (normalized.contains("ciepl") || normalized.contains("ogrzew"))
        || (normalized.contains("awari") && normalized.contains("ciepl"));
    if !is_heat_interruption {
        return None;
    }

    let localities = mentioned_localities(&text);
    if localities.is_empty() {
        return None;
    }
    let ParsedSchedule {
        start_date,
        end_date,
        start,
        end,
    } = parse_schedule(&text)?;
    if end.is_some_and(|end| end <= now_local)
        || (end.is_none()
            && start.is_none()
            && start_date < now_local.date().format("%Y-%m-%d").to_string())
        || (end.is_none() && start.is_some_and(|start| start.date() < now_local.date()))
    {
        return None;
    }

    let streets_text = if listed_streets.is_empty() {
        full_text.clone()
    } else {
        listed_streets.join("; ")
    };
    let matching_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && api_logic::is_address_applicable_for_provider(&AlertSource::OpecGdynia, address)
                && address_matches_notice(address, &localities, &streets_text)
        })
        .map(|(index, _)| index);

    let locality_label = localities
        .iter()
        .map(|locality| locality.label())
        .collect::<Vec<_>>()
        .join(", ");
    let location = if matching_address.is_some() {
        let streets = if listed_streets.is_empty() {
            String::new()
        } else {
            listed_streets
                .iter()
                .map(|street| location_street_entry(street))
                .collect::<Vec<_>>()
                .join("; ")
        };
        if streets.is_empty() {
            format!("Miejscowość: {locality_label}")
        } else {
            format!("Miejscowość: {locality_label}; {streets}")
        }
    } else {
        format!("Miejscowość: {locality_label}")
    };

    let incident_type = if normalized.contains("awari") {
        "Awaria ciepła"
    } else {
        "Planowana przerwa w dostawie ciepła"
    };
    let message = format!("{incident_type}: {full_text}");
    let mut alert = UnifiedAlert {
        source: AlertSource::OpecGdynia,
        startDate: Some(start_date),
        endDate: end_date,
        message: Some(message),
        location: Some(location),
        address_index: matching_address,
        is_local: Some(matching_address.is_some()),
        hash: None,
    };
    alert.hash = Some(format!("opec_gdynia_{}_{}", post.id, alert.to_hash()));
    Some(alert)
}

#[cfg(test)]
fn parse_posts_json(
    payload: &str,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Result<Vec<UnifiedAlert>, String> {
    let posts: Vec<OpecPost> = serde_json::from_str(payload)
        .map_err(|error| format!("OPEC Gdynia response JSON error: {error}"))?;
    Ok(posts
        .iter()
        .filter_map(|post| parse_post(post, settings, now_local))
        .collect())
}

async fn fetch_posts_page(client: &Client, page: usize) -> Result<(Vec<OpecPost>, usize), String> {
    let response = client
        .get(format!("{OPEC_GDYNIA_URL}&page={page}"))
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
        .unwrap_or(1);
    let body = response.text().await.map_err(|error| error.to_string())?;
    let posts = serde_json::from_str(&body)
        .map_err(|error| format!("OPEC Gdynia response JSON error: {error}"))?;
    Ok((posts, total_pages.max(1)))
}

async fn fetch_posts(client: &Client) -> Result<Vec<OpecPost>, String> {
    let (mut posts, total_pages) =
        crate::utils::retry(|| async { fetch_posts_page(client, 1).await }, 3)
            .await
            .map_err(|error| error.to_string())?;

    for page in 2..=total_pages {
        let (mut page_posts, _) =
            crate::utils::retry(|| async { fetch_posts_page(client, page).await }, 3)
                .await
                .map_err(|error| error.to_string())?;
        posts.append(&mut page_posts);
    }
    Ok(posts)
}

#[async_trait]
impl AlertProvider for OpecGdyniaProvider {
    fn id(&self) -> String {
        "opec_gdynia".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::OpecGdynia
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }

        let posts = match fetch_posts(client).await {
            Ok(posts) => posts,
            Err(error) => {
                let message = format!("OPEC Gdynia fetch error: {error}");
                log::error!("{message}");
                return (Vec::new(), vec![message]);
            }
        };

        let now_local = Utc::now().with_timezone(&Warsaw).naive_local();
        let alerts = posts
            .iter()
            .filter_map(|post| parse_post(post, settings, now_local))
            .collect();
        (alerts, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_POST: &str = r#"[
      {
        "id": 6623,
        "date": "2026-10-20T12:17:30",
        "modified": "2026-10-20T12:44:11",
        "title": {"rendered": "INFORMACJA O PRZERWIE W DOSTAWIE CIEPŁA DLA POTRZEB CIEPŁEJ WODY UŻYTKOWEJ"},
        "content": {"rendered": "<p>W dniu 21.10.2026 od godz. 8.00 do 15.00</p><p>nastąpi przerwa w dostawie ciepła dla potrzeb ciepłej wody użytkowej do odbiorców z następujących ulic Gdyni:</p><ul><li>Unruga 90, 92;</li><li>Podgórska 1, 2, 2A, 3, 5, 7, 9, 11.</li></ul><p>Przerwa spowodowana jest koniecznością usunięcia awarii.</p>"},
        "categories": [8]
      }
    ]"#;

    fn address(city: &str, street: &str, house_no: &str) -> AddressEntry {
        AddressEntry {
            city_name: city.to_string(),
            voivodeship: "Pomorskie".to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            house_no: house_no.to_string(),
            is_active: true,
            ..Default::default()
        }
    }

    fn date_time(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn parses_live_post_schedule_streets_and_cause() {
        let mut settings = Settings::default();
        settings
            .addresses
            .push(address("Gdynia", "Podgórska", "2A"));
        let alerts =
            parse_posts_json(LIVE_POST, &settings, date_time("2026-10-20 12:00:00")).unwrap();

        assert_eq!(alerts.len(), 1);
        let alert = &alerts[0];
        assert_eq!(alert.source, AlertSource::OpecGdynia);
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-21T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-21T15:00:00"));
        assert_eq!(alert.address_index, Some(0));
        assert_eq!(alert.is_local, Some(true));
        assert!(alert.message.as_deref().unwrap().contains("Unruga 90, 92"));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("usunięcia awarii")
        );
        assert!(
            alert
                .location
                .as_deref()
                .unwrap()
                .contains("ul. Unruga 90, 92")
        );
        assert!(
            alert
                .location
                .as_deref()
                .unwrap()
                .contains("ul. Podgórska 1, 2, 2A")
        );
    }

    #[test]
    fn honors_street_and_locality_and_keeps_other_notice_locality_only() {
        let mut settings = Settings::default();
        settings
            .addresses
            .push(address("Gdynia", "Podgórska", "99"));
        let alerts =
            parse_posts_json(LIVE_POST, &settings, date_time("2026-10-20 12:00:00")).unwrap();

        assert_eq!(alerts[0].is_local, Some(true));
        assert_eq!(alerts[0].address_index, Some(0));

        settings.addresses[0].city_name = "Sopot".to_string();
        settings.addresses[0].city_id = None;
        let alerts =
            parse_posts_json(LIVE_POST, &settings, date_time("2026-10-20 12:00:00")).unwrap();
        assert_eq!(alerts[0].is_local, Some(false));
        assert_eq!(alerts[0].location.as_deref(), Some("Miejscowość: Gdynia"));
        assert!(alerts[0].message.as_deref().unwrap().contains("Podgórska"));
    }

    #[test]
    fn empty_and_informational_posts_produce_no_alerts() {
        let settings = Settings::default();
        assert!(
            parse_posts_json("[]", &settings, date_time("2026-10-20 12:00:00"))
                .unwrap()
                .is_empty()
        );
        let informational = r#"[{"id":1,"date":"2026-10-20T12:00:00","modified":"2026-10-20T12:00:00","title":{"rendered":"INFORMACJA O PRACY SYSTEMU"},"content":{"rendered":"<p>Parametry pracy są poprawne, a system działa stabilnie.</p>"},"categories":[8]}]"#;
        assert!(
            parse_posts_json(informational, &settings, date_time("2026-10-20 12:00:00"))
                .unwrap()
                .is_empty()
        );
        let missing_schedule = LIVE_POST.replace(
            "W dniu 21.10.2026 od godz. 8.00 do 15.00",
            "Informujemy o przerwie w dostawie ciepła",
        );
        assert!(
            parse_posts_json(
                &missing_schedule,
                &settings,
                date_time("2026-10-20 12:00:00")
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn expired_and_wrong_category_posts_are_ignored() {
        let settings = Settings::default();
        let expired = LIVE_POST.replace("21.10.2026", "19.10.2026");
        assert!(
            parse_posts_json(&expired, &settings, date_time("2026-10-20 12:00:00"))
                .unwrap()
                .is_empty()
        );
        let other_category = LIVE_POST.replace("\"categories\": [8]", "\"categories\": [6]");
        assert!(
            parse_posts_json(&other_category, &settings, date_time("2026-10-20 12:00:00"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn parses_multiple_notices_and_hashes_change_only_with_notice_content() {
        let mut settings = Settings::default();
        settings
            .addresses
            .push(address("Gdynia", "Podgórska", "2A"));
        let mut posts: Vec<serde_json::Value> = serde_json::from_str(LIVE_POST).unwrap();
        let mut second = posts[0].clone();
        second["id"] = serde_json::json!(6624);
        second["content"]["rendered"] = serde_json::Value::String(
            second["content"]["rendered"]
                .as_str()
                .unwrap()
                .replace("21.10.2026", "22.10.2026")
                .replace("08.00", "09.00")
                .replace("8.00", "9.00"),
        );
        posts.push(second);
        let payload = serde_json::to_string(&posts).unwrap();
        let now = date_time("2026-10-20 12:00:00");
        let alerts = parse_posts_json(&payload, &settings, now).unwrap();
        let repeated = parse_posts_json(&payload, &settings, now).unwrap();

        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].hash, repeated[0].hash);
        assert_ne!(alerts[0].hash, alerts[1].hash);
    }

    #[test]
    fn parses_dotted_times_and_leaves_unknown_end_unset() {
        let one_time = LIVE_POST.replace(" do 15.00", "");
        let mut settings = Settings::default();
        settings.addresses.push(address("Gdynia", "Unruga", "90"));
        let alerts =
            parse_posts_json(&one_time, &settings, date_time("2026-10-20 12:00:00")).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].startDate.as_deref(), Some("2026-10-21T08:00:00"));
        assert_eq!(alerts[0].endDate, None);
    }
}
