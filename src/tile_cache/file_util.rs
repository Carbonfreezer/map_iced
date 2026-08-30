//! Contains helper functionality for file loading and saving.

use std::path::{Path, PathBuf};
use tokio::fs::*;
pub struct FileUtil {
    base_path: PathBuf,
}

impl FileUtil {

    /// Crates a file util with the indicated root directory.
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self { base_path: base_path.as_ref().to_path_buf() }
    }


    /// Tries to load the data, returns a None if the data does not exist.
    pub async fn try_load_plain(&self, file_name: impl AsRef<Path>) -> Option<Vec<u8>> {
        let final_path = self.base_path.join(file_name);
        read(&final_path).await.ok()
    }

    /// Converts a u8 Vector into an u64 vector.
    pub fn convert_to_u64(input: &[u8]) -> Vec<u64> {
        assert!(input.len().is_multiple_of(8), "Input size is not multiple of 8");

        let final_length = input.len() / 8;
        let mut result = Vec::with_capacity(final_length);
        for i in 0..final_length {
            result.push(u64::from_be_bytes(input[i*8..(i+1)*8].try_into().unwrap()));
        }
        result
    }

    /// Converts the input sequence into
    pub fn convert_to_u8(input: &[u64]) -> Vec<u8> {
        input.iter().flat_map(|&x| u64::to_be_bytes(x)).collect()
    }

    /// Makes a save safe, which means the data gets first written to a temporary file, that gets then
    /// moved to the final destination to avoid data inconsistency with crashes.
    pub async fn save_safe(&self, file_name: impl AsRef<Path>, data: &[u8]) {
        todo!()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_test() {
        let base = vec![12,23,24,25];
        let back_test = FileUtil::convert_to_u64(&FileUtil::convert_to_u8(&base));
        assert_eq!(back_test, base, "Vectors should be the same.");
    }
}