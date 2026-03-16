use anyhow::{Context, Result};
use reqwest::{Client, RequestBuilder};

use crate::types::{
    AxleCheckRequest, AxleCheckResponse, AxleDisproveRequest, AxleDisproveResponse,
    AxleSorry2LemmaRequest, AxleSorry2LemmaResponse,
};

pub struct AxleClient {
    api_key: Option<String>,
    base_url: String,
    client: Client,
}

impl AxleClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        Self {
            api_key,
            base_url,
            client: Client::new(),
        }
    }

    pub fn from_env(base_url: String) -> Self {
        let resolved_url = std::env::var("AXLE_API_URL").unwrap_or(base_url);
        let api_key = std::env::var("AXLE_API_KEY")
            .ok()
            .filter(|key| !key.trim().is_empty());
        Self::new(resolved_url, api_key)
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    fn authenticate(&self, builder: RequestBuilder) -> RequestBuilder {
        match &self.api_key {
            Some(key) => builder.bearer_auth(key),
            None => builder,
        }
    }

    pub async fn check(&self, request: &AxleCheckRequest) -> Result<AxleCheckResponse> {
        let url = format!("{}/check", self.base_url);
        self.authenticate(self.client.post(&url))
            .json(request)
            .send()
            .await
            .context("AXLE /check request failed")?
            .error_for_status()
            .context("AXLE /check returned error status")?
            .json::<AxleCheckResponse>()
            .await
            .context("failed to parse AXLE /check response")
    }

    pub async fn disprove(&self, request: &AxleDisproveRequest) -> Result<AxleDisproveResponse> {
        let url = format!("{}/disprove", self.base_url);
        self.authenticate(self.client.post(&url))
            .json(request)
            .send()
            .await
            .context("AXLE /disprove request failed")?
            .error_for_status()
            .context("AXLE /disprove returned error status")?
            .json::<AxleDisproveResponse>()
            .await
            .context("failed to parse AXLE /disprove response")
    }

    pub async fn sorry2lemma(
        &self,
        request: &AxleSorry2LemmaRequest,
    ) -> Result<AxleSorry2LemmaResponse> {
        let url = format!("{}/sorry2lemma", self.base_url);
        self.authenticate(self.client.post(&url))
            .json(request)
            .send()
            .await
            .context("AXLE /sorry2lemma request failed")?
            .error_for_status()
            .context("AXLE /sorry2lemma returned error status")?
            .json::<AxleSorry2LemmaResponse>()
            .await
            .context("failed to parse AXLE /sorry2lemma response")
    }
}
