//! Contains helper functionality for file loading and saving.

use crate::tile_cache::tile_name_conversion::TileSpecification;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::fs::*;

/// The data for the file utility.
pub struct FileUtil {
    /// The base path where we operate in.
    base_path: PathBuf,
    ///  a counter for unique temp files.
    temp_file_counter: AtomicU32,
}

/// An entry for a tile consisting of the id and the disc usage.
pub struct TileData {
    /// The id of the tile.
    pub tile_id: u64,
    /// The disc space usage of the tile.
    pub size_on_disc: u64,
}

/// 4K Block size for files.
const BLOCK_SIZE: u64 = 4 * 1024;

/// The filename for the LRU table.
pub const LRU_TABLE_FILE: &str = "LRU.bin";

pub fn round_to_final_consumption(raw_size: u64) -> u64 {
    raw_size.div_ceil(BLOCK_SIZE) * BLOCK_SIZE
}

impl FileUtil {
    /// Converts a u8 Vector into an u64 vector.
    pub fn convert_to_u64(input: &[u8]) -> Vec<u64> {
        assert!(
            input.len().is_multiple_of(8),
            "Input size is not multiple of 8"
        );

        let final_length = input.len() / 8;
        let mut result = Vec::with_capacity(final_length);
        for i in 0..final_length {
            result.push(u64::from_be_bytes(
                input[i * 8..(i + 1) * 8].try_into().unwrap(),
            ));
        }
        result
    }

    /// Converts the input sequence into
    pub fn convert_to_u8(input: &[u64]) -> Vec<u8> {
        input.iter().flat_map(|&x| u64::to_be_bytes(x)).collect()
    }

    /// Crates a file util with the indicated root directory.
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            temp_file_counter: AtomicU32::new(0),
        }
    }

    /// Tries to load the data, returns a None if the data does not exist.
    pub async fn try_load_plain(&self, file_name: impl AsRef<Path>) -> Option<Vec<u8>> {
        let final_path = self.base_path.join(file_name);
        read(&final_path).await.ok()
    }

    /// Makes a save safe, which means the data gets first written to a temporary file, that gets then
    /// moved to the final destination to avoid data inconsistency with crashes.
    pub async fn safe_save(&self, file_name: impl AsRef<Path>, data: &[u8]) -> Result<(), String> {
        let final_path = self.base_path.join(file_name);
        let dir = final_path.parent().unwrap();
        // For the case the directory does not exist yet, we create it.
        create_dir_all(dir).await.map_err(|err| err.to_string())?;
        let combined = (process::id() as u64) << 32
            | self.temp_file_counter.fetch_add(1, Ordering::Relaxed) as u64;
        // Now we create a transient file that will get moved.
        let transient = self.base_path.join(format!("{:016x}.tmp", combined));
        write(&transient, data)
            .await
            .map_err(|err| err.to_string())?;
        rename(&transient, &final_path)
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    /// Removes all the files with the tile ids handed over.
    pub async fn remove_files(&self, list_of_ids: &[u64]) {
        for &x in list_of_ids {
            let final_path = self.base_path.join(TileSpecification::from(x).filename());
            let _ = remove_file(&final_path).await;
        }
    }

    /// Removes all temp files from the base directory. This is intended at startup to clear any left overs.
    pub async fn remove_temps_from_base(&self) {
        let Ok(mut dir_scan) = read_dir(&self.base_path).await else {
            return;
        };

        let mut clean_list = Vec::new();
        while let Ok(Some(entry)) = dir_scan.next_entry().await {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };
            let path = entry.path();
            if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                clean_list.push(path);
            }
        }

        for file in clean_list {
            let _ = remove_file(file).await;
        }
    }

    /// Checks if the file at `path` is one the cache has written: a tile, a temp file or the LRU table.
    fn is_cache_file(path: &Path) -> bool {
        let extension = path.extension().and_then(|e| e.to_str());
        let stem = path.file_stem().and_then(|s| s.to_str());
        match extension {
            Some("png") => stem.is_some_and(|s| u64::from_str_radix(s, 16).is_ok()),
            Some("tmp") => true,
            _ => path.file_name().and_then(|n| n.to_str()) == Some(LRU_TABLE_FILE),
        }
    }

    /// Removes all files the cache has written below the base directory, and the
    /// subdirectories that are empty afterwards. Anything else stays, because the base
    /// directory is configurable and may hold other things; the base directory itself
    /// stays as well. Returns the first error, files gone in the meantime are none.
    pub async fn remove_cache_files(&self) -> Result<(), String> {
        let mut first_error: Option<String> = None;
        let mut note_error = |err: std::io::Error| {
            if err.kind() != std::io::ErrorKind::NotFound && first_error.is_none() {
                first_error = Some(err.to_string());
            }
        };
        let mut visited_dirs = Vec::new();
        let mut pending = vec![self.base_path.clone()];

        while let Some(dir) = pending.pop() {
            let mut dir_scan = match read_dir(&dir).await {
                Ok(scan) => scan,
                Err(err) => {
                    note_error(err);
                    continue;
                }
            };

            while let Ok(Some(entry)) = dir_scan.next_entry().await {
                let Ok(file_type) = entry.file_type().await else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    pending.push(path.clone());
                    visited_dirs.push(path);
                } else if file_type.is_file()
                    && Self::is_cache_file(&path)
                    && let Err(err) = remove_file(&path).await
                {
                    note_error(err);
                }
            }
        }

        // Children are visited after their parents, so the reverse order removes the
        // deepest first. A directory that still holds something foreign stays.
        for dir in visited_dirs.into_iter().rev() {
            let _ = remove_dir(&dir).await;
        }

        first_error.map_or(Ok(()), Err)
    }

    /// Gets all png filenames recursively interpreted as u64. and returns also the accumulated size in files.
    pub async fn get_all_pngs_interpreted_as_u64(&self) -> Vec<TileData> {
        let mut result = Vec::new();
        let mut pending = vec![self.base_path.clone()];

        while let Some(dir) = pending.pop() {
            let Ok(mut dir_scan) = read_dir(&dir).await else {
                continue;
            };

            while let Ok(Some(entry)) = dir_scan.next_entry().await {
                let Ok(file_type) = entry.file_type().await else {
                    continue;
                };
                let path = entry.path();

                if file_type.is_dir() {
                    pending.push(path);
                } else if file_type.is_file() {
                    if path.extension().and_then(|e| e.to_str()) != Some("png") {
                        continue;
                    }
                    let length = round_to_final_consumption(
                        metadata(&path)
                            .await
                            .expect("The file existent was just scanned")
                            .len(),
                    );
                    debug_assert!(length > 0, "There should be no empty files");
                    if let Some(n) = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| u64::from_str_radix(s, 16).ok())
                    {
                        result.push(TileData {
                            tile_id: n,
                            size_on_disc: length,
                        });
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::tile_name_conversion::*;
    use super::*;
    use crate::tile_cache::lru_list::LastRecentlyUsedList;

    #[test]
    fn conversion_test() {
        let base = vec![12, 23, 24, 25];
        let back_test = FileUtil::convert_to_u64(&FileUtil::convert_to_u8(&base));
        assert_eq!(back_test, base, "Vectors should be the same.");
    }

    #[tokio::test]
    async fn fake_png_test() {
        let base = vec![12, 23, 24, 25];
        let base_index = TileSpecification::new(1, 2, 3);
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.safe_save(base_index.filename(), &base).await.unwrap();
        let existing_pngs = util.get_all_pngs_interpreted_as_u64().await;
        assert_eq!(
            TileSpecification::from(existing_pngs[0].tile_id),
            base_index
        );
        assert_eq!(existing_pngs[0].size_on_disc, 4 * 1024);
    }

    #[tokio::test]
    async fn lru_cache_test() {
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        let test_vector = vec![12, 23, 24, 25];
        let test_collection = test_vector
            .iter()
            .map(|&t| TileData {
                tile_id: t,
                size_on_disc: 1,
            })
            .collect::<Vec<_>>();
        let mut cache = LastRecentlyUsedList::new(20);
        cache.reconstruct_from(&test_vector, &test_collection);
        util.safe_save(
            "Transient.bin",
            &FileUtil::convert_to_u8(&cache.generate_usage_list()),
        )
        .await
        .unwrap();
        let mut cache_b = LastRecentlyUsedList::new(20);
        cache_b.reconstruct_from(
            &FileUtil::convert_to_u64(&util.try_load_plain("Transient.bin").await.unwrap()),
            &test_collection,
        );
        assert_eq!(
            cache_b.generate_usage_list(),
            test_vector,
            "They should be the same."
        );
    }

    #[tokio::test]
    async fn remove_file_test() {
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.safe_save("Test.tmp", &[1, 2, 3]).await.unwrap();
        util.remove_temps_from_base().await;
        let data = util.try_load_plain("Test.tmp").await;
        assert_eq!(data, None, "The fils should have gone by now");
    }

    #[tokio::test]
    async fn remove_cache_files_test() {
        let base = tempfile::tempdir().unwrap();
        let util = FileUtil::new(&base);
        for tile in [
            TileSpecification::new(1, 0, 1),
            TileSpecification::new(1, 1, 0),
            TileSpecification::new(3, 2, 5),
        ] {
            util.safe_save(tile.filename(), &[1, 2, 3]).await.unwrap();
        }
        util.safe_save(LRU_TABLE_FILE, &[0; 8]).await.unwrap();
        util.safe_save("Left.tmp", &[1]).await.unwrap();
        util.safe_save("notes.txt", &[1]).await.unwrap();
        util.safe_save("foreign/data.png", &[1]).await.unwrap();

        util.remove_cache_files().await.unwrap();

        assert!(util.get_all_pngs_interpreted_as_u64().await.is_empty());
        assert_eq!(util.try_load_plain(LRU_TABLE_FILE).await, None);
        assert_eq!(util.try_load_plain("Left.tmp").await, None);
        assert!(util.try_load_plain("notes.txt").await.is_some());
        assert!(util.try_load_plain("foreign/data.png").await.is_some());
        assert!(
            !base.path().join("1").exists(),
            "Tile folders should be gone"
        );
        assert!(
            !base.path().join("3").exists(),
            "Tile folders should be gone"
        );
        assert!(base.path().exists(), "The base directory stays");
    }
}
