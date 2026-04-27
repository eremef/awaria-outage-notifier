use reqwest::{Client, Error};
use tokio::sync::OnceCell;

pub struct NetworkState {
    client_cell: OnceCell<Client>,
    client_http1_cell: OnceCell<Client>,
}

impl NetworkState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            client_cell: OnceCell::new(),
            client_http1_cell: OnceCell::new(),
        })
    }

    pub async fn get_client(&self) -> Result<Client, String> {
        self.client_cell
            .get_or_try_init(|| async {
                Self::build_client().map_err(|e| e.to_string())
            })
            .await
            .map(|c| c.clone())
    }

    pub async fn get_client_http1(&self) -> Result<Client, String> {
        self.client_http1_cell
            .get_or_try_init(|| async {
                Self::build_client_http1().map_err(|e| e.to_string())
            })
            .await
            .map(|c| c.clone())
    }

    pub fn build_client() -> Result<Client, Error> {
        Client::builder().build()
    }

    pub fn build_client_http1() -> Result<Client, Error> {
        Client::builder().http1_only().build()
    }
}
