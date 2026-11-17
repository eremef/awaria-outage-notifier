use crate::api_logic::{
    AddressEntry, AlertProvider, AlertSource, Settings, UnifiedAlert,
    is_address_applicable_for_provider,
};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Europe::Warsaw;
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const PEC_TYCHY_CATEGORY_URL: &str = "https://pec.tychy.pl/planowane-wylaczenia-i-awarie";
const TYCHY_SIMC: u64 = 944_534;
const LAZISKA_GORNE_SIMC: u64 = 941_139;

pub struct PecTychyProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoveredCity {
    Tychy,
    LaziskaGorne,
}

impl CoveredCity {
    fn simc(self) -> u64 {
        match self {
            Self::Tychy => TYCHY_SIMC,
            Self::LaziskaGorne => LAZISKA_GORNE_SIMC,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Tychy => "Tychy",
            Self::LaziskaGorne => "Łaziska Górne",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NoticeLink {
    id: u64,
    title: String,
    url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StreetNotice {
    name: String,
    house_specs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParsedArticle {
    title: String,
    body: String,
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

fn normalize_whitespace(value: &str) -> String {
    value
        .replace(['\u{a0}', '\u{202f}'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn element_text(element: ElementRef<'_>) -> String {
    normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "))
}

fn block_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)</?(?:p|div|li|tr|h[1-6])\b[^>]*>|<br\s*/?>").unwrap())
}

fn html_to_lines(html: &str) -> Vec<String> {
    let with_breaks = block_tag_regex().replace_all(html, "\n");
    let document = Html::parse_fragment(&with_breaks);
    document
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join("")
        .lines()
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
        .collect()
}

fn html_to_text(html: &str) -> String {
    html_to_lines(html).join("\n")
}

fn article_id_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^/planowane-wylaczenia-i-awarie/(\d+)-[^/]+/?$").unwrap())
}

/// Reads the official Joomla blog items. The empty-category sentence is the
/// source's explicit empty state; an otherwise unrecognized page is an error.
fn parse_category(html: &str) -> Result<Vec<NoticeLink>, String> {
    let document = Html::parse_document(html);
    let card_selector = Selector::parse(".blog-item[itemprop='blogPost']")
        .map_err(|error| format!("invalid category card selector: {error}"))?;
    let link_selector = Selector::parse("h2[itemprop='name'] a[href]")
        .map_err(|error| format!("invalid category link selector: {error}"))?;
    let body_selector = Selector::parse("body")
        .map_err(|error| format!("invalid category body selector: {error}"))?;
    let pager_selector = Selector::parse(".pagination, a[rel='next']")
        .map_err(|error| format!("invalid pagination selector: {error}"))?;

    if document.select(&pager_selector).next().is_some() {
        return Err("PEC Tychy category pagination is not supported".to_string());
    }

    let base_url = Url::parse(PEC_TYCHY_CATEGORY_URL)
        .map_err(|error| format!("invalid PEC Tychy category URL: {error}"))?;
    let mut links = Vec::new();
    let mut seen_ids = HashSet::new();

    for card in document.select(&card_selector) {
        let link = card
            .select(&link_selector)
            .next()
            .ok_or_else(|| "PEC Tychy category item has no title link".to_string())?;
        let href = link
            .value()
            .attr("href")
            .ok_or_else(|| "PEC Tychy category link has no URL".to_string())?;
        let url = base_url
            .join(href)
            .map_err(|error| format!("invalid PEC Tychy article URL: {error}"))?;
        if url.host_str() != base_url.host_str() || url.scheme() != "https" {
            return Err(format!("unexpected PEC Tychy article host: {}", url));
        }
        let id = article_id_regex()
            .captures(url.path())
            .and_then(|captures| captures.get(1))
            .and_then(|value| value.as_str().parse::<u64>().ok())
            .ok_or_else(|| format!("unrecognized PEC Tychy article URL: {url}"))?;
        if !seen_ids.insert(id) {
            continue;
        }
        let title = element_text(link);
        if title.is_empty() {
            return Err(format!("PEC Tychy article {id} has an empty title"));
        }
        links.push(NoticeLink {
            id,
            title,
            url: url.to_string(),
        });
    }

    if !links.is_empty() {
        return Ok(links);
    }

    let body = document
        .select(&body_selector)
        .next()
        .map(element_text)
        .unwrap_or_default();
    if normalize_polish(&body).contains("w tej kategorii nie ma artykulow") {
        Ok(Vec::new())
    } else {
        Err("PEC Tychy category has neither notices nor the recognized empty state".to_string())
    }
}

fn parse_article(html: &str) -> Result<ParsedArticle, String> {
    let document = Html::parse_document(html);
    let article_selector = Selector::parse(".item-page[itemtype='https://schema.org/Article']")
        .map_err(|error| format!("invalid article selector: {error}"))?;
    let title_selector = Selector::parse("h2[itemprop='headline']")
        .map_err(|error| format!("invalid article title selector: {error}"))?;
    let body_selector = Selector::parse("[itemprop='articleBody']")
        .map_err(|error| format!("invalid article body selector: {error}"))?;
    let article = document
        .select(&article_selector)
        .next()
        .ok_or_else(|| "PEC Tychy article structure was not recognized".to_string())?;
    let title = article
        .select(&title_selector)
        .next()
        .map(element_text)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PEC Tychy article title is missing".to_string())?;
    let body_element = article
        .select(&body_selector)
        .next()
        .ok_or_else(|| "PEC Tychy article body is missing".to_string())?;
    let body = html_to_text(&body_element.inner_html());
    if body.trim().is_empty() {
        return Err("PEC Tychy article body is empty".to_string());
    }
    Ok(ParsedArticle { title, body })
}

fn is_outage_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    let mentions_heat = ["ciepl", "ogrzew", "cwu"]
        .iter()
        .any(|term| normalized.contains(term));
    let mentions_interruption = ["awari", "przerw", "wylaczen", "wstrzym", "brak dostaw"]
        .iter()
        .any(|term| normalized.contains(term));
    mentions_heat && mentions_interruption
}

fn street_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b(?:ul\.?|ulica|ulicy|ulicę|ulicach)\s+").unwrap())
}

fn street_schedule_tail_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\s+(?:w godzinach|od godziny|od godz\.?)\b").unwrap())
}

fn number_spec_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)\b(\d+[a-z]?)(?:\s*[-–—]\s*(\d+[a-z]?))?\b").unwrap())
}

fn split_commas(value: &str) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
            && let Some(previous) = output.last_mut()
        {
            previous.push_str(", ");
            previous.push_str(part);
        } else {
            output.push(part.to_string());
        }
    }
    output
}

fn split_street_chunks(value: &str) -> Vec<String> {
    let conjunction = Regex::new(r"(?i)\s+(?:oraz|i)\s+").unwrap();
    let mut output = Vec::new();
    for piece in conjunction.split(value) {
        output.extend(split_commas(piece));
    }
    output
}

fn parse_street_chunk(chunk: &str) -> Option<StreetNotice> {
    let chunk = chunk
        .trim()
        .trim_matches(|character: char| character.is_whitespace() || ".;,:".contains(character));
    if chunk.is_empty() {
        return None;
    }

    let house_start = number_spec_regex()
        .captures_iter(chunk)
        .filter_map(|captures| captures.get(0))
        .find(|number| chunk[..number.start()].chars().any(char::is_alphabetic));
    let (name, house_text) = match house_start {
        Some(number) => (&chunk[..number.start()], &chunk[number.start()..]),
        None => (chunk, ""),
    };
    let name = name
        .trim()
        .trim_matches(|character: char| character.is_whitespace() || ".;,:".contains(character))
        .to_string();
    if name.is_empty() {
        return None;
    }
    let house_specs = number_spec_regex()
        .captures_iter(house_text)
        .filter_map(|captures| {
            captures
                .get(0)
                .map(|value| value.as_str().trim().to_string())
        })
        .collect();
    Some(StreetNotice { name, house_specs })
}

fn extract_affected_streets(text: &str) -> Vec<StreetNotice> {
    let mut streets = Vec::new();
    for line in text.lines() {
        let markers = street_marker_regex().find_iter(line).collect::<Vec<_>>();
        for (index, marker) in markers.iter().enumerate() {
            let end = markers
                .get(index + 1)
                .map(|next| next.start())
                .unwrap_or(line.len());
            let fragment = line[marker.end()..end].trim();
            // Avoid collecting following schedule prose as part of a street name.
            let fragment = street_schedule_tail_regex()
                .split(fragment)
                .next()
                .unwrap_or(fragment)
                .trim_end_matches(|character: char| ".;:".contains(character));
            for chunk in split_street_chunks(fragment) {
                if let Some(street) = parse_street_chunk(&chunk)
                    && !streets.iter().any(|existing: &StreetNotice| {
                        normalize_polish(&existing.name) == normalize_polish(&street.name)
                    })
                {
                    streets.push(street);
                }
            }
        }
    }
    streets
}

fn schedule_regexes() -> (&'static Regex, &'static Regex) {
    static DATE: OnceLock<Regex> = OnceLock::new();
    static TIME: OnceLock<Regex> = OnceLock::new();
    (
        DATE.get_or_init(|| Regex::new(r"\b(\d{1,2})[.\-/](\d{1,2})[.\-/](\d{4})\b").unwrap()),
        TIME.get_or_init(|| Regex::new(r"\b(\d{1,2}):(\d{2})\b").unwrap()),
    )
}

fn parse_schedule_with_fallback_date(
    text: &str,
    fallback_date: Option<NaiveDate>,
) -> (Option<String>, Option<String>) {
    let (date_regex, time_regex) = schedule_regexes();
    let has_date_expression = date_regex.is_match(text);
    let dates = date_regex
        .captures_iter(text)
        .filter_map(|captures| {
            NaiveDate::from_ymd_opt(
                captures.get(3)?.as_str().parse().ok()?,
                captures.get(2)?.as_str().parse().ok()?,
                captures.get(1)?.as_str().parse().ok()?,
            )
        })
        .collect::<Vec<_>>();
    let times = time_regex
        .captures_iter(text)
        .filter_map(|captures| {
            NaiveTime::from_hms_opt(
                captures.get(1)?.as_str().parse().ok()?,
                captures.get(2)?.as_str().parse().ok()?,
                0,
            )
        })
        .collect::<Vec<_>>();
    let start_date = dates
        .first()
        .copied()
        .or_else(|| (!has_date_expression).then_some(fallback_date).flatten());
    let Some(start_date) = start_date else {
        return (None, None);
    };

    let start_time = times.first().copied();
    let start = format_schedule_point(start_date, start_time);
    let end = if let Some(explicit_end_date) = dates.get(1).copied() {
        Some(format_schedule_point(
            explicit_end_date,
            times.get(1).copied(),
        ))
    } else if let Some(end_time) = times.get(1).copied() {
        let mut end_date = start_date;
        if start_time.is_some_and(|start_time| end_time <= start_time) {
            end_date += Duration::days(1);
        }
        Some(format_schedule_point(end_date, Some(end_time)))
    } else {
        None
    };
    (Some(start), end)
}

fn format_schedule_point(date: NaiveDate, time: Option<NaiveTime>) -> String {
    match time {
        Some(time) => format!("{}T{}", date.format("%Y-%m-%d"), time.format("%H:%M:%S")),
        None => date.format("%Y-%m-%d").to_string(),
    }
}

fn today_in_warsaw() -> NaiveDate {
    Utc::now().with_timezone(&Warsaw).date_naive()
}

fn is_emergency_notice(text: &str) -> bool {
    let normalized = normalize_polish(text);
    normalized.contains("awari") && !normalized.contains("planowan")
}

fn street_search_terms(street: &str) -> Vec<String> {
    let street = street.trim();
    let mut terms = vec![street.to_string()];
    let lower = street.to_lowercase();
    for (suffix, nominative_suffix) in [("skiej", "ska"), ("ckiej", "cka")] {
        if lower.ends_with(suffix) && street.len() > suffix.len() {
            terms.push(format!(
                "{}{}",
                &street[..street.len() - suffix.len()],
                nominative_suffix
            ));
            break;
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

fn street_exists_in_city(
    street: &str,
    city: CoveredCity,
    app_handle: Option<&tauri::AppHandle>,
    teryt_path: Option<&Path>,
) -> bool {
    for term in street_search_terms(street) {
        let result = if let Some(app_handle) = app_handle {
            crate::teryt::lookup_streets(app_handle, city.simc(), &term)
        } else if let Some(path) = teryt_path {
            crate::teryt::lookup_streets_by_path(path, city.simc(), &term)
        } else {
            return false;
        };
        if result.is_ok_and(|streets| !streets.is_empty()) {
            return true;
        }
    }
    false
}

/// Resolve a notice only when every named street belongs to exactly one of
/// PEC's two cities. A shared name such as `Nowa` cannot resolve a notice alone.
fn infer_notice_city(
    streets: &[StreetNotice],
    app_handle: Option<&tauri::AppHandle>,
    teryt_path: Option<&Path>,
) -> Option<CoveredCity> {
    if streets.is_empty() {
        return None;
    }
    let belongs_to = |city| {
        streets
            .iter()
            .all(|street| street_exists_in_city(&street.name, city, app_handle, teryt_path))
    };
    match (
        belongs_to(CoveredCity::Tychy),
        belongs_to(CoveredCity::LaziskaGorne),
    ) {
        (true, false) => Some(CoveredCity::Tychy),
        (false, true) => Some(CoveredCity::LaziskaGorne),
        _ => None,
    }
}

fn default_teryt_path() -> Option<PathBuf> {
    std::env::var_os("AWARIA_TERYT_PATH")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets")
                .join("teryt");
            path.is_file().then_some(path)
        })
}

fn location_text(city: Option<CoveredCity>, streets: &[StreetNotice]) -> String {
    let locality = city.map_or_else(
        || "Miejscowość nierozpoznana".to_string(),
        |city| city.label().to_string(),
    );
    let mut parts = vec![locality];
    parts.extend(streets.iter().map(|street| {
        if street.house_specs.is_empty() {
            street.name.clone()
        } else {
            format!("{} ({})", street.name, street.house_specs.join(", "))
        }
    }));
    parts.join("; ")
}

fn is_address_in_city(address: &AddressEntry, city: CoveredCity) -> bool {
    if !is_address_applicable_for_provider(&AlertSource::PecTychy, address) {
        return false;
    }
    let city_name = normalize_covered_area_name(&address.city_name);
    let commune = normalize_covered_area_name(&address.commune);
    let expected = normalize_polish(city.label());
    let other = match city {
        CoveredCity::Tychy => CoveredCity::LaziskaGorne,
        CoveredCity::LaziskaGorne => CoveredCity::Tychy,
    };
    if city_name == normalize_polish(other.label()) {
        return false;
    }
    if city_name == expected {
        return address.city_id.is_none_or(|city_id| city_id == city.simc());
    }
    if let Some(city_id) = address.city_id {
        if city_id == other.simc() {
            return false;
        }
        if city_id == city.simc() {
            return true;
        }
    }
    commune == expected
}

fn normalize_covered_area_name(value: &str) -> String {
    let normalized = normalize_polish(value.trim());
    let mut words = normalized
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    for prefix in [
        &["gmina", "miejsko", "wiejska"][..],
        &["gmina", "miejska"][..],
        &["gmina", "wiejska"][..],
        &["gmina"][..],
        &["gm"][..],
        &["miasto"][..],
        &["m"][..],
    ] {
        if words.starts_with(prefix) {
            words.drain(..prefix.len());
            break;
        }
    }
    words.join(" ")
}

pub(super) fn location_matches_address(location: &str, address: &AddressEntry) -> bool {
    let mut parts = location.split(';').map(str::trim);
    let Some(notice_city) = parts.next() else {
        return false;
    };
    let city = [CoveredCity::Tychy, CoveredCity::LaziskaGorne]
        .into_iter()
        .find(|city| normalize_polish(notice_city) == normalize_polish(city.label()));
    let Some(city) = city else {
        return false;
    };
    if !is_address_in_city(address, city) {
        return false;
    }

    let street_candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    parts.any(|segment| {
        street_candidates
            .iter()
            .any(|street| !street.trim().is_empty() && street_matches(segment, street))
    })
}

fn street_matches(location_segment: &str, street: &str) -> bool {
    let street = crate::utils::strip_street_prefixes(street.trim());
    let expected = normalize_polish(street)
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() > 2)
        .map(|token| {
            let length = token.chars().count().saturating_sub(2).max(3);
            token.chars().take(length).collect::<String>()
        })
        .collect::<Vec<_>>();
    let normalized_location = normalize_polish(location_segment);
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

fn parse_house_number(value: &str) -> Option<(u32, String)> {
    let captures = number_spec_regex().captures(value)?;
    let start = captures.get(1)?.as_str();
    let number = start
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .ok()?;
    let suffix = start
        .chars()
        .skip_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .to_ascii_lowercase();
    Some((number, suffix))
}

pub(super) fn house_number_matches_notice(location: &str, address: &AddressEntry) -> bool {
    if address.house_no.trim().is_empty() {
        return true;
    }
    let street_candidates = crate::utils::configured_street_name_candidates(
        &address.street_name,
        &address.street_name_1,
        address.street_name_2.as_deref(),
    );
    let matched_segments = location
        .split(';')
        .skip(1)
        .filter(|segment| {
            street_candidates
                .iter()
                .any(|street| !street.trim().is_empty() && street_matches(segment, street))
        })
        .collect::<Vec<_>>();
    if matched_segments.is_empty() {
        return true;
    }

    let mut had_known_numbers = false;
    for segment in matched_segments {
        let Some((_, number_text)) = segment.split_once('(') else {
            continue;
        };
        let number_text = number_text.trim_end_matches(')').trim();
        let specs = number_spec_regex()
            .captures_iter(number_text)
            .collect::<Vec<_>>();
        if specs.is_empty() {
            continue;
        }
        had_known_numbers = true;
        let Some((requested_number, requested_suffix)) = parse_house_number(&address.house_no)
        else {
            continue;
        };
        for spec in specs {
            let Some(start_text) = spec.get(1).map(|value| value.as_str()) else {
                continue;
            };
            let Some((start_number, start_suffix)) = parse_house_number(start_text) else {
                continue;
            };
            let end = spec
                .get(2)
                .and_then(|value| parse_house_number(value.as_str()));
            let matches = if let Some((end_number, end_suffix)) = end {
                if start_number != end_number {
                    (start_number.min(end_number)..=start_number.max(end_number))
                        .contains(&requested_number)
                } else {
                    let requested = (requested_number, requested_suffix.as_str());
                    let (lower, upper) = if (start_number, start_suffix.as_str())
                        <= (end_number, end_suffix.as_str())
                    {
                        (
                            (start_number, start_suffix.as_str()),
                            (end_number, end_suffix.as_str()),
                        )
                    } else {
                        (
                            (end_number, end_suffix.as_str()),
                            (start_number, start_suffix.as_str()),
                        )
                    };
                    requested >= lower && requested <= upper
                }
            } else {
                requested_number == start_number
                    && (start_suffix.is_empty()
                        || requested_suffix.is_empty()
                        || start_suffix == requested_suffix)
            };
            if matches {
                return true;
            }
        }
    }
    !had_known_numbers
}

fn build_alert(
    article_id: u64,
    article: ParsedArticle,
    settings: &Settings,
    app_handle: Option<&tauri::AppHandle>,
    teryt_path: Option<&Path>,
) -> Option<UnifiedAlert> {
    build_alert_at_date(
        article_id,
        article,
        settings,
        app_handle,
        teryt_path,
        today_in_warsaw(),
    )
}

fn build_alert_at_date(
    article_id: u64,
    article: ParsedArticle,
    settings: &Settings,
    app_handle: Option<&tauri::AppHandle>,
    teryt_path: Option<&Path>,
    today: NaiveDate,
) -> Option<UnifiedAlert> {
    let combined = format!("{}\n{}", article.title, article.body);
    if !is_outage_notice(&combined) {
        return None;
    }

    let streets = extract_affected_streets(&article.body);
    let city = infer_notice_city(&streets, app_handle, teryt_path);
    let location = location_text(city, &streets);
    let fallback_date = is_emergency_notice(&combined).then_some(today);
    let (start_date, end_date) = parse_schedule_with_fallback_date(&combined, fallback_date);
    let matching_address = settings
        .addresses
        .iter()
        .enumerate()
        .find(|(_, address)| {
            address.is_active
                && is_address_applicable_for_provider(&AlertSource::PecTychy, address)
                && location_matches_address(&location, address)
        })
        .map(|(index, _)| index);
    let has_covered_address = settings.addresses.iter().any(|address| {
        address.is_active && is_address_applicable_for_provider(&AlertSource::PecTychy, address)
    });

    let normalized = normalize_polish(&combined);
    let kind = if normalized.contains("planowan") {
        "Planowana przerwa w dostawie ciepła"
    } else if normalized.contains("awari") {
        "Awaria ciepła"
    } else {
        "Przerwa w dostawie ciepła"
    };
    let message = format!("{kind}: {}\n{}", article.title, article.body);
    let mut alert = UnifiedAlert {
        source: AlertSource::PecTychy,
        startDate: start_date,
        endDate: end_date,
        message: Some(message),
        location: Some(location),
        address_index: matching_address,
        // The central matcher re-evaluates streets and the optional house filter.
        is_local: Some(has_covered_address),
        hash: None,
    };
    alert.hash = Some(format!("pec_tychy_{article_id}_{}", alert.to_hash()));
    Some(alert)
}

async fn fetch_text(client: &Client, url: &str) -> Result<String, String> {
    let response = client
        .get(url)
        .header(
            "User-Agent",
            "Awaria/1.0 (+https://github.com/eremef/awaria)",
        )
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP status {}", response.status()));
    }
    response
        .text()
        .await
        .map_err(|error| format!("response read failed: {error}"))
}

#[async_trait]
impl AlertProvider for PecTychyProvider {
    fn id(&self) -> String {
        "pec_tychy".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::PecTychy
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        if !crate::api_logic::is_provider_applicable(self.source(), settings) {
            return (Vec::new(), Vec::new());
        }

        let category_html =
            match crate::utils::retry(|| fetch_text(client, PEC_TYCHY_CATEGORY_URL), 3).await {
                Ok(html) => html,
                Err(error) => {
                    return (
                        Vec::new(),
                        vec![format!("PEC Tychy category request error: {error}")],
                    );
                }
            };
        let links = match parse_category(&category_html) {
            Ok(links) => links,
            Err(error) => {
                return (
                    Vec::new(),
                    vec![format!("PEC Tychy category error: {error}")],
                );
            }
        };
        let teryt_path = default_teryt_path();
        let mut alerts = Vec::new();
        let mut errors = Vec::new();
        for link in links {
            let html = match crate::utils::retry(|| fetch_text(client, &link.url), 3).await {
                Ok(html) => html,
                Err(error) => {
                    errors.push(format!(
                        "PEC Tychy article {} request error: {error}",
                        link.id
                    ));
                    continue;
                }
            };
            let article = match parse_article(&html) {
                Ok(article) => article,
                Err(error) => {
                    errors.push(format!(
                        "PEC Tychy article {} parse error: {error}",
                        link.id
                    ));
                    continue;
                }
            };
            if let Some(alert) = build_alert(
                link.id,
                article,
                settings,
                app_handle,
                teryt_path.as_deref(),
            ) {
                alerts.push(alert);
            }
        }
        (crate::api_logic::deduplicate_alerts(alerts), errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const CURRENT_CATEGORY: &str = r#"
      <div class="blog-item mb-5" itemprop="blogPost" itemscope itemtype="https://schema.org/BlogPosting">
        <div class="item-content"><div class="page-header"><h2 itemprop="name">
          <a href="/planowane-wylaczenia-i-awarie/507-awaria-na-osiedlu-sosnowe" itemprop="url">Awaria na osiedlu Sosnowe</a>
        </h2></div><div class="introtext">Awaria sieci ciepłowniczej</div></div>
      </div>
    "#;
    const CURRENT_ARTICLE: &str = r#"
      <div class="item-page" itemscope itemtype="https://schema.org/Article">
        <div class="page-header"><h2 itemprop="headline">Awaria na osiedlu Sosnowe</h2></div>
        <div itemprop="articleBody"><p>W związku z koniecznością usunięcia awarii na sieci ciepłowniczej nastąpi<br>
          przerwa dostawy energii cieplnej w godzinach od 8:00 do 17:00 obejmująca ul. Żaków,Nowa,Żorska.<br>
          Przepraszamy za utrudnienia.</p></div>
      </div>
    "#;
    const HISTORIC_ARTICLE: &str = r#"
      <div class="item-page" itemscope itemtype="https://schema.org/Article">
        <h2 itemprop="headline">Awaria sieci ciepłowniczej</h2>
        <div itemprop="articleBody"><p>PEC informuje o konieczności usunięcia awarii.</p>
          <p>Na osiedlu H-6 w dniu 15-02-2022 wyłączono dostawę ciepła dla budynków przy ul. Owczarskiej oraz Harcerskiej 31-41 i Hetmańskiej 2-14.</p>
        </div>
      </div>
    "#;

    fn settings(addresses: Vec<AddressEntry>, filter_by_house_no: bool) -> Settings {
        Settings {
            addresses,
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

    fn address(city: &str, city_id: u64, street: &str, house_no: &str) -> AddressEntry {
        AddressEntry {
            name: "Home".into(),
            city_name: city.into(),
            voivodeship: "Śląskie".into(),
            district: String::new(),
            commune: city.into(),
            street_name: street.into(),
            street_name_1: street.into(),
            street_name_2: None,
            house_no: house_no.into(),
            city_id: Some(city_id),
            street_id: None,
            is_active: true,
        }
    }

    fn bundled_teryt() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("teryt")
    }

    #[test]
    fn parses_current_category_and_distinguishes_empty_from_unrecognized() {
        let links = parse_category(CURRENT_CATEGORY).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].id, 507);
        assert_eq!(
            links[0].url,
            "https://pec.tychy.pl/planowane-wylaczenia-i-awarie/507-awaria-na-osiedlu-sosnowe"
        );

        let empty = r#"<html><body><div class="category-desc">W tej kategorii nie ma artykułów.</div></body></html>"#;
        assert!(parse_category(empty).unwrap().is_empty());
        assert!(parse_category("<html><body>changed page</body></html>").is_err());
        assert!(
            parse_category(&CURRENT_CATEGORY.replace("507-", "999-"))
                .unwrap()
                .iter()
                .any(|link| link.id == 999)
        );
    }

    #[test]
    fn parses_current_date_less_notice_and_resolves_every_street_to_tychy() {
        let article = parse_article(CURRENT_ARTICLE).unwrap();
        let streets = extract_affected_streets(&article.body);
        assert_eq!(
            streets
                .iter()
                .map(|street| street.name.as_str())
                .collect::<Vec<_>>(),
            ["Żaków", "Nowa", "Żorska"]
        );
        assert!(streets.iter().all(|street| street.house_specs.is_empty()));
        assert_eq!(
            parse_schedule_with_fallback_date(&article.body, None),
            (None, None)
        );

        let city = infer_notice_city(&streets, None, Some(&bundled_teryt()));
        assert_eq!(city, Some(CoveredCity::Tychy));
        let location = location_text(city, &streets);
        assert!(location_matches_address(
            &location,
            &address("Tychy", TYCHY_SIMC, "Nowa", "1")
        ));
        assert!(!location_matches_address(
            &location,
            &address("Łaziska Górne", LAZISKA_GORNE_SIMC, "Nowa", "1")
        ));
    }

    #[test]
    fn a_shared_street_alone_does_not_resolve_the_notice_city() {
        let streets = vec![StreetNotice {
            name: "Nowa".into(),
            house_specs: Vec::new(),
        }];
        assert_eq!(
            infer_notice_city(&streets, None, Some(&bundled_teryt())),
            None
        );
    }

    #[test]
    fn locality_within_a_covered_commune_matches_its_resolved_city() {
        let mut laziska_dolne = address("Łaziska Dolne", 941_168, "Nowa", "1");
        laziska_dolne.commune = "Łaziska Górne".into();
        assert!(is_address_applicable_for_provider(
            &AlertSource::PecTychy,
            &laziska_dolne
        ));
        assert!(is_address_in_city(
            &laziska_dolne,
            CoveredCity::LaziskaGorne
        ));
        assert!(street_matches("Nowa", "Nowa"));
        assert!(location_matches_address(
            "Łaziska Górne; Nowa",
            &laziska_dolne
        ));
        assert!(!location_matches_address("Tychy; Nowa", &laziska_dolne));
    }

    #[test]
    fn parses_historical_ranges_and_keeps_them_attached_to_each_street() {
        let article = parse_article(HISTORIC_ARTICLE).unwrap();
        let streets = extract_affected_streets(&article.body);
        assert_eq!(streets.len(), 3);
        assert_eq!(streets[0].name, "Owczarskiej");
        assert!(streets[0].house_specs.is_empty());
        assert_eq!(streets[1].name, "Harcerskiej");
        assert_eq!(streets[1].house_specs, ["31-41"]);
        assert_eq!(streets[2].name, "Hetmańskiej");
        assert_eq!(streets[2].house_specs, ["2-14"]);
        assert_eq!(
            parse_schedule_with_fallback_date(&article.body, None)
                .0
                .as_deref(),
            Some("2022-02-15")
        );
        assert_eq!(
            parse_schedule_with_fallback_date(&article.body, None).1,
            None
        );
        assert_eq!(
            infer_notice_city(&streets, None, Some(&bundled_teryt())),
            Some(CoveredCity::Tychy)
        );

        let location = location_text(Some(CoveredCity::Tychy), &streets);
        assert!(house_number_matches_notice(
            &location,
            &address("Tychy", TYCHY_SIMC, "Harcerska", "35")
        ));
        assert!(!house_number_matches_notice(
            &location,
            &address("Tychy", TYCHY_SIMC, "Harcerska", "42")
        ));
        assert!(house_number_matches_notice(
            &location,
            &address("Tychy", TYCHY_SIMC, "Owczarska", "5")
        ));
    }

    #[test]
    fn date_less_emergency_uses_warsaw_day_and_keeps_article_identity() {
        let article = parse_article(CURRENT_ARTICLE).unwrap();
        let mut home_settings = settings(vec![address("Tychy", TYCHY_SIMC, "Żaków", "10")], true);
        let today = NaiveDate::from_ymd_opt(2026, 11, 17).unwrap();
        let alert = build_alert_at_date(
            507,
            article,
            &home_settings,
            None,
            Some(&bundled_teryt()),
            today,
        )
        .unwrap();
        assert_eq!(alert.startDate.as_deref(), Some("2026-11-17T08:00:00"));
        assert_eq!(alert.endDate.as_deref(), Some("2026-11-17T17:00:00"));
        assert!(alert.hash.unwrap().starts_with("pec_tychy_507_"));

        let empty_settings = settings(Vec::new(), false);
        let date_only_emergency = ParsedArticle {
            title: "Awaria sieci ciepłowniczej".into(),
            body: "Awaria sieci ciepłowniczej w Tychach.".into(),
        };
        let date_only_alert = build_alert_at_date(
            509,
            date_only_emergency,
            &empty_settings,
            None,
            Some(&bundled_teryt()),
            today,
        )
        .unwrap();
        assert_eq!(date_only_alert.startDate.as_deref(), Some("2026-11-17"));
        assert_eq!(date_only_alert.endDate, None);
        assert_eq!(
            parse_schedule_with_fallback_date("Awaria od 22:00 do 02:00", Some(today)),
            (
                Some("2026-11-17T22:00:00".into()),
                Some("2026-11-18T02:00:00".into())
            )
        );
        assert_eq!(
            parse_schedule_with_fallback_date(
                "Awaria dnia 99-99-2026 od 08:00 do 17:00",
                Some(today)
            ),
            (None, None)
        );

        home_settings.addresses.clear();
        assert!(
            build_alert(
                507,
                parse_article(CURRENT_ARTICLE).unwrap(),
                &home_settings,
                None,
                Some(&bundled_teryt())
            )
            .is_some()
        );
    }

    #[test]
    fn valid_article_and_bad_article_structure_are_distinguished() {
        let article = parse_article(CURRENT_ARTICLE).unwrap();
        assert!(is_outage_notice(&format!(
            "{} {}",
            article.title, article.body
        )));
        assert!(parse_article("<html><body>not an article</body></html>").is_err());
        assert!(parse_article(
            r#"<div class="item-page" itemtype="https://schema.org/Article"><h2 itemprop="headline">Awaria</h2></div>"#
        )
        .is_err());
    }

    #[test]
    fn planned_wording_takes_precedence_over_repair_reference_to_a_failure() {
        let article = ParsedArticle {
            title: "Planowane wyłączenie ciepła".into(),
            body: "Planowana przerwa w dostawie ciepła z powodu usunięcia awarii.".into(),
        };
        let alert = build_alert(
            508,
            article,
            &settings(Vec::new(), false),
            None,
            Some(&bundled_teryt()),
        )
        .unwrap();
        assert!(
            alert
                .message
                .as_deref()
                .unwrap()
                .starts_with("Planowana przerwa w dostawie ciepła:")
        );
        assert_eq!(alert.startDate, None);
        assert_eq!(alert.endDate, None);
    }

    #[tokio::test]
    async fn fetch_text_reports_non_success_http_status() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/category")
            .with_status(503)
            .create_async()
            .await;
        let client = Client::new();
        let error = fetch_text(&client, &format!("{}/category", server.url()))
            .await
            .unwrap_err();
        assert!(error.contains("HTTP status 503"));
    }
}
