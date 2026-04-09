use reqwest::Client;

pub fn build_client() -> Result<Client, String> {
    reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())
}

pub fn build_client_http1() -> Result<Client, String> {
    reqwest::Client::builder()
        .http1_only()
        .build()
        .map_err(|e| e.to_string())
}

pub fn parse_date(date_str: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{DateTime, TimeZone, Utc, NaiveDateTime};
    
    DateTime::parse_from_rfc3339(date_str)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|nd| Utc.from_utc_datetime(&nd))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(|nd| Utc.from_utc_datetime(&nd))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|nd| Utc.from_utc_datetime(&nd))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M")
                .ok()
                .map(|nd| Utc.from_utc_datetime(&nd))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(date_str, "%d-%m-%Y %H:%M")
                .ok()
                .map(|nd| Utc.from_utc_datetime(&nd))
        })
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
}
