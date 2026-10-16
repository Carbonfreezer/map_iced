//! This module contains the functionality to interact with web requests.

use crate::tile_cache::tile_name_conversion::TileSpecification;
use bytes::Bytes;

/// The dummy test image we use for test purposes.
const TEST_IMAGE: &[u8] = include_bytes!("../../assets/Test.png");

/// Why a [`Requester`] could not be set up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequesterError {
    /// The user agent, given here, is no valid HTTP header value.
    InvalidUserAgent(String),
    /// reqwest could not set up its client, typically the TLS backend; its message.
    HttpClient(String),
}

/// A requester struct that does the web requests, or serves the test image without a
/// client, see [`Self::dummy`].
pub struct Requester {
    client: Option<reqwest::Client>,
    intro_url: String,
    post_url: String,
}

impl Requester {
    /// Generates a new web requester. The intro url is what should get prepended to the
    /// standard tile specification and the post part is what should get postpended. This
    /// is for instance the access token for Mapbox. The user agent is the one OSM asks for, the application name and a contact, e.g. `MyMapApp/1.0 (me@mydomain.org)`
    pub fn new(
        intro_url: &str,
        post_url: &str,
        user_agent: &str,
    ) -> Result<Self, RequesterError> {
        let user_agent_value = reqwest::header::HeaderValue::from_str(user_agent)
            .map_err(|_| RequesterError::InvalidUserAgent(user_agent.to_string()))?;
        let client = reqwest::Client::builder()
            .default_headers(
                [(reqwest::header::USER_AGENT, user_agent_value)]
                    .into_iter()
                    .collect(),
            )
            .build()
            .map_err(|e| RequesterError::HttpClient(e.to_string()))?;

        Ok(Self {
            client: Some(client),
            intro_url: intro_url.to_string(),
            post_url: post_url.to_string(),
        })
    }

    /// Returns a requester that serves a built-in test image for every tile.
    pub fn dummy() -> Self {
        Self {
            client: None,
            intro_url: String::new(),
            post_url: String::new(),
        }
    }

    /// Gets the image data from the tile specification.
    pub async fn get_image_data(&self, specification: TileSpecification) -> Result<Bytes, String> {
        if let Some(client) = &self.client {
            let final_url = self.intro_url.clone()
                + specification.get_partial_url().as_str()
                + self.post_url.as_str();
            Ok(client
                .get(final_url)
                .send()
                .await
                .map_err(|x| x.to_string())?
                .error_for_status()
                .map_err(|x| x.to_string())?
                .bytes()
                .await
                .map_err(|x| x.to_string())?)
        } else {
            Ok(Bytes::from_static(TEST_IMAGE))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tile_name_conversion::*;
    use super::*;

    #[tokio::test]
    async fn dummy_requester() {
        let dummy = Requester::dummy();
        assert_eq!(
            dummy
                .get_image_data(TileSpecification::new(0, 0, 0))
                .await
                .unwrap(),
            TEST_IMAGE.to_vec()
        );
    }

    // This test is normally ignored not to spam OSM with requests.
    #[ignore]
    #[tokio::test]
    async fn real_requester() {
        let requester =
            Requester::new("https://tile.openstreetmap.org/", "", "test_runner").unwrap();
        let data = requester
            .get_image_data(TileSpecification::new(0, 0, 0))
            .await
            .unwrap();
        assert!(
            !data.is_empty(),
            "We should have gotten some data from OSM."
        );
    }
}
