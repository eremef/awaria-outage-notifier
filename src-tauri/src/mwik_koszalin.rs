use crate::api_logic::{AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use std::sync::OnceLock;

pub const MWIK_KOSZALIN_NOTICES_URL: &str = "https://mwik.koszalin.pl/ogloszenia";
const MAX_NOTICE_LINKS: usize = 20;

pub struct MwikKoszalinProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeKind {
    Planned,
    Failure,
    General,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Planned => "Planowane prace wodociągowe",
            Self::Failure => "Awaria wodociągowa",
            Self::General => "Komunikat wodociągowy",
        }
    }
}

fn normalize(text: &str) -> String {
    crate::utils::clean_text(text)
}

fn whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn date_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(\d{1,2})\.(\d{1,2})\.(\d{4})\s*r?\.?\b").unwrap())
}

fn time_range_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:od\s*)?(\d{1,2})[:.](\d{2})\s*(?:-|–|—|do)\s*(\d{1,2})[:.](\d{2})\b")
            .unwrap()
    })
}

fn street_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(?:ulica|ulicy|ulicach|ul\.)\s*").unwrap())
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    let captures = date_regex().captures(text)?;
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn parse_clock(hour: &str, minute: &str) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

fn format_local(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn schedule(text: &str) -> (Option<String>, Option<String>) {
    let Some(date) = parse_date(text) else {
        // Do not substitute the article publication date for an absent outage date.
        return (None, None);
    };
    if let Some(captures) = time_range_regex().captures(text) {
        let (Some(start_time), Some(end_time)) = (
            parse_clock(&captures[1], &captures[2]),
            parse_clock(&captures[3], &captures[4]),
        ) else {
            return (Some(date.format("%Y-%m-%d").to_string()), None);
        };
        let start = date.and_time(start_time);
        let mut end = date.and_time(end_time);
        if end <= start {
            end += Duration::days(1);
        }
        return (Some(format_local(start)), Some(format_local(end)));
    }
    (Some(date.format("%Y-%m-%d").to_string()), None)
}

fn element_lines(element: scraper::ElementRef<'_>) -> String {
    let line_selector = Selector::parse("p, li, h1, h2, h3, h4").unwrap();
    let lines: Vec<String> = element
        .select(&line_selector)
        .map(|part| whitespace(&part.text().collect::<Vec<_>>().join(" ")))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        whitespace(&element.text().collect::<Vec<_>>().join(" "))
    } else {
        lines.join("\n")
    }
}

fn article_text(html: &str) -> Result<String, String> {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("h1, h2").map_err(|error| error.to_string())?;
    if document.select(&title_selector).next().is_none() {
        return Err("MWiK Koszalin article has no heading".to_string());
    }

    let container_selector = Selector::parse(
        "article, main, [itemprop='articleBody'], .entry-content, .article-content, .content, #content",
    )
    .map_err(|error| error.to_string())?;
    let containers: Vec<_> = document.select(&container_selector).collect();
    let relevant = |text: &str| {
        let normalized = normalize(text);
        normalized.contains("wody")
            && (normalized.contains("przerw")
                || normalized.contains("awari")
                || normalized.contains("dostaw")
                || normalized.contains("wodociag"))
    };

    if let Some(text) = containers
        .iter()
        .map(|container| element_lines(*container))
        .filter(|text| relevant(text))
        .max_by_key(String::len)
    {
        return Ok(text);
    }

    let line_selector = Selector::parse("p, li, h1, h2, h3, h4").unwrap();
    let lines: Vec<String> = document
        .select(&line_selector)
        .map(|part| whitespace(&part.text().collect::<Vec<_>>().join(" ")))
        .filter(|line| !line.is_empty())
        .collect();
    let text = lines.join("\n");
    if relevant(&text) {
        Ok(text)
    } else {
        Err("MWiK Koszalin page has no outage article content".to_string())
    }
}

fn classify(text: &str) -> Option<NoticeKind> {
    let text = normalize(text);
    if [
        "dostawa wody zostala przywrocona",
        "przywrocono dostawe wody",
        "woda zostala przywrocona",
        "awaria zostala usunieta",
        "awaria usunieta",
        "prace zostaly zakonczone",
        "przerwa zostala odwolana",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        return None;
    }
    let is_water_outage = text.contains("przerw") && text.contains("wod")
        || text.contains("awari") && text.contains("wod")
        || text.contains("brak wody")
        || text.contains("wstrzyman") && text.contains("dostaw")
        || text.contains("zaklocen") && text.contains("dostaw");
    if !is_water_outage {
        return None;
    }
    if text.contains("awari") || text.contains("uszkodzen") || text.contains("peknie") {
        Some(NoticeKind::Failure)
    } else if text.contains("planowan")
        || text.contains("prac")
        || text.contains("wymian")
        || text.contains("remont")
    {
        Some(NoticeKind::Planned)
    } else {
        Some(NoticeKind::General)
    }
}

fn affected_streets(text: &str) -> String {
    let mut found = Vec::new();
    for line in text.lines() {
        let normalized_line = normalize(line);
        // Replacement-water points may be on a different street than the outage area.
        if normalized_line.contains("zastepcze punkty")
            || normalized_line.contains("punkt poboru wody")
        {
            continue;
        }
        let Some(marker) = street_marker_regex().find(line) else {
            continue;
        };
        let mut portion = line[marker.start()..].trim().to_string();
        let normalized = normalize(&portion);
        let terminators = [
            "prace wykonuje",
            "zastepcze punkty",
            "za wynikle",
            "za powstale",
            "za utrudnienia",
            "telefon kontaktowy",
        ];
        if let Some(end) = terminators
            .iter()
            .filter_map(|term| normalized.find(term))
            .min()
        {
            portion.truncate(end);
        }
        let portion = whitespace(&portion);
        if !portion.is_empty() && !found.iter().any(|existing| existing == &portion) {
            found.push(portion);
        }
    }
    found.join("; ")
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    let text: Vec<String> = normalize(text)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let phrase: Vec<String> = normalize(crate::utils::strip_street_prefixes(phrase))
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    !phrase.is_empty()
        && text
            .windows(phrase.len())
            .any(|candidate| candidate == phrase.as_slice())
}

fn address_has_street(address: &AddressEntry) -> bool {
    !address.street_name.trim().is_empty()
        || !address.street_name_1.trim().is_empty()
        || address
            .street_name_2
            .as_deref()
            .is_some_and(|street| !street.trim().is_empty())
}

fn matches_address(text: &str, address: &AddressEntry) -> bool {
    if !address_has_street(address) {
        return true;
    }
    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .iter()
    .any(|candidate| contains_phrase(text, candidate))
}

fn parse_article(text: &str, settings: &Settings) -> Option<UnifiedAlert> {
    let kind = classify(text)?;
    let (start_date, end_date) = schedule(text);
    let affected = affected_streets(text);
    let source = AlertSource::MwikKoszalin;
    let matching_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && crate::api_logic::is_address_applicable_for_provider(&source, address)
                && !affected.is_empty()
                && matches_address(&affected, address)
        })
        .map(|(index, _)| index)
        .or_else(|| {
            settings
                .addresses
                .iter()
                .enumerate()
                .find(|(_, address)| {
                    address.is_active
                        && !address_has_street(address)
                        && crate::api_logic::is_address_applicable_for_provider(&source, address)
                })
                .map(|(index, _)| index)
        });
    let location = if affected.is_empty() {
        "Miejscowość: Koszalin".to_string()
    } else {
        format!("Miejscowość: Koszalin; {affected}")
    };
    Some(UnifiedAlert {
        source,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{}: {}", kind.label(), whitespace(text))),
        location: Some(location),
        address_index: matching_address,
        is_local: Some(matching_address.is_some()),
        hash: None,
    })
}

fn notice_urls(html: &str) -> Result<Vec<Url>, String> {
    let document = Html::parse_document(html);
    let heading_selector = Selector::parse("h1, h2, h3").map_err(|error| error.to_string())?;
    if !document.select(&heading_selector).any(|heading| {
        normalize(&heading.text().collect::<Vec<_>>().join(" ")).contains("ogloszenia")
    }) {
        return Err("MWiK Koszalin notices page has no Ogłoszenia heading".to_string());
    }
    let anchor_selector = Selector::parse("a[href]").map_err(|error| error.to_string())?;
    let base = Url::parse(MWIK_KOSZALIN_NOTICES_URL).map_err(|error| error.to_string())?;
    let mut urls = Vec::new();
    for anchor in document.select(&anchor_selector) {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(url) = base.join(href) else {
            continue;
        };
        if url.host_str() == Some("mwik.koszalin.pl")
            && url.path().contains("/aktualnosci/")
            && !urls.contains(&url)
        {
            urls.push(url);
            if urls.len() >= MAX_NOTICE_LINKS {
                break;
            }
        }
    }
    Ok(urls)
}

pub fn parse_article_html(html: &str, settings: &Settings) -> Result<Option<UnifiedAlert>, String> {
    let text = article_text(html)?;
    Ok(parse_article(&text, settings))
}

#[async_trait]
impl AlertProvider for MwikKoszalinProvider {
    fn id(&self) -> String {
        "mwik_koszalin".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::MwikKoszalin
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
        let mut errors = Vec::new();
        let response = match client.get(MWIK_KOSZALIN_NOTICES_URL).send().await {
            Ok(response) if response.status().is_success() => response,
            Ok(response) => {
                return (
                    Vec::new(),
                    vec![format!(
                        "MWiK Koszalin list HTTP error: {}",
                        response.status()
                    )],
                );
            }
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MWiK Koszalin list request error: {error}")],
                );
            }
        };
        let html = match response.text().await {
            Ok(html) => html,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("MWiK Koszalin list read error: {error}")],
                );
            }
        };
        let urls = match notice_urls(&html) {
            Ok(urls) => urls,
            Err(error) => return (Vec::new(), vec![error]),
        };
        let mut alerts = Vec::new();
        for url in urls {
            let response = match client.get(url.clone()).send().await {
                Ok(response) if response.status().is_success() => response,
                Ok(response) => {
                    errors.push(format!(
                        "MWiK Koszalin article HTTP error for {url}: {}",
                        response.status()
                    ));
                    continue;
                }
                Err(error) => {
                    errors.push(format!(
                        "MWiK Koszalin article request error for {url}: {error}"
                    ));
                    continue;
                }
            };
            let html = match response.text().await {
                Ok(html) => html,
                Err(error) => {
                    errors.push(format!(
                        "MWiK Koszalin article read error for {url}: {error}"
                    ));
                    continue;
                }
            };
            match parse_article_html(&html, settings) {
                Ok(Some(alert)) => alerts.push(alert),
                Ok(None) => {}
                Err(error) => errors.push(format!(
                    "MWiK Koszalin article parse error for {url}: {error}"
                )),
            }
        }
        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"<html><h1>Ogłoszenia</h1><a href="/aktualnosci/przerwa-w-dostawie-wody-przy-ul-sportowej">więcej informacji</a></html>"#;
    const ARTICLE: &str = r#"
        <html><article><h1>Przerwa w dostawie wody</h1>
        <div class="entry-content">
          <p>W związku z wymianą sieci wodociągowej przy ul. Sportowej nastąpi przerwa w dostawie wody.</p>
          <p>dnia 27.10.2026r. w godz.: 08:00 - 15:00</p>
          <p>w ul. Sportowa (cała), Piłsudskiego 55, ul. Fałata 34.</p>
          <p>Prace wykonuje firma. Zastępcze punkty poboru wody: ul. Sportowa 19.</p>
        </div></article></html>
    "#;

    fn settings(city: &str, street: &str, house_no: &str, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses: vec![AddressEntry {
                name: "Dom".into(),
                city_name: city.into(),
                street_name: street.into(),
                street_name_1: street.into(),
                house_no: house_no.into(),
                is_active: true,
                ..AddressEntry::default()
            }],
            filter_by_house_no,
            ..Settings::default()
        }
    }

    #[test]
    fn parses_outage_schedule_and_several_affected_streets() {
        let alert = parse_article_html(ARTICLE, &settings("Koszalin", "Sportowa", "99", true))
            .unwrap()
            .unwrap();
        assert_eq!(alert.source, AlertSource::MwikKoszalin);
        // The publication date is not used as the event date.
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-27T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-27T15:00:00"));
        assert_eq!(alert.is_local, Some(true));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("Piłsudskiego 55")
        );
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("Zastępcze punkty")
        );
        assert!(alert.location.as_deref().unwrap().contains("Fałata 34"));
    }

    #[test]
    fn matching_is_street_level_and_does_not_apply_house_number_filtering() {
        let street_match =
            parse_article_html(ARTICLE, &settings("Koszalin", "Fałata", "999", true))
                .unwrap()
                .unwrap();
        assert_eq!(street_match.is_local, Some(true));
        assert_eq!(street_match.address_index, Some(0));

        let other_city = parse_article_html(ARTICLE, &settings("Słupsk", "Sportowa", "19", false))
            .unwrap()
            .unwrap();
        assert_eq!(other_city.is_local, Some(false));
        assert_eq!(other_city.address_index, None);
    }

    #[test]
    fn replacement_water_points_do_not_make_a_street_local() {
        let article = ARTICLE
            .replace("przy ul. Sportowej", "w rejonie prac")
            .replace(
                "w ul. Sportowa (cała), Piłsudskiego 55, ul. Fałata 34.",
                "Prace obejmą obiekt techniczny.",
            );
        let alert = parse_article_html(&article, &settings("Koszalin", "Sportowa", "19", false))
            .unwrap()
            .unwrap();
        assert_eq!(alert.is_local, Some(false));
    }

    #[test]
    fn ignores_restored_or_non_outage_articles() {
        let restored = ARTICLE.replace(
            "nastąpi przerwa w dostawie wody",
            "dostawa wody została przywrócona",
        );
        assert!(
            parse_article_html(&restored, &settings("Koszalin", "Sportowa", "1", false))
                .unwrap()
                .is_none()
        );
        assert!(
            parse_article_html(
                "<article><h1>Ogłoszenie</h1><p>Przetarg na roboty.</p></article>",
                &Settings::default()
            )
            .is_err()
        );
    }

    #[test]
    fn list_accepts_valid_empty_results_and_rejects_unrecognized_markup() {
        assert_eq!(notice_urls(LIST).unwrap().len(), 1);
        assert!(
            notice_urls("<html><h1>Ogłoszenia</h1><p>Brak komunikatów</p></html>")
                .unwrap()
                .is_empty()
        );
        assert!(notice_urls("<html><h1>Aktualności</h1></html>").is_err());
    }
}
