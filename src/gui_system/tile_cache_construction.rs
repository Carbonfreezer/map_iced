//! This module contains helper functions and structures to generate a high level tile cache.

use crate::gui_system::high_level_tile_cache::TileCache;
use crate::tile_cache::cache_core::{generate_cache, generate_dummy_cache};
use crate::tile_cache::web_requester::{DummyRequester, WebRequester};
use dirs::cache_dir;
use std::path::PathBuf;

/// The different types of aching directories we offer.
pub enum CachingDirectory {
    /// Completely manually constructed.
    FullyConstructed(PathBuf),
    /// Relative to the OSes temp dir.
    TempDirectory(PathBuf),
    /// Fixed to the OSes temp dir (Tiles)
    TempDirFixed,
}

impl CachingDirectory {
    fn get_path(&self) -> PathBuf {
        match self {
            CachingDirectory::FullyConstructed(path) => path.clone(),
            CachingDirectory::TempDirectory(path) => cache_dir().unwrap().join(path),
            CachingDirectory::TempDirFixed => cache_dir().unwrap().join("Tiles"),
        }
    }
}

/// The tile source where we obtain our pngs from.
/// They all follow the [slippy map convention](https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames).
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
    /// The [mapbox](https://www.mapbox.com/)) api. This defaults to the standard street set.
    MapBoxTilesStreet { api_key: String },
    /// Thunderforst [maps](https://www.thunderforest.com/docs/map-tiles-api).
    Thunderforest { style: String, api_key: String },
}

pub struct TripleInfo {
    start_url: String,
    end_url: String,
    user_agent: String,
}

impl TileSource {
    pub fn get_triple(&self) -> TripleInfo {
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
                user_agent: "".to_string(),
            },
            TileSource::MapBoxTiles {
                tileset_id,
                api_key,
            } => TripleInfo {
                start_url: "https://api.mapbox.com/v4/".to_string() + tileset_id + "/",
                end_url: "?access_token=".to_string() + api_key,
                user_agent: "".to_string(),
            },
            TileSource::MapBoxTilesStreet { api_key } => TripleInfo {
                start_url: "https://api.mapbox.com/v4/mapbox.mapbox-streets-v8/".to_string(),
                end_url: "?access_token=".to_string() + api_key,
                user_agent: "".to_string(),
            },
            TileSource::Thunderforest { style, api_key } => TripleInfo {
                start_url: "https://api.thunderforest.com/".to_string() + style + "/",
                end_url: "?apikey=".to_string() + api_key,
                user_agent: "".to_string(),
            },
        }
    }
}

/// Generates the debug tile cache system with an indicated cache size, A simple internal image is used here.
pub fn generate_debug_tile_cache(
    dir_info: CachingDirectory,
    cache_size: u64,
) -> Result<TileCache<DummyRequester>, String> {
    TileCache::new(generate_dummy_cache(dir_info.get_path(), cache_size))
}

pub fn generate_web_tile_cache(
    dir_info: CachingDirectory,
    cache_size: u64,
    tile_source: TileSource,
) -> Result<TileCache<WebRequester>, String> {
    let description = tile_source.get_triple();
    TileCache::new(generate_cache(
        &description.start_url,
        &description.end_url,
        &description.user_agent,
        dir_info.get_path(),
        cache_size,
    ))
}
