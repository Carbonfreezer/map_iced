//! This module contains helper functions and structures to generate a high level tile cache.

use crate::gui_system::high_level_tile_cache::TileCache;
use crate::tile_cache::cache_core::{generate_cache, generate_dummy_cache};
use dirs::cache_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The different types of aching directories we offer.
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
    fn get_path(&self) -> Result<PathBuf, String> {
        match self {
            CachingDirectory::FullyConstructed(path) => Ok(path.clone()),
            CachingDirectory::CacheDirectory(path) => Ok(cache_dir()
                .ok_or("Cache directory not found on system")?
                .join(path)),
            CachingDirectory::CacheDirFixed => Ok(cache_dir()
                .ok_or("Cache directory not found on system")?
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
    pub(crate) fn get_quadruple(&self) -> QuadrupleInfo {
        match self {
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
        }
    }
}

impl TileCacheConfig {
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
    /// This generates a subfolder besides the working directory called cache/osm, allows
    /// for 1oo MB of disc space and uses OSM for tile queries.
    pub fn from_json_str(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| e.to_string())
    }

    /// Creates the tile cache from the internal representation.
    pub fn build(self) -> Result<TileCache, String> {
        let dir_info = self.cache;
        let cache_size = self.cache_size;
        let tile_source = self.source;
        let description = tile_source.get_quadruple();
        TileCache::new(generate_cache(
            &description.start_url,
            &description.end_url,
            &description.user_agent,
            description.copyright_text,
            dir_info.get_path()?,
            cache_size,
        )?)
    }
}

/// Offline-Cache with internal dummy cache for debug purposes.
pub fn tile_cache_debug_default() -> Result<TileCache, String> {
    let dir_info = CachingDirectory::CacheDirFixed;
    TileCache::new(generate_dummy_cache(dir_info.get_path()?, 30_000))
}
