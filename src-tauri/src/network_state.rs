use reqwest::Client;

pub struct NetworkState {
    pub client: Client,
    pub client_http1: Client,
}

impl NetworkState {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| format!("Failed to create shared client: {:?}", e))?;
            
        let client_http1 = reqwest::Client::builder()
            .http1_only()
            .build()
            .map_err(|e| format!("Failed to create shared HTTP/1 client: {:?}", e))?;
            
        Ok(Self {
            client,
            client_http1,
        })
    }
}
