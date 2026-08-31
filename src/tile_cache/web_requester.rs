//! this module contains the functionality to interact with web requests.

use crate::tile_cache::tile_name_conversion::TileSpecification;

/// The dummy test image we use for test purposes.
const TEST_IMAGE: &'static [u8] = include_bytes!("../../assets/Test.png");

/// A general requester interface used here, because we have one real requester and one dummy requester
/// for experimentation and test purposes. This trait is not dyn compatible.
pub trait Requester : Send + Sync + 'static {
    /// Tries to get the image data from the tile specification if possible.
    async fn get_image_data(&self, specification: TileSpecification) -> Result<Vec<u8>, String>;
}

/// The dummy requester for
pub struct DummyRequester;
impl Requester for DummyRequester {
    async fn get_image_data(&self, _: TileSpecification) -> Result<Vec<u8>, String> {
        Ok(TEST_IMAGE.to_vec())
    }
}

pub struct WebRequester {
    client: reqwest::Client,
    intro_url: String,
    post_url: String,
}

impl WebRequester {
    /// Generates a new web requester. The intro url is what should get prepended to the
    /// standard tile specification and the post part is what should get postpended. This
    /// is for instance the user id in mat box. The user agent is the agent, that is required for
    /// OSM Example: `MyCoolMappingApp/1.0 (contact@example.com)`
    pub fn new(intro_url: &str, post_url: &str, user_agent: &str) -> WebRequester {
        let client = reqwest::Client::builder()
            .default_headers(
                [(
                    reqwest::header::USER_AGENT,
                    reqwest::header::HeaderValue::from_str(user_agent).unwrap(),
                )]
                .into_iter()
                .collect(),
            )
            .build()
            .unwrap();

        Self {
            client,
            intro_url: intro_url.to_string(),
            post_url: post_url.to_string(),
        }
    }
}

impl Requester for WebRequester {
    async fn get_image_data(&self, specification: TileSpecification) ->  Result<Vec<u8>, String> {
        let final_url = self.intro_url.clone()
            + specification.get_partial_url().as_str()
            + self.post_url.as_str();
        Ok(
            self.client.get(final_url)
                .send()
                .await
                .map_err(|x| x.to_string())?
                .bytes()
                .await.map_err(|x| x.to_string())?
                .to_vec(),
        )
    }
}


#[cfg(test)]
mod tests {
    use super::super::tile_name_conversion::*;
    use super::*;


    #[tokio::test]
    async fn dummy_requester() {
        let dummy = DummyRequester;
        assert_eq!(dummy.get_image_data(TileSpecification::new(0, 0, 0)).await.unwrap(), TEST_IMAGE.to_vec());
    }

    // This test is normally ignored not to spam OSM with requests.
    #[ignore]
    #[tokio::test]
    async fn real_requester() {
        let requester = WebRequester::new("https://tile.openstreetmap.org/", "", "test_runner christoph.luerig@gmail.com");
        let data = requester.get_image_data(TileSpecification::new(0, 0, 0)).await.unwrap();
        assert!(data.len() > 0, "We should have gotten some data from OSM.");
    }

}
