use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

pub const TARNOWSKIE_WODOCIAGI_URL: &str = "https://www.tw.tarnow.pl/lista-awarii/";
pub struct TarnowskieWodociagiProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Notice {
    url: String,
    status: String,
    data_type: String,
    card_html: String,
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
            Self::Planned => "Planowana przerwa w dostawie wody",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedNotice {
    kind: NoticeKind,
    start: Option<String>,
    end: Option<String>,
    locality: String,
    location_detail: String,
    description: String,
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize(value: &str) -> String {
    crate::utils::clean_text(value)
}

fn date_time_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})-(\d{1,2})-(\d{4})\s+(\d{1,2}):(\d{2})\b").unwrap())
}

fn parse_date_time(value: &str) -> Option<NaiveDateTime> {
    date_time_regex().captures(value).and_then(|captures| {
        let date = NaiveDate::from_ymd_opt(
            captures.get(3)?.as_str().parse().ok()?,
            captures.get(2)?.as_str().parse().ok()?,
            captures.get(1)?.as_str().parse().ok()?,
        )?;
        date.and_hms_opt(
            captures.get(4)?.as_str().parse().ok()?,
            captures.get(5)?.as_str().parse().ok()?,
            0,
        )
    })
}

fn format_date_time(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn status_is_finished(value: &str) -> bool {
    let status = normalize(value);
    if status == "5" {
        // The provider's official map script renders status 5 with the same
        // grey marker used by its "Zakończone" legend entry.
        return true;
    }
    [
        "zakoncz",
        "zrealizowan",
        "zamkniet",
        "usuniet",
        "odwol",
        "completed",
        "closed",
        "finished",
    ]
    .iter()
    .any(|word| status.contains(word))
}

fn collect_notices(html: &str) -> Result<Vec<Notice>, String> {
    let document = Html::parse_document(html);
    let main_selector = Selector::parse("#main").map_err(|error| error.to_string())?;
    if document.select(&main_selector).next().is_none() {
        return Err("Tarnowskie Wodociągi: outage listing content was not found".to_string());
    }
    let wrapper_selector =
        Selector::parse(".report-list-white__elements").map_err(|error| error.to_string())?;
    let card_selector = Selector::parse(".report-list-white__elements--element")
        .map_err(|error| error.to_string())?;
    let marker_selector =
        Selector::parse(".map-event[data-url]").map_err(|error| error.to_string())?;
    let link_selector =
        Selector::parse("a[href*='szczegoly-awarii']").map_err(|error| error.to_string())?;
    let base = Url::parse(TARNOWSKIE_WODOCIAGI_URL).map_err(|error| error.to_string())?;
    let mut seen = std::collections::HashSet::new();
    let mut notices = Vec::new();

    for wrapper in document.select(&wrapper_selector) {
        let marker = wrapper.select(&marker_selector).next();
        let status = marker
            .and_then(|element| element.value().attr("data-status"))
            .unwrap_or_default()
            .to_string();
        let data_type = marker
            .and_then(|element| element.value().attr("data-type"))
            .unwrap_or_default()
            .to_string();
        let marker_url = marker.and_then(|element| element.value().attr("data-url"));

        for card in wrapper.select(&card_selector) {
            let raw_url = marker_url.or_else(|| {
                card.select(&link_selector)
                    .next()
                    .and_then(|element| element.value().attr("href"))
            });
            let url = raw_url
                .and_then(|raw| base.join(raw).ok())
                .filter(|url| {
                    ["www.tw.tarnow.pl", "tw.tarnow.pl", "strona.tw.tarnow.pl"]
                        .contains(&url.host_str().unwrap_or_default())
                        && url.path().contains("szczegoly-awarii")
                })
                .map(|url| url.to_string())
                .unwrap_or_default();
            let identity = if url.is_empty() {
                card.inner_html()
            } else {
                url.clone()
            };
            if seen.insert(identity) {
                notices.push(Notice {
                    url,
                    status: status.clone(),
                    data_type: data_type.clone(),
                    card_html: card.inner_html(),
                });
            }
        }
    }

    Ok(notices)
}

fn labeled_value(text: &str, label: &str, stop_labels: &[&str]) -> Option<String> {
    let normalized_text = normalize_whitespace(text);
    let lowercase = normalized_text.to_lowercase();
    let label_index = lowercase.find(&label.to_lowercase())?;
    let value_start = label_index + label.len();
    let rest = &normalized_text[value_start..];
    let lowercase_rest = &lowercase[value_start..];
    let stop_index = stop_labels
        .iter()
        .filter_map(|stop| lowercase_rest.find(&stop.to_lowercase()))
        .min()
        .unwrap_or(rest.len());
    let value = rest[..stop_index].to_string();
    let value = value.trim().trim_start_matches(':').trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn parse_kind(text: &str, data_type: &str) -> Option<NoticeKind> {
    let data_type = normalize(data_type);
    if data_type.contains("planned") || data_type.contains("planowan") {
        return Some(NoticeKind::Planned);
    }
    if data_type.contains("failure") || data_type.contains("awaria") || data_type.contains("fault")
    {
        return Some(NoticeKind::Failure);
    }
    let normalized = normalize(text);
    if normalized.contains("planowana przerwa") || normalized.contains("planowane wylaczenie") {
        Some(NoticeKind::Planned)
    } else if normalized.contains("awari") {
        Some(NoticeKind::Failure)
    } else {
        None
    }
}

fn parse_detail_html(html: &str, status: &str, data_type: &str) -> Option<ParsedNotice> {
    if status_is_finished(status) {
        return None;
    }
    let document = Html::parse_document(html);
    let body_selector = Selector::parse("body").ok()?;
    let text = document
        .select(&body_selector)
        .next()
        .map(|body| normalize_whitespace(&body.text().collect::<Vec<_>>().join(" ")))
        .unwrap_or_else(|| {
            normalize_whitespace(&document.root_element().text().collect::<Vec<_>>().join(" "))
        });
    let kind = parse_kind(&text, data_type)?;
    let location = labeled_value(&text, "Lokalizacja", &["Data przerwy", "Opis", "Szczegóły"])?;
    let schedule = labeled_value(&text, "Data przerwy", &["Opis", "Szczegóły"]).unwrap_or_default();
    let description = labeled_value(
        &text,
        "Opis",
        &["Szczegóły", "Pogotowie wodociągowo kanalizacyjne"],
    )
    .unwrap_or_default();
    let locality = location
        .strip_prefix("Miejscowość")
        .or_else(|| location.strip_prefix("Miejscowosc"))
        .unwrap_or(&location)
        .trim()
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();

    let start = parse_date_time(&schedule).or_else(|| parse_date_time(&text));
    let all_dates: Vec<_> = date_time_regex()
        .find_iter(&schedule)
        .filter_map(|capture| parse_date_time(capture.as_str()))
        .collect();
    let start = all_dates.first().copied().or(start);
    let end = all_dates.get(1).copied();

    Some(ParsedNotice {
        kind,
        start: start.map(format_date_time),
        end: end.map(format_date_time),
        locality,
        location_detail: location,
        description,
    })
}

fn locality_matches(notice: &str, address: &AddressEntry) -> bool {
    let normalized_notice = normalize(notice);
    [
        address.city_name.as_str(),
        address.commune.as_str(),
        address.district.as_str(),
    ]
    .iter()
    .filter(|value| !value.trim().is_empty())
    .any(|value| normalize(value) == normalized_notice)
}

pub(crate) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    let Some((locality, details)) = location.split_once(';') else {
        return false;
    };
    if !locality_matches(locality.trim(), address) {
        return false;
    }
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    if candidates.is_empty() {
        return true;
    }
    let details = normalize(details);
    candidates.into_iter().any(|candidate| {
        let candidate = normalize(crate::utils::strip_street_prefixes(candidate.trim()));
        !candidate.is_empty() && details.contains(&candidate)
    })
}

fn make_alert(parsed: ParsedNotice, notice_url: &str, settings: &Settings) -> UnifiedAlert {
    let location = format!("{}; {}", parsed.locality, parsed.location_detail);
    let address_index = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(
                    &AlertSource::TarnowskieWodociagi,
                    address,
                )
                && location_matches_address(&location, address)
        })
        .map(|(index, _)| index);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    notice_url.hash(&mut hasher);
    parsed.kind.hash(&mut hasher);
    parsed.start.hash(&mut hasher);
    parsed.end.hash(&mut hasher);
    parsed.description.hash(&mut hasher);

    UnifiedAlert {
        source: AlertSource::TarnowskieWodociagi,
        startDate: parsed.start,
        endDate: parsed.end,
        message: Some(format!("{}: {}", parsed.kind.label(), parsed.description)),
        location: Some(location),
        address_index,
        is_local: Some(address_index.is_some()),
        hash: Some(format!("tarnowskie_wodociagi_{:x}", hasher.finish())),
    }
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

#[async_trait]
impl AlertProvider for TarnowskieWodociagiProvider {
    fn id(&self) -> String {
        "tarnowskie_wodociagi".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::TarnowskieWodociagi
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
        let html =
            match crate::utils::retry(|| fetch_text(client, TARNOWSKIE_WODOCIAGI_URL), 2).await {
                Ok(html) => html,
                Err(error) => {
                    return (
                        Vec::new(),
                        vec![format!("Tarnowskie Wodociągi fetch error: {error}")],
                    );
                }
            };
        let notices = match collect_notices(&html) {
            Ok(notices) => notices,
            Err(error) => return (Vec::new(), vec![error]),
        };
        let mut alerts = Vec::new();
        let mut warnings = Vec::new();
        for notice in notices {
            if status_is_finished(&notice.status) {
                continue;
            }
            if let Some(parsed) =
                parse_detail_html(&notice.card_html, &notice.status, &notice.data_type)
            {
                alerts.push(make_alert(parsed, &notice.url, settings));
            } else {
                warnings.push(format!(
                    "Tarnowskie Wodociągi: could not parse active notice {}",
                    notice.url
                ));
            }
        }
        (alerts, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIVE_DETAIL: &str = r#"
      <html><body><main>
        <h1>Szczegóły awarii</h1>
        <p>Mapa prezentuje dokładną lokalizację aktualnie zgłoszonej awarii wodociągu.</p>
        <h2>Planowana przerwa</h2>
        <p>06-10-2026 08:00</p>
        <p>Lokalizacja: Miejscowość Tarnów, Krasińskiego</p>
        <p>Data przerwy: od 06-10-2026 08:00 do 06-10-2026 15:00</p>
        <p>Opis: Tarnowskie Wodociągi informują odbiorców, że z powodu prac nastąpi czasowa przerwa. Dotyczy ulic Krasińskiego i Słowackiego.</p>
      </main></body></html>
    "#;

    const LISTING_WITH_MAP_MARKERS: &str = r#"
      <main id="main">
        <div class="report-list-white__elements">
          <div class="map-event" data-url="https://www.tw.tarnow.pl/szczegoly-awarii/?id=96" data-status="2" data-type="planned"></div>
          <div class="report-list-white__elements--element">
            <div class="report-list-white__elements--element__title"><strong>Planowana przerwa</strong> 06-10-2026 08:00</div>
            <div class="report-list-white__elements--element__content">
              <p><strong>Lokalizacja:</strong><br>Miejscowość Tarnów, Krasińskiego</p>
              <p><strong>Data przerwy:</strong><br>od 06-10-2026 08:00 do 06-10-2026 15:00</p>
              <p><strong>Opis:</strong><br>Przerwa w dostawie wody.</p>
              <a href="https://www.tw.tarnow.pl/szczegoly-awarii/?id=96">Szczegóły</a>
            </div>
          </div>
        </div>
        <div class="report-list-white__elements">
          <div class="map-event" data-url="https://www.tw.tarnow.pl/szczegoly-awarii/?id=95" data-status="5" data-type="failure"></div>
          <div class="report-list-white__elements--element">
            <div class="report-list-white__elements--element__content">
              <p><strong>Lokalizacja:</strong><br>Miejscowość Tarnów, Kwiatowa</p>
              <p><strong>Data przerwy:</strong><br>od 06-10-2026 08:00 do 06-10-2026 15:00</p>
              <p><strong>Opis:</strong><br>Awaria wodociągowa.</p>
            </div>
          </div>
        </div>
      </main>
    "#;

    #[test]
    fn parses_detail_schedule_location_and_description_separately() {
        let parsed = parse_detail_html(ACTIVE_DETAIL, "2", "").unwrap();
        assert_eq!(parsed.kind, NoticeKind::Planned);
        assert_eq!(parsed.start.as_deref(), Some("2026-10-06T08:00:00"));
        assert_eq!(parsed.end.as_deref(), Some("2026-10-06T15:00:00"));
        assert_eq!(parsed.locality, "Tarnów");
        assert!(parsed.description.contains("Krasińskiego"));
    }

    #[test]
    fn ignores_completed_markers_and_detail_pages() {
        assert!(status_is_finished("Zakończone"));
        assert!(status_is_finished("5"));
        assert!(!status_is_finished("2"));
        assert!(parse_detail_html(ACTIVE_DETAIL, "5", "").is_none());
    }

    #[test]
    fn parses_listing_cards_with_numeric_status_and_data_type() {
        let notices = collect_notices(LISTING_WITH_MAP_MARKERS).unwrap();
        assert_eq!(notices.len(), 2);
        assert!(notices[0].url.contains("id=96"));
        assert_eq!(notices[0].status, "2");
        assert_eq!(notices[0].data_type, "planned");
        assert!(notices[1].url.contains("id=95"));
        assert_eq!(notices[1].status, "5");
        assert_eq!(notices[1].data_type, "failure");
    }

    #[test]
    fn data_type_classifies_card_when_the_visible_heading_is_missing() {
        let card = r#"
          <div class="report-list-white__elements--element__content">
            <p><strong>Lokalizacja:</strong> Miejscowość Tarnów, Krasińskiego</p>
            <p><strong>Data przerwy:</strong> od 06-10-2026 08:00 do 06-10-2026 15:00</p>
            <p><strong>Opis:</strong> Przerwa w dostawie wody.</p>
          </div>
        "#;
        assert_eq!(
            parse_detail_html(card, "2", "planned").unwrap().kind,
            NoticeKind::Planned
        );
    }

    #[test]
    fn matches_configured_street_only_inside_the_notice_locality() {
        let address = AddressEntry {
            city_name: "Tarnów".to_string(),
            street_name: "Krasińskiego".to_string(),
            ..AddressEntry::default()
        };
        assert!(location_matches_address(
            "Tarnów; Miejscowość Tarnów, Krasińskiego",
            &address
        ));
        assert!(!location_matches_address(
            "Tuchów; Miejscowość Tuchów, Krasińskiego",
            &address
        ));
    }
}
