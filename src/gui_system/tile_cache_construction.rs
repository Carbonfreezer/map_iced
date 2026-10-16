//! The configuration of the tile cache: where the tiles come from and where they are kept
//! on disc. [`MapWidgetSystem::boot`](super::map_widget_system::MapWidgetSystem::boot) takes it.
//!
//! A [`TileCacheConfig`] is either built in code, with [`TileCacheConfig::new`] or one of
//! its shortcuts, or read from JSON with [`TileCacheConfig::from_json_str`]. JSON is
//! the way to go when the tile provider needs an API key: the key stays in a file next
//! to the application instead of in the source code.
//!
//! # The JSON file
//!
//! The file is one object with three entries, all of them required:
//!
//! | Entry        | Meaning                                                   | See                  |
//! |--------------|-----------------------------------------------------------|----------------------|
//! | `cache`      | The directory the tiles are stored in.                    | [`CachingDirectory`] |
//! | `cache_size` | The disc space for the tiles in bytes, e.g. `100000000` for 100 MB. When it is used up, the least recently used tiles are deleted. | |
//! | `source`     | The tile provider and its access data.                    | [`TileSource`]       |
//!
//! `cache` and `source` name a variant of their enum. A variant without data is
//! just its name as a string, a variant with data is an object with the variant name
//! as its only key:
//!
//! ```
//! # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
//! # TileCacheConfig::from_json_str(r#"
//! {
//!   "cache": "CacheDirFixed",
//!   "cache_size": 100000000,
//!   "source": { "Thunderforest": { "style": "cycle", "api_key": "your-api-key" } }
//! }
//! # "#).unwrap();
//! ```
//!
//! Each variant of [`CachingDirectory`] and [`TileSource`] shows its JSON form. A
//! file with an API key does not belong into version control; this repository ignores
//! `*.json` for that reason.
//!
//! # Loading the file
//!
//! Either read it at run time, so the key can change without a rebuild, or compile it
//! in with `include_str!`, as the examples of this crate do:
//!
//! ```no_run
//! use map_iced::gui_system::map_widget_system::MapWidgetSystem;
//! use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
//!
//! let json = std::fs::read_to_string("tiles.json").expect("tiles.json readable");
//! let config = TileCacheConfig::from_json_str(&json).expect("tiles.json valid");
//! let (system, task) = MapWidgetSystem::boot(config).expect("tile cache set up");
//! ```

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

/// The directory the tiles are stored in. It is created if it does not exist. Different
/// tile sources need different directories, otherwise their tiles mix.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum CachingDirectory {
    /// The path as given. A relative path is relative to the working directory of the
    /// application.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache_size": 1, "source": "TestPattern",
    /// "cache": { "FullyConstructed": "cache/osm" }
    /// # }"#).unwrap();
    /// ```
    FullyConstructed(PathBuf),
    /// A subfolder of the cache directory of the operating system, see
    /// [`cache_dir()`](https://docs.rs/dirs/7.0.0/dirs/fn.cache_dir.html), e.g.
    /// `C:\Users\<name>\AppData\Local\OpenStreetMap` on Windows.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache_size": 1, "source": "TestPattern",
    /// "cache": { "CacheDirectory": "OpenStreetMap" }
    /// # }"#).unwrap();
    /// ```
    CacheDirectory(PathBuf),
    /// The subfolder `Tiles` of the cache directory of the operating system.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache_size": 1, "source": "TestPattern",
    /// "cache": "CacheDirFixed"
    /// # }"#).unwrap();
    /// ```
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

/// The provider the PNG tiles are downloaded from. All of them follow the
/// [slippy map convention](https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames):
/// a tile is requested as `<start>{z}/{x}/{y}.png<end>`. Each variant names the URL
/// it builds and the copyright text the map shows.
///
/// The providers with an `api_key` send the user agent `map-iced/<version>`.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum TileSource {
    /// Any provider that follows the convention: the tiles come from
    /// `<start_url>{z}/{x}/{y}.png<end_url>`, with `user_agent` as the HTTP user agent
    /// and `copyright_text` shown on the map.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1, "source":
    /// {
    ///   "FullyConstructed": {
    ///     "start_url": "https://tiles.example.com/",
    ///     "end_url": "?key=your-api-key",
    ///     "user_agent": "PlaceholderMapping",
    ///     "copyright_text": "© Example Maps"
    ///   }
    /// }
    /// # }"#).unwrap();
    /// ```
    FullyConstructed {
        start_url: String,
        end_url: String,
        user_agent: String,
        copyright_text: String,
    },
    /// [OpenStreetMap](https://www.openstreetmap.org), from
    /// `https://tile.openstreetmap.org/{z}/{x}/{y}.png`. Free, but its
    /// [tile usage policy](https://operations.osmfoundation.org/policies/tiles/) asks for
    /// a `user_agent` that names the application and a contact. `PlaceholderMapping`
    /// below is only a stand-in: put in your own application name and e-mail, e.g.
    /// `MyMapApp/1.0 (me@mydomain.org)`. OSM blocks user agents that are copied from
    /// examples or shared by many applications and then serves an "Access blocked"
    /// image instead of the tiles, which ends up in the disc cache like any other tile.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1, "source":
    /// { "OpenStreetMap": { "user_agent": "PlaceholderMapping" } }
    /// # }"#).unwrap();
    /// ```
    OpenStreetMap { user_agent: String },
    /// [Map Tiles API](https://www.maptilesapi.com), from
    /// `https://maptiles.p.rapidapi.com/en/map/v1/{z}/{x}/{y}.png?rapidapi-key=<api_key>`.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1, "source":
    /// { "MapTilesApi": { "api_key": "your-rapidapi-key" } }
    /// # }"#).unwrap();
    /// ```
    MapTilesApi { api_key: String },
    /// A [Mapbox](https://www.mapbox.com) tileset, from
    /// `https://api.mapbox.com/v4/<tileset_id>/{z}/{x}/{y}.png?access_token=<api_key>`.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1, "source":
    /// { "MapBoxTiles": { "tileset_id": "mapbox.satellite", "api_key": "your-access-token" } }
    /// # }"#).unwrap();
    /// ```
    MapBoxTiles { tileset_id: String, api_key: String },
    /// The [Mapbox](https://www.mapbox.com) satellite imagery, the same as
    /// [`TileSource::MapBoxTiles`] with the tileset `mapbox.satellite`.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1, "source":
    /// { "MapBoxSatellite": { "api_key": "your-access-token" } }
    /// # }"#).unwrap();
    /// ```
    MapBoxSatellite { api_key: String },
    /// A [Thunderforest](https://www.thunderforest.com/docs/map-tiles-api) map style
    /// such as `cycle` or `transport`, from
    /// `https://api.thunderforest.com/<style>/{z}/{x}/{y}.png?apikey=<api_key>`.
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1, "source":
    /// { "Thunderforest": { "style": "cycle", "api_key": "your-api-key" } }
    /// # }"#).unwrap();
    /// ```
    Thunderforest { style: String, api_key: String },
    /// No map at all: the built-in test image `assets/Test.png` for every position
    /// and zoom level, without any network access. For tests only, see
    /// [`TileCacheConfig::test_pattern`].
    ///
    /// ```
    /// # use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// # TileCacheConfig::from_json_str(r#"{ "cache": "CacheDirFixed", "cache_size": 1,
    /// "source": "TestPattern"
    /// # }"#).unwrap();
    /// ```
    TestPattern,
}

/// Where the tiles come from and where they are kept on disc, built in code or read
/// from JSON, see the [module documentation](self) for the JSON file.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TileCacheConfig {
    /// The directory the tiles are stored in.
    cache: CachingDirectory,
    /// The disc space for the tiles in bytes. When it is used up, the least recently
    /// used tiles are deleted.
    cache_size: u64,
    /// The tile provider.
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
    /// the application and a contact; put in your own application name and e-mail
    /// instead of `PlaceholderMapping`, see [`TileSource::OpenStreetMap`].
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// // Your own application name and e-mail instead of the placeholder.
    /// let config = TileCacheConfig::open_street_map("PlaceholderMapping");
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

    /// Reads the configuration from JSON text, read from a file or compiled in with
    /// `include_str!`. The [module documentation](self) explains the format. Fails with
    /// [`TileCacheError::InvalidJson`], which names line and column, if the text is no
    /// valid JSON or does not have the expected shape. Nothing is created on disc or
    /// requested from the network yet, that happens in
    /// [`MapWidgetSystem::boot`](super::map_widget_system::MapWidgetSystem::boot).
    ///
    /// # Example
    /// ```
    /// use map_iced::gui_system::tile_cache_construction::TileCacheConfig;
    /// // OpenStreetMap tiles in the folder cache/osm of the working directory, 100 MB.
    /// // Your own application name and e-mail instead of the placeholder user agent.
    /// let config = TileCacheConfig::from_json_str(r#"
    ///     {
    ///       "cache": { "FullyConstructed": "cache/osm" },
    ///       "cache_size": 100000000,
    ///       "source": {
    ///         "OpenStreetMap": { "user_agent": "PlaceholderMapping" }
    ///       }
    ///     }
    /// "#);
    /// assert!(config.is_ok());
    /// ```
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
