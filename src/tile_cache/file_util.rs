//! Contains helper functionality for file loading and saving.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::fs::*;

/// The data for the file utility.
pub struct FileUtil {
    /// The base path where we operate in.
    base_path: PathBuf,
    ///  a counter for unique temp files.
    temp_file_counter: AtomicU32,
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
    pub async fn save_safe(&self, file_name: impl AsRef<Path>, data: &[u8]) {
        let final_path = self.base_path.join(file_name);
        let dir = final_path.parent().unwrap();
        // For the case the directory does not exist yet, we create it.
        create_dir_all(dir).await.unwrap();
        // Now we create a transient file that will get moved.
        let transient = self.base_path.join(format!(
            "{:08x}.tmp",
            self.temp_file_counter.fetch_add(1, Ordering::Relaxed)
        ));
        write(&transient, data).await.unwrap();
        rename(&transient, &final_path).await.unwrap();
    }

    /// Removes a file from the position.
    pub async fn remove_file(&self, file_name: impl AsRef<Path>) {
        let final_path = self.base_path.join(file_name);
        // If the file is already away it does not matter. 
        let _ = remove_file(final_path).await;
    }


    /// Gets all png filenames recursively interpreted as u64.
    pub async fn get_all_pngs_interpreted_as_u64(&self) -> Vec<u64> {
        let mut result = Vec::new();
        let mut pending = vec![self.base_path.clone()];

        while let Some(dir) = pending.pop() {
            let Ok(mut dir_scan) = read_dir(&dir).await else { continue };

            while let Ok(Some(entry)) = dir_scan.next_entry().await {
                let Ok(file_type) = entry.file_type().await else { continue };
                let path = entry.path();

                if file_type.is_dir() {
                    pending.push(path);
                } else if file_type.is_file() {
                    if path.extension().and_then(|e| e.to_str()) != Some("png") {
                        continue;
                    }
                    if let Some(n) = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| u64::from_str_radix(s, 16).ok())
                    {
                        result.push(n);
                    }
                }
            }

        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_test() {
        let base = vec![12, 23, 24, 25];
        let back_test = FileUtil::convert_to_u64(&FileUtil::convert_to_u8(&base));
        assert_eq!(back_test, base, "Vectors should be the same.");
    }


    #[tokio::test]
    async fn  file_test() {
        let base = vec![12, 23, 24, 25];
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.save_safe("my_test/blob.tmp", &base).await;
        let back_data = util.try_load_plain("my_test/blob.tmp").await.unwrap();
        assert_eq!(back_data, base);
        assert_eq!(util.get_file_length("my_test/blob.tmp").await, 4 * 1024);
        util.remove_file("my_test/blob.tmp").await;
        assert_eq!(util.get_file_length("my_test/blob.tmp").await, 0);
    }

    #[tokio::test]
    async fn fake_png_test() {
        let base = vec![12, 23, 24, 25];
        use crate::tile_cache::tile_name_conversion::*;
        let base_index = TileSpecification::new(1, 2, 3);
        let util = FileUtil::new(tempfile::tempdir().unwrap());
        util.save_safe(base_index.filename(), &base).await;
        let existing_pngs = util.get_all_pngs_interpreted_as_u64().await;
        assert_eq!(TileSpecification::from( existing_pngs[0]), base_index);
    }
}
