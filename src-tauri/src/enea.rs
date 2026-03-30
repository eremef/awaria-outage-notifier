use crate::api_logic::{AlertSource, UnifiedAlert};
use reqwest::Client;
use serde::Deserialize;
use std::sync::OnceLock;

pub const ENEA_BASE_URL: &str = "https://www.wylaczenia-eneaoperator.pl/rss/rss_unpl_";

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
        _ => (1..=32).collect(),
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
        }
    }

    pub fn matches_address(
        &self,
        city: &str,
        _commune: &str,
        street_name_1: &str,
        street_name_2: &Option<String>,
    ) -> bool {
        let Some(message) = &self.description else {
            return false;
        };

        fn word_match(text: &str, word: &str) -> bool {
            let pattern = format!(r"(?i)\b{}\b", regex::escape(word));
            regex::Regex::new(&pattern)
                .map(|r| r.is_match(text))
                .unwrap_or(false)
        }

        // Match city in description
        if !word_match(message, city) {
            return false;
        }

        let mut candidates: Vec<String> = Vec::new();

        if let Some(n2) = street_name_2 {
            let compound = format!("{} {}", n2.trim(), street_name_1.trim());
            candidates.push(compound);
        }

        for word in street_name_1.split_whitespace() {
            if word.len() >= 3 {
                candidates.push(word.to_string());
            }
        }
        if let Some(n2) = street_name_2 {
            for word in n2.split_whitespace() {
                if word.len() >= 3 {
                    candidates.push(word.to_string());
                }
            }
        }

        if candidates.is_empty() {
            return true;
        }

        candidates.iter().any(|c| word_match(message, c))
    }
}

pub async fn fetch_all_enea_outages(client: &Client, target_regions: &[u32]) -> Result<Vec<EneaItem>, String> {
    let mut futures = Vec::new();
    
    for (id, expected_region) in ENEA_REGIONS {
        if !target_regions.contains(id) {
            continue;
        }
        let url = format!("{}{}.xml", ENEA_BASE_URL, id);
        let expected_name = (*expected_region).to_string();
        futures.push(async move {
            let res = client.get(&url).send().await.ok()?;
            if !res.status().is_success() {
                log::warn!("Enea region {} failed with status {}", id, res.status());
                return None;
            }
            let xml = res.text().await.ok()?;
            let rss: Rss = match quick_xml::de::from_str(&xml) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("Failed to parse XML for Region {}: {}", id, e);
                    return None;
                }
            };
            
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
            
            Some(items)
        });
    }

    let results = futures::future::join_all(futures).await;
    let mut all_items = Vec::new();
    for res in results.into_iter().flatten() {
        all_items.extend(res);
    }
    
    Ok(all_items)
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

        assert!(item.matches_address("Piaski", "", "Piaski", &None));
        assert!(item.matches_address("Świdnica", "", "", &None));
        assert!(!item.matches_address("Wrocław", "", "", &None));

        let kicin = EneaItem {
            title: Some("Kicin, 2026-04-16".to_string()),
            description: Some("Obszar Kicin\nw dniach: 2026-04-16\nKicin: ul. Swarzędzka od 1 do 9, ul. Gwarna 2, 4, ,ul. Poznańska 43, 45, 47.".to_string()),
        };

        let result = kicin.matches_address("Kicin", "", "Poznańska", &None);
        println!("Kicin Poznańska matched: {}", result);
        assert!(result);
    }
}
