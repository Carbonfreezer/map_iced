//! The configuration of the tile cache: where the tiles come from and where they are kept
//! on disc. [`MapWidgetSystem::boot`](super::map_widget_system::MapWidgetSystem::boot) takes it.

use crate::gui_system::high_level_tile_cache::TileCache;
use crate::tile_cache::cache_core::{generate_cache, generate_dummy_cache};
use crate::tile_cache::web_requester::RequesterError;
use dirs::cache_dir;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// What can go wrong when setting up the tile cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TileCacheError {
    /// The configuration text is no valid JSON for a [`TileCacheConfig`].
    InvalidJson(String),
    /// The configuration refers to the OS cache directory, but the system has none.
    NoCacheDirectory,
    /// The user agent, given here, is no valid HTTP header value, e.g. because it
    /// contains a line break.
    InvalidUserAgent(String),
    /// The HTTP client could not be set up, typically its TLS backend. Holds the
    /// message of reqwest.
    HttpClient(String),
}

impl fmt::Display for TileCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TileCacheError::InvalidJson(message) => {
                write!(f, "invalid tile cache configuration: {message}")
            }
            TileCacheError::NoCacheDirectory => {
                write!(f, "the system has no cache directory")
            }
            TileCacheError::InvalidUserAgent(user_agent) => {
                write!(f, "the user agent {user_agent:?} is no valid HTTP header value")
            }
            TileCacheError::HttpClient(message) => {
                write!(f, "the HTTP client could not be set up: {message}")
            }
        }
    }
}

impl std::error::Error for TileCacheError {}

/// The different types of caching directories we offer.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum CachingDirectory {
    /// Completely manually constructed.
    FullyConstructed(PathBuf),
    /// Relative to the OSes caching dir see here [`cache_dir()`](https://docs.rs/dirs/7.0.0/dirs/fn.cache_dir.html).
    CacheDirectory(PathBuf),
    /// Fixed to the OSes cache dir (**Tiles**)
    CacheDirFixed,
}

impl CachingDirectory {
    fn get_path(&self) -> Result<PathBuf, TileCacheError> {
        match self {
            CachingDirectory::FullyConstructed(path) => Ok(path.clone()),
            CachingDirectory::CacheDirectory(path) => Ok(cache_dir()
                .ok_or(TileCacheError::NoCacheDirectory)?
                .join(path)),
            CachingDirectory::CacheDirFixed => Ok(cache_dir()
                .ok_or(TileCacheError::NoCacheDirectory)?
                .join("Tiles")),
        }
    }
}

/// The tile source where we obtain our pngs from.
/// They all follow the [slippy map convention](https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames).
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum TileSource {
    /// The most flexible form where the beginning, ending and the user agent are given.
    FullyConstructed {
        start_url: String,
        end_url: String,
        user_agent: String,
        copyright_text: String,
    },
    /// The open street map  [access](https://operations.osmfoundation.org/policies/tiles/)
    OpenStreetMap { user_agent: String },
    /// The map [tiles api](https://www.maptilesapi.com).
    MapTilesApi { api_key: String },
    /// The [mapbox](https://www.mapbox.com/)) api. Here you can specify a tile set id.
    MapBoxTiles { tileset_id: String, api_key: String },
    /// The [mapbox](https://www.mapbox.com/)) api. This defaults to the standard sattelite.
    MapBoxSatellite { api_key: String },
    /// Thunderforst [maps](https://www.thunderforest.com/docs/map-tiles-api).
    Thunderforest { style: String, api_key: String },
    /// No map at all: the built-in test image `assets/Test.png` for every position
    /// and zoom level, without any network access. For tests only, see
    /// [`TileCacheConfig::test_pattern`].
    TestPattern,
}

/// The combined information for serialization. It contains the real
/// caching inforation and the access to the tile provider.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TileCacheConfig {
    /// The info where the caching directory resides.
    cache: CachingDirectory,
    /// The size of the cache we use.
    cache_size: u64,
    /// The source of the information where to get tiles from.
    source: TileSource,
}

pub(crate) struct QuadrupleInfo {
    start_url: String,
    end_url: String,
    user_agent: String,
    copyright_text: String,
}

impl TileSource {
    /// The web access for the source, `None` for the [`TileSource::TestPattern`].
    pub(crate) fn get_quadruple(&self) -> Option<QuadrupleInfo> {
        let quadruple = match self {
            TileSource::FullyConstructed {
                start_url,
                end_url,
                user_agent,
                copyright_text,
            } => QuadrupleInfo {
                start_url: start_url.clone(),
                end_url: end_url.clone(),
                user_agent: user_agent.clone(),
                copyright_text: copyright_text.clone(),
            },
            TileSource::OpenStreetMap { user_agent } => QuadrupleInfo {
                start_url: "https://tile.openstreetmap.org/".to_string(),
                end_url: "".to_string(),
                user_agent: user_agent.clone(),
                copyright_text: "© OpenStreetMap Contributors".to_string(),
            },
            TileSource::MapTilesApi { api_key } => QuadrupleInfo {
                start_url: "https://maptiles.p.rapidapi.com/en/map/v1/".to_string(),
                end_url: "?rapidapi-key=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
                copyright_text: "Map © Map Tiles API | Map data © OpenStreetMap contributors"
                    .to_string(),
            },
            TileSource::MapBoxTiles {
                tileset_id,
                api_key,
            } => QuadrupleInfo {
                start_url: "https://api.mapbox.com/v4/".to_string() + tileset_id + "/",
                end_url: "?access_token=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
                copyright_text: "© Mapbox © OpenStreetMap".to_string(),
            },
            TileSource::MapBoxSatellite { api_key } => QuadrupleInfo {
                start_url: "https://api.mapbox.com/v4/mapbox.satellite/".to_string(),
                end_url: "?access_token=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
                copyright_text: "© Mapbox © OpenStreetMap".to_string(),
            },
            TileSource::Thunderforest { style, api_key } => QuadrupleInfo {
                start_url: "https://api.thunderforest.com/".to_string() + style + "/",
                end_url: "?apikey=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
                copyright_text: "Maps © www.thunderforest.com, Data © www.osm.org/copyright"
                    .to_string(),
            },
            TileSource::TestPattern => return None,
        };
        Some(quadruple)
    }
}

impl TileCacheConfig {
    /// The disc space [`Self::open_street_map`] allows for, 100 MB.
    pub const DEFAULT_CACHE_SIZE: u64 = 100_000_000;

    /// The configuration built in code: tiles from `source`, stored in `cache` with
    /// at most `cache_size` bytes on disc.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::tile_cache_construction::{CachingDirectory, TileCacheConfig, TileSource};
    /// // Mapbox satellite imagery, 500 MB in the folder Satellite of the OS cache directory.
    /// let config = TileCacheConfig::new(
    ///     TileSource::MapBoxSatellite { api_key: "your-access-token".to_string() },
    ///     CachingDirectory::CacheDirectory("Satellite".into()),
    ///     500_000_000,
    /// );
    /// ```
    pub fn new(source: TileSource, cache: CachingDirectory, cache_size: u64) -> Self {
        Self {
            cache,
            cache_size,
            source,
        }
    }

    /// OpenStreetMap tiles in the subfolder `OpenStreetMap` of the OS cache directory,
    /// with [`Self::DEFAULT_CACHE_SIZE`]. OSM asks for a `user_agent` that identifies
    /// the application and a contact, e.g. `MyCoolMappingApp/1.0 (contact@example.com)`.
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// let config = TileCacheConfig::open_street_map("MyCoolMappingApp/1.0 (contact@example.com)");
    /// ```
    pub fn open_street_map(user_agent: impl Into<String>) -> Self {
        Self::new(
            TileSource::OpenStreetMap {
                user_agent: user_agent.into(),
            },
            CachingDirectory::CacheDirectory("OpenStreetMap".into()),
            Self::DEFAULT_CACHE_SIZE,
        )
    }

    /// The [`TileSource::TestPattern`]: one test image for every tile, no network
    /// access. Meant for tests, or as the fallback of an example whose configuration
    /// failed, never as a map. Kept in its own subfolder of the OS cache directory, so
    /// the test images never mix with real tiles.
    ///
    /// # Example
    /// ```no_run
    /// use map_iced::gui_system::map_widget_system::MapWidgetSystem;
    /// use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// // In a test of the application's own logic, without network access.
    /// let (system, task) = MapWidgetSystem::boot(TileCacheConfig::test_pattern()).expect("test pattern set up");
    /// ```
    pub fn test_pattern() -> Self {
        Self::new(
            TileSource::TestPattern,
            CachingDirectory::CacheDirectory("TestPattern".into()),
            30_000,
        )
    }

    /// Generates the tile cache config from a string, that can either be loaded from from
    /// a file or generated with include_str!
    ///
    /// On the highest level the json file consists of three entries.
    /// * cache: Here we refer to the file cache construction which gets serialized from [`CachingDirectory`]
    /// * cache_size: The amount of bytes we allow for the cache size on disc
    /// * source: The source of the tiles as explained in [`TileSource`]
    ///
    /// # Example
    /// ```text
    ///  {
    ///   "cache": {
    ///     "FullyConstructed": "cache/osm"
    ///   },
    ///   "cache_size" : 100000000,
    ///   "source": {
    ///     "OpenStreetMap": {
    ///       "user_agent": "My Test Test mymail@gmail.com"
    ///     }
    ///   }
    /// }
    /// ```
    /// This generates the folder cache/osm in the working directory, allows
    /// for 100 MB of disc space and uses OSM for tile queries.
    pub fn from_json_str(s: &str) -> Result<Self, TileCacheError> {
        serde_json::from_str(s).map_err(|e| TileCacheError::InvalidJson(e.to_string()))
    }

    /// Creates the tile cache from the internal representation.
    pub(crate) fn build(self) -> Result<TileCache, TileCacheError> {
        let path = self.cache.get_path()?;
        let system = match self.source.get_quadruple() {
            Some(description) => generate_cache(
                &description.start_url,
                &description.end_url,
                &description.user_agent,
                description.copyright_text,
                path,
                self.cache_size,
            )
            .map_err(|e| match e {
                RequesterError::InvalidUserAgent(user_agent) => {
                    TileCacheError::InvalidUserAgent(user_agent)
                }
                RequesterError::HttpClient(message) => TileCacheError::HttpClient(message),
            })?,
            None => generate_dummy_cache(path, self.cache_size),
        };
        Ok(TileCache::new(system))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_break_in_user_agent_is_reported() {
        let config = TileCacheConfig::new(
            TileSource::OpenStreetMap {
                user_agent: "MyApp\n(contact@example.com)".to_string(),
            },
            CachingDirectory::FullyConstructed("cache/never_created".into()),
            1000,
        );
        assert!(matches!(
            config.build(),
            Err(TileCacheError::InvalidUserAgent(_))
        ));
    }

    #[test]
    fn test_pattern_from_json() {
        let config = TileCacheConfig::from_json_str(
            r#"{ "cache": "CacheDirFixed", "cache_size": 1000, "source": "TestPattern" }"#,
        )
        .unwrap();
        assert!(config.source.get_quadruple().is_none());
    }
}
