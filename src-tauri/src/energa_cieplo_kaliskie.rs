use crate::api_logic::{AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const ENERGA_CIEPLO_KALISKIE_URL: &str = "https://energa-cieplokaliskie.pl/aktualnosci";

pub struct EnergaCieploKaliskieProvider;

fn date_regex() -> &'static Regex {
    static DATE_RE: OnceLock<Regex> = OnceLock::new();
    DATE_RE.get_or_init(|| Regex::new(r"\b\d{1,2}\.\d{1,2}\.\d{4}\b").unwrap())
}

fn time_regex() -> &'static Regex {
    static TIME_RE: OnceLock<Regex> = OnceLock::new();
    TIME_RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\s*[:.]\s*(\d{2})\b").unwrap())
}

fn superscript_minute_regex() -> &'static Regex {
    static SUPER_RE: OnceLock<Regex> = OnceLock::new();
    SUPER_RE.get_or_init(|| Regex::new(r"(?is)<sup>\s*(\d{2})\s*</sup>").unwrap())
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

fn classify_incident(title: &str, body: &str) -> Option<&'static str> {
    let normalized = normalize_polish(&format!("{title} {body}"));
    let heat_related = normalized.contains("ciepl") || normalized.contains("ogrzew");
    let outage_related = [
        "awari",
        "przerw",
        "wstrzyman",
        "zaklocen",
        "planowan",
        "prac",
        "brak dostaw",
    ]
    .iter()
    .any(|term| normalized.contains(term));

    if !heat_related || !outage_related {
        return None;
    }

    if normalized.contains("awari") {
        Some("Awaria ogrzewania")
    } else if normalized.contains("planowan") || normalized.contains("prac") {
        Some("Planowane wyłączenie ogrzewania")
    } else {
        Some("Zakłócenia w dostawie ciepła")
    }
}

#[derive(Clone, Copy)]
struct ParsedClock {
    value: NaiveTime,
    next_day: bool,
}

fn parse_clock(hour: &str, minute: &str) -> Option<ParsedClock> {
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    if hour == 24 && minute == 0 {
        return Some(ParsedClock {
            value: NaiveTime::MIN,
            next_day: true,
        });
    }

    Some(ParsedClock {
        value: NaiveTime::from_hms_opt(hour, minute, 0)?,
        next_day: false,
    })
}

fn format_datetime(datetime: NaiveDateTime) -> String {
    datetime.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>) {
    let dates = date_regex()
        .find_iter(text)
        .filter_map(|found| NaiveDate::parse_from_str(found.as_str(), "%d.%m.%Y").ok())
        .collect::<Vec<_>>();
    let Some(first_date) = dates.first().copied() else {
        return (None, None);
    };

    // Remove dates first so their dots cannot be mistaken for clock separators.
    let without_dates = date_regex().replace_all(text, " ");
    if time_regex().is_match(&without_dates)
        && !time_regex()
            .captures_iter(&without_dates)
            .any(|captures| parse_clock(&captures[1], &captures[2]).is_some())
    {
        return (None, None);
    }
    let clocks = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|captures| parse_clock(&captures[1], &captures[2]))
        .collect::<Vec<_>>();
    let start_clock = clocks.first().copied();
    let start_date = if start_clock.is_some_and(|clock| clock.next_day) {
        let Some(next_date) = first_date.succ_opt() else {
            return (None, None);
        };
        next_date
    } else {
        first_date
    };
    let start = start_date.and_time(
        start_clock
            .map(|clock| clock.value)
            .unwrap_or(NaiveTime::MIN),
    );

    let explicit_end = clocks.get(1).copied().and_then(|end_clock| {
        let mut end_date = dates.get(1).copied().unwrap_or(start.date());
        if end_clock.next_day {
            end_date = end_date.succ_opt()?;
        }
        let mut end = end_date.and_time(end_clock.value);
        if dates.len() == 1 && !end_clock.next_day && end <= start {
            end = end_date.succ_opt()?.and_time(end_clock.value);
        }
        (end > start).then_some(end)
    });

    // News pages may retain old notices. A dated notice without an explicit end
    // expires at the end of its stated outage day rather than its publication day.
    let fallback_date = if dates.len() > 1 {
        dates.last().copied().unwrap_or(start.date())
    } else {
        start.date()
    };
    let end = explicit_end.or_else(|| {
        fallback_date
            .and_hms_opt(23, 59, 0)
            .filter(|end_of_day| *end_of_day >= start)
    });

    (Some(format_datetime(start)), end.map(format_datetime))
}

fn extract_outage_detail_links(index_html: &str) -> Vec<String> {
    let document = Html::parse_document(index_html);
    let Ok(card_selector) = Selector::parse(".basic-card") else {
        return Vec::new();
    };
    let Ok(title_selector) = Selector::parse(".basic-card-content h5") else {
        return Vec::new();
    };
    let Ok(body_selector) = Selector::parse(".basic-card-content p") else {
        return Vec::new();
    };
    let Ok(link_selector) = Selector::parse("a.read-more[href]") else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    let mut links = Vec::new();
    for card in document.select(&card_selector) {
        let title = card
            .select(&title_selector)
            .next()
            .map(|element| element.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let excerpt = card
            .select(&body_selector)
            .next()
            .map(|element| element.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        if classify_incident(&title, &excerpt).is_none() {
            continue;
        }

        let Some(href) = card
            .select(&link_selector)
            .next()
            .and_then(|element| element.value().attr("href"))
        else {
            continue;
        };
        let url = if href.starts_with("https://energa-cieplokaliskie.pl/aktualnosci/") {
            href.to_string()
        } else if href.starts_with("/aktualnosci/") {
            format!("https://energa-cieplokaliskie.pl{href}")
        } else {
            continue;
        };

        if seen.insert(url.clone()) {
            links.push(url);
        }
    }

    links
}

fn tokenized_polish(text: &str) -> Vec<String> {
    normalize_polish(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn street_tokens_match(street_tokens: &[String], notice_tokens: &[String]) -> bool {
    !street_tokens.is_empty()
        && street_tokens.iter().all(|street_token| {
            let characters = street_token.chars().collect::<Vec<_>>();
            let prefix_length = if characters.len() > 5 {
                characters.len().saturating_sub(2).max(3)
            } else {
                characters.len()
            };
            let prefix = characters
                .iter()
                .take(prefix_length)
                .copied()
                .collect::<String>();
            notice_tokens
                .iter()
                .any(|notice_token| notice_token.starts_with(&prefix))
        })
}

fn street_matches_address(notice_text: &str, address: &crate::api_logic::AddressEntry) -> bool {
    let mut streets = vec![address.street_name.as_str(), address.street_name_1.as_str()];
    if let Some(street_name_2) = address.street_name_2.as_deref() {
        streets.push(street_name_2);
    }
    let notice_tokens = tokenized_polish(notice_text);

    streets.into_iter().any(|street| {
        let street = crate::utils::strip_street_prefixes(street.trim());
        let normalized = normalize_polish(street);
        let full_tokens = tokenized_polish(&normalized);
        if street_tokens_match(&full_tokens, &notice_tokens) {
            return true;
        }

        // Some notices shorten a hyphenated TERYT street name to its final part.
        if street.contains('-') {
            if let Some(last_token) = full_tokens.last() {
                return street_tokens_match(std::slice::from_ref(last_token), &notice_tokens);
            }
        }
        false
    })
}

fn parse_article_html(html: &str, settings: &Settings) -> Option<UnifiedAlert> {
    let document = Html::parse_document(html);
    let article_selector = Selector::parse("article.publication").ok()?;
    let title_selector = Selector::parse(".article-title h1").ok()?;
    let content_selector = Selector::parse(".prnt-pub__content").ok()?;
    let article = document.select(&article_selector).next()?;
    let title = article
        .select(&title_selector)
        .next()?
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let content = article.select(&content_selector).next()?;
    let normalized_html = superscript_minute_regex()
        .replace_all(&content.inner_html(), ":$1")
        .into_owned();
    let content_document = Html::parse_fragment(&normalized_html);
    let body = content_document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let incident_type = classify_incident(&title, &body)?;
    let (start_date, end_date) = parse_schedule(&body);

    let local_match = settings.addresses.iter().enumerate().find(|(_, address)| {
        address.is_active
            && crate::api_logic::is_address_applicable_for_provider(
                &AlertSource::EnergaCieploKaliskie,
                address,
            )
            && street_matches_address(&body, address)
    });
    let (address_index, is_local) = match local_match {
        Some((index, _)) => (Some(index), Some(true)),
        None => (None, Some(false)),
    };

    Some(UnifiedAlert {
        source: AlertSource::EnergaCieploKaliskie,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{incident_type} — {title}. {body}")),
        location: Some("Miejscowość: Kalisz".to_string()),
        address_index,
        is_local,
        hash: None,
    })
}

async fn fetch_html(client: &Client, url: &str) -> Result<String, String> {
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
impl AlertProvider for EnergaCieploKaliskieProvider {
    fn id(&self) -> String {
        "energa_cieplo_kaliskie".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::EnergaCieploKaliskie
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

        let index_html = match fetch_html(client, ENERGA_CIEPLO_KALISKIE_URL).await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("Energa Ciepło Kaliskie request error: {error}")],
                );
            }
        };

        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        for url in extract_outage_detail_links(&index_html) {
            match fetch_html(client, &url).await {
                Ok(html) => match parse_article_html(&html, settings) {
                    Some(alert) => alerts.push(alert),
                    None => errors.push(format!(
                        "Energa Ciepło Kaliskie could not parse outage notice: {url}"
                    )),
                },
                Err(error) => errors.push(format!(
                    "Energa Ciepło Kaliskie article request error ({url}): {error}"
                )),
            }
        }

        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_logic::AddressEntry;

    fn settings(filter_by_house_no: bool) -> Settings {
        Settings {
            filter_by_house_no,
            addresses: vec![AddressEntry {
                name: "Home".to_string(),
                city_name: "Kalisz".to_string(),
                voivodeship: "Wielkopolskie".to_string(),
                district: "Kalisz".to_string(),
                commune: "Kalisz".to_string(),
                street_name: "ul. Legionów".to_string(),
                street_name_1: "Legionów".to_string(),
                street_name_2: None,
                house_no: "40A".to_string(),
                city_id: Some(936579),
                street_id: None,
                is_active: true,
            }],
            ..Settings::default()
        }
    }

    const OFFICIAL_ARTICLE: &str = r#"
        <article class="publication">
          <div class="row expanded collapse publication__teaser prnt-pub-hide">
            <div class="article-title"><h1>Awaria sieci 01.10.2026</h1></div>
          </div>
          <div class="row align-center">
            <div class="small-12 large-9 columns prnt-pub__content">
              <p>Informujemy, że ze względu na konieczność usunięcia awarii na sieci ciepłowniczej, w dniu <strong>01.10.2026 od godziny 7<sup>00</sup> do godz. 21<sup>00</sup>,</strong> nastąpi wstrzymanie dostawy ciepła do budynków przy ul.:</p>
              <p>- Legionów 42,</p>
              <p>- Legionów 40,</p>
              <p>- Legionów 40A,</p>
              <p>- Legionów 29,</p>
              <p>- Polna 20 - Pawilon</p>
              <p>Przepraszamy za zaistniałe niedogodności.</p>
            </div>
          </div>
          <div class="prnt-pub__date"><time datetime="2026-10-01T07:11">01 października 2026, 07:11</time></div>
        </article>
    "#;

    const OUTAGE_INDEX: &str = r#"
        <main>
          <div class="basic-card">
            <div class="basic-card-content">
              <h5>Awaria sieci 01.10.2026</h5>
              <p>W dniu 01.10.2026 od godziny 700 do godz. 2100 nastąpi wstrzymanie dostawy ciepła.</p>
            </div>
            <div class="links"><a class="float-right read-more" href="https://energa-cieplokaliskie.pl/aktualnosci/awaria-sieci-01-10-2026">więcej</a></div>
          </div>
          <div class="basic-card">
            <div class="basic-card-content">
              <h5>Sprzedaż żużla paleniskowego</h5>
              <p>Energa Ciepło Kaliskie informuje o możliwości sprzedaży żużla.</p>
            </div>
            <div class="links"><a class="float-right read-more" href="https://energa-cieplokaliskie.pl/aktualnosci/sprzedaz-zuzla">więcej</a></div>
          </div>
        </main>
    "#;

    #[test]
    fn parses_official_article_schedule_and_matches_street_without_house_filtering() {
        let alert = parse_article_html(OFFICIAL_ARTICLE, &settings(false)).unwrap();
        assert_eq!(alert.source, AlertSource::EnergaCieploKaliskie);
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-01T07:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-01T21:00:00"));
        assert_eq!(alert.address_index, Some(0));
        assert_eq!(alert.is_local, Some(true));
        let message = alert.message.as_deref().unwrap();
        assert!(message.contains("Legionów 40A"));
        assert!(message.contains("Polna 20 - Pawilon"));
        assert_eq!(alert.location.as_deref(), Some("Miejscowość: Kalisz"));
    }

    #[test]
    fn house_number_setting_does_not_change_provider_street_matching() {
        let without_filter = parse_article_html(OFFICIAL_ARTICLE, &settings(false)).unwrap();
        let with_filter = parse_article_html(OFFICIAL_ARTICLE, &settings(true)).unwrap();
        assert_eq!(without_filter.address_index, with_filter.address_index);
        assert_eq!(without_filter.is_local, Some(true));
        assert_eq!(with_filter.is_local, Some(true));
    }

    #[test]
    fn rejects_same_street_in_a_different_city() {
        let mut settings = settings(false);
        settings.addresses[0].city_name = "Toruń".to_string();
        settings.addresses[0].voivodeship = "Kujawsko-Pomorskie".to_string();
        settings.addresses[0].city_id = None;

        let alert = parse_article_html(OFFICIAL_ARTICLE, &settings).unwrap();
        assert_eq!(alert.address_index, None);
        assert_eq!(alert.is_local, Some(false));
    }

    #[test]
    fn parses_candidate_links_and_ignores_unrelated_news() {
        let links = extract_outage_detail_links(OUTAGE_INDEX);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0],
            "https://energa-cieplokaliskie.pl/aktualnosci/awaria-sieci-01-10-2026"
        );
        assert!(extract_outage_detail_links("<main><p>Brak aktualności</p></main>").is_empty());
    }

    #[test]
    fn parses_planned_work_and_same_day_end_fallback() {
        let planned = r#"
            <article class="publication"><div class="article-title"><h1>Prace na sieci ciepłowniczej</h1></div>
              <div class="prnt-pub__content"><p>W dniu 02.10.2026 od godz. 8.00 do godz. 15.00 zostanie wstrzymana dostawa ciepła.</p></div>
            </article>
        "#;
        let alert = parse_article_html(planned, &settings(false)).unwrap();
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .starts_with("Planowane wyłączenie ogrzewania")
        );
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-02T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-02T15:00:00"));

        let missing_end = parse_schedule("w dniu 02.10.2026 od godz. 08:00");
        assert_eq!(missing_end.0.as_deref(), Some("2026-10-02T08:00:00"));
        assert_eq!(missing_end.1.as_deref(), Some("2026-10-02T23:59:00"));
    }

    #[test]
    fn parses_multi_day_and_midnight_schedules_and_rejects_malformed_values() {
        let (start, end) =
            parse_schedule("w dniach 03.10.2026 od godz. 22:00 do dnia 04.10.2026 do godz. 02:00");
        assert_eq!(start.as_deref(), Some("2026-10-03T22:00:00"));
        assert_eq!(end.as_deref(), Some("2026-10-04T02:00:00"));

        let (start, end) = parse_schedule("w dniu 02.10.2026 od godz. 23.00 do godz. 24.00");
        assert_eq!(start.as_deref(), Some("2026-10-02T23:00:00"));
        assert_eq!(end.as_deref(), Some("2026-10-03T00:00:00"));

        assert_eq!(
            parse_schedule("w dniu 31.02.2026 od godz. 08:00"),
            (None, None)
        );
        assert_eq!(
            parse_schedule("w dniu 02.10.2026 od godz. 25:80"),
            (None, None)
        );
    }

    #[test]
    fn publication_date_is_not_used_when_event_date_is_missing() {
        let article = r#"
            <article class="publication">
              <div class="article-title"><h1>Awaria sieci</h1></div>
              <div class="prnt-pub__date"><time datetime="2026-10-01T07:11">01 października 2026</time></div>
              <div class="prnt-pub__content"><p>Awaria sieci ciepłowniczej. Szczegóły wkrótce.</p></div>
            </article>
        "#;
        let alert = parse_article_html(article, &settings(false)).unwrap();
        assert_eq!(alert.startDate, None);
        assert_eq!(alert.endDate, None);
    }

    #[test]
    fn supports_multiple_detail_links_and_rejects_external_links() {
        let html = OUTAGE_INDEX.replace(
            "</main>",
            r#"<div class="basic-card"><div class="basic-card-content"><h5>Prace przy sieci ciepłowniczej</h5><p>Planowane wstrzymanie dostawy ciepła w dniu 02.10.2026.</p></div><div class="links"><a class="read-more" href="/aktualnosci/prace-cieplownicze">więcej</a></div></div>
               <div class="basic-card"><div class="basic-card-content"><h5>Awaria ciepła</h5><p>Wstrzymano dostawę ciepła.</p></div><div class="links"><a class="read-more" href="https://example.com/notice">więcej</a></div></div></main>"#,
        );
        let links = extract_outage_detail_links(&html);
        assert_eq!(links.len(), 2);
        assert!(links[1].ends_with("/aktualnosci/prace-cieplownicze"));
    }

    #[tokio::test]
    #[ignore = "requires network access to the official provider site"]
    async fn live_official_pages_are_reachable_with_the_app_client() {
        let client = crate::network_state::NetworkState::build_client().unwrap();
        let index_html = fetch_html(&client, ENERGA_CIEPLO_KALISKIE_URL)
            .await
            .unwrap();
        assert!(index_html.contains("basic-card"));

        let article_html = fetch_html(
            &client,
            "https://energa-cieplokaliskie.pl/aktualnosci/awaria-sieci-01-10-2026",
        )
        .await
        .unwrap();
        let alert = parse_article_html(&article_html, &settings(false)).unwrap();
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-01T07:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-01T21:00:00"));
    }
}
