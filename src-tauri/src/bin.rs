use reqwest::Client;
use std::error::Error;

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = Client::builder()
        .http1_only()
        .timeout(std::time::Duration::from_secs(30))
        .build().unwrap();
    match client.get("https://wodociagi-kalisz.pl/Wy%C5%82%C4%85czenia").send().await {
        Ok(_) => println!("Success!"),
        Err(e) => {
            println!("Error: {}", e);
            let mut current = e.source();
            while let Some(cause) = current {
                println!("Caused by: {}", cause);
                current = cause.source();
            }
        }
    }
}
