use crate::api_logic::{AlertProvider, AlertSource, Settings, UnifiedAlert};
use crate::utils::retry;
use async_trait::async_trait;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

pub const ENEA_BASE_URL_PRODUCTION: &str = "https://wylaczenia.operator.enea.pl";

fn get_enea_base_url() -> String {
    #[cfg(test)]
    {
        std::env::var("ENEA_BASE_URL").unwrap_or_else(|_| ENEA_BASE_URL_PRODUCTION.to_string())
    }
    #[cfg(not(test))]
    {
        ENEA_BASE_URL_PRODUCTION.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EneaNoticeKind {
    Planned,
    Failure,
}

impl EneaNoticeKind {
    fn page_title(self) -> &'static str {
        match self {
            Self::Planned => "Planowane wyłączenia",
            Self::Failure => "Awarie",
        }
    }

    fn route_name(self) -> &'static str {
        match self {
            Self::Planned => "planned outages",
            Self::Failure => "failures",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EneaItem {
    pub area: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub description: String,
    pub kind: EneaNoticeKind,
}

#[derive(Debug, Default)]
struct EneaFetchResult {
    items: Vec<EneaItem>,
    errors: Vec<String>,
    successful_pages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EneaSchedule {
    start_date: Option<String>,
    end_date: Option<String>,
}

fn page_notice_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("div.unpl.block.info").unwrap())
}

fn heading_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("h1, h2").unwrap())
}

fn schedule_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("p.subtext").unwrap())
}

fn area_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("h4.title_").unwrap())
}

fn description_selector() -> &'static Selector {
    static SELECTOR: OnceLock<Selector> = OnceLock::new();
    SELECTOR.get_or_init(|| Selector::parse("p.description").unwrap())
}

pub const ENEA_REGIONS: &[(u32, &str)] = &[
    (1, "Zielona Góra"),
    (2, "Żary"),
    (3, "Wolsztyn"),
    (4, "Świebodzin"),
    (5, "Nowa Sól"),
    (6, "Krosno Odrzańskie"),
    (7, "Poznań"),
    (8, "Wałcz"),
    (9, "Września"),
    (10, "Szamotuły"),
    (11, "Piła"),
    (12, "Opalenica"),
    (13, "Leszno"),
    (14, "Kościan"),
    (15, "Gniezno"),
    (16, "Chodzież"),
    (17, "Bydgoszcz"),
    (18, "Świecie"),
    (19, "Nakło"),
    (20, "Mogilno"),
    (21, "Inowrocław"),
    (22, "Chojnice"),
    (23, "Szczecin"),
    (24, "Stargard"),
    (25, "Międzyzdroje"),
    (26, "Gryfice"),
    (27, "Goleniów"),
    (28, "Gorzów Wlkp."),
    (29, "Sulęcin"),
    (30, "Międzychód"),
    (31, "Dębno"),
    (32, "Choszczno"),
];

fn iso_datetime_range_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(\d{4}-\d{2}-\d{2})\s+(\d{1,2}:\d{2})\s*-\s*(\d{4}-\d{2}-\d{2})\s+(\d{1,2}:\d{2})",
        )
        .unwrap()
    })
}

fn iso_date_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap())
}

fn polish_date_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?iu)(\d{1,2})\s+([\p{L}]+)\s+(\d{4})\s*r\.?").unwrap())
}

fn time_range_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(\d{1,2}:\d{2})\s*-\s*(\d{1,2}:\d{2})").unwrap())
}

fn restoration_time_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?iu)do\s+godz(?:iny)?\.?\s*(\d{1,2}:\d{2})").unwrap())
}

fn parse_polish_date(text: &str) -> Option<String> {
    let caps = polish_date_regex().captures(text)?;
    let day: u32 = caps.get(1)?.as_str().parse().ok()?;
    let month_name = caps.get(2)?.as_str().to_lowercase();
    let month = match month_name.as_str() {
        "stycznia" => 1,
        "lutego" => 2,
        "marca" => 3,
        "kwietnia" => 4,
        "maja" => 5,
        "czerwca" => 6,
        "lipca" => 7,
        "sierpnia" => 8,
        "września" => 9,
        "października" => 10,
        "listopada" => 11,
        "grudnia" => 12,
        _ => return None,
    };
    let year: i32 = caps.get(3)?.as_str().parse().ok()?;
    chrono::NaiveDate::from_ymd_opt(year, month, day)
        .map(|date| date.format("%Y-%m-%d").to_string())
}

fn format_datetime(date: &str, time: &str) -> Option<String> {
    let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let time = chrono::NaiveTime::parse_from_str(time, "%H:%M").ok()?;
    Some(format!(
        "{}T{}:00",
        date.format("%Y-%m-%d"),
        time.format("%H:%M")
    ))
}

fn parse_enea_schedule(kind: EneaNoticeKind, markup: &str, visible_text: &str) -> EneaSchedule {
    if let Some(caps) = iso_datetime_range_regex().captures(markup)
        && let (Some(start_date), Some(start_time), Some(end_date), Some(end_time)) =
            (caps.get(1), caps.get(2), caps.get(3), caps.get(4))
    {
        let start = format_datetime(start_date.as_str(), start_time.as_str());
        let end = format_datetime(end_date.as_str(), end_time.as_str());
        if start.is_some() && end.is_some() {
            return EneaSchedule {
                start_date: start,
                end_date: end,
            };
        }
    }

    let visible_date = parse_polish_date(visible_text);
    if let Some(caps) = time_range_regex().captures(visible_text)
        && let (Some(date), Some(start_time), Some(end_time)) =
            (visible_date.as_deref(), caps.get(1), caps.get(2))
    {
        let start = format_datetime(date, start_time.as_str());
        let end = format_datetime(date, end_time.as_str());
        if start.is_some() && end.is_some() {
            return EneaSchedule {
                start_date: start,
                end_date: end,
            };
        }
    }

    let mut dates: Vec<String> = iso_date_regex()
        .find_iter(markup)
        .filter_map(|m| chrono::NaiveDate::parse_from_str(m.as_str(), "%Y-%m-%d").ok())
        .map(|date| date.format("%Y-%m-%d").to_string())
        .collect();
    if dates.is_empty()
        && let Some(date) = visible_date
    {
        dates.push(date);
    }

    let start_day = dates.first().cloned();
    let end_day = dates.last().cloned().or_else(|| start_day.clone());
    let end_date = restoration_time_regex()
        .captures(markup)
        .or_else(|| restoration_time_regex().captures(visible_text))
        .and_then(|caps| caps.get(1))
        .and_then(|time| {
            end_day
                .as_deref()
                .and_then(|date| format_datetime(date, time.as_str()))
        });

    let start_date = if kind == EneaNoticeKind::Failure {
        start_day
    } else {
        None
    };

    EneaSchedule {
        start_date,
        end_date,
    }
}

pub fn get_enea_regions_for_district(district: &str) -> Vec<u32> {
    let d = district.to_lowercase();
    let d = d.strip_prefix("m. ").unwrap_or(&d);
    match d {
        "zielonogórski" | "zielona góra" => vec![1],
        "żarski" | "żagański" => vec![2],
        "wolsztyński" => vec![3],
        "świebodziński" => vec![4],
        "nowosolski" | "wschowski" => vec![5],
        "krośnieński" => vec![6],
        "poznański" | "poznań" | "śremski" | "obornicki" => vec![7],
        "wałecki" => vec![8],
        "wrzesiński" | "słupecki" | "średzki" => vec![9],
        "szamotulski" => vec![10],
        "pilski" | "piła" | "złotowski" => vec![11],
        "nowotomyski" | "grodziski" => vec![12],
        "leszczyński" | "leszno" | "gostyński" | "rawicki" => vec![13],
        "kościański" => vec![14],
        "gnieźnieński" | "gniezno" => vec![15],
        "chodzieski" | "czarnkowsko-trzcianecki" => vec![16],
        "bydgoski" | "bydgoszcz" => vec![17],
        "świecki" | "chełmiński" | "tucholski" => vec![18],
        "nakielski" | "sępoleński" => vec![19],
        "mogileński" | "żniński" => vec![20],
        "inowrocławski" | "inowrocław" => vec![21],
        "chojnicki" | "człuchowski" => vec![22],
        "szczeciński" | "szczecin" | "policki" => vec![23],
        "stargardzki" | "pyrzycki" | "stargard" => vec![24],
        "kamieński" | "świnoujście" => vec![25],
        "gryficki" | "łobeski" => vec![26],
        "goleniowski" => vec![27],
        "gorzowski" | "gorzów wlkp." | "gorzów wielkopolski" | "strzelecko-drezdenecki" => {
            vec![28]
        }
        "sulęciński" | "słubicki" => vec![29],
        "międzychodzki" => vec![30],
        "myśliborski" => vec![31],
        "choszczeński" => vec![32],
        _ => Vec::new(),
    }
}

impl EneaItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        let message = match self.kind {
            EneaNoticeKind::Planned => self.description.clone(),
            EneaNoticeKind::Failure => format!("Awaria: {}", self.description),
        };

        UnifiedAlert {
            source: AlertSource::Enea,
            startDate: self.start_date.clone(),
            endDate: self.end_date.clone(),
            message: Some(message),
            location: None,
            address_index: None,
            is_local: None,
            hash: None,
        }
    }
}

pub struct CompiledEneaRegex {
    pub city: regex::Regex,
    pub street_candidates: Vec<regex::Regex>,
}

impl CompiledEneaRegex {
    pub fn new(
        city: &str,
        street_name: &str,
        street_name_1: &str,
        street_name_2: &Option<String>,
    ) -> Self {
        let city_pattern = format!(
            r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)",
            regex::escape(city)
        );
        let city_regex =
            regex::Regex::new(&city_pattern).unwrap_or_else(|_| regex::Regex::new("").unwrap());

        let mut street_candidates = Vec::new();
        if !street_name_1.is_empty() {
            for word in crate::utils::configured_street_name_candidates(
                street_name,
                street_name_1,
                street_name_2.as_deref(),
            ) {
                let p = format!(
                    r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)",
                    regex::escape(&word)
                );
                if let Ok(r) = regex::Regex::new(&p) {
                    street_candidates.push(r);
                }
            }
        }

        Self {
            city: city_regex,
            street_candidates,
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        if !self.city.is_match(text) {
            return false;
        }
        if self.street_candidates.is_empty() {
            return true;
        }
        self.street_candidates.iter().any(|r| r.is_match(text))
    }
}

impl EneaItem {
    pub fn matches_address_compiled(&self, compiled: &CompiledEneaRegex) -> bool {
        compiled.is_match(&self.description)
    }
}

fn selected_text(element: scraper::ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_expected_empty_page(headings: &[String], kind: EneaNoticeKind) -> bool {
    headings.iter().any(|heading| {
        let heading = heading.to_lowercase();
        match kind {
            EneaNoticeKind::Planned => {
                heading.contains("wyłączenia w rejonie dystrybucji")
                    || heading.contains("brak wyłączeń")
                    || heading.contains("brak planowanych wyłączeń")
            }
            EneaNoticeKind::Failure => {
                heading.contains("awarie w rejonie dystrybucji") || heading.contains("brak awarii")
            }
        }
    })
}

pub fn parse_enea_page(html: &str, kind: EneaNoticeKind) -> Result<Vec<EneaItem>, String> {
    let document = Html::parse_document(html);
    let headings: Vec<String> = document
        .select(heading_selector())
        .map(selected_text)
        .collect();
    let h1 = document
        .select(&Selector::parse("h1").unwrap())
        .next()
        .map(selected_text)
        .ok_or_else(|| {
            format!(
                "Enea {} page is missing its main heading",
                kind.route_name()
            )
        })?;

    if !h1.eq_ignore_ascii_case(kind.page_title()) {
        return Err(format!(
            "Unexpected Enea {} page heading: '{}'",
            kind.route_name(),
            h1
        ));
    }

    let cards: Vec<_> = document.select(page_notice_selector()).collect();
    if cards.is_empty() {
        if is_expected_empty_page(&headings, kind) {
            return Ok(Vec::new());
        }
        return Err(format!(
            "Enea {} page had no notice cards or recognized empty-state heading",
            kind.route_name()
        ));
    }

    cards
        .into_iter()
        .enumerate()
        .map(|(index, card)| {
            let area = card
                .select(area_selector())
                .next()
                .map(selected_text)
                .filter(|s| !s.is_empty());
            let description = card
                .select(description_selector())
                .next()
                .map(selected_text)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    format!(
                        "Enea {} notice {} is missing its description",
                        kind.route_name(),
                        index + 1
                    )
                })?;
            let schedule_element = card.select(schedule_selector()).next();
            let schedule = schedule_element
                .map(|element| {
                    parse_enea_schedule(kind, &element.inner_html(), &selected_text(element))
                })
                .unwrap_or(EneaSchedule {
                    start_date: None,
                    end_date: None,
                });

            Ok(EneaItem {
                area,
                start_date: schedule.start_date,
                end_date: schedule.end_date,
                description,
                kind,
            })
        })
        .collect()
}

fn build_enea_region_url(base_url: &str, region_id: u32, kind: EneaNoticeKind) -> String {
    let base_url = base_url.trim_end_matches('/');
    match kind {
        EneaNoticeKind::Planned => format!("{base_url}/index.php?rejon={region_id}"),
        EneaNoticeKind::Failure => format!("{base_url}/index.php?page=awarie&rejon={region_id}"),
    }
}

async fn fetch_enea_region_page(
    client: &Client,
    url: &str,
    kind: EneaNoticeKind,
) -> Result<Vec<EneaItem>, String> {
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP status {}", response.status()));
    }
    let html = response.text().await.map_err(|e| e.to_string())?;
    parse_enea_page(&html, kind)
}

async fn fetch_all_enea_outages_with_errors(
    client: &Client,
    target_regions: &[u32],
) -> Result<EneaFetchResult, String> {
    fetch_all_enea_outages_from(client, target_regions, &get_enea_base_url()).await
}

async fn fetch_all_enea_outages_from(
    client: &Client,
    target_regions: &[u32],
    base_url: &str,
) -> Result<EneaFetchResult, String> {
    let mut result = EneaFetchResult::default();
    if target_regions.is_empty() {
        return Ok(result);
    }

    let semaphore = Arc::new(Semaphore::new(5));
    let mut requests = Vec::new();
    for (region_id, region_name) in ENEA_REGIONS {
        if !target_regions.contains(region_id) {
            continue;
        }
        for kind in [EneaNoticeKind::Planned, EneaNoticeKind::Failure] {
            let sem = semaphore.clone();
            let client = client.clone();
            let region_id = *region_id;
            let region_name = (*region_name).to_string();
            let url = build_enea_region_url(base_url, region_id, kind);
            requests.push(async move {
                let _permit = sem.acquire().await.ok();
                let page_result = retry(
                    || async { fetch_enea_region_page(&client, &url, kind).await },
                    3,
                )
                .await;
                (region_id, region_name, kind, page_result)
            });
        }
    }

    let responses = futures::future::join_all(requests).await;
    for (region_id, region_name, kind, response) in responses {
        match response {
            Ok(items) => {
                result.successful_pages += 1;
                result.items.extend(items);
            }
            Err(error) => result.errors.push(format!(
                "Enea {} page for {} (region {}): {}",
                kind.route_name(),
                region_name,
                region_id,
                error
            )),
        }
    }

    if result.successful_pages == 0 {
        return Err(if result.errors.is_empty() {
            "No Enea region pages were requested".to_string()
        } else {
            result.errors.join("; ")
        });
    }

    Ok(result)
}

#[cfg(test)]
pub async fn fetch_all_enea_outages(
    client: &Client,
    target_regions: &[u32],
) -> Result<Vec<EneaItem>, String> {
    let result = fetch_all_enea_outages_with_errors(client, target_regions).await?;
    for error in result.errors {
        log::warn!("{}", error);
    }
    Ok(result.items)
}

pub struct EneaProvider;

#[async_trait]
impl AlertProvider for EneaProvider {
    fn id(&self) -> String {
        "enea".to_string()
    }

    fn source(&self) -> AlertSource {
        AlertSource::Enea
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let mut target_regions = Vec::new();
        for addr in settings.addresses.iter().filter(|a| {
            a.is_active
                && crate::api_logic::is_address_applicable_for_provider(&AlertSource::Enea, a)
        }) {
            target_regions.extend(get_enea_regions_for_district(&addr.district));
        }
        target_regions.sort();
        target_regions.dedup();

        if target_regions.is_empty() {
            return (Vec::new(), Vec::new());
        }

        match fetch_all_enea_outages_with_errors(client, &target_regions).await {
            Ok(fetch_result) => {
                let errors = fetch_result.errors;
                let items = fetch_result.items;
                let mut alerts = Vec::new();
                let active_addresses: Vec<(usize, Arc<CompiledEneaRegex>, String)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.is_active)
                    .map(|(idx, a)| {
                        (
                            idx,
                            Arc::new(CompiledEneaRegex::new(
                                &a.city_name,
                                &a.street_name,
                                &a.street_name_1,
                                &a.street_name_2,
                            )),
                            a.city_name.clone(),
                        )
                    })
                    .collect();

                for (idx, compiled, city_name) in active_addresses {
                    let local_items: Vec<UnifiedAlert> = items
                        .iter()
                        .filter(|item| item.matches_address_compiled(&compiled))
                        .map(|item| {
                            let mut alert = item.to_unified();
                            alert.address_index = Some(idx);
                            alert.is_local = Some(true);
                            alert.location = Some(format!("Miejscowość: {}", city_name));
                            alert
                        })
                        .collect();
                    alerts.extend(local_items);
                }
                (alerts, errors)
            }
            Err(e) => (Vec::new(), vec![format!("Enea API Error: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_planned_notices_from_region_html() {
        let html = r#"
            <h1>Planowane wyłączenia</h1>
            <h2>Wyłączenia w Rejonie Dystrybucji Poznań</h2>
            <div class="unpl block info">
              <h4 class="title_">Obszar Poznań Grunwald <span>dzisiaj</span></h4>
              <p class="bold subtext">23 października 2026 r. w godz. 08:30 - 11:30
                <!-- 2026-10-23, w godz. 2026-10-23 08:30 - 2026-10-23 11:30 -->
              </p>
              <p class="description">Poznań: ul. Witalisa 43, 45, ul. Złotej Kaczki.</p>
            </div>
            <div class="unpl block info">
              <h4 class="title_">Obszar Plewiska</h4>
              <p class="bold subtext">25 października 2026 r. w godz. 08:00 - 16:00
                <!-- 2026-10-25, w godz. 2026-10-25 08:00 - 2026-10-25 16:00 -->
              </p>
              <p class="description">Plewiska: ul. Szkolna 70, 72, ul. Grunwaldzka od 608 do 630.</p>
            </div>
        "#;

        let items = parse_enea_page(html, EneaNoticeKind::Planned).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].area.as_deref(),
            Some("Obszar Poznań Grunwald dzisiaj")
        );
        assert_eq!(items[0].start_date.as_deref(), Some("2026-10-23T08:30:00"));
        assert_eq!(items[0].end_date.as_deref(), Some("2026-10-23T11:30:00"));
        assert_eq!(
            items[0].description,
            "Poznań: ul. Witalisa 43, 45, ul. Złotej Kaczki."
        );
        assert_eq!(items[1].start_date.as_deref(), Some("2026-10-25T08:00:00"));
        assert_eq!(items[1].end_date.as_deref(), Some("2026-10-25T16:00:00"));
    }

    #[test]
    fn parses_polish_visible_schedule_without_html_comment() {
        let schedule = parse_enea_schedule(
            EneaNoticeKind::Planned,
            "23 października 2026 r. w godz. 08:30 - 11:30",
            "23 października 2026 r. w godz. 08:30 - 11:30",
        );

        assert_eq!(schedule.start_date.as_deref(), Some("2026-10-23T08:30:00"));
        assert_eq!(schedule.end_date.as_deref(), Some("2026-10-23T11:30:00"));
    }

    #[test]
    fn parses_failure_with_end_estimate_and_unknown_start_time() {
        let html = r#"
            <h1>Awarie</h1>
            <h2>Awarie w Rejonie Dystrybucji Poznań</h2>
            <div class="unpl block info">
              <h4 class="title_">Obszar Poznań Grunwald, Poznań Jeżyce</h4>
              <p class="bold subtext">23 października 2026 r. do godziny 21:00
                <!-- 2026-10-23, w godz. 2026-10-23 do godziny 21:00 -->
              </p>
              <p class="description">Poznań os. Na Murawie, ul. Gromadzka 10, 10A, ul. Gronowa 11, 13.</p>
            </div>
        "#;

        let items = parse_enea_page(html, EneaNoticeKind::Failure).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].start_date.as_deref(), Some("2026-10-23"));
        assert_eq!(items[0].end_date.as_deref(), Some("2026-10-23T21:00:00"));
        assert_eq!(
            items[0].to_unified().message.as_deref(),
            Some("Awaria: Poznań os. Na Murawie, ul. Gromadzka 10, 10A, ul. Gronowa 11, 13.")
        );
    }

    #[test]
    fn failure_without_estimated_end_keeps_the_reported_day_only() {
        let schedule = parse_enea_schedule(
            EneaNoticeKind::Failure,
            "<!-- 2026-10-23 -->",
            "23 października 2026 r.",
        );

        assert_eq!(schedule.start_date.as_deref(), Some("2026-10-23"));
        assert_eq!(schedule.end_date, None);
    }

    #[test]
    fn recognized_empty_failure_page_ignores_subscription_copy() {
        let html = r#"
            <h1>Awarie</h1>
            <h2>Brak awarii dla wybranych kryteriów wyszukiwania</h2>
            <p>Informujemy, iż informacje o planowanych wyłączeniach wysyłane są z rejonu dystrybucji Poznań.</p>
        "#;
        assert!(
            parse_enea_page(html, EneaNoticeKind::Failure)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_unrecognized_error_page_and_incomplete_notice_card() {
        assert!(parse_enea_page("<h1>403 Forbidden</h1>", EneaNoticeKind::Failure).is_err());

        let html = r#"
            <h1>Planowane wyłączenia</h1>
            <h2>Wyłączenia w Rejonie Dystrybucji Poznań</h2>
            <div class="unpl block info"><h4 class="title_">Obszar</h4><p class="subtext">data</p></div>
        "#;
        assert!(parse_enea_page(html, EneaNoticeKind::Planned).is_err());
    }

    #[test]
    fn builds_both_region_page_urls() {
        assert_eq!(
            build_enea_region_url(
                "https://wylaczenia.operator.enea.pl/",
                7,
                EneaNoticeKind::Planned
            ),
            "https://wylaczenia.operator.enea.pl/index.php?rejon=7"
        );
        assert_eq!(
            build_enea_region_url(
                "https://wylaczenia.operator.enea.pl",
                7,
                EneaNoticeKind::Failure
            ),
            "https://wylaczenia.operator.enea.pl/index.php?page=awarie&rejon=7"
        );
    }

    #[tokio::test]
    async fn partial_route_failure_preserves_successful_notice_data() {
        let mut server = mockito::Server::new_async().await;
        let planned_html = r#"
            <h1>Planowane wyłączenia</h1>
            <h2>Wyłączenia w Rejonie Dystrybucji Poznań</h2>
            <div class="unpl block info">
              <h4 class="title_">Obszar Poznań</h4>
              <p class="subtext">25 października 2026 r. w godz. 08:00 - 12:00</p>
              <p class="description">Poznań: ul. Kwiatowa 1.</p>
            </div>
        "#;
        let planned = server
            .mock("GET", "/index.php")
            .match_query(mockito::Matcher::Regex("^rejon=7$".to_string()))
            .with_status(200)
            .with_header("content-type", "text/html; charset=utf-8")
            .with_body(planned_html)
            .expect(1)
            .create_async()
            .await;
        let failed = server
            .mock("GET", "/index.php")
            .match_query(mockito::Matcher::Regex("^page=awarie&rejon=7$".to_string()))
            .with_status(503)
            .expect(3)
            .create_async()
            .await;

        let result = fetch_all_enea_outages_from(&Client::new(), &[7], &server.url())
            .await
            .unwrap();

        assert_eq!(result.successful_pages, 1);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].kind, EneaNoticeKind::Planned);
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].contains("failures page for Poznań"));
        planned.assert_async().await;
        failed.assert_async().await;
    }

    #[test]
    fn city_and_street_matching_still_covers_long_failure_address_text() {
        let item = EneaItem {
            area: Some("Obszar Poznań Grunwald".to_string()),
            start_date: Some("2026-10-23".to_string()),
            end_date: Some("2026-10-23T21:00:00".to_string()),
            description: "Poznań os. Na Murawie, ul. Gromadzka 10, 10A, ul. Gronowa 11, 13, ul. Sadowa 1, ul. Wójtowska 1A".to_string(),
            kind: EneaNoticeKind::Failure,
        };

        let gronowa = CompiledEneaRegex::new("Poznań", "Gronowa", "Gronowa", &None);
        let wrong_street = CompiledEneaRegex::new("Poznań", "Witalisa", "Witalisa", &None);
        let wrong_city = CompiledEneaRegex::new("Szczecin", "Gronowa", "Gronowa", &None);
        assert!(item.matches_address_compiled(&gronowa));
        assert!(!item.matches_address_compiled(&wrong_street));
        assert!(!item.matches_address_compiled(&wrong_city));
    }

    #[tokio::test]
    #[ignore = "Live Enea provider check; run explicitly after offline parser tests"]
    async fn test_fetch_enea_live_pages() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client().unwrap();
        let items = fetch_all_enea_outages(&client, &[7]).await.unwrap();
        let planned = items
            .iter()
            .filter(|item| item.kind == EneaNoticeKind::Planned)
            .count();
        let failures = items
            .iter()
            .filter(|item| item.kind == EneaNoticeKind::Failure)
            .count();
        let no_failures_url =
            build_enea_region_url(&get_enea_base_url(), 15, EneaNoticeKind::Failure);
        let no_failures =
            fetch_enea_region_page(&client, &no_failures_url, EneaNoticeKind::Failure)
                .await
                .unwrap();

        println!("Fetched {planned} planned and {failures} Enea failure notices for Poznań");
        assert!(
            no_failures.is_empty(),
            "Gniezno was expected to have no active failures during this live check"
        );
    }
}
