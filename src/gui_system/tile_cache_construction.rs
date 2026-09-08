//! This module contains helper functions and structures to generate a high level tile cache.

use crate::gui_system::high_level_tile_cache::TileCache;
use crate::tile_cache::cache_core::{generate_cache, generate_dummy_cache};
use dirs::cache_dir;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The different types of aching directories we offer.
#[derive(Deserialize)]
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
#[derive(Deserialize)]
pub enum TileSource {
    /// The most flexible form where the beginning, ending and the user agent are given.
    FullyConstructed {
        start_url: String,
        end_url: String,
        user_agent: String,
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

/// The combined information for serialization.
#[derive(Deserialize)]
struct CombinedInfo {
    /// The info where the caching directory resides.
    cache: CachingDirectory,
    /// The size of the cache we use.
    cache_size: u64,
    /// The source of the information where to get tiles from.
    source: TileSource,
}

pub(crate) struct TripleInfo {
    start_url: String,
    end_url: String,
    user_agent: String,
}

impl TileSource {
    pub(crate) fn get_triple(&self) -> TripleInfo {
        match self {
            TileSource::FullyConstructed {
                start_url,
                end_url,
                user_agent,
            } => TripleInfo {
                start_url: start_url.clone(),
                end_url: end_url.clone(),
                user_agent: user_agent.clone(),
            },
            TileSource::OpenStreetMap { user_agent } => TripleInfo {
                start_url: "https://tile.openstreetmap.org/".to_string(),
                end_url: "".to_string(),
                user_agent: user_agent.clone(),
            },
            TileSource::MapTilesApi { api_key } => TripleInfo {
                start_url: "https://maptiles.p.rapidapi.com/en/map/v1/".to_string(),
                end_url: "?rapidapi-key=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
            },
            TileSource::MapBoxTiles {
                tileset_id,
                api_key,
            } => TripleInfo {
                start_url: "https://api.mapbox.com/v4/".to_string() + tileset_id + "/",
                end_url: "?access_token=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
            },
            TileSource::MapBoxSatellite { api_key } => TripleInfo {
                start_url: "https://api.mapbox.com/v4/mapbox.satellite/".to_string(),
                end_url: "?access_token=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
            },
            TileSource::Thunderforest { style, api_key } => TripleInfo {
                start_url: "https://api.thunderforest.com/".to_string() + style + "/",
                end_url: "?apikey=".to_string() + api_key,
                user_agent: concat!("map-iced/", env!("CARGO_PKG_VERSION")).to_string(),
            },
        }
    }
}

/// Generates the debug tile cache system with an indicated cache size, A simple internal image is used here.
fn generate_debug_tile_cache(
    dir_info: CachingDirectory,
    cache_size: u64,
) -> Result<TileCache, String> {
    TileCache::new(generate_dummy_cache(dir_info.get_path()?, cache_size))
}

fn generate_web_tile_cache(
    dir_info: CachingDirectory,
    cache_size: u64,
    tile_source: TileSource,
) -> Result<TileCache, String> {
    let description = tile_source.get_triple();
    TileCache::new(generate_cache(
        &description.start_url,
        &description.end_url,
        &description.user_agent,
        dir_info.get_path()?,
        cache_size,
    )?)
}

/// Reads in `config.json` and generates the tile cache from.
fn generate_from_config_json_internal(name: impl AsRef<Path>) -> Result<TileCache, String> {
    let file = fs::File::open(name).map_err(|e| e.to_string())?;
    let combined: CombinedInfo = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    generate_web_tile_cache(combined.cache, combined.cache_size, combined.source)
}

/// Generates a configuration from a json file if this is not possible it defaults to a test configuration.
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
/// This generates a subfolder besides the program directory called cached/osm, allows
/// for 1 million bytes of disc space and uses OSM for tile queries.


// TODO: Copyright information has to be implemented as an overlay here.

pub fn generate_from_config_default(name: impl AsRef<Path>) -> Result<TileCache, String> {
    let result = generate_from_config_json_internal(name);
    if let Err(e) = &result {
        eprintln!("Error in configuration {}", e);
        return generate_debug_tile_cache(CachingDirectory::CacheDirFixed, 30_000);
    }

    result
}
