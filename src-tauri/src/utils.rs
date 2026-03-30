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
