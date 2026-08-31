//! Contains helper functionality for file loading and saving.

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

/// A collection of the tiles in combination with their total used memory.
pub struct TileCollection {
    /// The ids with the files.
    pub tile_ids: Vec<u64>,
    /// The total disc size of all files.
    pub total_file_size: u64,
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

    /// Returns a given file length 0 if non-existent.
    pub async fn get_file_length(&self, file_name: impl AsRef<Path>) -> u32 {
        // 4K Block size.
        const BLOCK_SIZE: u32 = 4 * 1024;

        let final_path = self.base_path.join(file_name);
        let length = match metadata(&final_path).await {
            Ok(metadata) => metadata.len() as u32,
            Err(_) => 0,
        };

        length.div_ceil(BLOCK_SIZE) * BLOCK_SIZE
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

    /// Removes a file from the position.
    pub async fn remove_file(&self, file_name: impl AsRef<Path>) {
        let final_path = self.base_path.join(file_name);
        // If the file is already away it does not matter.
        let _ = remove_file(final_path).await;
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

    /// Gets all png filenames recursively interpreted as u64. and returns also the accumulated size in files.
    pub async fn get_all_pngs_interpreted_as_u64(&self) -> TileCollection {
        let mut id_list = Vec::new();
        let mut pending = vec![self.base_path.clone()];
        let mut accumulated_size = 0;

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
                    accumulated_size += self.get_file_length(&path).await as u64;
                    if let Some(n) = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| u64::from_str_radix(s, 16).ok())
                    {
                        id_list.push(n);
                    }
                }
            }
        }

        TileCollection {
            tile_ids: id_list,
            total_file_size: accumulated_size,
        }
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
    async fn file_test() {
        let base = vec![12, 23, 24, 25];
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.safe_save("my_test/blob.tmp", &base).await.unwrap();
        let back_data = util.try_load_plain("my_test/blob.tmp").await.unwrap();
        assert_eq!(back_data, base);
        assert_eq!(util.get_file_length("my_test/blob.tmp").await, 4 * 1024);
        util.remove_file("my_test/blob.tmp").await;
        assert_eq!(util.get_file_length("my_test/blob.tmp").await, 0);
    }

    #[tokio::test]
    async fn fake_png_test() {
        let base = vec![12, 23, 24, 25];
        let base_index = TileSpecification::new(1, 2, 3);
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.safe_save(base_index.filename(), &base).await.unwrap();
        let existing_pngs = util.get_all_pngs_interpreted_as_u64().await;
        assert_eq!(
            TileSpecification::from(existing_pngs.tile_ids[0]),
            base_index
        );
        assert_eq!(existing_pngs.total_file_size, 4 * 1024);
    }

    #[tokio::test]
    async fn lru_cache_test() {
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        let test_vector = vec![12, 23, 24, 25];
        let mut cache = LastRecentlyUsedList::default();
        cache.reconstruct_from(&test_vector);
        util.safe_save(
            "Transient.bin",
            &FileUtil::convert_to_u8(&cache.generate_usage_list()),
        )
        .await
        .unwrap();
        let mut cache_b = LastRecentlyUsedList::default();
        cache_b.reconstruct_from( &FileUtil::convert_to_u64(
            &util.try_load_plain("Transient.bin").await.unwrap(),
        ));
        assert_eq!(
            cache_b.generate_usage_list(),
            test_vector,
            "They should be the same."
        );
    }

    #[tokio::test]
    async fn remove_file_test() {
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.safe_save("Test.tmp", &[1,2,3]).await.unwrap();
        util.remove_temps_from_base().await;
        let memory = util.get_file_length("Test.tmp").await;
        assert_eq!(memory, 0, "The fils should have gone by now");
    }
}
