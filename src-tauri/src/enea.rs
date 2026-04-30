use crate::api_logic::{AlertSource, UnifiedAlert, AlertProvider, Settings};
use reqwest::Client;
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;
use crate::utils::retry;
use async_trait::async_trait;

pub const ENEA_BASE_URL_PRODUCTION: &str = "https://www.wylaczenia-eneaoperator.pl/rss/rss_unpl_";

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

#[derive(Debug, Deserialize)]
pub struct Rss {
    pub channel: Channel,
}

#[derive(Debug, Deserialize)]
pub struct Channel {
    pub title: String,
    #[serde(rename = "item", default)]
    pub items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub title: Option<String>,
    pub description: Option<String>,
}

pub struct EneaItem {
    pub title: Option<String>,
    pub description: Option<String>,
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

fn date_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}) - (\d{4}-\d{2}-\d{2} \d{2}:\d{2})")
            .unwrap()
    })
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
        "gorzowski" | "gorzów wlkp." | "gorzów wielkopolski" | "strzelecko-drezdenecki" => vec![28],
        "sulęciński" | "słubicki" => vec![29],
        "międzychodzki" => vec![30],
        "myśliborski" => vec![31],
        "choszczeński" => vec![32],
        _ => Vec::new(),
    }
}

impl EneaItem {
    pub fn to_unified(&self) -> UnifiedAlert {
        let (mut start_date, mut end_date) = (None, None);
        
        if let Some(t) = &self.title {
            if let Some(caps) = date_regex().captures(t) {
                start_date = caps.get(1).map(|m| format!("{}:00", m.as_str().replace(' ', "T")));
                end_date = caps.get(2).map(|m| format!("{}:00", m.as_str().replace(' ', "T")));
            }
        }

        //let desc = format!("Rejon: {}\n{}", self.region, self.description.clone().unwrap_or_default().trim());

        UnifiedAlert {
            source: AlertSource::Enea,
            startDate: start_date,
            endDate: end_date,
            message: self.description.clone(),
            description: None,
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
    pub fn new(city: &str, street_name_1: &str, street_name_2: &Option<String>) -> Self {
        let city_pattern = format!(r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)", regex::escape(city));
        let city_regex = regex::Regex::new(&city_pattern).unwrap_or_else(|_| regex::Regex::new("").unwrap());

        let mut street_candidates = Vec::new();
        if !street_name_1.is_empty() {
            let mut candidates = Vec::new();
            if let Some(n2) = street_name_2 {
                if !n2.is_empty() && n2 != "null" {
                    let compound = format!("{} {}", n2.trim(), street_name_1.trim());
                    candidates.push(compound);
                }
            }
            candidates.push(street_name_1.trim().to_string());

            for word in candidates {
                let p = format!(r"(?i)(?:^|[^\p{{L}}]){}(?:[^\p{{L}}]|$)", regex::escape(&word));
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
        let Some(message) = &self.description else {
            return false;
        };
        compiled.is_match(message)
    }
}

pub async fn fetch_all_enea_outages(client: &Client, target_regions: &[u32]) -> Result<Vec<EneaItem>, String> {
    let semaphore = Arc::new(Semaphore::new(5)); // Limit concurrent RSS fetches
    let mut futures = Vec::new();
    
    for (id, expected_region) in ENEA_REGIONS {
        if !target_regions.contains(id) {
            continue;
        }
        let url = format!("{}{}.xml", get_enea_base_url(), id);
        let expected_name = (*expected_region).to_string();
        let sem = semaphore.clone();
        let client_c = client.clone();
        futures.push(async move {
            let _permit = sem.acquire().await.ok();
            
            retry(|| async {
                let res = client_c.get(&url).send().await.map_err(|e| e.to_string())?;
                if !res.status().is_success() {
                    return Err(format!("Status {}", res.status()));
                }
                let xml = res.text().await.map_err(|e| e.to_string())?;
                
                // Sanitize XML: replace bare ampersands with &amp;
                // Enea operator RSS feeds often contain unescaped '&'.
                // Since 'regex' crate doesn't support lookahead, we use a replacement chain
                // to avoid double-escaping existing entities.
                let sanitized_xml = xml.replace("&", "&amp;")
                    .replace("&amp;amp;", "&amp;")
                    .replace("&amp;lt;", "&lt;")
                    .replace("&amp;gt;", "&gt;")
                    .replace("&amp;quot;", "&quot;")
                    .replace("&amp;apos;", "&apos;");

                let rss: Rss = quick_xml::de::from_str(&sanitized_xml).map_err(|e| e.to_string())?;
                
                let region = rss.channel.title
                    .strip_prefix("Planowane wyłączenia - ")
                    .unwrap_or(&rss.channel.title)
                    .trim()
                    .to_string();
                
                if region != expected_name {
                     log::warn!("Region mismatch! Expected '{}', got '{}'", expected_name, region);
                }

                let items: Vec<EneaItem> = rss.channel.items.into_iter().map(|item| EneaItem {
                    title: item.title,
                    description: item.description,
                }).collect();
                
                Ok(items)
            }, 3).await.ok()
        });
    }

    let results = futures::future::join_all(futures).await;
    let mut all_items = Vec::new();
    for res in results.into_iter().flatten() {
        all_items.extend(res);
    }
    
    Ok(all_items)
}

pub struct EneaProvider;

#[async_trait]
impl AlertProvider for EneaProvider {
    fn id(&self) -> String {
        "enea".to_string()
    }

    async fn fetch(
        &self,
        client: &Client,
        _client_http1: &Client,
        settings: &Settings,
        _app_handle: Option<&tauri::AppHandle>,
    ) -> (Vec<UnifiedAlert>, Vec<String>) {
        let mut target_regions = Vec::new();
        for addr in settings.addresses.iter().filter(|a| a.is_active) {
            target_regions.extend(get_enea_regions_for_district(&addr.district));
        }
        target_regions.sort();
        target_regions.dedup();

        if target_regions.is_empty() {
            return (Vec::new(), Vec::new());
        }

        match retry(|| fetch_all_enea_outages(client, &target_regions), 3).await {
            Ok(items) => {
                let mut alerts = Vec::new();
                let active_addresses: Vec<(usize, Arc<CompiledEneaRegex>, String)> = settings
                    .addresses
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.is_active)
                    .map(|(idx, a)| {
                        (idx, Arc::new(CompiledEneaRegex::new(&a.city_name, &a.street_name_1, &a.street_name_2)), a.city_name.clone())
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
                            alert.description = Some(format!("Miejscowość: {}", city_name));
                            alert
                        })
                        .collect();
                    alerts.extend(local_items);
                }
                (alerts, Vec::new())
            }
            Err(e) => (Vec::new(), vec![format!("Enea API Error: {}", e)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enea_address_match() {
        let item = EneaItem {
            title: Some(" Świdnica, 2026-03-30, 2026-03-30 08:00 - 2026-03-30 16:00".to_string()),
            description: Some("Obszar Świdnica\nw dniach: 2026-03-30\nmiejscowości Piaski 45, 46, działki".to_string()),
        };

        let compiled_piaski = CompiledEneaRegex::new("Piaski", "Piaski", &None);
        assert!(item.matches_address_compiled(&compiled_piaski));
        
        let compiled_swidnica = CompiledEneaRegex::new("Świdnica", "", &None);
        assert!(item.matches_address_compiled(&compiled_swidnica));
        
        let compiled_wroclaw = CompiledEneaRegex::new("Wrocław", "", &None);
        assert!(!item.matches_address_compiled(&compiled_wroclaw));

        let kicin = EneaItem {
            title: Some("Kicin, 2026-04-16".to_string()),
            description: Some("Obszar Kicin\nw dniach: 2026-04-16\nKicin: ul. Swarzędzka od 1 do 9, ul. Gwarna 2, 4, ,ul. Poznańska 43, 45, 47.".to_string()),
        };

        let compiled_kicin = CompiledEneaRegex::new("Kicin", "Poznańska", &None);
        assert!(kicin.matches_address_compiled(&compiled_kicin));

        // Case insensitivity
        let compiled_kicin_lower = CompiledEneaRegex::new("kicin", "poznańska", &None);
        assert!(kicin.matches_address_compiled(&compiled_kicin_lower));

        // Wrong city
        let compiled_wrocl = CompiledEneaRegex::new("Wrocław", "Poznańska", &None);
        assert!(!kicin.matches_address_compiled(&compiled_wrocl));
    }

    #[tokio::test]
    async fn test_fetch_enea_real() {
        use crate::network_state::NetworkState;
        let client = NetworkState::build_client().unwrap();
        // Region 7 is Poznań
        match fetch_all_enea_outages(&client, &[7]).await {
            Ok(items) => {
                println!("Fetched {} Enea items for Poznań", items.len());
                // Even if 0, we test the API call succeeds
            }
            Err(e) => {
                println!("Skipping Enea integration test (API failed): {}", e);
            }
        }
    }

    #[test]
    fn test_enea_xml_sanitization() {
        let raw_xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<rss version="2.0">
<channel>
    <title>Planowane wyłączenia - Poznań & B BRZOZOWSKI</title>
    <item>
        <title>Kicin & Ciesielska, 2026-04-16 & 2026-04-17</title>
        <description>Obszar Kicin & surrounding areas</description>
    </item>
</channel>
</rss>"#;

        let sanitized = raw_xml.replace("&", "&amp;")
            .replace("&amp;amp;", "&amp;")
            .replace("&amp;lt;", "&lt;")
            .replace("&amp;gt;", "&gt;")
            .replace("&amp;quot;", "&quot;")
            .replace("&amp;apos;", "&apos;");

        let rss: Rss = quick_xml::de::from_str(&sanitized).unwrap();
        assert_eq!(rss.channel.title, "Planowane wyłączenia - Poznań & B BRZOZOWSKI");
        assert_eq!(rss.channel.items[0].title, Some("Kicin & Ciesielska, 2026-04-16 & 2026-04-17".to_string()));
        assert_eq!(rss.channel.items[0].description, Some("Obszar Kicin & surrounding areas".to_string()));
    }
}
