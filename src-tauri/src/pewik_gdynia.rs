use crate::api_logic::{self, AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::OnceLock;

pub const PEWIK_GDYNIA_EMERGENCY_URL: &str =
    "https://app.bluealert.pl/pewikgdynia/articles/external/1/";
pub const PEWIK_GDYNIA_PLANNED_URL: &str =
    "https://app.bluealert.pl/pewikgdynia/articles/external/2/";

pub struct PewikGdyniaProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoticeKind {
    Emergency,
    Planned,
}

impl NoticeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Emergency => "Awaria wodociągowa",
            Self::Planned => "Planowane wyłączenie wody",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Emergency => "emergency",
            Self::Planned => "planned",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Locality {
    Gdynia,
    Rumia,
    Reda,
    Wejherowo,
    Kosakowo,
    DebogorzeWybudowanie,
    Kazimierz,
    Pogorze,
    Bolszewo,
    Goscicino,
    Gora,
    Kniewo,
    Orle,
    Warszkowo,
    Zamostne,
}

impl Locality {
    fn label(self) -> &'static str {
        match self {
            Self::Gdynia => "Gdynia",
            Self::Rumia => "Rumia",
            Self::Reda => "Reda",
            Self::Wejherowo => "Wejherowo",
            Self::Kosakowo => "Kosakowo",
            Self::DebogorzeWybudowanie => "Dębogórze Wybudowanie",
            Self::Kazimierz => "Kazimierz",
            Self::Pogorze => "Pogórze",
            Self::Bolszewo => "Bolszewo",
            Self::Goscicino => "Gościcino",
            Self::Gora => "Góra",
            Self::Kniewo => "Kniewo",
            Self::Orle => "Orle",
            Self::Warszkowo => "Warszkowo",
            Self::Zamostne => "Zamostne",
        }
    }

    fn matches_address(self, address: &AddressEntry) -> bool {
        let city = normalize_name(&address.city_name).replace('-', " ");
        let commune = normalize_name(&address.commune).replace('-', " ");
        match self {
            Self::Gdynia => city.starts_with("gdyn"),
            Self::Rumia => city.starts_with("rumi"),
            Self::Reda => city.starts_with("reda"),
            Self::Wejherowo => {
                city.starts_with("wejherow")
                    || commune.starts_with("wejherowo")
                    || commune.starts_with("gmina wejherowo")
            }
            Self::Kosakowo => {
                city.starts_with("kosakowo")
                    || commune.starts_with("kosakowo")
                    || commune.starts_with("gmina kosakowo")
            }
            Self::DebogorzeWybudowanie => city.starts_with("debogorze wybud"),
            Self::Kazimierz => city.starts_with("kazimierz"),
            Self::Pogorze => city.starts_with("pogorze"),
            Self::Bolszewo => city.starts_with("bolszewo"),
            Self::Goscicino => city.starts_with("goscicino"),
            Self::Gora => city.starts_with("gora"),
            Self::Kniewo => city.starts_with("kniewo"),
            Self::Orle => city.starts_with("orle"),
            Self::Warszkowo => city.starts_with("warszkowo"),
            Self::Zamostne => city.starts_with("zamostne"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StreetNotice {
    name: String,
    house_numbers: Option<String>,
}

impl StreetNotice {
    fn location_segment(&self) -> String {
        match self.house_numbers.as_deref() {
            Some(numbers) => format!("{} {numbers}", self.name),
            None => self.name.clone(),
        }
    }
}

fn normalize_name(value: &str) -> String {
    crate::utils::clean_text(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn street_marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\b(?:ulic\p{L}*|ul\.)\s*:?\s*").unwrap())
}

fn house_suffix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(.+?)\s+(\d+[a-z]?(?:\s*(?:-|–|—|do|/|,)\s*\d+[a-z]?)*(?:\s+(?:nieparzyste|parzyste))?)$",
        )
        .unwrap()
    })
}

fn date_from_captures(captures: &regex::Captures<'_>) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        captures.get(3)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        captures.get(1)?.as_str().parse().ok()?,
    )
}

fn time_from_captures(captures: &regex::Captures<'_>) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(
        captures.get(1)?.as_str().parse().ok()?,
        captures.get(2)?.as_str().parse().ok()?,
        0,
    )
}

fn parse_schedule(text: &str) -> (Option<String>, Option<String>, Option<NaiveDateTime>) {
    let dates: Vec<NaiveDate> = date_regex()
        .captures_iter(text)
        .filter_map(|captures| date_from_captures(&captures))
        .collect();
    let without_dates = date_regex().replace_all(text, " ");
    let times: Vec<NaiveTime> = time_regex()
        .captures_iter(&without_dates)
        .filter_map(|captures| time_from_captures(&captures))
        .collect();

    let Some(start_date) = dates.first().copied() else {
        return (None, None, None);
    };
    let normalized = normalize_name(text);
    let expected_end = normalized.contains("przywroc")
        || normalized.contains("do godz")
        || normalized.contains("do godziny");
    let explicit_start = normalized.contains("od godz") || normalized.contains("od godziny");

    let mut start = None;
    let mut end = None;
    if times.len() >= 2 {
        start = Some(start_date.and_time(times[0]));
        let end_date = dates.get(1).copied().unwrap_or(start_date);
        let mut parsed_end = end_date.and_time(times[1]);
        if parsed_end <= start.unwrap() && dates.len() < 2 {
            parsed_end += Duration::days(1);
        }
        end = Some(parsed_end);
    } else if let Some(time) = times.first().copied() {
        if expected_end {
            end = Some(dates.get(1).copied().unwrap_or(start_date).and_time(time));
        } else if explicit_start {
            start = Some(start_date.and_time(time));
        }
    }

    let start_date = start
        .map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_else(|| start_date.format("%Y-%m-%d").to_string());
    let end_date = end.map(|value| value.format("%Y-%m-%dT%H:%M:%S").to_string());
    (Some(start_date), end_date, end)
}

fn extract_blocks(html: &str) -> Result<Vec<String>, String> {
    let document = Html::parse_document(html);
    let container_selector = Selector::parse(".message_body")
        .map_err(|error| format!("PEWIK Gdynia selector error: {error}"))?;
    let Some(container) = document.select(&container_selector).next() else {
        let body_selector = Selector::parse("body")
            .map_err(|error| format!("PEWIK Gdynia body selector error: {error}"))?;
        let body_text = document
            .select(&body_selector)
            .next()
            .map(|body| normalize_whitespace(&body.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();
        if body_text.is_empty() {
            return Ok(Vec::new());
        }
        return Err("PEWIK Gdynia response has no .message_body notice container".to_string());
    };

    let paragraph_selector = Selector::parse("p")
        .map_err(|error| format!("PEWIK Gdynia paragraph selector error: {error}"))?;
    let mut blocks = Vec::new();
    for paragraph in container.select(&paragraph_selector) {
        // Avoid duplicating the message if the upstream page nests paragraph tags.
        if paragraph.select(&paragraph_selector).next().is_some() {
            continue;
        }
        let text = normalize_whitespace(&paragraph.text().collect::<Vec<_>>().join(" "));
        if !text.is_empty() && blocks.last() != Some(&text) {
            blocks.push(text);
        }
    }

    if blocks.is_empty() {
        let text = normalize_whitespace(&container.text().collect::<Vec<_>>().join(" "));
        if !text.is_empty() {
            blocks.push(text);
        }
    }
    Ok(blocks)
}

fn locality_from_token(token: &str, next: Option<&str>) -> Option<Locality> {
    if token.starts_with("gdyn") {
        Some(Locality::Gdynia)
    } else if token.starts_with("rumi") {
        Some(Locality::Rumia)
    } else if token.starts_with("red") {
        Some(Locality::Reda)
    } else if token.starts_with("wejherow") {
        Some(Locality::Wejherowo)
    } else if token.starts_with("kosakow") {
        Some(Locality::Kosakowo)
    } else if token.starts_with("debogorz") {
        if next.is_some_and(|word| word.starts_with("wybud")) {
            Some(Locality::DebogorzeWybudowanie)
        } else {
            None
        }
    } else if token.starts_with("kazimierz") {
        Some(Locality::Kazimierz)
    } else if token.starts_with("pogorz") {
        Some(Locality::Pogorze)
    } else if token.starts_with("bolszew") {
        Some(Locality::Bolszewo)
    } else if token.starts_with("goscicin") {
        Some(Locality::Goscicino)
    } else if token.starts_with("gor") {
        Some(Locality::Gora)
    } else if token.starts_with("kniew") {
        Some(Locality::Kniewo)
    } else if token.starts_with("orl") {
        Some(Locality::Orle)
    } else if token.starts_with("warszkow") {
        Some(Locality::Warszkowo)
    } else if token.starts_with("zamostn") {
        Some(Locality::Zamostne)
    } else {
        None
    }
}

fn mentioned_localities(text: &str) -> Vec<Locality> {
    let normalized = normalize_name(text);
    let words: Vec<&str> = normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let contexts = [
        "w",
        "na",
        "miescie",
        "miejscowosci",
        "miejscowosc",
        "gminie",
        "gminy",
        "gmina",
        "terenie",
    ];
    let mut localities = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if index == 0 || !contexts.contains(&words[index - 1]) {
            continue;
        }
        let next = words.get(index + 1).copied();
        if let Some(locality) = locality_from_token(word, next)
            && !localities.contains(&locality)
        {
            localities.push(locality);
        }
    }
    localities
}

fn trim_street_tail(value: &str) -> &str {
    let normalized = normalize_name(value);
    let continuation_phrases = [
        " w zwiazku z tym",
        " moga wystapic",
        " moga wystepowac",
        " przewidywany czas",
        " za utrudnienia",
        " przepraszamy",
    ];
    let cutoff = continuation_phrases
        .iter()
        .filter_map(|phrase| normalized.find(phrase))
        .min();
    let value = cutoff
        .and_then(|index| {
            value
                .char_indices()
                .nth(normalized[..index].chars().count())
                .map(|(byte, _)| &value[..byte])
        })
        .unwrap_or(value);
    value.trim().trim_end_matches(['.', ':', '!', '?']).trim()
}

fn house_suffix(value: &str) -> (String, Option<String>) {
    if let Some(captures) = house_suffix_regex().captures(value) {
        let name = captures
            .get(1)
            .map(|capture| capture.as_str().trim())
            .unwrap_or_default();
        let numbers = captures
            .get(2)
            .map(|capture| capture.as_str().trim().to_string());
        if !name.is_empty() {
            return (name.to_string(), numbers);
        }
    }
    (value.trim().to_string(), None)
}

fn is_number_list(value: &str) -> bool {
    let cleaned = normalize_name(value);
    let parts: Vec<&str> = cleaned.split_whitespace().collect();
    !parts.is_empty()
        && parts.iter().all(|part| {
            matches!(
                *part,
                "do" | "od" | "parzyste" | "nieparzyste" | "parz" | "nieparz"
            ) || (part
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
                && part.chars().all(|character| {
                    character.is_ascii_digit()
                        || character.is_ascii_alphabetic()
                        || matches!(character, '-' | '–' | '—' | '/')
                }))
        })
        && cleaned.chars().any(|character| character.is_ascii_digit())
}

fn push_street_part(streets: &mut Vec<StreetNotice>, part: &str) {
    let part = part
        .trim()
        .trim_start_matches(':')
        .trim()
        .trim_start_matches("ul.")
        .trim();
    if part.is_empty() {
        return;
    }
    if is_number_list(part) {
        if let Some(previous) = streets.last_mut() {
            let value = previous.house_numbers.get_or_insert_with(String::new);
            if !value.is_empty() {
                value.push_str(", ");
            }
            value.push_str(part);
        }
        return;
    }
    let (name, house_numbers) = house_suffix(part);
    let name = name.trim().trim_end_matches(['.', ',', ';']).trim();
    if name.is_empty() {
        return;
    }
    streets.push(StreetNotice {
        name: name.to_string(),
        house_numbers,
    });
}

fn extract_streets(blocks: &[String]) -> Vec<StreetNotice> {
    let mut streets = Vec::new();
    for block in blocks {
        let markers: Vec<_> = street_marker_regex().find_iter(block).collect();
        for (index, marker) in markers.iter().enumerate() {
            let end = markers
                .get(index + 1)
                .map(|next| next.start())
                .unwrap_or(block.len());
            let raw = trim_street_tail(&block[marker.end()..end]);
            for comma_part in raw.split([',', ';']) {
                for part in comma_part
                    .split(" oraz ")
                    .flat_map(|part| part.split(" i "))
                {
                    push_street_part(&mut streets, part);
                }
            }
        }
    }

    let mut deduplicated = Vec::new();
    for street in streets {
        if !deduplicated.iter().any(|existing: &StreetNotice| {
            street_names_match(&existing.name, &street.name)
                && existing.house_numbers == street.house_numbers
        }) {
            deduplicated.push(street);
        }
    }
    deduplicated
}

fn street_words(value: &str) -> Vec<String> {
    crate::utils::clean_text(crate::utils::strip_street_prefixes(value))
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

fn words_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let shortest = left_chars.len().min(right_chars.len());
    if shortest < 4 {
        return false;
    }
    let common = left_chars
        .iter()
        .zip(right_chars.iter())
        .take_while(|(left, right)| left == right)
        .count();
    common >= shortest.saturating_sub(2)
}

fn street_names_match(left: &str, right: &str) -> bool {
    let left_words = street_words(left);
    let right_words = street_words(right);
    !left_words.is_empty()
        && left_words.len() == right_words.len()
        && left_words
            .iter()
            .zip(right_words.iter())
            .all(|(left, right)| words_match(left, right))
}

fn address_matches_notice(
    address: &AddressEntry,
    localities: &[Locality],
    streets: &[StreetNotice],
) -> bool {
    if !localities
        .iter()
        .any(|locality| locality.matches_address(address))
    {
        return false;
    }
    if streets.is_empty() {
        return true;
    }

    crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    )
    .iter()
    .filter(|name| !name.trim().is_empty())
    .any(|address_street| {
        streets
            .iter()
            .any(|notice_street| street_names_match(address_street, &notice_street.name))
    })
}

pub(crate) fn address_house_matches_notice(address: &AddressEntry, location: &str) -> bool {
    if !location.contains(';') {
        return true;
    }
    let candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    for segment in location.split(';').skip(1) {
        let (notice_street, numbers) = house_suffix(segment.trim());
        if candidates
            .iter()
            .filter(|candidate| !candidate.trim().is_empty())
            .any(|candidate| street_names_match(candidate, &notice_street))
        {
            return crate::utils::match_house_number(
                &address.house_no,
                numbers.as_deref().unwrap_or_default(),
                None,
            );
        }
    }
    false
}

fn has_no_active_notice(text: &str) -> bool {
    let normalized = normalize_name(text);
    [
        "brak aktualnych awarii",
        "brak awarii",
        "brak planowanych wylaczen",
        "brak planowanych wylaczen wody",
        "nie ma awarii",
        "nie wystepuja awarie",
        "brak komunikatow",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn looks_like_notice(text: &str) -> bool {
    let normalized = normalize_name(text);
    [
        "awari",
        "braki wody",
        "brak wody",
        "przerw",
        "wylaczen",
        "wstrzyman",
        "dostaw wody",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn parse_notice_html(
    html: &str,
    kind: NoticeKind,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Result<Option<UnifiedAlert>, String> {
    let blocks = extract_blocks(html)?;
    let full_text = blocks.join("\n");
    if full_text.is_empty() || has_no_active_notice(&full_text) || !looks_like_notice(&full_text) {
        return Ok(None);
    }

    let localities = mentioned_localities(&full_text);
    if localities.is_empty() {
        return Err(format!(
            "PEWIK Gdynia {} notice has no supported locality",
            kind.description()
        ));
    }
    let streets = extract_streets(&blocks);
    let (start_date, end_date, parsed_end) = parse_schedule(&full_text);
    if parsed_end.is_some_and(|end| end <= now_local) {
        return Ok(None);
    }

    let matched_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && api_logic::is_address_applicable_for_provider(&AlertSource::PewikGdynia, address)
                && address_matches_notice(address, &localities, &streets)
        })
        .map(|(index, _)| index);

    let locality_label = localities
        .iter()
        .map(|locality| locality.label())
        .collect::<Vec<_>>()
        .join(", ");
    let locality_location = format!("Miejscowość: {locality_label}");
    let location = if matched_address.is_some() && !streets.is_empty() {
        format!(
            "{locality_location}; {}",
            streets
                .iter()
                .map(StreetNotice::location_segment)
                .collect::<Vec<_>>()
                .join("; ")
        )
    } else {
        locality_location
    };

    let mut alert = UnifiedAlert {
        source: AlertSource::PewikGdynia,
        startDate: start_date,
        endDate: end_date,
        message: Some(format!("{}: {full_text}", kind.label())),
        location: Some(location),
        address_index: matched_address,
        is_local: Some(matched_address.is_some()),
        hash: None,
    };
    alert.hash = Some(format!("pewik_gdynia_{}", alert.to_hash()));
    Ok(Some(alert))
}

async fn fetch_notice(
    client: &Client,
    url: &str,
    kind: NoticeKind,
    settings: &Settings,
    now_local: NaiveDateTime,
) -> Result<Option<UnifiedAlert>, String> {
    let response = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 Awaria/1.0")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let html = response.text().await.map_err(|error| error.to_string())?;
    parse_notice_html(&html, kind, settings, now_local)
}

#[async_trait]
impl AlertProvider for PewikGdyniaProvider {
    fn id(&self) -> String {
        "pewik_gdynia".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PewikGdynia
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

        let now_local = Utc::now().with_timezone(&Warsaw).naive_local();
        let endpoints = [
            (PEWIK_GDYNIA_EMERGENCY_URL, NoticeKind::Emergency),
            (PEWIK_GDYNIA_PLANNED_URL, NoticeKind::Planned),
        ];
        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        for (url, kind) in endpoints {
            match fetch_notice(client, url, kind, settings, now_local).await {
                Ok(Some(alert)) => alerts.push(alert),
                Ok(None) => {}
                Err(error) => {
                    let message = format!(
                        "PEWIK Gdynia {} notice fetch error: {error}",
                        kind.description()
                    );
                    log::warn!("{message}");
                    errors.push(message);
                }
            }
        }
        (alerts, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_NOTICE_FIXTURE: &str = r#"
        <div class="message_body">
          <p class="message_body">
            <p>Dzień dobry,</p>
            <p>21.10.2026, Informujemy o wystąpieniu awarii wodociągowej w Gdyni na ulicy Owsianej</p>
            <p>W związku z tym mogą wystąpić braki wody na ulicy:&nbsp; Owsianej,Lnianej</p>
            <p>Przewidywany czas przywrócenia dostawy wody godz. 19.00</p>
            <p>Przepraszamy za utrudnienia.</p>
          </p>
        </div>
    "#;

    fn address(city: &str, street: &str, commune: &str) -> AddressEntry {
        AddressEntry {
            name: "Home".to_string(),
            city_name: city.to_string(),
            voivodeship: "Pomorskie".to_string(),
            commune: commune.to_string(),
            street_name: street.to_string(),
            street_name_1: street.to_string(),
            house_no: "12".to_string(),
            is_active: true,
            ..Default::default()
        }
    }

    fn now(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn parses_live_emergency_notice_with_multiple_streets_and_expected_end() {
        let settings = Settings {
            addresses: vec![address("Gdynia", "Owsiana", "Gdynia")],
            ..Default::default()
        };
        let alert = parse_notice_html(
            LIVE_NOTICE_FIXTURE,
            NoticeKind::Emergency,
            &settings,
            now("2026-10-21T15:00:00"),
        )
        .unwrap()
        .unwrap();

        assert_eq!(alert.source, AlertSource::PewikGdynia);
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-21"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-21T19:00:00"));
        assert_eq!(alert.address_index, Some(0));
        assert_eq!(alert.is_local, Some(true));
        assert!(alert.location.as_deref().unwrap().contains("Owsianej"));
        assert!(alert.location.as_deref().unwrap().contains("Lnianej"));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("Przepraszamy za utrudnienia")
        );
    }

    #[test]
    fn matches_polish_inflected_street_names_and_multiple_notice_locations() {
        let settings = Settings {
            addresses: vec![address("Gdynia", "Lniana", "Gdynia")],
            ..Default::default()
        };
        let alert = parse_notice_html(
            LIVE_NOTICE_FIXTURE,
            NoticeKind::Emergency,
            &settings,
            now("2026-10-21T15:00:00"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(alert.address_index, Some(0));
        assert_eq!(alert.is_local, Some(true));
    }

    #[test]
    fn ignores_empty_and_expired_publications_without_inventing_times() {
        let empty = r#"<div class="message_body"><p>Brak awarii.</p></div>"#;
        assert!(
            parse_notice_html(
                empty,
                NoticeKind::Emergency,
                &Settings::default(),
                now("2026-10-21T15:00:00"),
            )
            .unwrap()
            .is_none()
        );
        assert!(
            parse_notice_html(
                "<!doctype html><html><body></body></html>",
                NoticeKind::Planned,
                &Settings::default(),
                now("2026-10-21T15:00:00"),
            )
            .unwrap()
            .is_none()
        );
        assert!(
            parse_notice_html(
                "<html><body>Unexpected non-empty response</body></html>",
                NoticeKind::Planned,
                &Settings::default(),
                now("2026-10-21T15:00:00"),
            )
            .is_err()
        );

        let alert = parse_notice_html(
            LIVE_NOTICE_FIXTURE,
            NoticeKind::Emergency,
            &Settings::default(),
            now("2026-10-21T19:01:00"),
        )
        .unwrap();
        assert!(alert.is_none());

        let (start, end, _) = parse_schedule("Awaria w Gdyni 21.10.2026 bez podanej godziny");
        assert_eq!(start.as_deref(), Some("2026-10-21"));
        assert_eq!(end, None);
    }

    #[test]
    fn parses_planned_time_ranges_and_house_numbers() {
        let html = r#"
          <div class="message_body">
            <p>Planowane wyłączenie dostawy wody w Gdyni w dniu 22.10.2026 w godz. 8.00-14.00</p>
            <p>Przerwa obejmie ulicę Morską 1-5 oraz 7.</p>
          </div>
        "#;
        let settings = Settings {
            addresses: vec![address("Gdynia", "Morska", "Gdynia")],
            ..Default::default()
        };
        let alert = parse_notice_html(
            html,
            NoticeKind::Planned,
            &settings,
            now("2026-10-21T10:00:00"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(alert.startDate.as_deref(), Some("2026-10-22T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-10-22T14:00:00"));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .starts_with("Planowane wyłączenie wody")
        );
        assert!(
            alert.location.as_deref().unwrap().contains("1-5, 7"),
            "location = {:?}",
            alert.location
        );

        let mut address = address("Gdynia", "Morska", "Gdynia");
        address.house_no = "3".to_string();
        assert!(address_house_matches_notice(
            &address,
            "Miejscowość: Gdynia; Morska 1-5, 7"
        ));
    }

    #[test]
    fn keeps_unmatched_notice_locality_only_and_rejects_other_cities() {
        let settings = Settings {
            addresses: vec![address("Rumia", "Owsiana", "Rumia")],
            ..Default::default()
        };
        let alert = parse_notice_html(
            LIVE_NOTICE_FIXTURE,
            NoticeKind::Emergency,
            &settings,
            now("2026-10-21T15:00:00"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(alert.is_local, Some(false));
        assert_eq!(alert.location.as_deref(), Some("Miejscowość: Gdynia"));
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .contains("Owsianej,Lnianej")
        );
    }
}
