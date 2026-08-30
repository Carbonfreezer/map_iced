//! This module contains helper functions to convert tiles /to/from u64 and into the different
//! formats required for the tile cacher.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileSpecification {
    level: u8,
    x: u32,
    y: u32,
}

impl TileSpecification {
    pub fn new(level: u8, x: u32, y: u32) -> Self {
        assert!(level < 29, "Level is too large to be represented");
        assert!(x < (1 << 29) && y < (1 << 29), "Coordinate out of range");
        TileSpecification { level, x, y }
    }

    /// Gets the filename, where this coordinate would be stored in the cache.
    pub fn filename(&self) -> PathBuf {
         format!("{}/{}/{:016x}.png", self.level, self.x, u64::from(*self)).into()
    }

    /// Gets the relative path used in tile coordinate systems for web services.
    pub fn get_partial_url(&self) -> PathBuf {
        format!("{}/{}/{}.png", self.level, self.x, self.y).into()
    }
}

impl From<u64> for TileSpecification {
    fn from(item: u64) -> Self {
        let level = (item >> 58) as u8;
        let y = ((item >> 29) & 0x1FFF_FFFF) as u32;
        let x = (item & 0x1FFF_FFFF) as u32;
        Self {
            level : level as u8,
            y : y as u32,
            x : x as u32,
        }
    }
}

impl From<TileSpecification> for u64 {
    fn from(item: TileSpecification) -> u64 {
        (item.level as u64) << 58 | (item.y as u64) << 29 | item.x as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_test() {
        let spec = TileSpecification::new(1, 2, 3);
        let new_spec = TileSpecification::from(u64::from(spec));
        assert_eq!(spec, new_spec);
    }

    #[test]
    fn name_test() {
        let spec = TileSpecification::new(0, 10, 0);
        assert_eq!(spec.filename().to_str().unwrap(), "0/10/000000000000000a.png");

        let spec = TileSpecification::new(1, 2, 3);
        assert_eq!(spec.get_partial_url().to_str().unwrap(), "1/2/3.png");

    }

}