//! this module contains the functionality to interact with web requests.

use bytes::Bytes;
use crate::tile_cache::tile_name_conversion::TileSpecification;

/// The dummy test image we use for test purposes.
const TEST_IMAGE: &[u8] = include_bytes!("../../assets/Test.png");



/// A requester struct that does the web requests, defaults to the dummy data
/// if no client is set.
pub struct Requester {
    client: Option<reqwest::Client>,
    intro_url: String,
    post_url: String,
}

impl Requester {
    /// Generates a new web requester. The intro url is what should get prepended to the
    /// standard tile specification and the post part is what should get postpended. This
    /// is for instance the user id in mat box. The user agent is the agent, that is required for
    /// OSM Example: `MyCoolMappingApp/1.0 (contact@example.com)`
    pub fn new(intro_url: &str, post_url: &str, user_agent: &str) ->Result<Self, String> {
        let client = reqwest::Client::builder()
            .default_headers(
                [(
                    reqwest::header::USER_AGENT,
                    reqwest::header::HeaderValue::from_str(user_agent).map_err(|e| e.to_string())?,
                )]
                    .into_iter()
                    .collect(),
            )
            .build()
            .map_err(|e| e.to_string())?;

        Ok (Self {
            client: Some(client),
            intro_url: intro_url.to_string(),
            post_url: post_url.to_string(),
        })
    }

    /// Returns a requester that serves a built-in test image for every tile.
    pub fn dummy() -> Self {
        Self { client: None, intro_url: String::new(), post_url: String::new() }
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
        }
        else {
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
        let requester = Requester::new(
            "https://tile.openstreetmap.org/",
            "",
            "test_runner christoph.luerig@gmail.com",
        ).unwrap();
        let data = requester
            .get_image_data(TileSpecification::new(0, 0, 0))
            .await
            .unwrap();
        assert!(data.len() > 0, "We should have gotten some data from OSM.");
    }
}
