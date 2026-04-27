use reqwest::{Client, Error};

pub struct NetworkState {
    pub client: Client,
    pub client_http1: Client,
}

impl NetworkState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client: Self::build_client().map_err(|e| e.to_string())?,
            client_http1: Self::build_client_http1().map_err(|e| e.to_string())?,
        })
    }

    pub fn build_client() -> Result<Client, Error> {
        Client::builder().build()
    }

    pub fn build_client_http1() -> Result<Client, Error> {
        Client::builder().http1_only().build()
    }
}
