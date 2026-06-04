pub fn parse_date(date_str: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, TimeZone, Utc, NaiveDateTime};
    use chrono_tz::Europe::Warsaw;
    
    let parse_local = |nd: NaiveDateTime| {
        Warsaw.from_local_datetime(&nd)
            .latest() // Use latest in case of ambiguous times (DST fallback)
            .or_else(|| Warsaw.from_local_datetime(&nd).earliest()) // or earliest for DST gaps
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc.from_utc_datetime(&nd))
    };

    DateTime::parse_from_rfc3339(date_str)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(parse_local)
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(parse_local)
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(parse_local)
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M")
                .ok()
                .map(parse_local)
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%d-%m-%Y %H:%M")
                .ok()
                .map(parse_local)
        })
        .or_else(|| {
            // Handle "14.05.2026 godz. 08:00" or "14.05.2026 08:00"
            let cleaned = date_str.replace("godz.", "").replace(".", "-").trim().to_string();
            NaiveDateTime::parse_from_str(&cleaned, "%d-%m-%Y %H:%M")
                .ok()
                .map(parse_local)
        })
        .or_else(|| {
            // Handle just "14.05.2026"
            let cleaned = date_str.replace(".", "-").trim().to_string();
            chrono::NaiveDate::parse_from_str(&cleaned, "%d-%m-%Y")
                .ok()
                .map(|nd| parse_local(nd.and_hms_opt(23, 59, 59).unwrap()))
        })
}

pub fn format_date(dt: chrono::DateTime<chrono::Utc>) -> String {
    use chrono_tz::Europe::Warsaw;
    dt.with_timezone(&Warsaw).format("%d-%m-%Y %H:%M").to_string()
}

pub async fn retry<T, E, F, Fut>(mut f: F, max_retries: usize) -> Result<T, E>

where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;
    for attempt in 0..max_retries {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                log::warn!("Attempt {}/{} failed: {}", attempt + 1, max_retries, e);
                last_error = Some(e);
                if attempt < max_retries - 1 {
                    let delay = 200 * (attempt + 1) as u64;
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }
    Err(last_error.unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_first_time() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        
        let result = retry(|| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            async { Ok::<u32, &str>(42) }
        }, 3).await;

        assert_eq!(result, Ok(42));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_success_after_failure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        
        let result = retry(|| {
            let val = counter_clone.fetch_add(1, Ordering::SeqCst);
            async move {
                if val < 2 {
                    Err("fail")
                } else {
                    Ok(42)
                }
            }
        }, 5).await;

        assert_eq!(result, Ok(42));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_eventual_failure() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        
        let result = retry(|| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            async { Err::<u32, &str>("constant fail") }
        }, 3).await;

        assert_eq!(result, Err("constant fail"));
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_parse_numeric_prefixes() {
        assert_eq!(parse_numeric_prefixes("12"), vec![12]);
        assert_eq!(parse_numeric_prefixes("12A"), vec![12]);
        assert_eq!(parse_numeric_prefixes("12/14"), vec![12, 14]);
        assert_eq!(parse_numeric_prefixes("12-14"), vec![12, 14]);
        assert_eq!(parse_numeric_prefixes("od 1 do 9"), vec![1, 9]);
        assert_eq!(parse_numeric_prefixes("budynki 2, 4, 10a"), vec![2, 4, 10]);
        assert_eq!(parse_numeric_prefixes("brak"), Vec::<u32>::new());
    }

    #[test]
    fn test_match_house_number() {
        let match_hn = |u, s| match_house_number(u, s, None);

        // Exact matches and letter ignoring
        assert!(match_hn("15", "15"));
        assert!(match_hn("15A", "15"));
        assert!(match_hn("15", "15b"));
        assert!(match_hn("15/3", "15"));
        assert!(match_hn("15 B", "15"));

        // Multi-lot user address
        assert!(match_hn("12/14", "12"));
        assert!(match_hn("12/14", "14"));
        assert!(!match_hn("12/14", "13"));

        // Comma-separated list matching
        assert!(match_hn("12", "10, 12, 14"));
        assert!(match_hn("12", "10,12,14"));
        assert!(match_hn("12", "posesje: 10 , 12 , 14"));
        assert!(!match_hn("13", "10, 12, 14"));

        // Range matching (hyphen and Polish words)
        assert!(match_hn("15", "10-20"));
        assert!(match_hn("15", "od 10 do 20"));
        assert!(match_hn("15", "od nr 10 do 20"));
        assert!(!match_hn("25", "10-20"));

        // Parity logic (even / odd)
        assert!(match_hn("12", "10-20 parzyste"));
        assert!(match_hn("12", "10-20 parz."));
        assert!(match_hn("12", "10-20 parz"));
        assert!(!match_hn("13", "10-20 parzyste"));

        assert!(match_hn("15", "11-21 nieparzyste"));
        assert!(match_hn("15", "11-21 nieparz."));
        assert!(match_hn("15", "11-21 nieparz"));
        assert!(!match_hn("16", "11-21 nieparzyste"));

        // Empty spec (whole street / no numbers provided)
        assert!(match_hn("15", "cała ulica"));
        assert!(match_hn("15", ""));

        // Segment isolation tests
        let multi_street_spec = "Wschowa ul Wolsztyńska 10-20 parzyste, ul Grunwaldzka 1, 3, 5, ul Kolejowa 11-21 nieparz.";
        assert!(match_house_number("12", multi_street_spec, Some("Wolsztyńska")));
        assert!(!match_house_number("13", multi_street_spec, Some("Wolsztyńska")));
        assert!(match_house_number("3", multi_street_spec, Some("Grunwaldzka")));
        assert!(!match_house_number("2", multi_street_spec, Some("Grunwaldzka")));
        assert!(match_house_number("15", multi_street_spec, Some("Kolejowa")));
        assert!(!match_house_number("12", multi_street_spec, Some("Kolejowa")));

        // Regression: time range matching (e.g. 8:00 do 18:00 shouldn't match house number 1)
        let czestochowa_msg = "Prace planowane - W związku z rozbudową sieci wodociągowej w dn. 3.06.2026 r. mieszkańcy ul. Wały Dwernickiego ( od ul. Kiedrzyńskiej do ul. Cmentarnej ) oraz ul. Dekabrystów 84 zostaną pozbawieni dopływu wody w godz. od 8:00 do 18:00. Za utrudnienia przepraszamy.";
        assert!(!match_house_number("1", czestochowa_msg, Some("Dekabrystów")));
        // Stoen regression test
        let stoen_msg = "Modernizacja sieci 0.4 kV. Adresy: Radzymińska 194, 196, 198, 200, 202 - 202A";
        assert!(match_house_number("200", stoen_msg, Some("Radzymińska")));
        assert!(!match_house_number("201", stoen_msg, Some("Radzymińska")));
    }
}

pub fn clean_text(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            _ => c,
        })
        .collect()
}

pub fn isolate_street_segment(spec: &str, street: &str) -> String {
    let clean_spec = clean_text(spec);
    let clean_street = clean_text(street);
    
    // Find the street name index
    if let Some(idx) = clean_spec.find(&clean_street) {
        let sub = &spec[idx + clean_street.len()..];
        let sub_clean = &clean_spec[idx + clean_street.len()..];
        
        let prefixes = [
            "ul.", "ul ", "al.", "al ", "pl.", "pl ", 
            "ulica", "aleja", "plac", "os.", "osiedle", 
            "rondo", "skwer"
        ];
        
        let mut min_idx = sub.len();
        for p in prefixes {
            if let Some(p_idx) = sub_clean.find(p) {
                if p_idx < min_idx {
                    min_idx = p_idx;
                }
            }
        }
        
        sub[..min_idx].to_string()
    } else {
        // Fallback to significant words
        let words: Vec<&str> = clean_street.split_whitespace().collect();
        let sig_words: Vec<&str> = words.into_iter()
            .filter(|w| w.chars().count() > 3 && !w.chars().all(|c| c.is_numeric()))
            .collect();
        
        for w in sig_words {
            if let Some(idx) = clean_spec.find(w) {
                let sub = &spec[idx + w.len()..];
                let sub_clean = &clean_spec[idx + w.len()..];
                
                let prefixes = [
                    "ul.", "ul ", "al.", "al ", "pl.", "pl ", 
                    "ulica", "aleja", "plac", "os.", "osiedle", 
                    "rondo", "skwer"
                ];
                
                let mut min_idx = sub.len();
                for p in prefixes {
                    if let Some(p_idx) = sub_clean.find(p) {
                        if p_idx < min_idx {
                            min_idx = p_idx;
                        }
                    }
                }
                return sub[..min_idx].to_string();
            }
        }
        
        spec.to_string()
    }
}

pub fn parse_numeric_prefixes(s: &str) -> Vec<u32> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\d+").unwrap());
    re.find_iter(s)
        .filter_map(|m| m.as_str().parse::<u32>().ok())
        .collect()
}

pub fn strip_time_patterns(s: &str) -> String {
    // 1. Remove patterns like HH:MM and HH.MM (e.g. 8:00, 18.00)
    static TIME_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let time_re = TIME_RE.get_or_init(|| regex::Regex::new(r"\d{1,2}[:.]\d{2}").unwrap());
    let s = time_re.replace_all(s, "");

    // 2. Remove hour ranges like "godz. 8-18", "w godz. od 8 do 18", "godz 8 do 18"
    static HOUR_RANGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let hour_range_re = HOUR_RANGE_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)godz\p{L}*\.?\s*(?:od\s*)?\d{1,2}\s*(?:-|do)\s*\d{1,2}").unwrap()
    });
    let s = hour_range_re.replace_all(&s, "");

    s.into_owned()
}

pub fn match_house_number(user_house_no: &str, spec: &str, street_name: Option<&str>) -> bool {
    let spec_stripped = strip_time_patterns(spec);
    let isolated_spec = if let Some(street) = street_name {
        isolate_street_segment(&spec_stripped, street)
    } else {
        spec_stripped
    };

    let user_nums = parse_numeric_prefixes(user_house_no);
    if user_nums.is_empty() {
        return true;
    }

    let spec_lower = isolated_spec.to_lowercase();
    let spec_nums = parse_numeric_prefixes(&spec_lower);
    if spec_nums.is_empty() {
        return true;
    }

    let is_odd = spec_lower.contains("nieparzyste") || spec_lower.contains("nieparz");
    let is_even = (spec_lower.contains("parzyste") || spec_lower.contains("parz")) && !is_odd;

    let check_parity = |n: u32| {
        if is_odd {
            n % 2 != 0
        } else if is_even {
            n % 2 == 0
        } else {
            true
        }
    };

    static RANGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let range_re = RANGE_RE.get_or_init(|| regex::Regex::new(r"(\d+)\s*(?:-|do)\s*(\d+)").unwrap());

    for &u_num in &user_nums {
        if !check_parity(u_num) {
            continue;
        }

        // Check if u_num matches any ranges in the spec
        let mut matched_in_range = false;
        for caps in range_re.captures_iter(&spec_lower) {
            if let (Some(s_match), Some(e_match)) = (caps.get(1), caps.get(2)) {
                if let (Ok(start), Ok(end)) = (s_match.as_str().parse::<u32>(), e_match.as_str().parse::<u32>()) {
                    let min = std::cmp::min(start, end);
                    let max = std::cmp::max(start, end);
                    if u_num >= min && u_num <= max {
                        matched_in_range = true;
                        break;
                    }
                }
            }
        }
        if matched_in_range {
            return true;
        }

        // Check standalone lists
        if spec_nums.contains(&u_num) {
            return true;
        }
    }

    false
}

